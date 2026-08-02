/**
 * At-scale check on the widened placement trail — no LLM, no pipeline run.
 *
 * **Superseded by `058/trail-dump.ts` for anything that reads a tier count.**
 * The carry built below leaves `matchMap` EMPTY, which switches the `preempt`
 * and `fill` tiers off: this script reproduces the shipped run's placements
 * everywhere except the 0–8 statements those tiers decide per hop. exp058
 * measured the gap (shipped 6/0/1/2 preempt and 2/0/0/0 fill on the four gate
 * hops) and reads the real map off `prior-match-map.json` instead.
 *
 *   npx tsx experiments/057-alias-stability/trail-check.ts <freshBundle> <priorLedger>
 *
 * The split stage takes a bundle plus the prior release's ledger and needs
 * nothing else, so the trail can be exercised on the real 2.1.215→216 pair in
 * a minute rather than the ~1h a gated run costs. What it reports:
 *
 *  - COVERAGE: entries vs statements placed. The old trail described 1,192 of
 *    35,903 (3.3%) and held zero for the `hash` tier.
 *  - WHY A STATEMENT MOVED: the `hashMiss` breakdown over moved statements —
 *    hash placement cannot move one, so every move implies a specific miss.
 *  - SIZE: the serialized cost, which is what the old cap was protecting.
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { topLevelStatements } from "../../src/prior-version/statement-twin.js";
import { emptyPriorCarry } from "../../src/split/prior-carry.js";
import { placementTrail } from "../../src/split/placement-trail.js";
import { stableSplitFromCode } from "../../src/split/stable-split.js";

const [BUNDLE, LEDGER, PRIOR_BUNDLE] = process.argv.slice(2);
if (!BUNDLE || !LEDGER) {
  console.error(
    "usage: trail-check.ts <freshBundle> <priorLedger> [priorBundle]"
  );
  process.exit(1);
}

const code = fs.readFileSync(BUNDLE, "utf8");
const prior = JSON.parse(fs.readFileSync(LEDGER, "utf8"));

/**
 * The content-anchor tier — the only identity signal that survives BOTH a
 * content edit and a rename, and therefore the one that detects a move — needs
 * the prior release's statement texts. The pipeline reads them off the live
 * prior AST; rebuild them here the same way (`topLevelStatements`, the real
 * function) so the check exercises the tier rather than a stub. Without this
 * the trail can only report moves the matchMap catches, and reads 0.
 */
function priorStatementTexts(path: string): readonly string[] {
  const priorCode = fs.readFileSync(path, "utf8");
  const ast = parseSync(priorCode, {
    sourceType: "unambiguous",
    configFile: false,
    babelrc: false
  });
  if (!ast) throw new Error(`could not parse ${path}`);
  const graph = buildUnifiedGraph(ast, path);
  return topLevelStatements(graph).map((p) => {
    const n = p.node;
    return n.start != null && n.end != null
      ? priorCode.slice(n.start, n.end)
      : "";
  });
}

const carry = PRIOR_BUNDLE
  ? { ...emptyPriorCarry(), statementTexts: priorStatementTexts(PRIOR_BUNDLE) }
  : undefined;
console.log(
  carry
    ? `  anchor evidence: ${carry.statementTexts.length} prior statements`
    : "  anchor evidence: NONE (no prior bundle given — moves will read low)"
);

placementTrail.reset(true);
const result = await stableSplitFromCode(code, { prior, priorCarry: carry });
if (!result) throw new Error("split produced nothing");
const report = placementTrail.report();

const placed = Object.values(report.tiers).reduce((a, b) => a + b, 0);
const pct = (n: number, d: number) => `${((100 * n) / (d || 1)).toFixed(1)}%`;

console.log("\n=== COVERAGE ===");
console.log(`  statements placed : ${placed}`);
console.log(
  `  trail entries     : ${report.trails.length}  (${pct(report.trails.length, placed)})`
);
console.log("\n  by tier — entries / placed:");
const byTier = new Map<string, number>();
for (const t of report.trails)
  byTier.set(t.placedBy, (byTier.get(t.placedBy) ?? 0) + 1);
for (const [tier, n] of Object.entries(report.tiers).sort(
  (a, b) => b[1] - a[1]
)) {
  console.log(
    `    ${tier.padEnd(14)} ${String(byTier.get(tier) ?? 0).padStart(6)} / ${String(n).padStart(6)}`
  );
}

const known = report.trails.filter((t) => t.priorFile !== undefined);
const moved = known.filter((t) => t.priorFile !== t.file);
console.log("\n=== WHY A STATEMENT MOVED ===");
console.log(`  prior home known  : ${known.length} of ${report.trails.length}`);
console.log(
  `  MOVED             : ${moved.length}  (${pct(moved.length, known.length)} of those)`
);

const tally = (
  rows: typeof moved,
  key: (t: (typeof moved)[number]) => string
) => {
  const m = new Map<string, number>();
  for (const r of rows) m.set(key(r), (m.get(key(r)) ?? 0) + 1);
  return [...m.entries()].sort((a, b) => b[1] - a[1]);
};
console.log(
  "\n  hashMiss on moved statements (hash placement cannot move one):"
);
for (const [k, n] of tally(moved, (t) => t.hashMiss ?? "none")) {
  console.log(`    ${k.padEnd(16)} ${String(n).padStart(6)}`);
}
console.log("\n  which tier moved it, and on what evidence:");
for (const [k, n] of tally(
  moved,
  (t) => `${t.placedBy} (prior via ${t.priorFileFrom})`
)) {
  console.log(`    ${k.padEnd(34)} ${String(n).padStart(6)}`);
}
console.log("\n  a dissenting tier was available on:");
console.log(
  `    ${moved.filter((t) => t.alternatives).length} of ${moved.length} moved statements`
);

console.log("\n  first five moves, in full:");
for (const t of moved.slice(0, 5)) {
  console.log(
    `    [${t.index}] ${t.names.slice(0, 3).join(",")}\n       ${t.priorFile}\n    -> ${t.file}\n       placedBy=${t.placedBy} priorVia=${t.priorFileFrom} hashMiss=${t.hashMiss ?? "-"} alt=${JSON.stringify(t.alternatives ?? {})}`
  );
}

const DUMP = process.env.TRAIL_DUMP;
if (DUMP) {
  fs.writeFileSync(DUMP, JSON.stringify(report));
  console.log(`\n  wrote full trail to ${DUMP}`);
}

const bytes = Buffer.byteLength(JSON.stringify(report));
console.log("\n=== SIZE ===");
console.log(`  serialized trail  : ${(bytes / 1e6).toFixed(1)} MB`);
console.log(
  `  entries carrying vote arrays: ${report.trails.filter((t) => t.evidence.votes?.length).length}`
);
