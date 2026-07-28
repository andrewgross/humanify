/**
 * 051 task 0 — how much of the naming bucket survives a CORROBORATED pairing.
 *
 *   npx tsx experiments/051-naming-residual/pairing-audit.ts <priorSrc> <freshSrc> [label]
 *
 * `diff-composition` calls a statement pair "naming churn" when the two share a
 * statement hash and differ only in text. But `statementHash` masks every
 * identifier NAME — its own docstring warns short statements "collide across
 * unrelated code" — so a shared hash means "same shape, names blanked". Where a
 * file holds several statements of one shape, step 2 picks FIFO, and a wrong
 * pick manufactures a rename out of two unrelated statements and bills every
 * differing line to noise.
 *
 * Three numbers, per hop, and the deltas between them are the artifact:
 *
 *   1. FIFO naming, the rule every number in the 033-050 arc came out of.
 *   2. FIFO naming split by whether the pairing was FORCED (one candidate of
 *      that hash, no choice made) or CHOSEN (several — arbitrary). This split
 *      carries no threshold and no judgement.
 *   3. Corroborated naming: the same-hash candidate sharing the most identifier
 *      tokens, accepted only above a threshold. Refused pairs are not renames;
 *      their lines move to real change, where git prints them regardless.
 *
 * What each predicate tests, in one sentence each (rule 3):
 *   - "forced" tests whether ONE prior statement of this shape was available —
 *     it does NOT test that the pairing is correct, only that no choice existed.
 *   - "corroborated" tests whether the two statements share identifier tokens —
 *     it does NOT establish identity, it withholds the naming label from pairs
 *     with no evidence for it.
 */
import {
  type ComposeOptions,
  composeDiff,
  type NoiseSample,
  type Tally
} from "../037-noise-source-decomposition/diff-composition.js";

const [PRIOR, FRESH, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: pairing-audit.ts <priorSrc> <freshSrc> [label]");
  process.exit(1);
}

const THRESHOLDS = (process.env.AUDIT_THRESHOLDS ?? "0.3,0.5,0.7")
  .split(",")
  .map(Number);

function run(options?: ComposeOptions): {
  tally: Tally;
  samples: NoiseSample[];
} {
  const samples: NoiseSample[] = [];
  const tally = composeDiff(PRIOR, FRESH, { samples, cap: 400_000 }, options);
  return { tally, samples };
}

const noiseOf = (t: Tally) => t.naming + t.alias + t.reorder;
const pad = (n: number, w = 7) => String(n).padStart(w);

console.log(`=== PAIRING AUDIT — ${LABEL || `${PRIOR} -> ${FRESH}`} ===`);

const fifo = run();
console.log(
  `\nFIFO (the standing rule)\n` +
    `  naming ${pad(fifo.tally.naming)}   alias ${pad(fifo.tally.alias)}` +
    `   reorder ${pad(fifo.tally.reorder)}   noise ${pad(noiseOf(fifo.tally))}` +
    `   real ${pad(fifo.tally.real)}`
);

// --- 2. was the pairing forced, or chosen? -----------------------------------
let forcedLn = 0;
let chosenLn = 0;
let forcedN = 0;
let chosenN = 0;
const chosenByFile = new Map<string, number>();
for (const s of fifo.samples) {
  if (s.kind !== "naming") continue;
  if ((s.candidates ?? 1) > 1) {
    chosenLn += s.lines;
    chosenN++;
    chosenByFile.set(s.file, (chosenByFile.get(s.file) ?? 0) + s.lines);
  } else {
    forcedLn += s.lines;
    forcedN++;
  }
}
const namingLn = forcedLn + chosenLn;
const share = (n: number) =>
  namingLn ? `${((100 * n) / namingLn).toFixed(1)}%`.padStart(6) : "   n/a";
console.log(
  `\n  of that naming mass, by whether the rule had a CHOICE:\n` +
    `    FORCED  (1 candidate)  ${pad(forcedLn)} ln ${share(forcedLn)}  (${forcedN} instances)\n` +
    `    CHOSEN  (>1 candidate) ${pad(chosenLn)} ln ${share(chosenLn)}  (${chosenN} instances)`
);

// --- 3. corroborated pairing, at each threshold ------------------------------
console.log(`\nCORROBORATED (best token overlap, accepted above a threshold)`);
console.log(
  `  thresh   naming    alias  reorder    noise      real   naming delta`
);
for (const minOverlap of THRESHOLDS) {
  const c = run({ pairing: "corroborated", minOverlap });
  const d = c.tally.naming - fifo.tally.naming;
  console.log(
    `  ${minOverlap.toFixed(2)}  ${pad(c.tally.naming)}  ${pad(c.tally.alias)}` +
      `  ${pad(c.tally.reorder)}  ${pad(noiseOf(c.tally))}  ${pad(c.tally.real)}` +
      `   ${d >= 0 ? "+" : ""}${d}`
  );
  console.log(
    `ROW|${LABEL}|corroborated|${minOverlap}|${c.tally.naming}|${c.tally.alias}|${c.tally.reorder}|${c.tally.real}`
  );
}
console.log(
  `ROW|${LABEL}|fifo|-|${fifo.tally.naming}|${fifo.tally.alias}|${fifo.tally.reorder}|${fifo.tally.real}`
);
console.log(
  `ROW|${LABEL}|choice|-|${forcedLn}|${chosenLn}|${forcedN}|${chosenN}`
);

// --- where the arbitrary pairings live ---------------------------------------
console.log(`\n  files carrying the most CHOSEN (arbitrary) naming lines:`);
for (const [f, ln] of [...chosenByFile.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 8)) {
  console.log(`    ${pad(ln, 6)} ln  ${f}`);
}
