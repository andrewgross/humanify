/**
 * Did the pipeline itself say this run was valid?
 *
 * ## Why this exists
 *
 * `run.sh` verified that every artifact the run PROMISED was on disk — a
 * hardening added after a crash between writing the bundle and writing
 * `--stats-json` once left a pair looking successful. It never looked at the
 * pipeline's EXIT CODE.
 *
 * So a run that completed, wrote every artifact, printed
 *
 *   ERROR: <out>/runtime.js: Rename changed program structure beyond
 *   identifier names ... the output is not a pure rename of the input
 *   ERROR: 1 output file violated rename invariants — output was written for
 *   inspection, but this run is marked failed.
 *
 * and exited 1 was scored exactly like a clean one. That is the state of 2 of
 * the 4 eval pairs today (2.1.86 and 2.1.198), on main and back through
 * exp058: every KPI ever quoted for those pairs was computed from a tree the
 * pipeline had rejected, and nothing in `summary.json` said so.
 *
 * This is the same failure as the self-hop check that reported
 * "VIOLATED: 0 diff lines" when the run produced no output at all — a gate
 * reading a number without first asking whether the number means anything.
 *
 * ## The invariant this file protects
 *
 * A recorded failure must be impossible to read as a clean run, and an absent
 * recording must never be read as a pass. Those are different states and both
 * differ from success; collapsing either into "fine" is how the original bug
 * survived thirteen result sets.
 */
import * as fs from "node:fs";
import * as path from "node:path";

export interface PairRunStatus {
  /** The `to` version of the pair, e.g. "2.1.86". */
  version: string;
  /** The pipeline's exit code. Non-zero means IT declared the run invalid. */
  exitCode: number;
  /** `ERROR:` lines from the run's stdout — why it failed, in its own words. */
  errors: string[];
}

const STATUS_SUFFIX = "-run-status.json";
/** Cap so a pathological run cannot write a megabyte of banner. Counts LINES,
 *  not errors, now that each error carries its explanation with it. */
const MAX_RECORDED_LINES = 60;

/**
 * Record how the pipeline exited for one pair, alongside the `ERROR:` lines
 * that explain it. Called by `run.sh` immediately after the pipeline, while
 * `$?` still refers to it.
 */
/**
 * `ERROR:` lines AND the indented detail beneath each one.
 *
 * The invariant failure prints a headline and then the token-level divergence
 * under it — which is the whole reason the diagnostic exists. Filtering to
 * lines starting with `ERROR:` dropped every line of the explanation, and the
 * `.stdout` that still held it is gitignored, so the committed record said
 * "an invariant failed" and pointed at nothing.
 *
 * Indentation is the delimiter because it is what the emitter already uses and
 * needs no agreement about markers: a continuation line is indented, the next
 * unindented line ends the block.
 */
function extractErrorBlocks(stdout: string): string[] {
  const out: string[] = [];
  let inBlock = false;
  for (const line of stdout.split("\n")) {
    if (line.startsWith("ERROR:")) {
      inBlock = true;
      out.push(line);
    } else if (inBlock && /^\s+\S/.test(line)) {
      out.push(line);
    } else {
      inBlock = false;
    }
    if (out.length >= MAX_RECORDED_LINES) break;
  }
  return out;
}

export function writeRunStatus(
  resultsDir: string,
  version: string,
  exitCode: number
): void {
  const stdoutPath = path.join(resultsDir, `${version}.stdout`);
  let errors: string[] = [];
  if (exitCode !== 0 && fs.existsSync(stdoutPath)) {
    errors = extractErrorBlocks(fs.readFileSync(stdoutPath, "utf8"));
  }
  const status: PairRunStatus = { version, exitCode, errors };
  fs.writeFileSync(
    path.join(resultsDir, `${version}${STATUS_SUFFIX}`),
    JSON.stringify(status, null, 2)
  );
}

/**
 * Every status recorded in a results directory.
 *
 * Returns only what was actually recorded. Result sets committed before this
 * existed have no status file, and they are ABSENT from the list rather than
 * defaulted to clean — defaulting would relabel the two known-bad pairs as
 * good and quietly undo the fix.
 */
export function loadRunStatuses(resultsDir: string): PairRunStatus[] {
  if (!fs.existsSync(resultsDir)) return [];
  const out: PairRunStatus[] = [];
  for (const f of fs.readdirSync(resultsDir)) {
    if (!f.endsWith(STATUS_SUFFIX)) continue;
    try {
      const j = JSON.parse(fs.readFileSync(path.join(resultsDir, f), "utf8"));
      if (typeof j?.version === "string" && typeof j.exitCode === "number") {
        out.push({
          version: j.version,
          exitCode: j.exitCode,
          errors: Array.isArray(j.errors) ? j.errors : []
        });
      }
    } catch {
      /* not a status file */
    }
  }
  return out.sort((a, b) =>
    a.version.localeCompare(b.version, undefined, { numeric: true })
  );
}

/**
 * Lines to print when any recorded run failed — empty when none did.
 *
 * Empty on success is deliberate: a banner that always prints is a banner
 * nobody reads, which is exactly how the `ERROR:` lines already in every
 * `.stdout` went unnoticed for thirteen result sets.
 */
export function runStatusBanner(statuses: readonly PairRunStatus[]): string[] {
  const failed = statuses.filter((s) => s.exitCode !== 0);
  if (failed.length === 0) return [];

  const lines = [
    `!! ${failed.length} of ${statuses.length} scored pair(s) FAILED IN THE PIPELINE: ` +
      failed.map((f) => f.version).join(", ")
  ];
  lines.push(
    "   The KPIs below were computed from tree(s) the pipeline itself marked invalid."
  );
  for (const f of failed) {
    lines.push(`   ${f.version} (exit ${f.exitCode}):`);
    for (const e of f.errors) lines.push(`     ${e}`);
  }
  return lines;
}

/**
 * CLI, so `run.sh` records a status without a second implementation of the
 * format in bash:
 *
 *   npx tsx experiments/lib/invariants.ts <results-dir> <version> <exit-code>
 */
if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(import.meta.filename)
) {
  const [resultsDir, version, code] = process.argv.slice(2);
  if (!resultsDir || !version || code === undefined) {
    console.error("usage: invariants.ts <results-dir> <version> <exit-code>");
    process.exit(2);
  }
  writeRunStatus(resultsDir, version, Number(code));
}
