/**
 * The gate. ONE command that runs every check, and reports what it ran.
 *
 *   npm run check                      # everything
 *   npm run check -- --only unit,lint  # a subset, loudly labelled as partial
 *
 * ## Why this exists
 *
 * There used to be three commands and none of them ran everything:
 *
 *   check      typecheck + lint + unit + fingerprint          (no e2e, no knip)
 *   check:all  typecheck + lint + unit + fingerprint + knip   (no e2e)
 *   test       check + e2e                                    (no knip)
 *
 * So `test:e2e` sat outside the documented gate entirely, and `knip` sat
 * outside the one people actually ran — `check:all` was red on main for two
 * findings nobody had seen. That is measurement-pitfalls rule 8 in the test
 * suite instead of the metrics: every command passed honestly, over the wrong
 * scope. The fix is not a fourth command; it is one command with nothing
 * outside it.
 *
 * There was never a speed reason for the split. Since the cutover
 * (docs/rust-port/19-cutover.md) the pipeline is the Rust binary; the TS
 * stages cover the measurement harness that remains.
 *
 * ## The rule this file enforces
 *
 * **A stage that did not run must never look like a stage that passed.** Every
 * stage reports ran / passed / failed / skipped, the summary prints all of
 * them, and a partial run is labelled PARTIAL in the final line so it cannot be
 * pasted into a PR as a green gate. That is the same lesson as a boot check
 * that skips silently when `bun` is missing.
 */
import { spawnSync } from "node:child_process";

interface Stage {
  /** `--only` selector, and the summary label. */
  name: string;
  /** What it protects — printed when it fails, so the failure is actionable. */
  why: string;
  run: string;
  /**
   * An advisory stage's findings are an automated mini code-review for
   * Claude/agents to ACT on (unify or allowlist), not a correctness
   * verdict: nonzero exit prints REVIEW and the run's tail, but the gate
   * stays green and later stages still run. Standalone invocation keeps
   * the real exit code so an agent can key off it.
   */
  advisory?: boolean;
}

/**
 * Every check, in cheapest-first order so a broken tree fails fast.
 *
 * Adding a check is ONE entry here. If it is not in this list it does not run,
 * and that is the only place to look to find out what the gate covers.
 */
const STAGES: readonly Stage[] = [
  {
    name: "typecheck",
    why: "the harness TypeScript compiles (scripts/, test/, experiments/) — the pipeline itself is Rust since the cutover",
    run: "npm run typecheck"
  },
  {
    name: "lint",
    why: "prettier + biome over the living TypeScript, including the complexity ceiling pre-commit enforces",
    run: "npm run lint"
  },
  {
    name: "rust:fmt",
    why: "rustfmt defaults across crates/ — the prettier analog for the pipeline (docs/rust-port/05-rust-toolchain.md §8); formatting drift never reaches review",
    run: "cargo fmt --all --check"
  },
  {
    name: "rust:clippy",
    why: "the house rules as lints: complexity ceiling, hash-iteration ban in decision code, env reads confined to humanify-cli; warnings fatal at the gate only",
    run: "cargo clippy --workspace --all-targets -- -D warnings"
  },
  {
    name: "knip",
    why: "no dead exports or unused dependencies in the harness TypeScript",
    run: "npm run knip"
  },
  {
    name: "knip:prod",
    why: "no exports alive only because a test imports them — production dead code. Sat outside the gate until a burn-down found 3 dead functions that plain knip could not see",
    run: "npm run knip:prod"
  },
  {
    name: "census:clones",
    why: "ADVISORY — potential duplication in the living TypeScript for Claude to review (an automated mini code-review, not a correctness verdict). Act on findings: unify the code or allowlist with a justification in scripts/clone-census.ts",
    run: "npm run census:clones",
    advisory: true
  },
  {
    name: "unit",
    why: "EVERY *.test.ts in the repo — test/ and experiments/ (the harness's own tests; the KPI scorer's owners among them)",
    run: "npm run test:unit"
  },
  {
    name: "rust:unit",
    why: "every #[cfg(test)] and tests/ target in crates/ — the pipeline's unit and integration tests, the frozen test/parity goldens among them. `cargo test` until cargo-nextest is installed where the gate runs (docs/rust-port/RUNBOOK.md)",
    run: "cargo test --workspace"
  },
  {
    name: "rust:build",
    why: "the RELEASE binary — the pipeline the eval scores and the stages below run. `--locked`, the build run.sh makes: a lockfile drift fails here, not an hour into an eval",
    run: "cargo build --release --locked -p humanify-cli"
  },
  {
    name: "rust:format-golden",
    why: "the release binary's formatter replayed against test/parity/format-goldens.json — the TS beautifier's captured output, now the formatter's frozen spec — plus a planted perturbation that must be detected",
    run: "scripts/format-golden.sh"
  },
  {
    name: "rust:parity",
    why: "the parity differ proven able to fail (selftest: planted divergences detected) — it compares two --dump-artifacts dumps of the binary, Rust against Rust",
    run: "tsx scripts/rust-parity.ts"
  },
  {
    name: "e2e",
    why: "the release binary end to end on the committed e2e fixtures (fresh + prior, stub LLM), deterministic, and every output booted under Node with the input's export surface (scripts/e2e.ts)",
    run: "tsx scripts/e2e.ts"
  }
];

