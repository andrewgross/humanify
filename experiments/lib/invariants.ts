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
/**
 * A line that belongs to the ERROR block above it: indented detail, or the
 * failing line of a Babel code frame, which is marked with ">" instead of
 * indentation (`>  2 | const a = 2;` — 16-findings-queue #20).
 */
function isBlockContinuation(line: string): boolean {
  return /^\s+\S/.test(line) || /^>\s*\d+ \|/.test(line);
}

function extractErrorBlocks(stdout: string): string[] {
  const out: string[] = [];
  let inBlock = false;
  for (const line of stdout.split("\n")) {
    if (line.startsWith("ERROR:")) {
      inBlock = true;
      out.push(line);
    } else if (inBlock && isBlockContinuation(line)) {
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

export interface BootVerdict {
  version: string;
  ok: boolean;
}

/**
 * The WARM self-hop leg (5b self-hop gate, 00-control §3): the self-hop
 * re-run replaying a scratch COPY of the cache the cold self-hop filled.
 * With the model held fixed the pipeline must reproduce the cold leg's tree
 * byte for byte and ask nothing new — rule 10's permitted use of a cache.
 */
export interface WarmSelfHop {
  ran: boolean;
  /** Tree byte-identical to the cold leg's (diagnostics-only files aside). */
  identical: boolean;
  diffFiles: number;
  diffLines: number;
  /** Entries the warm leg added to its cache copy — must be 0. */
  cacheWrites: number;
  coldExit: number;
  warmExit: number;
  /** identical && cacheWrites === 0 && the exit codes match. */
  ok: boolean;
}

/** What cache the COLD self-hop leg ran with: none (as before the warm leg
 *  existed), a fresh empty dir it filled (still cold — every prompt live),
 *  or a copy of `--llm-cache` (NOT cold, so its count is not judged). */
export type ColdCache = "none" | "fresh" | "seeded";

export interface SelfHopVerdict {
  version: string;
  ran: boolean;
  identical: boolean;
  diffLines: number;
  /** Absent on labels recorded before 2026-09-25 — unknown, not "none". */
  coldCache?: ColdCache;
  warm?: WarmSelfHop;
}

/** The cold self-hop counts on record (self-hop-reference.json). */
export interface SelfHopReference {
  version: string;
  coldDiffLines: Record<string, number>;
}

const SELF_HOP_REFERENCE = path.resolve(
  import.meta.dirname,
  "../034-eval-harness/self-hop-reference.json"
);

export function loadSelfHopReference(): SelfHopReference {
  const j = JSON.parse(fs.readFileSync(SELF_HOP_REFERENCE, "utf8"));
  return { version: j.version, coldDiffLines: j.coldDiffLines };
}

/**
 * The cold half of the 5b self-hop gate, as one sentence. A cold count is
 * only comparable to the range when the leg really was cold and ran on the
 * version the range was measured on — anything else says "not judged"
 * rather than printing a verdict it cannot back.
 */
export function coldRangeVerdict(
  s: SelfHopVerdict,
  ref: SelfHopReference
): string {
  const counts = Object.values(ref.coldDiffLines);
  const lo = Math.min(...counts);
  const hi = Math.max(...counts);
  const head = `cold self-hop ${s.version}: ${s.diffLines} ln`;
  if (s.coldCache === "seeded") {
    return `${head} — not judged: the cold leg replayed a copy of --llm-cache, so it was not cold.`;
  }
  if (s.version !== ref.version) {
    return `${head} — not judged: the reference range (${lo}-${hi} ln) is ${ref.version}'s only.`;
  }
  if (s.diffLines > hi) {
    return `!! ${head} — OUTSIDE the reference range ${lo}-${hi} ln (${counts.length} cold runs on record).`;
  }
  if (s.diffLines < lo) {
    return `${head} — below the reference range ${lo}-${hi} ln (fewer re-rolls than any cold run on record).`;
  }
  return `${head} — inside the reference range ${lo}-${hi} ln.`;
}

function warmBanner(s: SelfHopVerdict): string[] {
  const w = s.warm;
  if (!w) return [];
  if (!w.ran) {
    return [
      `!! WARM SELF-HOP DID NOT RUN for ${s.version} — the determinism half of the gate was not checked.`
    ];
  }
  if (w.ok) return [];
  return [
    `!! WARM SELF-HOP FAILED for ${s.version}: ` +
      (w.identical
        ? "tree identical, "
        : `${w.diffFiles} file(s) / ${w.diffLines} ln differ, `) +
      `${w.cacheWrites} cache write(s), exit ${w.coldExit} vs ${w.warmExit} (must be 0 / 0 / 0 / equal) — ` +
      "nondeterminism with the model held fixed."
  ];
}

/** What pipeline scored the label (`pipeline.json`, run.sh). Absent on every
 *  label before 2026-09-25 — those were all the TS program. */
export interface PipelineVerdict {
  kind: string;
  bin?: { sha256: string; commit: string; dirty: boolean };
}

function readPipeline(p: string): PipelineVerdict | undefined {
  const v = readVerdictField(p, "pipeline");
  if (typeof v?.kind !== "string") return undefined;
  const bin = v.bin
    ? { sha256: v.bin.sha256, commit: v.bin.commit, dirty: v.bin.dirty }
    : undefined;
  return { kind: v.kind, bin };
}

function pipelineBanner(v: PairVerdicts): string[] {
  const p = v.pipeline;
  if (p?.kind !== "rust-bin") return [];
  const lines = [
    `NOTE: scored by the Rust binary ${p.bin?.sha256.slice(0, 12) ?? "?"} ` +
      `built from ${p.bin?.commit.slice(0, 12) || "an UNKNOWN commit"}${p.bin?.dirty ? " (DIRTY tree)" : ""}.`
  ];
  if (v.preflight?.covers === "ts-matcher") {
    lines.push(
      "NOTE: the matcher preflight validated the TS matcher (test/e2e/harness), not the binary that scored these pairs."
    );
  }
  return lines;
}

/** How far the matcher was checked before the pairs were scored. `ok` =
 * outcome set unchanged; `not-verified` = the check could not run (fixture
 * builds absent, the normal state of a frozen worktree); `skipped` =
 * `--skip-preflight`. A label carrying anything but `ok` was scored on a
 * matcher nobody validated, which the summary has to say out loud. */
export interface PreflightVerdict {
  verdict: string;
  status: number;
  /** "ts-matcher" on a --bin label: the preflight exercised the TS matcher,
   *  not the binary that scored the pairs. Absent = the TS pipeline scored
   *  them, and the matcher checked IS the one that ran. */
  covers?: string;
}

export interface PairVerdicts {
  boots: BootVerdict[];
  selfHops: SelfHopVerdict[];
  preflight?: PreflightVerdict;
  pipeline?: PipelineVerdict;
}

/**
 * Every boot and self-hop verdict recorded in a results directory.
 *
 * `run.sh` has always WRITTEN `<v>-boot.json` and `<v>-self-hop.json`;
 * until 2026-08-09 nothing read them, so a committed reference labelled
 * valid carried a self-hop `identical: false` that no summary mentioned.
 * A verdict written and unread is worse than none — it reads as assurance.
 */
export function loadPairVerdicts(resultsDir: string): PairVerdicts {
  const out: PairVerdicts = { boots: [], selfHops: [] };
  if (!fs.existsSync(resultsDir)) return out;
  for (const f of fs.readdirSync(resultsDir).sort()) {
    const p = path.join(resultsDir, f);
    if (f.endsWith("-boot.json")) pushBoot(out.boots, p);
    else if (f.endsWith("-self-hop.json")) pushSelfHop(out.selfHops, p);
    else if (f === "preflight-status.json") out.preflight = readPreflight(p);
    else if (f === "pipeline.json") out.pipeline = readPipeline(p);
  }
  return out;
}

/** The preflight verdict, if this label recorded one. Labels scored before
 * 2026-08-15 have none, which is UNKNOWN rather than clean — the preflight's
 * exit code went unread then, so those runs were also unvalidated; they just
 * did not say so. */
function readPreflight(p: string): PreflightVerdict | undefined {
  const v = readVerdictField(p, "preflight");
  if (typeof v?.verdict !== "string" || typeof v.status !== "number") {
    return undefined;
  }
  return typeof v.covers === "string"
    ? { verdict: v.verdict, status: v.status, covers: v.covers }
    : { verdict: v.verdict, status: v.status };
}

function pushBoot(list: BootVerdict[], p: string): void {
  const b = readVerdictField(p, "boot");
  if (typeof b?.version === "string" && typeof b.ok === "boolean") {
    list.push({ version: b.version, ok: b.ok });
  }
}

function pushSelfHop(list: SelfHopVerdict[], p: string): void {
  const s = readVerdictField(p, "selfHop");
  if (typeof s?.version === "string" && typeof s.identical === "boolean") {
    list.push({
      version: s.version,
      ran: s.ran !== false,
      identical: s.identical,
      diffLines: typeof s.diffLines === "number" ? s.diffLines : -1,
      ...(typeof s.coldCache === "string" ? { coldCache: s.coldCache } : {}),
      ...(s.warm && typeof s.warm.ok === "boolean" ? { warm: s.warm } : {})
    });
  }
}

// biome-ignore lint/suspicious/noExplicitAny: external verdict JSON shape
function readVerdictField(p: string, field: string): any {
  try {
    return JSON.parse(fs.readFileSync(p, "utf8"))?.[field];
  } catch {
    return undefined; /* not a verdict file */
  }
}

/**
 * Lines to print when a recorded boot failed or a self-hop diverged — empty
 * when everything recorded is clean, same rule as `runStatusBanner`.
 */
export function verdictBanner(
  v: PairVerdicts,
  ref: SelfHopReference = loadSelfHopReference()
): string[] {
  const lines: string[] = [];
  for (const b of v.boots.filter((b) => !b.ok)) {
    lines.push(
      `!! BOOT FAILED for ${b.version} — the scored tree does not run.`
    );
  }
  for (const s of v.selfHops.filter((s) => s.ran && !s.identical)) {
    lines.push(
      `NOTE: self-hop diverged for ${s.version} (${s.diffLines} diff lines). ` +
        "Expected on a COLD run (live LLM re-rolls, exp047); on a cached run " +
        "this is a determinism regression.",
      coldRangeVerdict(s, ref)
    );
  }
  for (const s of v.selfHops) lines.push(...warmBanner(s));
  lines.push(...pipelineBanner(v));
  if (v.preflight && v.preflight.verdict !== "ok") {
    lines.push(
      `NOTE: matcher preflight '${v.preflight.verdict}' — these pairs were ` +
        "scored WITHOUT a validated fingerprint matcher. Usually the fixture " +
        "builds are absent (a frozen worktree); it is not a matcher finding, " +
        "but it is not a clean bill of health either."
    );
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
