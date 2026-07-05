/**
 * Quantify the cost of Babel scope.rename() on module-level bindings of a
 * real 20MB Bun runtime vs a referencePaths-based fast rename.
 *
 * scope.rename() traverses scope.block — for wrapper-scope bindings that
 * is the ENTIRE bundle, once per rename. The pipeline performs tens of
 * thousands of such renames (transfers, propagation, function declaration
 * names), so per-rename cost × volume is the "100% CPU during renaming"
 * suspect.
 *
 * Run: node --max-old-space-size=16384 --import tsx/esm \
 *        experiments/013-bun-cjs-classification/bench-rename.ts
 * (expects the cached unpack from measure-binding-match.ts)
 */
import fs from "node:fs";
import { performance } from "node:perf_hooks";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type { ModuleBindingNode } from "../../src/analysis/types.js";

const RUNTIME = "/tmp/exp013-remeasure/v120/runtime.js";

async function main() {
  const code = fs.readFileSync(RUNTIME, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const graph = buildUnifiedGraph(
    ast,
    "runtime.js",
    undefined,
    undefined,
    code
  );

  const bindings: ModuleBindingNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "module-binding") bindings.push(node.node);
  }
  bindings.sort(
    (a, b) =>
      (graph.targetScope.bindings[b.name]?.referencePaths.length ?? 0) -
      (graph.targetScope.bindings[a.name]?.referencePaths.length ?? 0)
  );
  console.log(`bindings: ${bindings.length}`);

  // --- scope.rename (Babel renamer, full scope.block traversal) ---
  const SLOW_SAMPLE = 5;
  const t0 = performance.now();
  for (let i = 0; i < SLOW_SAMPLE; i++) {
    const b = bindings[i];
    b.scope.rename(b.name, `__bench_slow_${i}`);
  }
  const slowMs = (performance.now() - t0) / SLOW_SAMPLE;

  // --- referencePaths-based fast rename (applyModuleRename pattern) ---
  const FAST_SAMPLE = 2000;
  const t1 = performance.now();
  let renamed = 0;
  for (
    let i = SLOW_SAMPLE;
    i < SLOW_SAMPLE + FAST_SAMPLE && i < bindings.length;
    i++
  ) {
    const b = bindings[i];
    const binding = b.scope.bindings[b.name];
    if (!binding) continue;
    const newName = `__bench_fast_${i}`;
    binding.identifier.name = newName;
    for (const refPath of binding.referencePaths) {
      if (refPath.isIdentifier()) refPath.node.name = newName;
    }
    b.scope.bindings[newName] = binding;
    delete b.scope.bindings[b.name];
    renamed++;
  }
  const fastMs = (performance.now() - t1) / renamed;

  // --- attemptValidatedRename (validation checks + fast rename) ---
  const { attemptValidatedRename } = await import(
    "../../src/rename/validated-rename.js"
  );
  const VALIDATED_SAMPLE = 2000;
  const start = SLOW_SAMPLE + FAST_SAMPLE;
  const t2 = performance.now();
  let applied = 0;
  for (
    let i = start;
    i < start + VALIDATED_SAMPLE && i < bindings.length;
    i++
  ) {
    const b = bindings[i];
    const attempt = attemptValidatedRename(
      b.scope,
      b.name,
      `__bench_validated_${i}`
    );
    if (attempt.applied) applied++;
  }
  const validatedMs = (performance.now() - t2) / VALIDATED_SAMPLE;

  console.log(
    `scope.rename (highest-ref bindings): ${slowMs.toFixed(1)} ms/rename`
  );
  console.log(
    `fast rename: ${fastMs.toFixed(3)} ms/rename (${renamed} samples)`
  );
  console.log(
    `attemptValidatedRename: ${validatedMs.toFixed(3)} ms/rename (${applied} applied)`
  );
  console.log(
    `extrapolated for 14,000 binding transfers: scope.rename ${((slowMs * 14000) / 60000).toFixed(1)} min vs fast ${((fastMs * 14000) / 1000).toFixed(1)} s`
  );
  console.log(
    `extrapolated for 40,000 fn-decl/local renames at module level: ${((slowMs * 40000) / 3600000).toFixed(2)} h`
  );
}

main();
