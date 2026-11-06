import assert from "node:assert";
import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
import { describe, it } from "node:test";

import { CORPUS } from "../experiments/lib/match-truth/corpus.js";
import {
  cleanup,
  goldenOf,
  importClosure,
  newRecording,
  placeBinary,
  REPO,
  type Recording,
  recordedCalls,
  sandboxRepo,
  writeExec
} from "./launch-recorder.js";

/**
 * WHAT EVERY LIVING LAUNCHER LAUNCHES, recorded (exp191, harness lane L0;
 * docs/dev-harness.md §4 proof c). run.sh's launches are held by
 * experiments/034-eval-harness/run-launch.test.ts; these are the rest:
 * neutrality (both legs, the pre-rename baseline, the legacy no-`--config`
 * leg), the library runner, e2e (every leg), ask-trace, match-truth and the
 * boot gate's CLI argv + environment scrub. The Rust runners (L3/L4) must
 * reproduce each golden byte for byte.
 *
 * Each golden is a recorder's log from a LIVE run of the launcher
 * (launch-recorder.ts says how), never typed. To re-record after a
 * deliberate launch change: `UPDATE_LAUNCH_GOLDENS=1 npx tsx --test
 * test/launch-goldens.test.ts`, and say in the commit why every changed
 * line changed.
 *
 * The version walk (experiments/walk/walk.sh) has no golden: it is not a
 * verb of scripts/eval.ts (so, by CLAUDE.md, not a supported instrument),
 * nothing living launches it, and its default binary is still the
 * pre-exp186 `target/release/humanify`. Whether to port or retire it is
 * an open decision (dev-harness.md §1.8).
 */

const PORT = /http:\/\/127\.0\.0\.1:\d+\/v1/g;

function realNpx(): string {
  return spawnSync("bash", ["-c", "command -v npx"], {
    encoding: "utf8"
  }).stdout.trim();
}

function envWith(
  rec: Recording,
  extra: Record<string, string> = {}
): NodeJS.ProcessEnv {
  return {
    ...process.env,
    PATH: `${rec.shims}:${process.env.PATH ?? ""}`,
    HOME: rec.home,
    // npx keeps its cache under the REAL home (the recorder home is empty).
    npm_config_cache:
      process.env.npm_config_cache ?? path.join(process.env.HOME ?? "", ".npm"),
    HUMANIFY_API_KEY: "",
    NODE_OPTIONS: "",
    RECORDER_ROOT: rec.root,
    ...extra
  };
}

function check(name: string, recorded: string): void {
  assert.strictEqual(
    recorded,
    goldenOf(name, recorded),
    `the ${name} launches changed — the golden is the launch protocol the Rust runner must reproduce`
  );
}

// ─── neutrality ──────────────────────────────────────────────────────────

const PAIR_FROM = "2.1.85";
const PAIR_TO = "2.1.86";

/** The files neutrality runs: eval.ts (its one parser) and everything it
 *  imports, the shell launcher and what it sources, and the JSON they read. */
const NEUTRALITY_FILES = [
  ...importClosure(["scripts/eval.ts", "experiments/lib/pipeline-config.ts"]),
  "experiments/lib/neutrality.sh",
  "experiments/lib/build-bin.sh",
  "experiments/lib/llm-endpoint.sh",
  "experiments/034-eval-harness/pairs.json",
  "experiments/034-eval-harness/noise-bands.json"
];

const CRATE_NEW = { "crates/humanify-ag-cli/Cargo.toml": "[package]\n" };
const CRATE_OLD = { "crates/humanify-cli/Cargo.toml": "[package]\n" };

function neutralityCorpus(rec: Recording): void {
  const input = path.join(
    rec.root,
    `inputs/claude-code-${PAIR_TO}/binary-decompiled/src/entrypoints/index.js`
  );
  fs.mkdirSync(path.dirname(input), { recursive: true });
  fs.writeFileSync(input, "input\n");
  const prior = path.join(rec.root, `priors/${PAIR_FROM}-rebased/.humanify`);
  fs.mkdirSync(prior, { recursive: true });
  fs.writeFileSync(path.join(prior, "humanified.js"), "prior\n");
}

interface NeutralityRun {
  status: number;
  stdout: string;
  calls: string;
}

