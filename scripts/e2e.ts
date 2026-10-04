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
 *  5. SEQUENTIAL — the conservative naming schedule, `--sequential
 *     --batch-size 10` (docs/rust-port/20-fast-mode.md §defaults): fresh
 *     and prior each run twice, byte-identical, and BOTH trees must equal
 *     the committed legacy goldens (test/golden/legacy-default/) — the
 *     pre-2026-09-28 binary's default path (stashed at
 *     /work/preflip-humanify-00e171f1), captured at 00e171f1 and
 *     REGENERATED 2026-09-28: the original capture's bytes were not
 *     reproducible even by its own source binary (const-vs-let on two
 *     declarations; see /work/overnight-notes.md), so the goldens were
 *     re-captured with this file's exact stub + invocation, twice each,
 *     deterministic. The proof
 *     that `--sequential` still ships the old default's bytes — and the
 *     output boots (step 4). The DEFAULT schedule is the relaxed tier, so
 *     its determinism is step 3's assert (the old `--fast` legs are gone:
 *     `--fast` was deleted in the flip).
 *
 * What it cannot see: the split tree and its run scaffold (the fixtures are
 * single-module libraries, not bundles), and model quality. Both belong to
 * the eval (`npm run eval -- score`), which boots each split tree on four
 * real release pairs.
 *
 * BUNDLE fixtures (`"bundle": true` in fixture.config.json — e.g.
 * `esbuild-bundle`, a real esbuild 0.27.2 iife build) ARE split trees:
 * every leg runs with `--split`, and the boot step compares the input
 * bundle's observable behavior (stdout + exit of `node <input>`) with
 * `node run.cjs` in the emitted runnable graph (`bun-bundle`, a real Bun
 * CJS build, runs its input the way Bun's loader does —
 * BUN_CJS_LOADER). The legacy-golden step
 * only runs for pairs with committed goldens — a post-flip fixture has
 * no pre-flip golden to prove anything against, so the comparison is
 * skipped with a printed note instead of silently committed against
 * itself.

 *
 * Pass/fail, never advisory. Exit 1 on the first failure, naming it.
 */
import { spawn, spawnSync } from "node:child_process";
import * as fs from "node:fs";
import type * as http from "node:http";
import type { AddressInfo } from "node:net";
import * as os from "node:os";
import * as path from "node:path";

import { startStubLlm, stubAnswer } from "./lib/stub-llm.js";

const REPO = path.resolve(import.meta.dirname, "..");
const BIN = path.join(REPO, "target/release/humanify");
const FIXTURES = path.join(REPO, "test/e2e/fixtures");

interface VersionPair {
  v1: string;
  v2: string;
}

/**
 * A fixture whose committed build is a real BUNDLE (not a single-module
 * library). Such fixtures run the pipeline WITH `--split`, so the output
 * is the runnable CJS module graph (`src/` + `vendor/` + `run.cjs`), not
 * one rewritten file:
 *
 *   - the "rename landed" check scans the whole output tree;
 *   - `--prior-version` points at the fresh run's
 *     `.humanify/humanified.js`, the tree the next release diffs against;
 *   - the boot step compares BEHAVIOR (stdout + exit of `node run.cjs`
 *     vs `node <input>`, both run in place) — an iife bundle exports
 *     nothing, so there is no import surface to compare (the export-name
 *     invariant keeps its own fixture: `esm-exports`);
 *   - step 5's legacy-golden comparison only runs when a golden for the
 *     pair is committed (the goldens predate bundle fixtures; a new
 *     fixture has none and can never prove the pre-flip default).
 */
interface FixtureConfig {
  versionPairs: VersionPair[];
  bundle?: boolean;
}

function fail(msg: string): never {
  console.error(`E2E FAILED: ${msg}`);
  process.exit(1);
}

export { stubAnswer };

