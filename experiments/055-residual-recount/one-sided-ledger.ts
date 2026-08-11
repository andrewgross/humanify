/**
 * 055 Task 0b — how much of the ONE-SIDED mass is rename churn?
 *
 *   npx tsx experiments/055-residual-recount/one-sided-ledger.ts <priorSrc> <freshSrc> [label]
 *
 * RESULTS.md caveat 1: `real-ledger.ts` can only inspect REAL statements the
 * classifier managed to PAIR. 56-86% of REAL is one-sided add/remove —
 * statements that failed both the masked-head bucket and the >=50% token
 * overlap in `diff-composition` step 3.
 *
 * PLANT CHECK FINDING (2026-08-11): a pure same-file rename CANNOT land in
 * one-sided at all — the statement hash is already rename-blind, so step 2
 * classifies it as NAMING. A same-file masked-equality probe is therefore
 * zero BY CONSTRUCTION and proves nothing. What CAN land here and still be
 * rename churn: a statement that moved files while renaming (hash matching
 * is per-file), or one that changed in some non-identifier way alongside the
 * rename. This probe matches masked shapes ACROSS files to size the first
 * class; the second has no predicate and needs a hand-read (rule 1).
 *
 * The predicate tests: "this removed and this added statement become
 * byte-identical when every identifier token is replaced by a placeholder".
 * It does NOT test that the difference is a rename. Conservative two ways:
 * a statement any of whose lines the tokenizer cannot handle is excluded,
 * and a differing string literal (e.g. the exp054 export-key drift) breaks
 * the match — string-keyed name churn stays invisible here.
 */
import {
  composeDiff,
  type NoiseSample
} from "../037-noise-source-decomposition/diff-composition.js";
import { tokenizeLine } from "../../src/rename/diff-reconcile.js";

const [PRIOR, FRESH, LABEL = ""] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: one-sided-ledger.ts <priorSrc> <freshSrc> [label]");
  process.exit(1);
}

/** Masked rendering, or null when any line refuses to tokenize. */
function maskIdents(text: string): string | null {
  const out: string[] = [];
  for (const line of text.split("\n")) {
    const toks = tokenizeLine(line);
    if (!toks) return null;
    out.push(toks.map((t) => (t.kind === "ident" ? "§" : t.text)).join(""));
  }
  return out.join("\n");
}

const samples: NoiseSample[] = [];
composeDiff(PRIOR, FRESH, { samples, cap: 500_000 });

interface Side {
  file: string;
  lines: number;
  masked: string;
}
const removedByKey = new Map<string, Side[]>();
const added: Side[] = [];
let removedLn = 0;
let addedLn = 0;
let untokenizableLn = 0;

for (const s of samples.filter((x) => x.kind === "real")) {
  const oneSided = (s.priorText === undefined) !== (s.freshText === undefined);
  if (!oneSided) continue;
  const text = s.priorText ?? s.freshText ?? "";
  const masked = maskIdents(text);
  if (masked === null) {
    untokenizableLn += s.lines;
    continue;
  }
  const side: Side = { file: s.file, lines: s.lines, masked };
  if (s.priorText !== undefined) {
    removedLn += s.lines;
    const l = removedByKey.get(masked) ?? [];
    l.push(side);
    removedByKey.set(masked, l);
  } else {
    addedLn += s.lines;
    added.push(side);
  }
}

let matchedLn = 0;
let matchedPairs = 0;
let crossFilePairs = 0;
let anchoredLn = 0;
let bareLn = 0;
const byShape = new Map<string, number>();
const examples: string[] = [];
for (const a of added) {
  const bucket = removedByKey.get(a.masked);
  if (!bucket || bucket.length === 0) continue;
  // Prefer a same-file twin; fall back to any file (a moved statement).
  const i = bucket.findIndex((r) => r.file === a.file);
  const r = i >= 0 ? bucket.splice(i, 1)[0] : bucket.shift()!;
  if (r.file !== a.file) crossFilePairs++;
  matchedPairs++;
  const ln = a.lines + r.lines;
  matchedLn += ln;
  // An anchored shape carries a string literal the mask preserves (e.g. a
  // require path) — near-certain identity. A bare shape could pair any two
  // statements that merely look alike; its mass is an upper bound only.
  if (/["'`]/.test(a.masked)) anchoredLn += ln;
  else bareLn += ln;
  byShape.set(a.masked, (byShape.get(a.masked) ?? 0) + ln);
  if (examples.length < 10 && a.masked.length < 200) {
    examples.push(`${r.file} -> ${a.file}: ${a.masked.slice(0, 130)}`);
  }
}

const pad = (n: number, w = 9) => String(n).padStart(w);
const pct = (n: number, d: number) =>
  d ? `${((100 * n) / d).toFixed(1)}%` : "n/a";
const oneSidedTotal = removedLn + addedLn;

console.log(
  `=== 055 ONE-SIDED LEDGER — ${LABEL || `${PRIOR} -> ${FRESH}`} ===`
);
console.log(`  one-sided REAL lines            ${pad(oneSidedTotal)}`);
console.log(
  `    MASKED-TWIN (hidden rename)   ${pad(matchedLn)}  ${pct(matchedLn, oneSidedTotal)}  (${matchedPairs} pairs, ${crossFilePairs} cross-file)`
);
console.log(
  `    unmatched (genuine add/rm)    ${pad(oneSidedTotal - matchedLn)}  ${pct(oneSidedTotal - matchedLn, oneSidedTotal)}`
);
console.log(`    untokenizable (excluded)      ${pad(untokenizableLn)}`);
console.log(
  `ROW|${LABEL}|onesided|${oneSidedTotal}|${matchedLn}|${matchedPairs}|${crossFilePairs}|${untokenizableLn}`
);
console.log(`\n  masked shapes of matched pairs — first 10:`);
for (const e of examples) console.log(`    ${e}`);

console.log(`\n  composition:`);
console.log(`    string-anchored shapes        ${pad(anchoredLn)}`);
console.log(`    bare shapes (upper bound)     ${pad(bareLn)}`);
console.log(`ROW|${LABEL}|composition|${anchoredLn}|${bareLn}`);
const top = [...byShape.entries()].sort((x, y) => y[1] - x[1]).slice(0, 5);
for (const [shape, ln] of top)
  console.log(
    `    ${pad(ln, 6)} ln  ${shape.replace(/\n/g, "\\n").slice(0, 110)}`
  );
