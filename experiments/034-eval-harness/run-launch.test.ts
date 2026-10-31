import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as crypto from "node:crypto";
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
 *
 * TWO goldens since the base-mode switch (2026-10-03). The default rebuilds
 * each base from SCRATCH, so its rebase lines carry no `--prior-version`
 * (run-launch.golden.txt). `--seeded-base` is the old protocol — every
 * reference before 2026-10-03 — and its golden is the previous golden byte
 * for byte, plus the recorded `"baseMode"` in each run config
 * (run-launch.seeded-base.golden.txt).
 *
 * Config sweep 2026-10-13: the inert `NODE_OPTIONS=--max-old-space-size`
 * the launches carried (and the run configs' `heapMb`) were deleted with
 * `--heap-mb`; both goldens were re-recorded and differ ONLY there — every
 * argv is byte-identical to the one the references were scored by.
 *
 * Config sweep step 4 (2026-10-13): `--api-key` was retired — the binary
 * reads HUMANIFY_API_KEY only, and run.sh exports it. Both goldens were
 * re-recorded: every launch line loses `--api-key local` and records the
 * key it inherited (`HUMANIFY_API_KEY=local`); nothing else moved.
 *
 * Config sweep step 5 (2026-10-13): the binary takes only its operational
 * flags; every other setting is a key in a `--config` TOML file run.sh
 * writes per launch (experiments/lib/pipeline-config.ts). Both goldens
 * were re-recorded: each launch line keeps input, `--endpoint`, `--model`,
 * `-o`, `--prior-version` and `-vv` and passes `--config <file>`, and the
 * recorder logs that file under it (`humanify-config|` lines; each run
 * config carries it as `configToml`). The moved flags' VALUES are
 * unchanged — launch_parity_test.rs proves every golden launch resolves to
 * the effective settings of the launch it replaced.
 *
 * Config sweep step 7 (2026-10-13): run.sh parses no flags. `npm run eval
 * -- score` (scripts/eval.ts) validates every flag once and writes ONE run
 * config file; run.sh takes only its path. So the harness is driven here
 * through eval.ts, exactly as a user drives it. The goldens did NOT change:
 * every launch line, config file and run config is byte-identical. The
 * label's pipeline.json gains the run config's sha256 and the boot gate's
 * pinned model (neither is in a golden).
 *
 * exp171 (2026-10-31): names became the binary's DEFAULT, and a names
 * launch with no index would find or BUILD one at run time (its as-of the
 * day it ran). The dispatcher now hands every launch the full prebuilt
 * index (pairs.json `libraryIndex`, or `--library-index`). Both goldens
 * were re-recorded and differ ONLY by one `[library] index = "<that
 * index>"` section in every launch's config (10 launches: 4 rebases, 4
 * scored legs' `configToml`, 2 self-hop legs); every argv is unchanged. A
 * label scored with `--pipeline-set library.matching=off` is handed no
 * index (its configs carry `matching = "off"` instead).
 */

const HERE = import.meta.dirname;
const REPO = path.resolve(HERE, "../..");
const RUN_SH = path.join(HERE, "run.sh");
const EVAL_TS = path.join(REPO, "scripts/eval.ts");
const GOLDEN = path.join(HERE, "run-launch.golden.txt");
const SEEDED_GOLDEN = path.join(HERE, "run-launch.seeded-base.golden.txt");
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
echo "$(basename "$0") NODE_OPTIONS=${"$"}{NODE_OPTIONS:-} HUMANIFY_API_KEY=${"$"}{HUMANIFY_API_KEY:-} :: $(printf '%q ' "$@")" >> "$LOG"
config_of() {
  local prev=""
  for a in "$@"; do
    [[ "$prev" == "--config" ]] && { echo "$a"; return; }
    prev="$a"
  done
}
# The pipeline's --config file, logged under its launch line.
cfg="$(config_of "$@")"
if [[ "$(basename "$0")" != "npx" && -n "$cfg" && -f "$cfg" ]]; then
  sed 's/^/humanify-config| /; s/ $//' "$cfg" >> "$LOG"
fi
write_tree() {
  local out="" cache="" prev=""
  for a in "$@"; do
    [[ "$prev" == "-o" ]] && out="$a"
    prev="$a"
  done
  [[ -n "$cfg" && -f "$cfg" ]] && cache="$(sed -n 's/^cache_dir = "\(.*\)"$/\1/p' "$cfg")"
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
  *pipeline-bin.ts*|*pipeline-config.ts*) exec "$RECORDER_REAL_NPX" "$@" ;;
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
  /** The run config file eval.ts wrote (raw bytes as text), or "". */
  runConfig: string;
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
    .filter((l) =>
      /src\/index\.ts|^humanify |^humanify-config\||^cargo /.test(l)
    );
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
  // lib/boot-gate.sh is FATAL without bun (no soft mode since the config
  // sweep, 2026-10-13). The recorder writes no run.cjs, so bun is never
  // asked to boot anything; it only has to exist.
  fs.writeFileSync(path.join(shims, "bun"), "#!/usr/bin/env bash\nexit 0\n", {
    mode: 0o755
  });
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
      realNpx(),
      [
        "tsx",
        EVAL_TS,
        "score",
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
          // Never inherited: run.sh must export the key itself.
          HUMANIFY_API_KEY: "",
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
    const cfgFile = path.join(root, "work", `${label}.run-config.json`);
    return {
      root,
      label,
      runConfig: fs.existsSync(cfgFile) ? fs.readFileSync(cfgFile, "utf8") : "",
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
  it("every --bin launch is byte-identical to the golden (default: SCRATCH bases)", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.strictEqual(h.status, 0, h.stdout);
    if (process.env.UPDATE_RUN_LAUNCH_GOLDEN === "1") {
      fs.writeFileSync(GOLDEN, h.launches);
    }
    assert.strictEqual(
      h.launches,
      fs.readFileSync(GOLDEN, "utf8"),
      "the default --bin launches changed — the golden is the protocol a scratch-base label is scored by"
    );
  });

  it("--seeded-base launches are byte-identical to the pre-2026-10-03 protocol (every recorded reference)", () => {
    // main-2026-09-18, rust-relaxed-default-843826be, control-843826be-8gpu
    // and candidate-8af0574f-8gpu were all scored on bases rebuilt WITH the
    // archive as --prior-version. This golden is that protocol's launch
    // lines, kept byte-for-byte so a seeded re-score stays comparable to them.
    const h = runHarness([
      "--bin",
      "<TMP>/shims/humanify",
      "--force-mixed",
      "--seeded-base"
    ]);
    assert.strictEqual(h.status, 0, h.stdout);
    if (process.env.UPDATE_RUN_LAUNCH_GOLDEN === "1") {
      fs.writeFileSync(SEEDED_GOLDEN, h.launches);
    }
    assert.strictEqual(h.launches, fs.readFileSync(SEEDED_GOLDEN, "utf8"));
  });

  it("the DEFAULT rebuilds each base from SCRATCH: no rebase launch has a prior", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.strictEqual(h.status, 0, h.stdout);
    const rebases = h.launches
      .split("\n")
      .filter((l) => l.startsWith("humanify ") && l.includes("-rebased "));
    assert.strictEqual(rebases.length, 4, h.launches);
    for (const l of rebases) assert.doesNotMatch(l, /--prior-version/);
    // The scored leg still uses the rebuilt base as its prior.
    assert.match(
      h.launches,
      /"prior": "<TMP>\/work\/<LABEL>\/2\.1\.85-rebased\//
    );
    // Recorded: per pair (run config -> manifest) and for the label.
    assert.strictEqual(
      h.launches.match(/"baseMode": "scratch"/g)?.length,
      4,
      h.launches
    );
    assert.match(h.results, /pipeline\.json: .*"baseMode":"scratch"/);
    assert.match(h.stdout, /BASE: scratch/);
  });

  it("--seeded-base seeds every rebase with the ARCHIVE prior, and records it", () => {
    const h = runHarness([
      "--bin",
      "<TMP>/shims/humanify",
      "--force-mixed",
      "--seeded-base"
    ]);
    const rebases = h.launches
      .split("\n")
      .filter((l) => l.startsWith("humanify ") && l.includes("-rebased "));
    assert.strictEqual(rebases.length, 4, h.launches);
    for (const l of rebases) {
      assert.match(
        l,
        /--prior-version <TMP>\/priors\/claude-code-[\d.]+\/\.humanify\/humanified\.js/
      );
    }
    assert.strictEqual(h.launches.match(/"baseMode": "seeded"/g)?.length, 4);
    assert.match(h.results, /pipeline\.json: .*"baseMode":"seeded"/);
  });

  it("--archive-prior: no rebuild at all, the archive tree is the base", () => {
    const h = runHarness([
      "--bin",
      "<TMP>/shims/humanify",
      "--force-mixed",
      "--archive-prior"
    ]);
    assert.strictEqual(h.status, 0, h.stdout);
    assert.doesNotMatch(h.launches, /-rebased /);
    assert.strictEqual(h.launches.match(/"baseMode": "archive"/g)?.length, 4);
    assert.match(h.results, /pipeline\.json: .*"baseMode":"archive"/);
  });

  it("--seeded-base and --archive-prior together are refused before anything launches", () => {
    const h = runHarness([
      "--bin",
      "<TMP>/shims/humanify",
      "--force-mixed",
      "--seeded-base",
      "--archive-prior"
    ]);
    assert.strictEqual(h.status, 2, h.stdout);
    assert.strictEqual(h.launches, "", "nothing may launch after a refusal");
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

  it("records the binary as the label's pipeline", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
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

  it("renamed/deleted flags are refused upfront, naming what replaced them", () => {
    for (const [flag, want] of [
      ["--no-vendor", /--no-vendor-score/],
      ["--heap-mb", /removed/]
    ] as const) {
      const h = runHarness([
        "--bin",
        "<TMP>/shims/humanify",
        "--force-mixed",
        flag,
        "1"
      ]);
      assert.strictEqual(h.status, 2, `${flag}: ${h.stdout}`);
      assert.match(h.stdout, want);
      assert.strictEqual(h.launches, "", "nothing may launch after a refusal");
    }
  });

  it("no pipeline launch sets NODE_OPTIONS: the binary is not a Node process", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.strictEqual(h.status, 0, h.stdout);
    assert.doesNotMatch(h.launches, /max-old-space-size|heapMb/);
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

  it("--pipeline-set reaches ALL THREE launch sites' config files, once each, in order", () => {
    const h = runHarness([
      "--bin",
      "<TMP>/shims/humanify",
      "--force-mixed",
      "--pipeline-set",
      "naming.schedule=sequential",
      "--pipeline-set",
      "naming.batch_size=10"
    ]);
    assert.strictEqual(h.status, 0, h.stdout);
    const bin = h.launches.split("\n").filter((l) => l.startsWith("humanify "));
    // 4 rebases + cold + warm self-hop, each with its own config file.
    assert.strictEqual(bin.length, 6, h.launches);
    for (const l of bin)
      assert.match(l, /--config <TMP>\/work\/<LABEL>\/\S+\.toml/);
    // Each launch's file carries the settings once, after the harness's.
    const blocks = h.launches.split(/^humanify /m).slice(1);
    assert.strictEqual(blocks.length, 6);
    for (const b of blocks) {
      assert.strictEqual(
        b.match(
          /humanify-config\| \[naming\]\nhumanify-config\| schedule = "sequential"\nhumanify-config\| batch_size = 10$/m
        )?.length,
        1,
        b
      );
    }
    // The scored legs' run configs carry the file's text.
    const cfgs = h.launches.match(
      /"configToml": ".*\[naming\]\\nschedule = \\"sequential\\"\\nbatch_size = 10\\n"/g
    );
    assert.strictEqual(cfgs?.length, 4, h.launches);
    // The label's pipeline record names them (a label scored with extra
    // settings must not read as the default pipeline).
    assert.match(
      h.results,
      /pipeline\.json: .*"pipelineSettings":\["naming\.schedule=sequential","naming\.batch_size=10"\]/
    );
  });

  it("--pipeline-set refusals happen before anything launches", () => {
    for (const bad of [
      ["--pipeline-set"],
      ["--pipeline-set", "nodot=1"],
      // A key the harness sets itself: two layers disagreeing.
      ["--pipeline-set", "llm.concurrency=8"],
      // Retired: the settings are config keys now.
      ["--pipeline-arg", "--sequential"]
    ]) {
      const h = runHarness([
        "--bin",
        "<TMP>/shims/humanify",
        "--force-mixed",
        ...bad
      ]);
      assert.strictEqual(h.status, 2, `${bad}: ${h.stdout}`);
      assert.strictEqual(h.launches, "", "nothing may launch after a refusal");
    }
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

  it("pipeline.json records the run config's sha256 and the boot gate's model", () => {
    const h = runHarness(["--bin", "<TMP>/shims/humanify", "--force-mixed"]);
    assert.strictEqual(h.status, 0, h.stdout);
    assert.ok(h.runConfig, "eval.ts must write the run config");
    const sha = crypto.createHash("sha256").update(h.runConfig).digest("hex");
    const m = /pipeline\.json: (.*)\n/.exec(h.results);
    assert.ok(m, h.results);
    const pipeline = JSON.parse(m[1]);
    assert.strictEqual(pipeline.runConfig.sha256, sha);
    assert.strictEqual(
      pipeline.runConfig.path,
      "<TMP>/work/<LABEL>.run-config.json"
    );
    // The boot gate's pinned model is a recorded setting, not an env read.
    assert.strictEqual(pipeline.bootGateModel, "claude-opus-4-1");
  });

  it("run.sh refuses flags: eval.ts is the one parser", () => {
    for (const argv of [
      ["some-label"],
      ["some-label", "--bin", "x"],
      ["--pairs", "85->86"]
    ]) {
      const r = spawnSync("bash", [RUN_SH, ...argv], { encoding: "utf8" });
      assert.strictEqual(r.status, 2, `${argv}: ${r.stdout}${r.stderr}`);
      assert.match(r.stderr, /run config/);
    }
  });
});

// biome-ignore lint/suspicious/noExplicitAny: verdict JSON under test
function selfHopOf(h: Harness): any {
  const m = /-self-hop\.json: (.*)\n/.exec(h.results);
  assert.ok(m, `no self-hop verdict recorded:\n${h.stdout}`);
  return JSON.parse(m[1]).selfHop;
}
