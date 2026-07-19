/**
 * Deterministic repro of the ephemeron tombstone hang WITHOUT the LLM.
 *
 * Cycle = parse a real ~30MB walk bundle THROUGH THE PIPELINE'S PARSE FUNNEL
 * (parseFileAst) -> fill the node-keyed analysis caches via
 * computeStructuralSignature over the whole Program (millions of
 * identifier->binding WeakMap inserts + Babel's internal path cache) ->
 * drop the AST -> repeat, alternating the 2.1.207 / 2.1.208 bundles.
 *
 * Before the per-parse cache-era fix: dropped ASTs leave dead keys in the
 * module-level WeakMaps and each new cycle's bulk insert re-hashes the
 * tombstone-dense tables -> degraded, oscillating cycle times (the
 * 100%-CPU Rehash/WeakCollectionSet stall of exp030 at pipeline scale).
 * After the fix: parseFileAst starts a fresh cache era per big parse ->
 * flat cycles. Same script both sides; only the pipeline code differs.
 *
 * Usage:
 *   NODE_OPTIONS=--max-old-space-size=14336 npx tsx stress.mts [cycles] [--graph]
 *
 *   --graph  escalate the per-cycle filler from the structural signature to
 *            a full buildUnifiedGraph (heavier; use if signature-only fails
 *            to show contrast on some machine).
 *
 * Run under `timeout` from the shell — a truly quadratic cycle cannot be
 * interrupted from inside single-threaded JS.
 */
import fs from "node:fs";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { computeStructuralSignature } from "../../src/analysis/structural-hash.js";
import { parseFileAst, traverse } from "../../src/babel-utils.js";
import { NULL_PROFILER } from "../../src/profiling/index.js";

const V =
  process.env.VERSIONS_ROOT ??
  "/Users/andrewgross/Development/unpacked-claude-code-run-2026-07-17/versions";
const bundles = [207, 208].map((v) =>
  fs.readFileSync(`${V}/claude-code-2.1.${v}/.humanify/humanified.js`, "utf8")
);
const cycles = Number(process.argv[2] ?? 8);
const useGraph = process.argv.includes("--graph");

console.log(
  `filler: ${useGraph ? "buildUnifiedGraph" : "structural-signature"}, ` +
    `bundles ${bundles.map((b) => `${(b.length / 1e6).toFixed(1)}MB`).join("/")}`
);

for (let i = 0; i < cycles; i++) {
  const code = bundles[i % 2];
  const t0 = performance.now();
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  let marker = "";
  if (useGraph) {
    const graph = buildUnifiedGraph(ast, "input.js", NULL_PROFILER, () => true, code);
    marker = `nodes ${graph.nodes.size}`;
  } else {
    traverse(ast, {
      Program(path) {
        marker = `sig ${computeStructuralSignature(path).slice(0, 8)}`;
        path.stop();
      }
    });
  }
  const ms = Math.round(performance.now() - t0);
  const mem = Math.round(process.memoryUsage().rss / 1048576);
  console.log(`cycle ${i + 1}: ${ms}ms  rss ${mem}MB  (${marker})`);
}
console.log("done");
