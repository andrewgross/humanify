/**
 * Show WHAT the noise in a release diff actually is, with the kind labelled.
 *
 *   npx tsx experiments/048-family-permute-cold/show-noise.ts <priorSrc> <freshSrc> [perKind]
 *
 * The eval reports noise as a number per mechanism. A number cannot answer "so
 * what does it look like?", which is the first thing anyone reviewing a release
 * asks — and reading the instances is how six hypotheses in this arc were
 * refuted (measurement-pitfalls rule 1). This prints the biggest instances of
 * each kind, straight from the classifier that produces the totals, so the
 * examples cannot drift from the numbers.
 *
 * Kinds, in the classifier's own terms:
 *   reorder — byte-identical statement emitted at a different position. The diff
 *             charges a delete AND an add; nothing about the code changed.
 *   naming  — same statement shape and literals, different bound names.
 *   alias   — the same, but the statement is a require() header.
 */

import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";

const [priorSrc, freshSrc, perKindArg] = process.argv.slice(2);
if (!priorSrc || !freshSrc) {
  console.error(
    "usage: show-noise.ts <priorSrc> <freshSrc> [examples-per-kind]"
  );
  process.exit(1);
}
const perKind = Number(perKindArg ?? 4);

const samples: NoiseSample[] = [];
const tally = composeDiff(priorSrc, freshSrc, { samples, cap: 400_000 });

const noise = tally.naming + tally.alias + tally.reorder;
console.log(`=== NOISE IN THIS RELEASE DIFF ===`);
console.log(`  total noise lines: ${noise}`);
for (const k of ["reorder", "naming", "alias"] as const) {
  const of = samples.filter((s) => s.kind === k);
  const ln = of.reduce((a, s) => a + s.lines, 0);
  console.log(
    `    ${k.padEnd(8)} ${String(tally[k]).padStart(6)} lines  in ${of.length} instances`
  );
  if (ln !== tally[k]) console.log(`      (samples cover ${ln} of them)`);
}

const trim = (l: string) => (l.length > 160 ? `${l.slice(0, 160)}…` : l);

/** One line of a statement, trimmed for reading. */
const show = (text: string | undefined, n = 3): string[] =>
  (text ?? "").split("\n").slice(0, n).map(trim);

/**
 * The lines that actually DIFFER between two versions of one statement.
 *
 * Printing the first N lines is useless here: these statements share their
 * shape by construction (that is why they were classified as noise at all), so
 * the head is identical and the difference sits somewhere in the middle. Pair
 * the two sides positionally, which is exact when only names changed, and show
 * only the mismatches.
 */
function differingLines(
  prior: string | undefined,
  fresh: string | undefined,
  max = 4
): string[] {
  const a = (prior ?? "").split("\n");
  const b = (fresh ?? "").split("\n");
  const out: string[] = [];
  for (
    let i = 0;
    i < Math.max(a.length, b.length) && out.length < max * 2;
    i++
  ) {
    if (a[i] === b[i]) continue;
    if (a[i] !== undefined) out.push(`  - ${trim(a[i].trim())}`);
    if (b[i] !== undefined) out.push(`  + ${trim(b[i].trim())}`);
  }
  return out;
}

for (const kind of ["naming", "alias", "reorder"] as const) {
  const of = samples
    .filter((s) => s.kind === kind)
    .sort((a, b) => b.lines - a.lines)
    .slice(0, perKind);
  if (of.length === 0) continue;
  console.log(
    `\n\n######## ${kind.toUpperCase()} — ${of.length} biggest ########`
  );
  for (const s of of) {
    console.log(`\n--- ${s.file}  (charges ${s.lines} diff lines) ---`);
    if (kind === "reorder") {
      console.log(`  IDENTICAL in both versions, just emitted elsewhere:`);
      for (const l of show(s.freshText, 2)) console.log(`    ${l}`);
    } else {
      for (const l of differingLines(s.priorText, s.freshText)) console.log(l);
    }
  }
}