/**
 * One neutrality run: `npm run eval -- neutrality baseline 2.1.85:2.1.86`
 * from a sandbox whose HEAD is the candidate and whose `baseline` branch is
 * `baselineExtra`'s commit; both legs "built" by the cargo recorder. With
 * `direct`, eval.ts is skipped and neutrality.sh gets that run config.
 */
function runNeutrality(
  baselineExtra: Record<string, string>,
  flags: string[],
  direct?: (cfg: Record<string, unknown>) => Record<string, unknown>
): NeutralityRun {
  const rec = newRecording("launch-neutrality");
  try {
    neutralityCorpus(rec);
    const repo = sandboxRepo(rec, [
      { files: NEUTRALITY_FILES, extra: baselineExtra, branch: "baseline" },
      { files: NEUTRALITY_FILES, extra: CRATE_NEW }
    ]);
    const work = path.join(rec.root, "work");
    const args = [
      "neutrality",
      "baseline",
      `${PAIR_FROM}:${PAIR_TO}`,
      "--workdir",
      work,
      "--cache",
      path.join(work, "cache"),
      "--priors",
      path.join(rec.root, "priors"),
      "--inputs-base",
      path.join(rec.root, "inputs"),
      "--endpoint",
      "http://127.0.0.1:9/v1",
      ...flags
    ];
    const env = envWith(rec);
    let r = spawnSync(realNpx(), ["tsx", "scripts/eval.ts", ...args], {
      cwd: repo,
      encoding: "utf8",
      env
    });
    const cfgFile = path.join(work, "neutrality-run-config.json");
    if (direct) {
      const cfg = direct(JSON.parse(fs.readFileSync(cfgFile, "utf8")));
      fs.writeFileSync(cfgFile, `${JSON.stringify(cfg, null, 2)}\n`);
      fs.writeFileSync(path.join(rec.root, "calls.log"), "");
      r = spawnSync("bash", ["experiments/lib/neutrality.sh", cfgFile], {
        cwd: repo,
        encoding: "utf8",
        env
      });
    }
    const runConfig = fs.existsSync(cfgFile)
      ? fs
          .readFileSync(cfgFile, "utf8")
          .split("\n")
          .filter(Boolean)
          .map((l) => `run-config| ${l}`)
          .join("\n")
      : "run-config| <none>";
    const calls = `${recordedCalls(rec)}${runConfig.replaceAll(rec.root, "<TMP>")}\nexit ${r.status}\n`;
    return {
      status: r.status ?? -1,
      stdout: `${r.stdout}${r.stderr}`,
      calls
    };
  } finally {
    cleanup(rec);
  }
}

describe("neutrality launches (eval.ts -> neutrality.sh -> build-bin.sh)", () => {
  it("names mode (the default): both legs built at their commits and launched alike", () => {
    const n = runNeutrality(CRATE_NEW, []);
    assert.strictEqual(n.status, 0, n.stdout);
    assert.match(n.stdout, /^NEUTRAL: identical bytes/m);
    check("neutrality-names", n.calls);
  });

  it("off mode (library.matching=off): no index, the setting on both legs", () => {
    const n = runNeutrality(CRATE_NEW, [
      "--pipeline-set",
      "library.matching=off"
    ]);
    assert.strictEqual(n.status, 0, n.stdout);
    check("neutrality-off", n.calls);
  });

  it("a baseline from before exp186 builds -p humanify-cli and runs target/release/humanify", () => {
    const n = runNeutrality(CRATE_OLD, []);
    assert.strictEqual(n.status, 0, n.stdout);
    assert.match(
      n.calls,
      /-p humanify-cli \(in <TMP>\/work\/neutrality-baseline-src\)/
    );
    check("neutrality-baseline-pre-rename", n.calls);
  });

  it("a legacy baseline (no --config) is REFUSED through eval.ts: every run config carries a setting since exp171", () => {
    // Finding (exp191): eval.ts always hands neutrality.sh a setting —
    // library.index in names mode, library.matching=off in off mode — so
    // leg_flags' no---config branch is unreachable from the one parser.
    const n = runNeutrality({ ...CRATE_OLD, LEGACY_HELP: "" }, []);
    assert.strictEqual(n.status, 1, n.stdout);
    assert.match(
      n.stdout,
      /takes no --config file; --pipeline-set cannot reach it/
    );
    check("neutrality-legacy-refused", n.calls);
  });

  it("the legacy leg's own shape (a run config with no setting, which eval.ts cannot write today)", () => {
    const n = runNeutrality(
      { ...CRATE_OLD, LEGACY_HELP: "" },
      ["--pipeline-set", "library.matching=off"],
      (cfg) => ({ ...cfg, pipelineSettings: [] })
    );
    assert.strictEqual(n.status, 0, n.stdout);
    assert.match(n.calls, /--split --endpoint .* --llm-cache /);
    check("neutrality-legacy-leg", n.calls);
  });
});

