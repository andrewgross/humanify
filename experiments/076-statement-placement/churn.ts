/**
 * 076 — how many statements CHANGE FILE between two consecutive fossil
 * trees, and why.
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/076-statement-placement/churn.ts <priorBundle> <freshBundle>
 *
 * Reproduces exp074's headline cost (567 statements moved file, against 1
 * for the pre-fossil layout) offline, on raw minified bundles, with no
 * pipeline run and no LLM — so a placement change can be sized in a minute
 * instead of an hour.
 *
 * WHY IT IS NOT MEASURED THE OBVIOUS WAY. Pairing statements by hash is
 * what exp074 tried and had to retract: statement hashes are not unique, so
 * a hash→file map keeps only the first occurrence and the tallies exceed
 * the total. And pairing modules with the PRODUCTION matcher would report
 * zero by construction — a matched module inherits its path verbatim, so it
 * cannot move. The movement lives exactly in the gap between the two: pairs
 * that ARE the same module by content but that the production matcher
 * declined, which therefore mint a fresh path on each side independently.
 *
 * So identity here is deliberately INDEPENDENT of the production matcher:
 * mutual-best content overlap ≥ 0.7. That is ground truth for this question
 * and nothing in the pipeline consumes it.
 *
 * Bounds, stated: minified input means stems are junk, so the production
 * matcher runs without its stem tier and the unmatched population is an
 * OVERCOUNT of the real one. Statement counts stand in for line counts.
 * Direction and relative size, not absolute magnitude.
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { extractFossilModules } from "../../src/split/fossil-map.js";
import { assignFossil } from "../../src/split/fossil-assign.js";
import {
  statementHash,
  STATEMENT_HASH_VERSION
} from "../../src/split/statement-hash.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const [PRIOR, FRESH, LABEL = "run", FLOOR = "0.3"] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: churn.ts <priorBundle> <freshBundle> [label] [floor]");
  process.exit(1);
}
/**
 * Overlap a pair must clear to count as "the same module" for THIS
 * measurement. Deliberately BELOW the production matcher's floors, and
 * that is the whole point: a pair the matcher accepts inherits its path
 * and cannot move, so measuring at the matcher's own floor reports ~0 by
 * construction (it did, at 0.7 — 1 moved module out of 2,423). The
 * population that churns is the one the matcher DECLINES, and exp074's two
 * worst cases sit at 0.42 and 0.33 while being, by name and by process of
 * elimination, plainly the same file.
 */
const GROUND_TRUTH_FLOOR = Number(FLOOR);

function load(file: string, label: string) {
  const code = fs.readFileSync(file, "utf8");
  const ast = parseFileAst(code);
  if (!ast) throw new Error(`${label}: parse failed`);
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error(`${label}: no wrapper IIFE`);
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error(`${label}: no block`);
  const body = bodyNode.body;
  const hashes = body.map(statementHash);
  return { body, hashes, extract: extractFossilModules(body, hashes) };
}

const p = load(PRIOR, "prior");
const f = load(FRESH, "fresh");

// Release N: nothing to inherit. Release N+1: inherit through the ledger,
// exactly as the pipeline does.
const out85 = assignFossil(p.body, p.hashes, undefined);
const ledger: StableSplitLedger = {
  version: 1,
  files: [],
  nameToFiles: {},
  order: [],
  hashVersion: STATEMENT_HASH_VERSION,
  fossilModules: out85.fossilModules
};
const out86 = assignFossil(f.body, f.hashes, ledger);

// --- ground-truth identity: mutual-best content overlap, matcher-blind ---
function counts(hashes: string[]): Map<string, number> {
  const c = new Map<string, number>();
  for (const h of hashes) c.set(h, (c.get(h) ?? 0) + 1);
  return c;
}
const priorCounts = p.extract.modules.map((m) => counts(m.hashes));
const freshCounts = f.extract.modules.map((m) => counts(m.hashes));
function overlapAt(pi: number, fi: number): number {
  const a = priorCounts[pi];
  const b = freshCounts[fi];
  let inter = 0;
  for (const [h, n] of a) inter += Math.min(n, b.get(h) ?? 0);
  const union =
    p.extract.modules[pi].hashes.length +
    f.extract.modules[fi].hashes.length -
    inter;
  return union === 0 ? 0 : inter / union;
}
// Candidate pairs come from shared hashes only — an all-pairs sweep over
// 3,261 x 3,273 modules would be 10.7M multiset comparisons.
const priorByHash = new Map<string, number[]>();
p.extract.modules.forEach((m, i) => {
  for (const h of new Set(m.hashes)) {
    const list = priorByHash.get(h) ?? [];
    list.push(i);
    priorByHash.set(h, list);
  }
});
const bestForFresh = new Map<number, { pi: number; ov: number }>();
f.extract.modules.forEach((m, fi) => {
  const seen = new Set<number>();
  let best: { pi: number; ov: number } | undefined;
  for (const h of new Set(m.hashes)) {
    for (const pi of priorByHash.get(h) ?? []) {
      if (seen.has(pi)) continue;
      seen.add(pi);
      const ov = overlapAt(pi, fi);
      if (ov >= GROUND_TRUTH_FLOOR && (!best || ov > best.ov))
        best = { pi, ov };
    }
  }
  if (best) bestForFresh.set(fi, best);
});
// mutual best: a prior module claimed by two fresh modules pairs with neither
const claims = new Map<number, number[]>();
for (const [fi, b] of bestForFresh) {
  const list = claims.get(b.pi) ?? [];
  list.push(fi);
  claims.set(b.pi, list);
}
const pairs: Array<{ pi: number; fi: number }> = [];
for (const [pi, fis] of claims) {
  if (fis.length === 1) pairs.push({ pi, fi: fis[0] });
}

