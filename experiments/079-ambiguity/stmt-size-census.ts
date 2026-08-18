/**
 * 079 — how big are the enclosing statements the address rung depends on?
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/079-ambiguity/stmt-size-census.ts <humanified.js>
 *
 * The rung refuses any enclosing statement spanning more than
 * MAX_ENCLOSING_STMT_LINES (50). That number arrived with the feature in
 * July with a stated rationale and NO measurement behind it, and the hop
 * counted 3,736 functions falling out of the rung for "no usable
 * statement" — a bucket that mixes the cap with the harmless
 * function-IS-the-statement case.
 *
 * This splits that bucket and prints the span distribution, so the cap
 * becomes a measured trade rather than a round number. Reads the SAME
 * artifact production reads on the old side (the prior release's
 * humanified bundle), formatted by the same generator, so the line spans
 * are the ones the rung actually sees.
 *
 * CROWDING is the column that matters. The rung exists for functions that
 * share a statement with an identical sibling; a cap that mostly excludes
 * lone functions costs nothing, and one that excludes crowded ones costs
 * exactly the population the rung is for.
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import type * as t from "@babel/types";
import { buildFunctionGraph } from "../../src/analysis/function-graph.js";

const [BUNDLE] = process.argv.slice(2);
if (!BUNDLE) {
  console.error("usage: stmt-size-census.ts <humanified.js>");
  process.exit(1);
}

const ast = parseSync(fs.readFileSync(BUNDLE, "utf8"), {
  sourceType: "unambiguous",
  configFile: false,
  babelrc: false
}) as t.File;
if (!ast) throw new Error("parse failed");

const fns = buildFunctionGraph(ast, BUNDLE);

let isOwnStatement = 0;
let noStatement = 0;
let noLoc = 0;
const spans: number[] = [];
/** span per function, and how many functions share that same statement node */
const perFn: { span: number; node: t.Node }[] = [];
const holders = new Map<t.Node, number>();

for (const fn of fns) {
  const stmt = fn.path.getStatementParent();
  if (!stmt) {
    noStatement++;
    continue;
  }
  if (stmt.node === fn.path.node) {
    isOwnStatement++;
    continue;
  }
  const loc = stmt.node.loc;
  if (!loc) {
    noLoc++;
    continue;
  }
  const span = loc.end.line - loc.start.line + 1;
  spans.push(span);
  perFn.push({ span, node: stmt.node });
  holders.set(stmt.node, (holders.get(stmt.node) ?? 0) + 1);
}

const total = fns.length;
console.log(`functions in ${BUNDLE}: ${total}\n`);
console.log(
  `  function IS its own statement (rung inapplicable): ${isOwnStatement}`
);
console.log(
  `  no statement parent at all:                        ${noStatement}`
);
console.log(`  statement with no source position:                 ${noLoc}`);
console.log(
  `  has a measurable enclosing statement:              ${spans.length}\n`
);

spans.sort((a, b) => a - b);
const pct = (p: number) =>
  spans[Math.min(spans.length - 1, Math.floor((p / 100) * spans.length))];
console.log("enclosing-statement span, lines:");
for (const p of [50, 75, 90, 95, 99, 100]) {
  console.log(`  p${String(p).padStart(3)}  ${String(pct(p)).padStart(6)}`);
}
console.log(
  `  mean  ${(spans.reduce((a, b) => a + b, 0) / spans.length).toFixed(1).padStart(6)}\n`
);

console.log(
  `${"cap".padStart(6)}${"excluded".padStart(10)}${"% of all".padStart(10)}` +
    `${"CROWDED excl".padStart(14)}${"% of crowded".padStart(14)}`
);
const crowdedTotal = perFn.filter((f) => (holders.get(f.node) ?? 0) > 1).length;
for (const cap of [25, 50, 75, 100, 200, 500, Number.POSITIVE_INFINITY]) {
  const over = perFn.filter((f) => f.span > cap);
  const crowdedOver = over.filter((f) => (holders.get(f.node) ?? 0) > 1).length;
  console.log(
    `${(cap === Number.POSITIVE_INFINITY ? "none" : String(cap)).padStart(6)}` +
      `${String(over.length).padStart(10)}` +
      `${((100 * over.length) / perFn.length).toFixed(1).padStart(9)}%` +
      `${String(crowdedOver).padStart(14)}` +
      `${((100 * crowdedOver) / crowdedTotal).toFixed(1).padStart(13)}%`
  );
}
console.log(
  `\ncrowded = shares its enclosing statement with >=1 other function ` +
    `(${crowdedTotal} of ${perFn.length}).\nThose are the ONLY functions the rung ` +
    `can help; a cap is only costly where it excludes them.`
);