// ─── the boot gate ───────────────────────────────────────────────────────

/** Variables a launching session exports: every one BOOT_ENV strips, plus
 *  neighbours it does not name, and a control (KEEP_ME). The recorder bun
 *  logs which survive. Recorded 2026-11-06: ANTHROPIC_SMALL_FAST_MODEL (a
 *  model override) and CLAUDE_CONFIG_DIR survive the scrub — observed, not
 *  judged here (no code change in this lane). */
const SEEDED_ENV: Record<string, string> = {
  ANTHROPIC_MODEL: "m",
  ANTHROPIC_DEFAULT_OPUS_MODEL: "m",
  ANTHROPIC_DEFAULT_SONNET_MODEL: "m",
  ANTHROPIC_DEFAULT_HAIKU_MODEL: "m",
  ANTHROPIC_DEFAULT_FABLE_MODEL: "m",
  ANTHROPIC_SMALL_FAST_MODEL: "m",
  CLAUDE_CODE_SUBAGENT_MODEL: "m",
  ANTHROPIC_BASE_URL: "http://example.invalid",
  ANTHROPIC_API_KEY: "k",
  ANTHROPIC_AUTH_TOKEN: "t",
  CLAUDECODE: "1",
  CLAUDE_CODE_ENTRYPOINT: "cli",
  CLAUDE_CODE_CHILD_SESSION: "1",
  CLAUDE_CODE_SESSION_ID: "s",
  CLAUDE_CODE_MESSAGING_SOCKET: "s",
  CLAUDE_CODE_MESSAGING_TOKEN: "t",
  CLAUDE_CODE_BRIDGE_SESSION_ID: "s",
  CLAUDE_CONFIG_DIR: "/cfg",
  KEEP_ME: "1"
};

describe("the boot gate's launches (experiments/lib/boot-gate.sh)", () => {
  it("boot_version and boot_prompt: argv, cwd and the environment the walked CLI inherits", () => {
    const rec = newRecording("launch-boot");
    try {
      const tree = path.join(rec.root, "tree");
      fs.mkdirSync(tree);
      // `timeout` (the deadline) and `env` (the first word of BOOT_ENV,
      // resolved on PATH by timeout) are recorders that run the real tool,
      // so the deadline and the scrub list are seen exactly as launched.
      for (const tool of ["timeout", "env"]) {
        writeExec(
          path.join(rec.shims, tool),
          `#!/bin/bash\necho "${tool} :: $(printf '%q ' "$@")" >> "$RECORDER_ROOT/calls.log"\nexec /usr/bin/${tool} "$@"\n`
        );
      }
      // boot-gate.sh puts $HOME/.bun/bin first; the recorder bun lives there.
      writeExec(
        path.join(rec.home, ".bun/bin/bun"),
        [
          "#!/bin/bash",
          'echo "bun (in $PWD) :: $(printf \'%q \' "$@")" >> "$RECORDER_ROOT/calls.log"',
          `echo "bun-env| $(/usr/bin/env | sed 's/=.*//' | grep -v '^_$' | sort | tr '\\n' ' ')" >> "$RECORDER_ROOT/calls.log"`,
          'echo "boot-ok"'
        ].join("\n")
      );
      const script = [
        `source "${path.join(REPO, "experiments/lib/boot-gate.sh")}"`,
        `echo "version: $(boot_version "${tree}")"`,
        `echo "prompt: $(boot_prompt "${tree}")"`
      ].join("\n");
      // A clean environment (env -i) holding only the seeded session state,
      // so the recorded survivors are the same on every box.
      const r = spawnSync(
        "/usr/bin/env",
        [
          "-i",
          `PATH=${rec.shims}:/usr/bin:/bin`,
          `HOME=${rec.home}`,
          `RECORDER_ROOT=${rec.root}`,
          ...Object.entries(SEEDED_ENV).map(([k, v]) => `${k}=${v}`),
          "bash",
          "-c",
          script
        ],
        { encoding: "utf8" }
      );
      assert.strictEqual(r.status, 0, r.stderr);
      assert.match(r.stdout, /prompt: boot-ok/);
      const calls = recordedCalls(rec);
      assert.match(calls, /KEEP_ME/);
      assert.doesNotMatch(calls, /bun-env\|.*ANTHROPIC_MODEL /);
      check("boot-gate", calls);
    } finally {
      cleanup(rec);
    }
  });
});

