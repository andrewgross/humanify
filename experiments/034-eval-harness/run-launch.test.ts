import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { describe, it } from "node:test";

/**
 * WHAT run.sh LAUNCHES, observed rather than read.
 *
 * run.sh starts the pipeline in three places (the rebase, the scored leg via
 * run-pipeline.ts, the self-hop), and since the cutover every one of them
 * runs the Rust binary: `--bin <path>` names it, and without the flag it is
 * the repo's own `target/release/humanify`, built by the harness. The guard
 * that matters most is the one a reader cannot do by eye: every launch must
 * be byte-identical to the command lines the binary-scored references
 * (rust-5b-c3b272f-a/-b, docs/rust-port/18-5b-eval-result.md) were scored
 * by. So the harness is run for real, end to end, with `npx` and the binary
 * replaced by a recorder that writes the promised artifacts and nothing
 * else; the recorded launches are normalised and compared against a golden
 * captured from the pre-cutover run.sh under `--bin`.
 */

const HERE = import.meta.dirname;
const REPO = path.resolve(HERE, "../..");
const RUN_SH = path.join(HERE, "run.sh");
const GOLDEN = path.join(HERE, "run-launch.golden.txt");
const PAIRS: Array<[string, string]> = JSON.parse(
  fs.readFileSync(path.join(HERE, "pairs.json"), "utf8")
).pairs.map((p: { from: string; to: string }) => [p.from, p.to]);

/**
 * One recorder for `npx` and the fake binary. It logs every call, then plays
 * the pipeline's part just far enough for run.sh to proceed: the output
 * bundle + ledger (so the self-hop runs), the run-pipeline.ts artifacts, the
 * adapter's text. Anything else npx is asked to run (analyze, summarize,
 * invariants) is logged and stubbed, except the binary-provenance owner,
 * which is real code under test and is handed to the real npx.
 */
const RECORDER = String.raw`#!/usr/bin/env bash
LOG="$RECORDER_ROOT/calls.log"
echo "$(basename "$0") NODE_OPTIONS=${"$"}{NODE_OPTIONS:-} :: $(printf '%q ' "$@")" >> "$LOG"
write_tree() {
  local out="" cache="" prev=""
  for a in "$@"; do
    [[ "$prev" == "-o" ]] && out="$a"
    [[ "$prev" == "--llm-cache" ]] && cache="$a"
    prev="$a"
  done
  [[ -n "$out" ]] || return 0
  mkdir -p "$out/.humanify"
  echo "bundle" > "$out/.humanify/humanified.js"
  echo "{}" > "$out/.humanify/split-ledger.json"
  if [[ -n "$cache" && -n "${"$"}{RECORDER_CACHE_WRITE:-}" && "$cache" == *"$RECORDER_CACHE_WRITE"* ]]; then
    mkdir -p "$cache"; echo x > "$cache/written-$$"
  fi
  if [[ -n "${"$"}{RECORDER_DIVERGE:-}" && "$out" == *"$RECORDER_DIVERGE" ]]; then
    echo "diverged" >> "$out/.humanify/humanified.js"
  fi
}
if [[ "$(basename "$0")" != "npx" ]]; then write_tree "$@"; exit 0; fi
case "$*" in
  *pipeline-bin.ts*) exec "$RECORDER_REAL_NPX" "$@" ;;
  *run-pipeline.ts*)
    cfg="${"$"}{@: -1}"
    cp "$cfg" "$RECORDER_ROOT/runcfg-$(jq -r .version "$cfg").json"
    jq -r '.artifacts[]' "$cfg" | while read -r p; do mkdir -p "$(dirname "$p")"; echo x > "$p"; done
    ;;
esac
exit 0
`;

interface Harness {
  root: string;
  label: string;
  status: number;
  stdout: string;
  /** Launch lines + run configs, with every run-specific path replaced. */
  launches: string;
  results: string;
}

function realNpx(): string {
  const r = spawnSync("bash", ["-c", "command -v npx"], { encoding: "utf8" });
  return r.stdout.trim();
}

function fakeCorpus(root: string): void {
  for (const [from, to] of PAIRS) {
    for (const v of [from, to]) {
      const dir = path.join(
        root,
        `inputs/claude-code-${v}/binary-decompiled/src/entrypoints`
      );
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(path.join(dir, "index.js"), "input\n");
    }
    const prior = path.join(root, `priors/claude-code-${from}/.humanify`);
    fs.mkdirSync(prior, { recursive: true });
    fs.writeFileSync(path.join(prior, "humanified.js"), "prior\n");
    fs.writeFileSync(path.join(prior, "split-ledger.json"), "{}\n");
  }
}

function normalise(text: string, root: string, label: string): string {
  return text
    .replaceAll(root, "<TMP>")
    .replaceAll(REPO, "<REPO>")
    .replaceAll(label, "<LABEL>");
}

/** The recorded launches of the PIPELINE only: the three sites plus each
 *  run config — not the analysis steps, which --bin does not touch. */
