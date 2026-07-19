/**
 * Faithful post-naming repro: hold the MAIN bundle's AST + graph LIVE (as the
 * pipeline does — renameResult.ast is alive through validate/reconcile/sweep),
 * then run the REAL validateOutput and runPriorDiffReconciliation on the
 * output, timing each. bench-postnaming.mts ran them on a fresh heap and they
 * were fast; this adds the one missing variable (live main AST) to find where
 * the 207->208 hang actually lives WITH the parse funnel in place.
 *
 *   NODE_OPTIONS=--max-old-space-size=14336 npx tsx bench-live-postnaming.mts
 */
import fs from "node:fs";
import type { GeneratorOptions } from "@babel/generator";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { NULL_PROFILER } from "../../src/profiling/index.js";
import {
  captureSemanticBaseline,
  validateOutput
} from "../../src/output-validation.js";
import { runPriorDiffReconciliation } from "../../src/rename/reconcile-step.js";

const V =
  process.env.VERSIONS_ROOT ??
  "/Users/andrewgross/Development/unpacked-claude-code-run-2026-07-17/versions";
const out208 = fs.readFileSync(
  `${V}/claude-code-2.1.208/.humanify/humanified.js`,
  "utf8"
);
const prior207 = fs.readFileSync(
  `${V}/claude-code-2.1.207/.humanify/humanified.js`,
  "utf8"
);
const genOpts: GeneratorOptions = { compact: false };

function time<T>(label: string, fn: () => T): T {
  const t0 = performance.now();
  const r = fn();
  const rss = Math.round(process.memoryUsage().rss / 1048576);
  console.log(`  ${label}: ${Math.round(performance.now() - t0)}ms  rss ${rss}MB`);
  return r;
}

console.log(`out ${(out208.length / 1e6).toFixed(1)}MB, prior ${(prior207.length / 1e6).toFixed(1)}MB`);

// --free-graph drops the graph before the post-naming passes (it is dead
// after the rename pass); only the AST stays live (generate/ledger/census
// need it). Tests whether shedding the graph's live set rescues validate/
// reconcile from the GC-pressure slowdown.
const freeGraph = process.argv.includes("--free-graph");

// Naming-era stand-in: parse the bundle and build its graph, HELD LIVE below.
let graphRef: unknown;
const mainAst = time("parse+graph MAIN (held live)", () => {
  const ast = parseFileAst(out208) as t.File;
  graphRef = buildUnifiedGraph(ast, "input.js", NULL_PROFILER, () => true, out208);
  return { ast, nodes: (graphRef as { nodes: Map<unknown, unknown> }).nodes.size };
});
if (freeGraph) {
  graphRef = undefined;
  console.log("  [freed graph before post-naming passes]");
}

// The post-naming passes, in pipeline order, main AST still reachable.
const baseline = time("captureSemanticBaseline (invariant core)", () =>
  captureSemanticBaseline(mainAst.ast)
);
time("validateOutput (site C re-parse)", () => validateOutput(out208, baseline));
time("runPriorDiffReconciliation (site D re-parse + diff)", () =>
  runPriorDiffReconciliation(out208, prior207, () => true, genOpts)
);

console.log(`done (main still live: ${mainAst.ast.type}, ${mainAst.nodes} nodes)`);
