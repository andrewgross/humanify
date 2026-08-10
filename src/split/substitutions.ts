/**
 * Apply positional text substitutions to a file's lines.
 *
 * One owner for the question "rewrite these (line, col) occurrences in
 * TEXT" — bundle-carry and post-split-reconcile each carried a private
 * copy that differed by exactly one guard. The guard (skip a second
 * substitution at the same position) is the unified behaviour: applying
 * the same position twice corrupts the line, because the second splice
 * lands inside the first replacement's text. Producers that never emit
 * same-position duplicates (post-split reconcile) are unaffected.
 *
 * The replacement TEXT for each occurrence comes from
 * `renameSubstitutionText` (src/babel-utils.ts) — the shorthand-property
 * owner. This module only owns the splicing.
 */

export interface Substitution {
  /** 1-based line. */
  line: number;
  /** 0-based column. */
  col: number;
  from: string;
  to: string;
}

export function applySubstitutions(
  lines: string[],
  subs: Substitution[]
): string {
  const byLine = new Map<number, Substitution[]>();
  for (const sub of subs) {
    const list = byLine.get(sub.line) ?? [];
    list.push(sub);
    byLine.set(sub.line, list);
  }
  const out = lines.slice();
  for (const [lineNo, list] of byLine) {
    list.sort((a, b) => b.col - a.col);
    let text = out[lineNo - 1];
    let previousCol = Number.POSITIVE_INFINITY;
    for (const sub of list) {
      if (sub.col >= previousCol) continue; // same position twice: skip
      text =
        text.slice(0, sub.col) + sub.to + text.slice(sub.col + sub.from.length);
      previousCol = sub.col;
    }
    out[lineNo - 1] = text;
  }
  return out.join("\n");
}
