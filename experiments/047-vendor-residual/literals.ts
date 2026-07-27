/**
 * Shared instrument for 047 task 1.
 *
 * A vendored file here is 1-4 lines of minified text whose identifiers the
 * minifier rerolls every build (exp046 §A4), so identifiers carry no
 * cross-version signal. Library payloads -- especially the highlight.js/prism
 * grammars that dominate this tree -- are mostly string literals, and a literal
 * SET is insensitive to both renaming and reordering.
 *
 * Deliberately not `structuralHash`: it erases string values entirely
 * (measurement-pitfalls rule 8), which is exactly the signal wanted here.
 */

/** Quoted string literals of >= `minLen` chars, as a set. */
export function literalSet(text: string, minLen = 6): Set<string> {
  const out = new Set<string>();
  const re = new RegExp(
    `"((?:[^"\\\\\\n]|\\\\.){${minLen},})"|'((?:[^'\\\\\\n]|\\\\.){${minLen},})'`,
    "g"
  );
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    const v = m[1] ?? m[2];
    if (v !== undefined) out.add(v);
  }
  return out;
}

export function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 && b.size === 0) return 1;
  if (a.size === 0 || b.size === 0) return 0;
  let inter = 0;
  for (const x of a) if (b.has(x)) inter++;
  return inter / (a.size + b.size - inter);
}

export function overlap(a: Set<string>, b: Set<string>): number {
  let inter = 0;
  for (const x of a) if (b.has(x)) inter++;
  return inter;
}
