/**
 * The `e2e` check stage: the RELEASE BINARY run end to end on the committed
 * e2e fixtures, and its output booted.
 *
 *   npx tsx scripts/e2e.ts          # needs target/release/humanify (rust:build)
 *
 * Since the cutover (docs/rust-port/19-cutover.md) the pipeline is the Rust
 * binary, so this replaces the TS `*.e2etest.ts` suite. Per fixture with a
 * committed build (test/e2e/fixtures/<name>/build/v<ver>/build/index.js) and
 * per version pair in its fixture.config.json:
 *
 *  1. FRESH — the binary humanifies v1 against a stub LLM endpoint. The stub
 *     answers every naming prompt with `<id>Renamed` for each identifier the
 *     prompt lists, so the whole LLM path runs — request, parse, validate,
 *     apply — and a run where no rename lands fails the stage.
 *  2. PRIOR — v2 with `--prior-version` = step 1's output: the cross-version
 *     path (matching, name transfer, reconcile).
 *  3. DETERMINISM — step 2 again into a second directory: the two trees must
 *     be byte-identical (the stub is deterministic, so any difference is the
 *     binary's own).
 *  4. BOOT — the input and every output are imported by Node; each must load,
 *     export the same names with the same types, and every exported function
 *     called with no arguments must return a value of the same shape. A
 *     rename that breaks a binding, a scope or the module surface fails here.
 *     STRICT: an export name is the module's API, so an output that exports
 *     under any other name fails (finding #55 — the naming stage renamed
 *     `export function createStore`; the `esm-exports` fixture holds every
 *     export form).
 *
 * What it cannot see: the split tree and its run scaffold (the fixtures are
 * single-module libraries, not bundles), and model quality. Both belong to
 * the eval (`npm run eval -- score`), which boots each split tree on four
 * real release pairs.
 *
 * Pass/fail, never advisory. Exit 1 on the first failure, naming it.
 */
import { spawn, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as http from "node:http";
import type { AddressInfo } from "node:net";
import * as os from "node:os";
import * as path from "node:path";

const REPO = path.resolve(import.meta.dirname, "..");
const BIN = path.join(REPO, "target/release/humanify");
const FIXTURES = path.join(REPO, "test/e2e/fixtures");

interface VersionPair {
  v1: string;
  v2: string;
}

function fail(msg: string): never {
  console.error(`E2E FAILED: ${msg}`);
  process.exit(1);
}

/** Every identifier a naming prompt asks for, mapped to `<id>Renamed`. */
export function stubAnswer(requestBody: string): string {
  const out: Record<string, string> = {};
  let parsed: { messages?: Array<{ content?: string }> };
  try {
    parsed = JSON.parse(requestBody);
  } catch {
    return "{}";
  }
  for (const m of parsed.messages ?? []) {
    const hit = /Identifiers to rename: ([^\n]*)/.exec(m.content ?? "");
    if (!hit) continue;
    for (const id of hit[1].split(",").map((s) => s.trim())) {
      if (/^[A-Za-z_$][\w$]*$/.test(id)) out[id] = `${id}Renamed`;
    }
  }
  return JSON.stringify(out);
}

function startStub(): Promise<http.Server> {
  const server = http.createServer((req, res) => {
    let body = "";
    req.on("data", (c) => {
      body += c;
    });
    req.on("end", () => {
      res.setHeader("content-type", "application/json");
      res.end(
        JSON.stringify({
          id: "e2e",
          object: "chat.completion",
          created: 0,
          model: "e2e-stub",
          choices: [
            {
              index: 0,
              finish_reason: "stop",
              message: { role: "assistant", content: stubAnswer(body) }
            }
          ],
          usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 }
        })
      );
    });
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () => resolve(server))
  );
}

/**
 * ASYNC on purpose: the stub LLM lives in this process, so a synchronous
 * spawn would block the event loop that has to answer the binary's requests
 * (the first draft hung exactly so).
 */
async function runBinary(
  args: string[],
  endpoint: string,
  label: string
): Promise<void> {
  const child = spawn(
    BIN,
    [...args, "--endpoint", endpoint, "--api-key", "e2e", "--retries", "0"],
    { stdio: ["ignore", "ignore", "pipe"] }
  );
  let stderr = "";
  child.stderr.on("data", (c) => {
    stderr += c;
  });
  const code = await new Promise<number | null>((resolve) =>
    child.on("close", resolve)
  );
  if (code !== 0) {
    fail(`${label}: the binary exited ${code}\n${stderr.slice(-2000)}`);
  }
}

/** Every file under `dir`, relative, sorted. */
function treeFiles(dir: string): string[] {
  return (fs.readdirSync(dir, { recursive: true }) as string[])
    .filter((f) => fs.statSync(path.join(dir, f)).isFile())
    .sort();
}

function assertIdenticalTrees(a: string, b: string, label: string): void {
  const fa = treeFiles(a);
  const fb = treeFiles(b);
  if (fa.join("\n") !== fb.join("\n")) {
    fail(`${label}: the two runs wrote different file sets`);
  }
  for (const f of fa) {
    if (
      !fs.readFileSync(path.join(a, f)).equals(fs.readFileSync(path.join(b, f)))
    ) {
      fail(`${label}: ${f} differs between two identical runs`);
    }
  }
}

