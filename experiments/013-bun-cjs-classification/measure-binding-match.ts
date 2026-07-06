/**
 * Measure cross-version module-binding match rates on real claude-code
 * bundles, no LLM involved. Unpacks v119 and v120, then runs
 * matchPriorVersion with v119's runtime.js as "prior" and v120's as "new".
 * Names are minified on both sides, so transferred names are meaningless —
 * the match COUNTS are the metric.
 */
import fs from "node:fs";
import path from "node:path";
import { parseSync } from "@babel/core";
import { detectBundle } from "../../src/detection/index.js";
import { buildPipelineConfig } from "../../src/pipeline/config.js";
import { selectUnpackAdapter } from "../../src/unpack/index.js";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { matchPriorVersion } from "../../src/prior-version/prior-version.js";
import type {
  FunctionNode,
  ModuleBindingNode
} from "../../src/analysis/types.js";

const V119 =
  "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.119/binary-decompiled/src/entrypoints/index.js";
const V120 =
  "/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.120/binary-decompiled/src/entrypoints/index.js";
const WORK = "/tmp/exp013-remeasure";

async function unpackRuntime(bundlePath: string, outDir: string) {
  const marker = path.join(outDir, ".unpacked");
  const runtimePath = path.join(outDir, "runtime.js");
  if (fs.existsSync(marker) && fs.existsSync(runtimePath)) {
    console.log(`[cached] ${runtimePath}`);
    return runtimePath;
  }
  fs.mkdirSync(outDir, { recursive: true });
  const bundledCode = fs.readFileSync(bundlePath, "utf-8");
  const detection = detectBundle(bundledCode);
  const config = buildPipelineConfig(detection, {
    bundlerOverride: "bun",
    minifierOverride: "bun"
  });
  const adapter = selectUnpackAdapter(config);
  console.log(`Unpacking ${bundlePath} via ${adapter.name}...`);
  const { files } = await adapter.unpack(bundledCode, outDir);
  console.log(`  ${files.length} files`);
  const runtime = files.find((f) => f.path.endsWith("runtime.js"));
  if (!runtime) throw new Error("no runtime.js in unpack output");
  fs.writeFileSync(marker, "ok");
  return runtime.path;
}

function hms(ms: number) {
  return `${(ms / 1000).toFixed(1)}s`;
}

async function main() {
  const t0 = Date.now();
  const prior = await unpackRuntime(V119, path.join(WORK, "v119"));
  const next = await unpackRuntime(V120, path.join(WORK, "v120"));
  console.log(`unpack done ${hms(Date.now() - t0)}`);

  const newCode = fs.readFileSync(next, "utf-8");
  const priorCode = fs.readFileSync(prior, "utf-8");

  const t1 = Date.now();
  const newAst = parseSync(newCode, { sourceType: "unambiguous" });
  if (!newAst) throw new Error("parse failed");
  console.log(`parse v120 ${hms(Date.now() - t1)}`);

  const t2 = Date.now();
  const graph = buildUnifiedGraph(
    newAst,
    "runtime.js",
    undefined,
    undefined,
    newCode
  );
  const functions = new Map<string, FunctionNode>();
  const bindings: ModuleBindingNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") functions.set(node.node.sessionId, node.node);
    else bindings.push(node.node);
  }
  console.log(
    `graph v120 ${hms(Date.now() - t2)}: ${functions.size} functions, ${bindings.length} bindings`
  );

  const t3 = Date.now();
  const result = matchPriorVersion(priorCode, functions, bindings);
  console.log(`matchPriorVersion ${hms(Date.now() - t3)}`);

  const fnStats = result.matchResult.resolutionStats;
  console.log("\n=== FUNCTIONS ===");
  console.log(`matched: ${result.matchResult.matches.size}`);
  console.log(`close matches: ${result.closeMatchCount}`);
  console.log(`stats: ${JSON.stringify(fnStats)}`);

  console.log("\n=== MODULE BINDINGS ===");
  console.log(`total v120 bindings: ${bindings.length}`);
  console.log(
    `binding renames produced (binding cascade + fn-var): ${result.moduleBindingRenames?.length ?? 0}`
  );
  console.log(`peak rss: ${Math.round(process.memoryUsage().rss / 1e6)} MB`);
  console.log(`total ${hms(Date.now() - t0)}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