/** The e2e stub — the shared module's, on an ephemeral port. */
function startStub(): Promise<http.Server> {
  return startStubLlm().then(({ server }) => server);
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

/**
 * Bun's CommonJS output (`bun build --format=cjs`, first line
 * `// @bun @bun-cjs`) is ONE function expression,
 * `(function(exports, require, module, __filename, __dirname) {…})`, that
 * Bun's loader calls; Node evaluating the file as-is calls nothing and
 * prints nothing. This does what Bun's loader does: evaluate the file to
 * that function and call it with a real CommonJS module for the file.
 */
const BUN_CJS_LOADER = `
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const Module = require("node:module");
const file = path.resolve(process.argv[1]);
const mod = new Module(file, null);
mod.filename = file;
mod.paths = Module._nodeModulePaths(path.dirname(file));
const wrapper = vm.runInThisContext(fs.readFileSync(file, "utf8"), { filename: file });
wrapper.call(mod.exports, mod.exports, Module.createRequire(file), mod, file, path.dirname(file));
`;

/** True for Bun's CommonJS output (its `// @bun … @bun-cjs` first line). */
function isBunCjs(file: string): boolean {
  const first = fs.readFileSync(file, "utf8").split("\n", 1)[0] ?? "";
  return /^\/\/ @bun\b.*@bun-cjs\b/.test(first);
}

/**
 * A BUNDLE fixture's observable behavior: what the file prints and how it
 * exits when Node runs it AS-IS, in its own directory (a split tree's
 * run.cjs requires its sibling files — no copying it away). A Bun CJS
 * bundle runs through BUN_CJS_LOADER, as Bun would run it.
 */
function behaviorOf(
  file: string,
  label: string
): { stdout: string; status: number } {
  const args = isBunCjs(file)
    ? ["-e", BUN_CJS_LOADER, path.basename(file)]
    : [path.basename(file)];
  const r = spawnSync(process.execPath, args, {
    encoding: "utf8",
    cwd: path.dirname(file)
  });
  if (r.status === null) {
    fail(`${label}: ${file} could not be run by Node:\n${r.stderr}`);
  }
  return { stdout: r.stdout, status: r.status ?? -1 };
}

/** True when the committed legacy goldens cover this pair. */
function hasGolden(dir: string): boolean {
  return fs.existsSync(dir);
}

function fixturePairs(): Array<{
  name: string;
  pair: VersionPair;
  bundle: boolean;
}> {
  const out: Array<{ name: string; pair: VersionPair; bundle: boolean }> = [];
  for (const name of fs.readdirSync(FIXTURES).sort()) {
    const cfgPath = path.join(FIXTURES, name, "fixture.config.json");
    if (!fs.existsSync(cfgPath)) continue;
    const cfg = JSON.parse(fs.readFileSync(cfgPath, "utf8")) as FixtureConfig;
    for (const pair of cfg.versionPairs) {
      out.push({ name, pair, bundle: cfg.bundle === true });
    }
  }
  return out;
}

function inputOf(name: string, version: string): string {
  const p = path.join(FIXTURES, name, "build", `v${version}`, "build/index.js");
  if (!fs.existsSync(p)) fail(`${name}: no committed build at ${p}`);
  return p;
}

/**
 * Steps 1: the fresh run — the binary humanifies v1 against the stub, and
 * a stub rename MUST land (it proves the whole LLM path ran). A bundle
 * fixture's output is a tree, so the rename check scans every file.
 * Returns the output path the report and the prior legs read.
 */
async function freshLeg(
  name: string,
  pair: VersionPair,
  endpoint: string,
  bundle: boolean,
  fresh: string,
  splitArgs: string[]
): Promise<string> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  await runBinary(
    [inputOf(name, pair.v1), ...splitArgs, "-o", fresh],
    endpoint,
    `${label} fresh`
  );
  const freshOut = path.join(fresh, "index.js");
  if (!fs.existsSync(freshOut)) fail(`${label}: fresh run wrote no index.js`);
  const renamedLanded = bundle
    ? treeFiles(fresh).some((f) =>
        fs.readFileSync(path.join(fresh, f), "utf8").includes("Renamed")
      )
    : fs.readFileSync(freshOut, "utf8").includes("Renamed");
  if (!renamedLanded) {
    fail(
      `${label}: no stub rename landed — the LLM path did not run end to end`
    );
  }
  return freshOut;
}

/**
 * Steps 2-3: v2 with `--prior-version` (the cross-version path), twice —
 * the two trees must be byte-identical.
 */
async function priorLeg(
  name: string,
  pair: VersionPair,
  endpoint: string,
  priorReference: string,
  prior: string,
  again: string,
  splitArgs: string[]
): Promise<void> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  const priorArgs = [inputOf(name, pair.v2), "--prior-version", priorReference];
  await runBinary(
    [...priorArgs, ...splitArgs, "-o", prior],
    endpoint,
    `${label} prior`
  );
  await runBinary(
    [...priorArgs, ...splitArgs, "-o", again],
    endpoint,
    `${label} prior (again)`
  );
  assertIdenticalTrees(prior, again, label);
}