const folderOf = (file: string) => {
  const cut = file.lastIndexOf("/");
  return cut <= 0 ? "src" : file.slice(0, cut);
};
const nameOf = (file: string) => file.slice(file.lastIndexOf("/") + 1);

let held = 0;
let heldStmts = 0;
let moved = 0;
let movedStmts = 0;
let folderOnly = 0;
let folderOnlyStmts = 0;
let renameOnly = 0;
let renameOnlyStmts = 0;
let bothChanged = 0;
let bothChangedStmts = 0;
const examples: Array<[number, string, string]> = [];
for (const { pi, fi } of pairs) {
  const a = out85.fossilModules[pi].file;
  const b = out86.fossilModules[fi].file;
  const n = f.extract.modules[fi].statements.length;
  if (a === b) {
    held++;
    heldStmts += n;
    continue;
  }
  moved++;
  movedStmts += n;
  const sameFolder = folderOf(a) === folderOf(b);
  const sameName = nameOf(a) === nameOf(b);
  if (sameName && !sameFolder) {
    folderOnly++;
    folderOnlyStmts += n;
  } else if (sameFolder && !sameName) {
    renameOnly++;
    renameOnlyStmts += n;
  } else {
    bothChanged++;
    bothChangedStmts += n;
  }
  examples.push([n, a, b]);
}
examples.sort((x, y) => y[0] - x[0]);

const pct = (n: number, d: number) =>
  d === 0 ? "0.0" : ((100 * n) / d).toFixed(1);
console.log(`\n=== ${LABEL}: statements that CHANGE FILE across the hop ===`);
console.log(
  `ground-truth module pairs (overlap >= ${GROUND_TRUTH_FLOOR}, mutual best): ${pairs.length}`
);
console.log(`  held same file   ${held} modules, ${heldStmts} statements`);
console.log(
  `  MOVED            ${moved} modules, ${movedStmts} statements ` +
    `(${pct(movedStmts, heldStmts + movedStmts)}% of paired mass)`
);
console.log(`\nby kind:`);
console.log(
  `  folder churn (same file name)  ${folderOnly} modules, ${folderOnlyStmts} statements`
);
console.log(
  `  file rename (same folder)      ${renameOnly} modules, ${renameOnlyStmts} statements`
);
console.log(
  `  both changed                   ${bothChanged} modules, ${bothChangedStmts} statements`
);
console.log(`\nlargest moves:`);
for (const [n, a, b] of examples.slice(0, 12)) {
  console.log(`  ${String(n).padStart(4)} stmts  ${a}  ->  ${b}`);
}
/**
 * FOLDER-ONLY view — the one this instrument can actually resolve.
 *
 * On MINIFIED input a fresh module's file name is a kebab of a minified
 * identifier, and the minifier re-rolls those every release, so the
 * name half of every fresh path churns unconditionally here. That is an
 * artifact of the input, not of the pipeline: in production the stem comes
 * from a humanified name. Measuring "moved file" therefore reads ~100%
 * name-churn and cannot resolve a folder change at all (rule 11 — the gate
 * cannot see an effect under its own noise). Folders are derived from the
 * graph, not from names, so they ARE comparable on minified input.
 */
let folderHeld = 0;
let folderHeldStmts = 0;
let folderMoved = 0;
let folderMovedStmts = 0;
for (const { pi, fi } of pairs) {
  const n = f.extract.modules[fi].statements.length;
  if (
    folderOf(out85.fossilModules[pi].file) ===
    folderOf(out86.fossilModules[fi].file)
  ) {
    folderHeld++;
    folderHeldStmts += n;
  } else {
    folderMoved++;
    folderMovedStmts += n;
  }
}
console.log(`\nFOLDER-only (name churn excluded — see note in source):`);
console.log(
  `  folder held      ${folderHeld} modules, ${folderHeldStmts} statements`
);
console.log(
  `  folder CHANGED   ${folderMoved} modules, ${folderMovedStmts} statements ` +
    `(${pct(folderMovedStmts, folderHeldStmts + folderMovedStmts)}% of paired mass)`
);
console.log(
  `\nplacement signals (fresh side): ${JSON.stringify(out86.stats.signals)}`
);
console.log(
  `inherited ${out86.stats.inheritedFiles} / fresh-named ${out86.stats.freshNamedFiles}`
);