function launchesOf(root: string, label: string): string {
  const calls = fs
    .readFileSync(path.join(root, "calls.log"), "utf8")
    .split("\n")
    .filter((l) => /src\/index\.ts|^humanify |^cargo /.test(l));
  const cfgs = fs
    .readdirSync(root)
    .filter((f) => f.startsWith("runcfg-"))
    .sort()
    .map((f) => `${f}: ${fs.readFileSync(path.join(root, f), "utf8")}`);
  return normalise([...calls, ...cfgs].join("\n"), root, label);
}

function runHarness(
  flags: string[],
  env: Record<string, string> = {}
): Harness {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "run-launch-"));
  const label = `__run-launch-test-${process.pid}-${path.basename(root)}`;
  const shims = path.join(root, "shims");
  fs.mkdirSync(shims);
  for (const name of ["npx", "humanify"]) {
    fs.writeFileSync(path.join(shims, name), RECORDER, { mode: 0o755 });
  }
  // The harness builds the default binary with the user-level cargo
  // (pipeline-bin.ts puts $HOME/.cargo/bin first). A HOME under the scratch
  // root makes that cargo a recorder that FAILS, so a no---bin run is
  // observed reaching the build and stopping there — no real build, and no
  // launch of anything else.
  const cargoBin = path.join(root, "home/.cargo/bin");
  fs.mkdirSync(cargoBin, { recursive: true });
  fs.writeFileSync(
    path.join(cargoBin, "cargo"),
    `#!/usr/bin/env bash\necho "cargo :: $* (in $PWD)" >> "$RECORDER_ROOT/calls.log"\nexit 1\n`,
    { mode: 0o755 }
  );
  fakeCorpus(root);
  const results = path.join(HERE, "results", label);
  try {
    const r = spawnSync(
      "bash",
      [
        RUN_SH,
        label,
        "--workdir",
        path.join(root, "work"),
        "--inputs-base",
        path.join(root, "inputs"),
        "--priors-base",
        path.join(root, "priors"),
        "--endpoint",
        "http://127.0.0.1:9/v1",
        ...flags.map((f) => f.replaceAll("<TMP>", root))
      ],
      {
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: `${shims}:${process.env.PATH ?? ""}`,
          NODE_OPTIONS: "",
          BOOT_GATE_SOFT: "1",
          RECORDER_ROOT: root,
          RECORDER_REAL_NPX: realNpx(),
          ...Object.fromEntries(
            Object.entries(env).map(([k, v]) => [
              k,
              v.replaceAll("<TMP>", root)
            ])
          )
        }
      }
    );
    const snapshot = fs.existsSync(results)
      ? fs
          .readdirSync(results)
          .filter((f) => f.endsWith(".json"))
          .sort()
          .map((f) => `${f}: ${fs.readFileSync(path.join(results, f), "utf8")}`)
          .join("")
      : "";
    return {
      root,
      label,
      status: r.status ?? -1,
      stdout: normalise(`${r.stdout}${r.stderr}`, root, label),
      launches: fs.existsSync(path.join(root, "calls.log"))
        ? launchesOf(root, label)
        : "",
      results: normalise(snapshot, root, label)
    };
  } finally {
    fs.rmSync(results, { recursive: true, force: true });
    fs.rmSync(root, { recursive: true, force: true });
  }
}