async function checkPair(
  name: string,
  pair: VersionPair,
  endpoint: string,
  scratch: string,
  bundle: boolean
): Promise<void> {
  const label = `${name} ${pair.v1}->${pair.v2}`;
  const root = path.join(scratch, `${name}-${pair.v1}-${pair.v2}`);
  const fresh = path.join(root, "fresh");
  const prior = path.join(root, "prior-a");
  const again = path.join(root, "prior-b");
  // A bundle fixture's output is the runnable split tree.
  const splitArgs = bundle ? ["--split"] : [];

  const freshOut = await freshLeg(
    name,
    pair,
    endpoint,
    bundle,
    fresh,
    splitArgs
  );
  // The prior the next release diffs against: the split tree's
  // humanified source, or the single humanified file.
  const priorReference = bundle
    ? path.join(fresh, ".humanify", "humanified.js")
    : freshOut;
  await priorLeg(name, pair, endpoint, priorReference, prior, again, splitArgs);
  const seqPriorA = await sequencedLeg(
    root,
    label,
    name,
    pair,
    endpoint,
    bundle,
    splitArgs
  );

  if (bundle) {
    // A bundle's observable identity is its BEHAVIOR — same stdout, same
    // exit code — input bundle vs split tree, both run in place.
    for (const [version, out, tag] of [
      [pair.v1, fresh, "fresh"],
      [pair.v2, prior, "prior"],
      [pair.v2, seqPriorA, "sequential"]
    ] as const) {
      const want = behaviorOf(
        inputOf(name, version),
        `${label} input v${version}`
      );
      const got = behaviorOf(
        path.join(out, "run.cjs"),
        `${label} ${tag} output v${version}`
      );
      if (got.stdout !== want.stdout || got.status !== want.status) {
        fail(
          `${label}: v${version}'s split tree behaves differently\n  input:  ${JSON.stringify(want)}\n  output: ${JSON.stringify(got)}`
        );
      }
    }
    console.log(
      `  ${label}: fresh + prior (+ --sequential, twice) split trees ran, deterministic, boots with the input's behavior`
    );
  } else {
    for (const [version, out, tag] of [
      [pair.v1, freshOut, "fresh"],
      [pair.v2, path.join(prior, "index.js"), "prior"],
      [pair.v2, path.join(seqPriorA, "index.js"), "sequential"]
    ] as const) {
      const want = surfaceOf(
        asModule(inputOf(name, version), path.join(root, `boot-in-${version}`)),
        `${label} input v${version}`
      );
      const got = surfaceOf(
        asModule(out, path.join(root, `boot-out-${tag}`)),
        `${label} ${tag} output v${version}`
      );
      if (got !== want) {
        fail(
          `${label}: v${version}'s output boots with a different surface\n  input:  ${want}\n  output: ${got}`
        );
      }
    }
    console.log(
      `  ${label}: fresh + prior (+ --sequential, twice, vs the legacy goldens) ran, deterministic, boots with the input's surface`
    );
  }
}

/**
 * Step 5: the conservative schedule at the OLD batch size, fresh + prior
 * each run twice for byte-determinism, compared against the pre-flip
 * default's committed goldens when the pair has any (a post-flip fixture
 * has none — see the header). Returns the sequential-prior output dir (the
 * boot step's third leg).
 */
async function sequencedLeg(
  root: string,
  label: string,
  name: string,
  pair: VersionPair,
  endpoint: string,
  bundle: boolean,
  splitArgs: string[]
): Promise<string> {
  const seqArgs = ["--sequential", "--batch-size", "10"];
  const seqFreshA = path.join(root, "seq-fresh-a");
  const seqFreshB = path.join(root, "seq-fresh-b");
  const seqPriorA = path.join(root, "seq-prior-a");
  const seqPriorB = path.join(root, "seq-prior-b");
  const goldenDir = path.join(REPO, "test/golden/legacy-default");
  const freshGolden = path.join(goldenDir, `${name}-${pair.v1}-fresh`);
  const priorGolden = path.join(goldenDir, `${name}-${pair.v1}-${pair.v2}`);
  await runBinary(
    [inputOf(name, pair.v1), ...splitArgs, ...seqArgs, "-o", seqFreshA],
    endpoint,
    `${label} sequential fresh`
  );
  await runBinary(
    [inputOf(name, pair.v1), ...splitArgs, ...seqArgs, "-o", seqFreshB],
    endpoint,
    `${label} sequential fresh (again)`
  );
  assertIdenticalTrees(seqFreshA, seqFreshB, `${label} --sequential fresh`);
  compareGolden(seqFreshA, freshGolden, label, "--sequential fresh");
  const seqPriorReference = bundle
    ? path.join(seqFreshA, ".humanify", "humanified.js")
    : path.join(seqFreshA, "index.js");
  const seqPriorArgs = [
    inputOf(name, pair.v2),
    "--prior-version",
    seqPriorReference,
    ...seqArgs
  ];
  await runBinary(
    [...seqPriorArgs, ...splitArgs, "-o", seqPriorA],
    endpoint,
    `${label} sequential prior`
  );
  await runBinary(
    [...seqPriorArgs, ...splitArgs, "-o", seqPriorB],
    endpoint,
    `${label} sequential prior (again)`
  );
  assertIdenticalTrees(seqPriorA, seqPriorB, `${label} --sequential prior`);
  compareGolden(seqPriorA, priorGolden, label, "--sequential prior");
  return seqPriorA;
}

/** The byte-comparison against a committed legacy golden, when one exists. */
function compareGolden(
  actual: string,
  goldenDir: string,
  label: string,
  step: string
): void {
  if (hasGolden(goldenDir)) {
    assertIdenticalTrees(
      actual,
      goldenDir,
      `${label} ${step} vs the pre-flip default's golden`
    );
  } else {
    console.log(
      `  ${label}: no committed golden for this pair — golden comparison skipped (${step})`
    );
  }
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
    for (const { name, pair, bundle } of pairs) {
      await checkPair(
        name,
        pair,
        `http://127.0.0.1:${port}/v1`,
        scratch,
        bundle
      );
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
