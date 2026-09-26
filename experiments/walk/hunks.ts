/**
 * Picking EXAMPLE hunks out of a walk's release diff, for a human to read.
 *
 * This is presentation, not measurement: every NUMBER in the walk report comes
 * from the eval scorer (analyze.ts) or the changed-line owner (lib/diff.ts).
 * The classes here only decide which hunks are worth showing and what to call
 * them, so a reader can see real change and residual noise side by side.
 *
 *   name-only  removed and added lines are equal once local identifiers are
 *              masked (034's maskIdentifiers — the nameOnlyLines predicate)
 *   moved      removed and added lines are the same multiset (reordering)
 *   added      only additions      removed  only deletions
 *   real       anything else (may still carry a rename inside an edit)
 */
import { maskIdentifiers } from "../034-eval-harness/name-only-churn.js";

export interface Hunk {
  file: string;
  header: string;
  /** Body lines with their ' ', '+', '-' prefix. */
  lines: string[];
}

export type HunkKind = "name-only" | "moved" | "added" | "removed" | "real";

/** Split `git diff` unified output into hunks, each tagged with its file. */
export function parseHunks(diffText: string): Hunk[] {
  const out: Hunk[] = [];
  let file = "";
  let cur: Hunk | null = null;
  for (const line of diffText.split("\n")) {
    if (line.startsWith("diff --git ")) {
      cur = null;
      const m = / b\/(.*)$/.exec(line);
      file = m ? m[1] : "";
      continue;
    }
    if (line.startsWith("@@")) {
      cur = { file, header: line, lines: [] };
      out.push(cur);
      continue;
    }
    if (!cur) continue;
    if (line.startsWith("\\")) continue; // "\ No newline at end of file"
    if (/^[ +-]/.test(line)) cur.lines.push(line);
  }
  return out;
}

function sortedBody(lines: string[], prefix: string, mask: boolean): string[] {
  return lines
    .filter((l) => l.startsWith(prefix))
    .map((l) => (mask ? maskIdentifiers(l.slice(1)) : l.slice(1).trim()))
    .sort();
}

function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((x, i) => x === b[i]);
}

export function classifyHunk(h: Hunk): HunkKind {
  const minus = sortedBody(h.lines, "-", false);
  const plus = sortedBody(h.lines, "+", false);
  if (minus.length === 0) return "added";
  if (plus.length === 0) return "removed";
  if (sameList(minus, plus)) return "moved";
  if (
    sameList(sortedBody(h.lines, "-", true), sortedBody(h.lines, "+", true))
  ) {
    return "name-only";
  }
  return "real";
}

export function changedCount(h: Hunk): number {
  return h.lines.filter((l) => l.startsWith("+") || l.startsWith("-")).length;
}

/** How readable an example is: a few changed lines, not a wall. */
function readability(h: Hunk): number {
  const n = changedCount(h);
  // vendor/ keeps libraries as one minified line: unreadable as an example.
  if (n < 2 || h.lines.length > 40 || h.lines.some((l) => l.length > 300)) {
    return -1;
  }
  return n <= 16 ? 100 - Math.abs(8 - n) : 50 - n;
}

/**
 * Up to `perKind` examples of each kind, readable ones first, never two from
 * the same file (one file's hunks tend to be one story). Deterministic: ties
 * break on file path then header.
 */
export function pickExamples(
  hunks: Hunk[],
  kinds: HunkKind[],
  perKind: number
): Array<{ kind: HunkKind; hunk: Hunk }> {
  const out: Array<{ kind: HunkKind; hunk: Hunk }> = [];
  for (const kind of kinds) {
    const seen = new Set<string>();
    const ranked = hunks
      .filter((h) => readability(h) >= 0 && classifyHunk(h) === kind)
      .sort(
        (a, b) =>
          readability(b) - readability(a) ||
          a.file.localeCompare(b.file) ||
          a.header.localeCompare(b.header)
      );
    for (const h of ranked) {
      if (seen.has(h.file)) continue;
      seen.add(h.file);
      out.push({ kind, hunk: h });
      if (seen.size >= perKind) break;
    }
  }
  return out;
}

/** Tally changed lines per hunk kind — for the example section's framing only. */
export function tallyKinds(hunks: Hunk[]): Record<HunkKind, number> {
  const t: Record<HunkKind, number> = {
    "name-only": 0,
    moved: 0,
    added: 0,
    removed: 0,
    real: 0
  };
  for (const h of hunks) t[classifyHunk(h)] += changedCount(h);
  return t;
}