/**
 * The module's observable surface: export names → type, and for each
 * exported function its no-argument result's shape. Run in a child Node so
 * a module that throws on load fails this fixture, not the stage runner.
 */
function surfaceOf(file: string, label: string): string {
  const probe = `
    const m = await import(${JSON.stringify(`file://${file}`)});
    const shape = (v) => v === null ? "null" : typeof v === "object"
      ? "{" + Object.keys(v).sort().map((k) => k + ":" + typeof v[k]).join(",") + "}"
      : typeof v;
    const out = {};
    for (const k of Object.keys(m).sort()) {
      out[k] = typeof m[k];
      if (typeof m[k] === "function") {
        try { out[k + "()"] = shape(m[k]()); } catch (e) { out[k + "()"] = "throws " + e.constructor.name; }
      }
    }
    console.log(JSON.stringify(out));
  `;
  const r = spawnSync(process.execPath, ["--input-type=module", "-e", probe], {
    encoding: "utf8",
    cwd: path.dirname(file)
  });
  if (r.status !== 0) {
    fail(`${label}: ${file} does not load under Node:\n${r.stderr}`);
  }
  return r.stdout.trim();
}

/** Copy a file into an ESM scope so Node reads its `export`s as a module. */
function asModule(file: string, dir: string): string {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "package.json"), '{"type":"module"}\n');
  const dest = path.join(dir, "index.js");
  fs.copyFileSync(file, dest);
  return dest;
}

function fixturePairs(): Array<{ name: string; pair: VersionPair }> {
  const out: Array<{ name: string; pair: VersionPair }> = [];
  for (const name of fs.readdirSync(FIXTURES).sort()) {
    const cfgPath = path.join(FIXTURES, name, "fixture.config.json");
    if (!fs.existsSync(cfgPath)) continue;
    const cfg = JSON.parse(fs.readFileSync(cfgPath, "utf8")) as {
      versionPairs: VersionPair[];
    };
    for (const pair of cfg.versionPairs) out.push({ name, pair });
  }
  return out;
}

function inputOf(name: string, version: string): string {
  const p = path.join(FIXTURES, name, "build", `v${version}`, "build/index.js");
  if (!fs.existsSync(p)) fail(`${name}: no committed build at ${p}`);
  return p;
}

async function checkPair(
  name: string,
  pair: VersionPair,
  endpoint: string,
  scratch: string
): Promise<void> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  const root = path.join(scratch, `${name}-${pair.v1}-${pair.v2}`);
  const fresh = path.join(root, "fresh");
  const prior = path.join(root, "prior-a");
  const again = path.join(root, "prior-b");

  await runBinary(
    [inputOf(name, pair.v1), "-o", fresh],
    endpoint,
    `${label} fresh`
  );
  const freshOut = path.join(fresh, "index.js");
  if (!fs.existsSync(freshOut)) fail(`${label}: fresh run wrote no index.js`);
  if (!fs.readFileSync(freshOut, "utf8").includes("Renamed")) {
    fail(
      `${label}: no stub rename landed — the LLM path did not run end to end`
    );
  }

  const priorArgs = [inputOf(name, pair.v2), "--prior-version", freshOut];
  await runBinary([...priorArgs, "-o", prior], endpoint, `${label} prior`);
  await runBinary(
    [...priorArgs, "-o", again],
    endpoint,
    `${label} prior (again)`
  );
  assertIdenticalTrees(prior, again, label);

  for (const [version, out] of [
    [pair.v1, freshOut],
    [pair.v2, path.join(prior, "index.js")]
  ] as const) {
    const want = surfaceOf(
      asModule(inputOf(name, version), path.join(root, `boot-in-${version}`)),
      `${label} input v${version}`
    );
    const got = surfaceOf(
      asModule(out, path.join(root, `boot-out-${version}`)),
      `${label} output v${version}`
    );
    if (got !== want) {
      fail(
        `${label}: v${version}'s output boots with a different surface\n  input:  ${want}\n  output: ${got}`
      );
    }
  }
  console.log(
    `  ${label}: fresh + prior ran, deterministic, boots with the input's surface`
  );
}

async function main(): Promise<void> {
  if (!fs.existsSync(BIN)) {
    fail(
      `no binary at ${BIN} — the rust:build stage builds it (cargo build --release --locked -p humanify-cli)`
    );
  }
  const pairs = fixturePairs();
  if (pairs.length === 0) fail(`no fixture pairs under ${FIXTURES}`);
  const server = await startStub();
  const { port } = server.address() as AddressInfo;
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "humanify-e2e-"));
  try {
    for (const { name, pair } of pairs) {
      await checkPair(name, pair, `http://127.0.0.1:${port}/v1`, scratch);
    }
  } finally {
    server.close();
    fs.rmSync(scratch, { recursive: true, force: true });
  }
  console.log(`e2e: ${pairs.length} fixture pair(s) passed`);
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  void main();
}
