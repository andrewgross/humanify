/**
 * 076 Task 0 — how stable is fossil PLACEMENT across two releases?
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/076-statement-placement/stability.ts <priorBundle> <freshBundle>
 *
 * exp074 measured the cost (567 statements changed file between two fossil
 * trees where the old layout moved 1; 4,540 unique lines moved) and
 * attributed the top cases to FOLDER churn and FILE RENAMES on modules that
 * did not match. This sizes the mechanism BEFORE any code change (rule 11):
 *
 *   1. match rate — how much of the tree inherits its path verbatim
 *   2. for MATCHED pairs, is the inferred folder structurally the same?
 *      Compared by folder MEMBERSHIP (mapped through the match), never by
 *      folder NAME: names are derived from stems, and on a minified input
 *      stems are meaningless. A name-based comparison here would measure
 *      the minifier, not the layout.
 *   3. for UNMATCHED fresh modules — the population that mints a path and
 *      therefore can churn — how many have at least one MATCHED importer?
 *      That is the reach of "anchor a fresh module to the settled paths of
 *      its neighbours" and the number that decides whether to build it.
 *
 * Runs on RAW MINIFIED bundles, so it needs no pipeline run and no LLM.
 * Two consequences, stated because they bound what this can prove:
 *   - stems are minified junk, so the matcher runs WITHOUT them (tiers A+B).
 *     Production also has tier C, so the real match rate is HIGHER than the
 *     one printed here and the unmatched population is an OVERCOUNT.
 *   - statement counts stand in for line counts. Direction, not magnitude.
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { extractFossilModules } from "../../src/split/fossil-map.js";
import { inferFossilPlacements } from "../../src/split/fossil-assign.js";
import { matchFossilModules } from "../../src/split/fossil-match.js";
import { statementHash } from "../../src/split/statement-hash.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: stability.ts <priorBundle> <freshBundle>");
  process.exit(1);
}

interface Side {
  label: string;
  modules: ReturnType<typeof extractFossilModules>["modules"];
  eager: number[];
  placements: ReturnType<typeof inferFossilPlacements>;
}

function read(file: string, label: string): Side {
  const code = fs.readFileSync(file, "utf8");
  const ast = parseFileAst(code);
  if (!ast) throw new Error(`${label}: parse failed`);
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error(`${label}: no wrapper IIFE`);
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error(`${label}: no block`);
  const body = bodyNode.body;
  const hashes = body.map(statementHash);
  const extract = extractFossilModules(body, hashes);
  const placements = inferFossilPlacements(extract, body);
  console.error(
    `${label}: ${body.length} statements, ${extract.modules.length} modules, ` +
      `${extract.eagerZone.length} eager`
  );
  return {
    label,
    modules: extract.modules,
    eager: extract.eagerZone,
    placements
  };
}

const prior = read(PRIOR, "prior");
const fresh = read(FRESH, "fresh");

const { matches, tiers } = matchFossilModules(
  prior.modules.map((m) => ({ hashes: m.hashes, imports: m.imports })),
  fresh.modules.map((m) => ({ hashes: m.hashes, imports: m.imports }))
);
const priorToFresh = new Map<number, number>();
for (const [f, p] of matches) priorToFresh.set(p, f);

const stmtsOf = (side: Side, i: number) => side.modules[i].statements.length;
const totalFreshStmts = fresh.modules.reduce(
  (n, m) => n + m.statements.length,
  0
);

console.log("=== 1. match rate (tiers A+B only; production also has C) ===");
console.log(`prior modules      ${prior.modules.length}`);
console.log(`fresh modules      ${fresh.modules.length}`);
console.log(
  `matched            ${matches.size} ` +
    `(${((100 * matches.size) / fresh.modules.length).toFixed(1)}% of fresh)`
);
for (const [tier, n] of Object.entries(tiers)) console.log(`  ${tier}: ${n}`);

// --- 2. structural stability of the inferred folders, by MEMBERSHIP ---
function folderMembers(side: Side): Map<string, number[]> {
  const byFolder = new Map<string, number[]>();
  side.placements.forEach((p, i) => {
    const list = byFolder.get(p.folder) ?? [];
    list.push(i);
    byFolder.set(p.folder, list);
  });
  return byFolder;
}
const priorFolders = folderMembers(prior);
const freshFolders = folderMembers(fresh);
const folderOfPrior = prior.placements.map((p) => p.folder);
const folderOfFresh = fresh.placements.map((p) => p.folder);

// EXACT set equality is the wrong lens on a folder with ~1,000 members:
// one module joining the flat root would mark every one of them "moved".
// Three lenses instead, each answering a question exact equality conflates.
let sameSignal = 0;
let rootToFolder = 0;
let folderToRoot = 0;
let bothRoot = 0;
let bothFoldered = 0;
const jaccards: number[] = [];
for (const [f, p] of matches) {
  if (prior.placements[p].signal === fresh.placements[f].signal) sameSignal++;
  const wasRoot = folderOfPrior[p] === "src";
  const isRoot = folderOfFresh[f] === "src";
  if (wasRoot && !isRoot) rootToFolder++;
  else if (!wasRoot && isRoot) folderToRoot++;
  else if (wasRoot) bothRoot++;
  else bothFoldered++;
  const priorMates = new Set(
    (priorFolders.get(folderOfPrior[p]) ?? [])
      .map((x) => priorToFresh.get(x))
      .filter((x): x is number => x !== undefined)
  );
  const freshMates = new Set(freshFolders.get(folderOfFresh[f]) ?? []);
  let inter = 0;
  for (const x of priorMates) if (freshMates.has(x)) inter++;
  const union = priorMates.size + freshMates.size - inter;
  jaccards.push(union === 0 ? 1 : inter / union);
}
const identicalMates = jaccards.filter((j) => j > 0.999).length;
const nearMates = jaccards.filter((j) => j >= 0.9).length;
const scatteredMates = jaccards.filter((j) => j < 0.5).length;
console.log("\n=== 2. inferred-folder stability for MATCHED modules ===");
console.log("(mapped through the match, so folder NAMES never enter it)");
console.log(`same placement signal    ${sameSignal} / ${matches.size}`);
console.log(
  `root -> folder           ${rootToFolder}   folder -> root  ${folderToRoot}`
);
console.log(`stayed root ${bothRoot}   stayed foldered ${bothFoldered}`);
console.log(
  `folder-mates identical   ${identicalMates}   >=0.9 same  ${nearMates}` +
    `   <0.5 (scattered) ${scatteredMates}`
);
console.log(
  "NOTE: matched modules INHERIT their path in production, so this is not\n" +
    "a churn number — it is how reliable the inference would be if trusted."
);

// --- 3. reach of "anchor a fresh module to its settled neighbours" ---
const freshImporters = new Map<number, Set<number>>();
fresh.modules.forEach((m, i) => {
  for (const imp of m.imports) {
    if (imp === i) continue;
    const set = freshImporters.get(imp) ?? new Set<number>();
    set.add(i);
    freshImporters.set(imp, set);
  }
});

let unmatched = 0;
let unmatchedStmts = 0;
let withMatchedImporter = 0;
let withMatchedImporterStmts = 0;
let majorityMatchedImporter = 0;
let noImporters = 0;
let onlyUnmatchedImporters = 0;
for (let i = 0; i < fresh.modules.length; i++) {
  if (matches.has(i)) continue;
  unmatched++;
  unmatchedStmts += stmtsOf(fresh, i);
  const ups = [...(freshImporters.get(i) ?? [])];
  if (ups.length === 0) {
    noImporters++;
    continue;
  }
  const matchedUps = ups.filter((u) => matches.has(u));
  if (matchedUps.length === 0) {
    onlyUnmatchedImporters++;
    continue;
  }
  withMatchedImporter++;
  withMatchedImporterStmts += stmtsOf(fresh, i);
  if (matchedUps.length / ups.length >= 0.5) majorityMatchedImporter++;
}
console.log(
  "\n=== 3. fresh (unmatched) modules — the population that churns ==="
);
console.log(
  `unmatched          ${unmatched} modules, ${unmatchedStmts} statements ` +
    `(${((100 * unmatchedStmts) / totalFreshStmts).toFixed(1)}% of statement mass)`
);
console.log(
  `  >=1 MATCHED importer   ${withMatchedImporter} ` +
    `(${withMatchedImporterStmts} statements) <- anchorable to a settled path`
);
console.log(`  majority matched       ${majorityMatchedImporter}`);
console.log(`  importers all unmatched ${onlyUnmatchedImporters}`);
console.log(`  no importers at all     ${noImporters}`);
console.log(
  `\nREACH: ${((100 * withMatchedImporter) / Math.max(1, unmatched)).toFixed(1)}%` +
    ` of the churning population has a settled neighbour to anchor to.`
);
