/**
 * Build-constant churn: diff lines that exist only because a bundler inlined a
 * constants object at every use site.
 *
 * MEASUREMENT ONLY. Nothing here changes what the deobfuscator emits — Andrew,
 * 2026-08-19: "we don't need to special case the version, package url and build
 * time in our code, we can just make sure we account for it when looking at
 * noise in our experiments so we don't inflate our numbers."
 *
 * ## Why the scorecard needs this
 *
 * Measured on 2.1.215's tree: one 8-field build-metadata literal appears at
 * **216 byte-identical sites across 83 files**, holding 2,160 tree lines, and
 * 166 of those sites read a single field (`.VERSION`). Three of the fields
 * change every release, so every release pays ~1,300 diff lines for ONE fact.
 *
 * On a CALM release that is 82% of the entire diff, and 82 of the 109 changed
 * files change for no other reason. It is charged to `real` — correctly, the
 * values genuinely did change — so no noise KPI can see it, and every KPI that
 * includes it is inflated by a constant that no lever will ever move.
 *
 * ## Why line-level and not part of composeDiff
 *
 * `composeDiff` classifies STATEMENTS. The inlined literal sits inside a much
 * larger expression statement, so the statement-level view cannot isolate it —
 * it would charge the whole surrounding statement. This is a deliberately
 * separate, line-level pass, reported alongside rather than folded in.
 *
 * ## What counts
 *
 * A changed line counts when its key is one of `BUILD_STAMP_FIELDS`, appears on
 * BOTH sides of that file's diff (modified, not added or removed), and changed
 * in at least `MIN_FILES` files. See those constants for why each condition is
 * there — a metric whose job is to SUBTRACT lines has to be stingy, not clever.
 */
import * as fs from "node:fs";
import * as path from "node:path";

/**
 * The build-stamp fields, BY NAME. Andrew, 2026-08-19: "since it's just for our
 * experiments, we can just hardcode the specific name lines to ignore."
 *
 * Deliberately a fixed list rather than a SCREAMING_SNAKE pattern. A general
 * rule quietly absorbed 24 lines of genuinely new feature flags
 * (CLAUDE_AGENTS_SELECT and friends) on the busy hop — real change, silently
 * subtracted from the headline number. A metric that removes lines is far more
 * dangerous when it is generous, so this one names exactly what it removes and
 * nothing else.
 */
const BUILD_STAMP_FIELDS = new Set(["VERSION", "BUILD_TIME", "GIT_SHA"]);

/** `VERSION: "2.1.215",` — a constant field in an inlined literal. */
const CONST_FIELD = /^\s*([A-Za-z][A-Za-z0-9_]*):\s*.+$/;

/** Kept even though the field list is fixed: both guards are what make the
 *  subtraction defensible, and they cost nothing.
 *
 *  - a key must appear on BOTH sides for a file, so a stamp field ADDED or
 *    REMOVED stays real change;
 *  - a key must change in at least this many files, so a one-off edit to a
 *    VERSION constant that genuinely lives in one place is not absorbed. */
const MIN_FILES = 2;

export interface BuildConstantChurn {
  /** git lines (deletions + insertions) charged to modified constant fields. */
  lines: number;
  /** Distinct files holding at least one. */
  files: number;
  /** Per-key totals, so the report can name what it excluded. */
  byKey: Record<string, number>;
}

function walkFiles(dir: string, base = dir, out: string[] = []): string[] {
  if (!fs.existsSync(dir)) return out;
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walkFiles(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

/** Lines present in `a` but not `b`, as a multiset keyed by trimmed text. */
function surplus(a: string[], b: string[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const l of a) counts.set(l, (counts.get(l) ?? 0) + 1);
  for (const l of b) {
    const n = counts.get(l);
    if (n !== undefined) {
      if (n === 1) counts.delete(l);
      else counts.set(l, n - 1);
    }
  }
  return counts;
}

/**
 * Count build-constant churn between two emitted trees.
 *
 * Multiset difference rather than a real diff: a line whose text is unchanged
 * cannot be a changed line however the hunks fall, and a constant field that
 * moved without changing is not churn either. That makes this independent of
 * diff-algorithm choices, which matters for a number meant to be SUBTRACTED
 * from another tool's total.
 */
export function buildConstantChurn(
  priorDir: string,
  freshDir: string
): BuildConstantChurn {
  const files = new Set([...walkFiles(priorDir), ...walkFiles(freshDir)]);
  // key -> per-file line counts, so the MIN_FILES filter can be applied after
  // every file has been seen rather than guessed at per file.
  const perKey = new Map<string, Map<string, number>>();

  for (const rel of files) {
    const pPath = path.join(priorDir, rel);
    const fPath = path.join(freshDir, rel);
    if (!fs.existsSync(pPath) || !fs.existsSync(fPath)) continue;
    const prior = fs.readFileSync(pPath, "utf8").split("\n");
    const fresh = fs.readFileSync(fPath, "utf8").split("\n");

    const gone = surplus(prior, fresh);
    const came = surplus(fresh, prior);
    if (gone.size === 0 && came.size === 0) continue;

    const keyCount = (m: Map<string, number>): Map<string, number> => {
      const out = new Map<string, number>();
      for (const [text, n] of m) {
        const key = CONST_FIELD.exec(text)?.[1];
        if (key && BUILD_STAMP_FIELDS.has(key)) {
          out.set(key, (out.get(key) ?? 0) + n);
        }
      }
      return out;
    };
    const goneKeys = keyCount(gone);
    const cameKeys = keyCount(came);

    for (const [key, n] of goneKeys) {
      // Both sides: the field was MODIFIED. A key only on one side is a
      // constant added or removed, which is real change.
      const paired = Math.min(n, cameKeys.get(key) ?? 0);
      if (paired === 0) continue;
      let perFile = perKey.get(key);
      if (!perFile) {
        perFile = new Map();
        perKey.set(key, perFile);
      }
      perFile.set(rel, paired * 2);
    }
  }

  const byKey: Record<string, number> = {};
  const touchedFiles = new Set<string>();
  let lines = 0;
  for (const [key, perFile] of perKey) {
    if (perFile.size < MIN_FILES) continue;
    for (const [rel, n] of perFile) {
      byKey[key] = (byKey[key] ?? 0) + n;
      lines += n;
      touchedFiles.add(rel);
    }
  }

  return { lines, files: touchedFiles.size, byKey };
}