// ─── ask-trace ───────────────────────────────────────────────────────────

const ASK_TRACE_FILES = importClosure(["scripts/ask-trace.ts"]);

describe("ask-trace launches (scripts/ask-trace.ts)", () => {
  it("default, --prior + --sequential + --collide + --set + passthrough, and --set split.enabled", () => {
    const rec = newRecording("launch-asktrace");
    try {
      const repo = sandboxRepo(rec, [{ files: ASK_TRACE_FILES }]);
      placeBinary(path.join(repo, "target/release"), "humanify-ag", rec);
      const input = path.join(rec.root, "input.js");
      fs.writeFileSync(input, "input\n");
      fs.writeFileSync(path.join(rec.root, "prior.js"), "prior\n");
      const legs: string[][] = [
        [input, "--out", path.join(rec.root, "out-default")],
        [
          input,
          "--out",
          path.join(rec.root, "out-prior"),
          "--prior",
          path.join(rec.root, "prior.js"),
          "--sequential",
          "--collide",
          "takenName",
          "--set",
          "naming.batch_size=10",
          "--",
          "--model",
          "m"
        ],
        [
          input,
          "--out",
          path.join(rec.root, "out-split"),
          "--set",
          "split.enabled=true"
        ]
      ];
      for (const leg of legs) {
        const r = spawnSync(
          realNpx(),
          ["tsx", "scripts/ask-trace.ts", ...leg],
          {
            cwd: repo,
            encoding: "utf8",
            env: envWith(rec, {
              HUMANIFY_API_KEY: "inherited-must-be-replaced"
            })
          }
        );
        assert.strictEqual(r.status, 0, `${r.stdout}${r.stderr}`);
        assert.match(r.stdout, /^TOTAL 1 ask\(s\)/m);
      }
      check(
        "ask-trace",
        recordedCalls(rec, [[PORT, "http://127.0.0.1:<PORT>/v1"]])
      );
    } finally {
      cleanup(rec);
    }
  });
});

// ─── match-truth ─────────────────────────────────────────────────────────

/** npm: logs `npm pack <pkg>@<ver> --silent --pack-destination <dir>`,
 *  then writes the tarball it would download, holding package/<file> (the
 *  path the corpus entry names, from npm-files/<pkg>@<ver>). */
const NPM_RECORDER = String.raw`#!/bin/bash
echo "npm (in $PWD) :: $*" >> "$RECORDER_ROOT/calls.log"
spec="$2" dest="$5"
file="$(cat "$RECORDER_ROOT/npm-files/$spec")"
t="$(mktemp -d)"; mkdir -p "$t/package/$(dirname "$file")"
echo "// $spec" > "$t/package/$file"
name="${"$"}{spec%@*}"; ver="${"$"}{spec##*@}"
tar -czf "$dest/$name-$ver.tgz" -C "$t" package; rm -rf "$t"
`;