describe("run.sh pipeline launches", () => {
  it("every --bin launch is byte-identical to the binary-scored references'", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.strictEqual(h.status, 0, h.stdout);
    if (process.env.UPDATE_RUN_LAUNCH_GOLDEN === "1") {
      fs.writeFileSync(GOLDEN, h.launches);
    }
    assert.strictEqual(
      h.launches,
      fs.readFileSync(GOLDEN, "utf8"),
      "the --bin launches changed — the binary-scored references were scored by the golden's command lines"
    );
  });

  it("WITHOUT --bin the harness builds and runs the repo's own binary — the TS mode is gone", () => {
    const h = runHarness([], { HOME: "<TMP>/home" });
    // The recorder cargo fails, so the run stops at the build: exit 1 with
    // the build named, and NOTHING launched — above all not src/index.ts.
    assert.strictEqual(h.status, 1, h.stdout);
    assert.match(
      h.stdout,
      /building and recording <REPO>\/target\/release\/humanify/
    );
    assert.match(
      h.launches,
      /^cargo :: build --release --locked -p humanify-cli \(in <REPO>\)$/m
    );
    assert.doesNotMatch(h.launches, /src\/index\.ts|^humanify /m);
    assert.doesNotMatch(
      fs.readFileSync(RUN_SH, "utf8"),
      /src\/index\.ts/,
      "run.sh must not name the deleted TS entry point"
    );
  });

  it("--bin runs the binary at ALL THREE launch sites, never src/index.ts", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.strictEqual(h.status, 0, h.stdout);
    assert.doesNotMatch(h.launches, /src\/index\.ts/);
    const bin = h.launches.split("\n").filter((l) => l.startsWith("humanify "));
    // 4 rebases + the cold self-hop + the warm self-hop.
    assert.strictEqual(
      bin.filter((l) => l.includes("-rebased ")).length,
      4,
      h.launches
    );
    assert.strictEqual(bin.filter((l) => l.includes("-selfhop ")).length, 1);
    assert.strictEqual(
      bin.filter((l) => l.includes("-selfhop-warm ")).length,
      1
    );
    // The scored leg goes through run-pipeline.ts: its config carries the
    // command array instead of the implicit `npx tsx src/index.ts`.
    const cfgs = h.launches.match(/"command": \[\s*"<TMP>\/shims\/humanify"/g);
    assert.strictEqual(cfgs?.length, 4, h.launches);
  });

  it("says --heap-mb is inert and records the binary as the label's pipeline", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.match(h.stdout, /INERT for the Rust binary/);
    assert.match(h.results, /pipeline\.json: .*"kind":"rust-bin"/);
    // The matcher preflight tested the deleted TS matcher; it is retired
    // (docs/rust-port/19-cutover.md) and writes no verdict any more.
    assert.doesNotMatch(h.results, /preflight-status\.json/);
  });

  it("the retired flags are refused upfront: --skip-preflight, --warm-self-hop", () => {
    for (const flag of ["--skip-preflight", "--warm-self-hop"]) {
      const h = runHarness([
        "--bin",
        "<TMP>/shims/humanify",
        "--force-mixed",
        flag
      ]);
      assert.strictEqual(h.status, 2, `${flag}: ${h.stdout}`);
      assert.strictEqual(h.launches, "", "nothing may launch after a refusal");
    }
  });

  it("--bin whose build commit is not the label's commit is REFUSED without --force-mixed", () => {
    // The fake binary lives in no cargo workspace, so the commit it was
    // built from is unknowable — which must refuse exactly like a mismatch.
    const h = runHarness(["--bin", "<TMP>/shims/humanify"]);
    assert.strictEqual(h.status, 2, h.stdout);
    assert.match(h.stdout, /--force-mixed/);
    assert.strictEqual(h.launches, "", "nothing may launch after a refusal");
  });

  it("--ts-beautify-adapter is gone (WP5.6d): run.sh refuses it as an unknown flag", () => {
    const h = runHarness([
      "--bin",
      "<TMP>/shims/humanify",
      "--force-mixed",
      "--ts-beautify-adapter"
    ]);
    assert.strictEqual(h.status, 2, h.stdout);
    assert.strictEqual(h.launches, "", "nothing may launch after a refusal");
  });

  it("no binary launch carries a TS stage-6 text: the binary formats natively", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.strictEqual(h.status, 0, h.stdout);
    assert.doesNotMatch(h.launches, /--beautified-input|beautified\.js/);
    assert.doesNotMatch(h.results, /"adapters"/);
  });

  it("a missing diagnostics trail is a LOUD notice, not a swallowed failure", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.match(h.stdout, /NO DIAGNOSTICS TRAIL/);
  });

  it("the WARM self-hop replays a copy of the cold leg's cache: identical + 0 writes", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    const hop = selfHopOf(h);
    assert.strictEqual(hop.warm.ran, true);
    assert.strictEqual(hop.warm.identical, true);
    assert.strictEqual(hop.warm.cacheWrites, 0);
    assert.strictEqual(hop.warm.diffFiles, 0);
  });

  it("the warm self-hop FAILS on a cache write", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"], {
      RECORDER_CACHE_WRITE: "selfhop-warm-cache"
    });
    const hop = selfHopOf(h);
    assert.strictEqual(hop.warm.cacheWrites, 1);
    assert.strictEqual(hop.warm.ok, false);
    assert.match(h.stdout, /WARM SELF-HOP FAILED/);
  });

  it("the warm self-hop FAILS on a byte difference", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"], {
      RECORDER_DIVERGE: "selfhop-warm"
    });
    const hop = selfHopOf(h);
    assert.strictEqual(hop.warm.identical, false);
    assert.ok(hop.warm.diffLines > 0);
    assert.strictEqual(hop.warm.ok, false);
  });

  it("a --pairs subset self-hops the LAST SCORED pair, not pairs.json's last", () => {
    // It used to test pairs.json's last TO, which a subset never produced —
    // so `--pairs 85->86` silently ran no self-hop at all.
    const h = runHarness([
      "--bin",
      "<TMP>/shims/humanify",
      "--force-mixed",
      "--pairs",
      "85->86"
    ]);
    assert.strictEqual(h.status, 0, h.stdout);
    assert.strictEqual(selfHopOf(h).version, "2.1.86");
  });
});

// biome-ignore lint/suspicious/noExplicitAny: verdict JSON under test
function selfHopOf(h: Harness): any {
  const m = /-self-hop\.json: (.*)\n/.exec(h.results);
  assert.ok(m, `no self-hop verdict recorded:\n${h.stdout}`);
  return JSON.parse(m[1]).selfHop;
}