const argv = process.argv.slice(2);
const onlyArg = argv.find((a) => a.startsWith("--only"));
const onlyRaw = onlyArg?.includes("=")
  ? onlyArg.split("=")[1]
  : onlyArg
    ? argv[argv.indexOf(onlyArg) + 1]
    : undefined;
const only = onlyRaw
  ? new Set(
      onlyRaw
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
    )
  : null;

if (only) {
  const unknown = [...only].filter((n) => !STAGES.some((s) => s.name === n));
  if (unknown.length > 0) {
    console.error(
      `unknown stage(s): ${unknown.join(", ")}\n` +
        `known: ${STAGES.map((s) => s.name).join(", ")}`
    );
    process.exit(2);
  }
}

type Outcome = "passed" | "failed" | "skipped" | "review";
const results: Array<{ stage: Stage; outcome: Outcome; ms: number }> = [];

for (const stage of STAGES) {
  if (only && !only.has(stage.name)) {
    results.push({ stage, outcome: "skipped", ms: 0 });
    continue;
  }
  process.stdout.write(`\n━━━ ${stage.name} ━━━\n`);
  const started = Date.now();
  // The gate owns the env folklore, like scripts/eval.ts owns bun's PATH:
  // the Rust stages need the user-level cargo bin, which shells that never
  // sourced ~/.profile (commit hooks, agent tool shells) lack. A missing
  // cargo still fails loudly ("cargo: not found") — it is never skipped.
  const r = spawnSync(stage.run, {
    shell: true,
    stdio: "inherit",
    env: {
      ...process.env,
      PATH: `${process.env.HOME}/.cargo/bin:${process.env.HOME}/.bun/bin:${process.env.PATH ?? ""}`
    }
  });
  const ms = Date.now() - started;
  const outcome: Outcome =
    r.status === 0 ? "passed" : stage.advisory ? "review" : "failed";
  results.push({ stage, outcome, ms });
  // Fail fast: a later stage's output would bury the failure that matters.
  // Advisory findings never gate — they are Claude's to act on, after.
  if (outcome === "failed") break;
}

const notRun = results.filter((r) => r.outcome === "skipped");
const failed = results.filter((r) => r.outcome === "failed");
// A stage after a fail-fast break never got a result row; count it as not run
// rather than letting the summary imply the gate covered it.
const unreached = STAGES.length - results.length;

console.log(`\n${"═".repeat(56)}`);
for (const { stage, outcome, ms } of results) {
  const mark =
    outcome === "passed"
      ? "PASS"
      : outcome === "failed"
        ? "FAIL"
        : outcome === "review"
          ? "REVIEW"
          : "skip";
  const time = outcome === "skipped" ? "" : `${(ms / 1000).toFixed(1)}s`;
  console.log(`  ${mark.padEnd(5)} ${stage.name.padEnd(20)} ${time}`);
}
for (let i = results.length; i < STAGES.length; i++) {
  console.log(`  ---   ${STAGES[i].name.padEnd(20)} not reached`);
}

if (failed.length > 0) {
  console.log(`${"═".repeat(56)}`);
  for (const f of failed) {
    console.log(`FAILED: ${f.stage.name} — ${f.stage.why}`);
  }
  if (unreached > 0) {
    console.log(
      `${unreached} later stage(s) did not run. This is NOT a green gate.`
    );
  }
  process.exit(1);
}

const reviews = results.filter((r) => r.outcome === "review");
if (reviews.length > 0) {
  console.log(`${"═".repeat(56)}`);
  for (const r of reviews) {
    console.log(`REVIEW: ${r.stage.name} — ${r.stage.why}`);
  }
}

const partial = notRun.length > 0;
console.log(`${"═".repeat(56)}`);
console.log(
  partial
    ? `PARTIAL — ${results.length - notRun.length}/${STAGES.length} stages ran ` +
        `(skipped: ${notRun.map((r) => r.stage.name).join(", ")}). ` +
        `Not a green gate; run \`npm run check\` with no --only.`
    : `ALL ${STAGES.length} STAGES PASSED`
);
process.exit(0);