describe("match-truth launches (experiments/lib/match-truth/run.ts)", () => {
  it("`npm run match-truth` with its defaults: npm pack, format and match per corpus pair", () => {
    const rec = newRecording("launch-matchtruth");
    try {
      // Run from a scratch cwd: every default path is cwd-relative.
      const cwd = path.join(rec.root, "cwd");
      placeBinary(path.join(cwd, "target/release"), "humanify-ag", rec);
      const files = path.join(rec.root, "npm-files");
      fs.mkdirSync(files);
      for (const e of CORPUS) {
        for (const v of [e.oldVersion, e.newVersion]) {
          fs.writeFileSync(path.join(files, `${e.package}@${v}`), e.file);
        }
      }
      // npm: logs `npm pack`, then writes the tarball it would download,
      // holding package/<file> (the path the corpus entry names).
      writeExec(path.join(rec.shims, "npm"), NPM_RECORDER);
      const r = spawnSync(
        realNpx(),
        ["tsx", path.join(REPO, "experiments/lib/match-truth/run.ts")],
        {
          cwd,
          encoding: "utf8",
          env: envWith(rec, {
            RECORDER_MATCH_DUMP: path.join(
              REPO,
              "experiments/lib/match-truth/fixtures/dump.json"
            )
          })
        }
      );
      assert.strictEqual(r.status, 0, `${r.stdout}${r.stderr}`);
      assert.match(r.stdout, /^TOTAL /m);
      const calls = recordedCalls(rec);
      assert.strictEqual(
        calls.match(/humanify-ag \(in [^)]*\) .*:: match /g)?.length,
        CORPUS.length
      );
      check("match-truth", calls);
    } finally {
      cleanup(rec);
    }
  });
});

// ─── the library runner ──────────────────────────────────────────────────

const LIBRARY_DIR = "experiments/096-library-matching-score";

/** A python stand-in for a script run.py LAUNCHES (corpus.py verify,
 *  score.py): logs the full launch line, exits 0. */
const PY_RECORDER = [
  "import os, sys",
  "with open(os.path.join(os.environ['RECORDER_ROOT'], 'calls.log'), 'a') as fh:",
  "    fh.write('python :: ' + ' '.join(sys.orig_argv[1:]) + '\\n')"
].join("\n");

/**
 * Drives run.py's main() for real. The one patch: lib096.CORPORA's entry
 * for the corpus points at a scratch corpus (its data roots are absolute
 * /work paths). Every launch — the cargo build, the discover runs under
 * taskset, corpus.py verify, score.py — is run.py's own code.
 */
const DRIVER = (dir: string, corpus: string, data: string, argv: string[]) =>
  [
    "import json, os, sys",
    `sys.path.insert(0, ${JSON.stringify(dir)})`,
    "import lib096",
    `lib096.CORPORA[${JSON.stringify(corpus)}] = {k: os.path.join(${JSON.stringify(data)}, k + '.json') for k in ('manifest', 'origins', 'copies', 'chained')}`,
    `lib096.CORPORA[${JSON.stringify(corpus)}]['data'] = ${JSON.stringify(data)}`,
    "import run",
    `sys.argv = ['run.py'] + ${JSON.stringify(argv)}`,
    "run.main()"
  ].join("\n");

function libraryCorpus(data: string): void {
  const entries = [
    { id: "app-small", set: "setA", bundler: "bun", bytes: 10 },
    {
      id: "app-big",
      set: "setB",
      bundler: "esbuild",
      builtWith: "webpack",
      bytes: 20
    }
  ].map(({ bytes, ...e }) => {
    const dir = path.join(data, "corpus", e.id);
    fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(path.join(dir, "bundle.js"), "x".repeat(bytes));
    return { ...e, files: { bundle: { name: "bundle.js", bytes } } };
  });
  fs.writeFileSync(
    path.join(data, "manifest.json"),
    JSON.stringify({ entries })
  );
}

