/**
 * 052 — FAILED INSTRUMENT, kept as a warning. Use `reroll-rate.ts` instead.
 *
 * This keys on a NAME SPELLING, and that hole swallows the measurement: the
 * spellings the LLM proposed somewhere in a run (`error`, `options`, `length`,
 * `cache`) also sit on thousands of mechanically pinned locals, every one of
 * which agrees with the prior by construction. It reported 99.6% agreement on
 * 85->86; `--strict` moved it to 71.6% and the calm hops to 95-99%, which is
 * merely a different amount of wrong. The caveat below predicted the direction
 * and understated the size — measurement-pitfalls rule 3, one more time.
 *
 * ORIGINAL INTENT, unchanged below: how much cross-version noise is hidden
 * because the LLM happened to re-pick the prior release's name.
 *
 *   npx tsx experiments/052-llm-agreement/silent-agreement.ts \
 *     <priorSrc> <freshSrc> <freshDiag.json> [label]
 *
 * The diff only shows names the LLM got WRONG. A binding the pipeline could not
 * pin mechanically is a coin flip: the LLM re-derives a name, and when that name
 * happens to equal the one last release used, the line does not appear in the
 * diff at all. Those are wins we did not earn and cannot count on — and the
 * measured re-roll disagreement rate (two cold runs of the SAME input) says how
 * fragile they are.
 *
 * The predicate, in one sentence: inside a statement whose `statementHash`
 * occurs exactly ONCE on each side — so the two are the same code modulo names,
 * and identifier k on one side is identifier k on the other — count the
 * positions whose FRESH spelling is one the run's diagnostics record as an
 * LLM-proposed name, split by whether the prior spelling was the same.
 *
 * What it does NOT test, stated because it is the way to be wrong here: a
 * spelling, not an occurrence, is matched against the LLM set. A mechanically
 * pinned binding that happens to share a spelling with some LLM-named binding
 * elsewhere counts as LLM-decided. That inflates AGREEMENT specifically (a
 * pinned name agrees by construction), so the agreement figure is an UPPER
 * bound. `--strict` drops every spelling that any mechanical tier also produced,
 * giving a lower bound; report both.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { statementsOf } from "../037-noise-source-decomposition/diff-composition.js";

const args = process.argv.slice(2).filter((a) => a !== "--strict");
const STRICT = process.argv.includes("--strict");
const [PRIOR, FRESH, DIAG, LABEL = ""] = args;
if (!PRIOR || !FRESH || !DIAG) {
  console.error(
    "usage: silent-agreement.ts <priorSrc> <freshSrc> <freshDiag.json> [label] [--strict]"
  );
  process.exit(1);
}

interface Diag {
  renamed: { newName: string; strategy: string }[];
  strategyTrails: {
    trails: { trail: { newName?: string }[]; terminalBy?: string }[];
  };
}
const diag = JSON.parse(fs.readFileSync(DIAG, "utf8")) as Diag;

/** Spellings the LLM proposed in this run. */
const llmNames = new Set<string>();
for (const r of diag.renamed) {
  if (r.strategy === "llm") llmNames.add(r.newName);
}
/** Spellings some MECHANICAL tier also produced — the contamination set. */
const mechanicalNames = new Set<string>();
for (const t of diag.strategyTrails.trails) {
  for (const step of t.trail ?? []) {
    if (step.newName) mechanicalNames.add(step.newName);
  }
}
let shared = 0;
for (const n of llmNames) if (mechanicalNames.has(n)) shared++;
const isLlm = (name: string) =>
  llmNames.has(name) && (!STRICT || !mechanicalNames.has(name));

function walk(dir: string, base = dir, out: string[] = []): string[] {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out);
    else if (e.name.endsWith(".js")) out.push(path.relative(base, p));
  }
  return out;
}

const TOKEN = /[A-Za-z_$][\w$]*/g;

let agreed = 0;
let drifted = 0;
let mechAgreed = 0;
let mechDrifted = 0;
let twinPairs = 0;
let skippedUnaligned = 0;
const agreedNames = new Map<string, number>();
const driftedPairs = new Map<string, number>();

const freshFiles = walk(FRESH);
const priorSet = new Set(walk(PRIOR));

for (const f of freshFiles) {
  if (!priorSet.has(f)) continue;
  const priorStmts = statementsOf(fs.readFileSync(path.join(PRIOR, f), "utf8"));
  const freshStmts = statementsOf(fs.readFileSync(path.join(FRESH, f), "utf8"));
  // Same uniqueness gate every other tier uses: exactly one occurrence per side.
  const pc = new Map<string, number>();
  for (const s of priorStmts) pc.set(s.hash, (pc.get(s.hash) ?? 0) + 1);
  const fc = new Map<string, number>();
  for (const s of freshStmts) fc.set(s.hash, (fc.get(s.hash) ?? 0) + 1);
  const priorByHash = new Map(priorStmts.map((s) => [s.hash, s]));

  for (const s of freshStmts) {
    if (fc.get(s.hash) !== 1 || pc.get(s.hash) !== 1) continue;
    const twin = priorByHash.get(s.hash);
    if (!twin) continue;
    const ta = twin.text.match(TOKEN) ?? [];
    const tb = s.text.match(TOKEN) ?? [];
    if (ta.length !== tb.length) {
      skippedUnaligned++;
      continue;
    }
    twinPairs++;
    for (let i = 0; i < tb.length; i++) {
      const same = ta[i] === tb[i];
      if (isLlm(tb[i])) {
        if (same) {
          agreed++;
          agreedNames.set(tb[i], (agreedNames.get(tb[i]) ?? 0) + 1);
        } else {
          drifted++;
          const k = `${ta[i]} -> ${tb[i]}`;
          driftedPairs.set(k, (driftedPairs.get(k) ?? 0) + 1);
        }
      } else if (same) mechAgreed++;
      else mechDrifted++;
    }
  }
}

const pad = (n: number, w = 8) => String(n).padStart(w);
const pct = (n: number, d: number) =>
  d ? `${((100 * n) / d).toFixed(1)}%`.padStart(7) : "    n/a";

console.log(
  `=== SILENT AGREEMENT — ${LABEL || `${PRIOR} -> ${FRESH}`}${STRICT ? " [strict]" : ""} ===`
);
console.log(
  `  statement twins compared: ${twinPairs}` +
    ` (skipped, token counts differ: ${skippedUnaligned})`
);
console.log(
  `  distinct LLM-proposed spellings: ${llmNames.size}` +
    `, also produced by a mechanical tier: ${shared}`
);
console.log(`\n  identifier occurrences inside twin statements:`);
console.log(
  `    LLM-spelled, SAME as prior   ${pad(agreed)} ${pct(agreed, agreed + drifted)}  <- hidden noise`
);
console.log(
  `    LLM-spelled, DIFFERENT       ${pad(drifted)} ${pct(drifted, agreed + drifted)}  <- the noise we see`
);
console.log(
  `    mechanically pinned, same    ${pad(mechAgreed)}\n` +
    `    mechanically pinned, differ  ${pad(mechDrifted)}`
);
console.log(
  `ROW|${LABEL}|${STRICT ? "strict" : "loose"}|${agreed}|${drifted}|${mechAgreed}|${mechDrifted}|${twinPairs}`
);

console.log(`\n  most-repeated silent agreements (name, occurrences):`);
for (const [n, c] of [...agreedNames.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 8)) {
  console.log(`    ${pad(c, 6)}  ${n}`);
}
console.log(`\n  most-repeated visible drifts:`);
for (const [n, c] of [...driftedPairs.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 8)) {
  console.log(`    ${pad(c, 6)}  ${n}`);
}
