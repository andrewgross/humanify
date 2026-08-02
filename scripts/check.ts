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
 * The whole set takes ~25s, so there was never a speed reason for the split.
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
    why: "types compile",
    run: "npm run typecheck"
  },
  {
    name: "lint",
    why: "prettier + biome, including the complexity ceiling pre-commit enforces",
    run: "npm run lint"
  },
  {
    name: "knip",
    why: "no dead exports or unused dependencies",
    run: "npm run knip"
  },
  {
    name: "unit",
    why: "every *.test.ts colocated with its source",
    run: "npm run test:unit"
  },
  {
    name: "explib",
    why: "experiments/lib — the SHARED measurement library every gate depends on. It lives under experiments/, which `test:unit` does not scan, so without this stage its tests would never run",
    run: "npm run test:explib"
  },
  {
    name: "fingerprint",
    why: "e2e fingerprint snapshots in test/e2e — cross-version matching behaviour",
    run: "npm run test:fingerprint"
  },
  {
    name: "e2e",
    why: "*.e2etest.ts against a real build — the suite that used to sit outside the gate",
    run: "npm run test:e2e"
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

type Outcome = "passed" | "failed" | "skipped";
const results: Array<{ stage: Stage; outcome: Outcome; ms: number }> = [];

for (const stage of STAGES) {
  if (only && !only.has(stage.name)) {
    results.push({ stage, outcome: "skipped", ms: 0 });
    continue;
  }
  process.stdout.write(`\n━━━ ${stage.name} ━━━\n`);
  const started = Date.now();
  const r = spawnSync(stage.run, { shell: true, stdio: "inherit" });
  const ms = Date.now() - started;
  const outcome: Outcome = r.status === 0 ? "passed" : "failed";
  results.push({ stage, outcome, ms });
  // Fail fast: a later stage's output would bury the failure that matters.
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
    outcome === "passed" ? "PASS" : outcome === "failed" ? "FAIL" : "skip";
  const time = outcome === "skipped" ? "" : `${(ms / 1000).toFixed(1)}s`;
  console.log(`  ${mark.padEnd(5)} ${stage.name.padEnd(14)} ${time}`);
}
for (let i = results.length; i < STAGES.length; i++) {
  console.log(`  ---   ${STAGES[i].name.padEnd(14)} not reached`);
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