describe("library runner launches (096 run.py: build, discover per bundle under taskset, score)", () => {
  for (const [name, argv] of [
    ["library-structural", ["--rule", "structural", "--jobs", "1"]],
    [
      "library-agreement-sets",
      [
        "--rule",
        "agreement",
        "--jobs",
        "1",
        "--cpus",
        "16-31",
        "--sets",
        "setB",
        "--pipeline-set",
        "library.matching=names"
      ]
    ]
  ] as const) {
    it(`${name}: every launch run.py makes`, () => {
      const rec = newRecording("launch-library");
      try {
        const repo = sandboxRepo(rec, [
          {
            files: [
              `${LIBRARY_DIR}/run.py`,
              `${LIBRARY_DIR}/lib096.py`,
              "experiments/lib/build-bin.sh",
              ...importClosure(["experiments/lib/pipeline-config.ts"])
            ],
            extra: {
              ...CRATE_NEW,
              [`${LIBRARY_DIR}/corpus.py`]: PY_RECORDER,
              [`${LIBRARY_DIR}/score.py`]: PY_RECORDER
            }
          }
        ]);
        const data = path.join(rec.root, "data");
        libraryCorpus(data);
        const index = path.join(rec.root, "index");
        fs.mkdirSync(index);
        fs.writeFileSync(path.join(index, "manifest.json"), "{}\n");
        // taskset: logs its argv, runs the command it was given.
        writeExec(
          path.join(rec.shims, "taskset"),
          `#!/bin/bash\necho "taskset :: $*" >> "$RECORDER_ROOT/calls.log"\nshift 2\nexec "$@"\n`
        );
        const r = spawnSync(
          "python3",
          [
            "-c",
            DRIVER(path.join(repo, LIBRARY_DIR), "bun-dev", data, [
              "lbl",
              "--index",
              index,
              "--corpus",
              "bun-dev",
              ...argv
            ])
          ],
          { cwd: rec.root, encoding: "utf8", env: envWith(rec) }
        );
        assert.strictEqual(r.status, 0, `${r.stdout}${r.stderr}`);
        const py = spawnSync(
          "python3",
          ["-c", "import sys; print(sys.executable)"],
          {
            encoding: "utf8"
          }
        ).stdout.trim();
        const calls = recordedCalls(rec, [[py, "<PY>"]]);
        assert.match(
          calls,
          /^taskset :: -c \S+ <REPO>\/target\/release\/humanify-ag discover /m
        );
        check(name, calls);
      } finally {
        cleanup(rec);
      }
    });
  }
});

// ─── e2e ─────────────────────────────────────────────────────────────────

const REAL_BIN = path.join(REPO, "target/release/humanify-ag");

/** e2e's recorder: logs the launch and its --config file, then runs the
 *  real release binary with the same argv. */
const PASS_THROUGH = String.raw`#!/usr/bin/env bash
echo "$0 HUMANIFY_API_KEY=${"$"}{HUMANIFY_API_KEY:-} :: $(printf '%q ' "$@")" >> "$RECORDER_ROOT/calls.log"
prev=""
for a in "$@"; do
  [[ "$prev" == "--config" && -f "$a" ]] && sed 's/^/humanify-config| /; s/ $//' "$a" >> "$RECORDER_ROOT/calls.log"
  prev="$a"
done
exec "${REAL_BIN}" "$@"
`;

describe("e2e launches (scripts/e2e.ts: every leg of every fixture pair)", () => {
  it("the release binary runs every leg through a pass-through recorder", () => {
    // e2e checks what each leg WROTE (renames, boot, determinism, the legacy
    // goldens), so its recorder is a pass-through to the real binary —
    // built here exactly as the rust:build stage builds it (a no-op when
    // fresh), because this stage runs before rust:build in a clean checkout.
    const build = spawnSync(
      "cargo",
      ["build", "--release", "--locked", "-p", "humanify-ag-cli"],
      {
        cwd: REPO,
        encoding: "utf8",
        env: {
          ...process.env,
          PATH: `${path.join(process.env.HOME ?? "", ".cargo/bin")}:${process.env.PATH ?? ""}`
        }
      }
    );
    assert.strictEqual(build.status, 0, build.stderr.slice(-2000));
    const rec = newRecording("launch-e2e");
    try {
      const repo = sandboxRepo(rec, [
        { files: importClosure(["scripts/e2e.ts"]) }
      ]);
      for (const d of [
        "test/e2e/fixtures",
        "test/e2e/library-index",
        "test/golden/legacy-default"
      ]) {
        fs.mkdirSync(path.dirname(path.join(repo, d)), { recursive: true });
        fs.symlinkSync(path.join(REPO, d), path.join(repo, d));
      }
      writeExec(path.join(repo, "target/release/humanify-ag"), PASS_THROUGH);
      const r = spawnSync(realNpx(), ["tsx", "scripts/e2e.ts"], {
        cwd: repo,
        encoding: "utf8",
        env: { ...envWith(rec), HOME: process.env.HOME, TMPDIR: rec.root },
        maxBuffer: 1 << 28
      });
      assert.strictEqual(r.status, 0, `${r.stdout}${r.stderr}`.slice(-4000));
      const calls = recordedCalls(rec, [
        [PORT, "http://127.0.0.1:<PORT>/v1"],
        [/humanify-e2e-[A-Za-z0-9]{6}/g, "humanify-e2e-<RUN>"]
      ]);
      assert.match(calls, /:: detect --toolchain /);
      check("e2e", calls);
    } finally {
      cleanup(rec);
    }
  });
});
