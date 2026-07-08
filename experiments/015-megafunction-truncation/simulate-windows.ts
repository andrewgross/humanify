/**
 * Offline pre-flight for exp015: simulate the per-batch windowed code
 * selection (src/rename/code-window.ts) over every oversized function of a
 * prepared input, exactly as buildRequest does it (batches of 10 in
 * collection order), and report visibility + prompt-size stats. No LLM —
 * this predicts the A/B coverage outcome for free.
 *
 * Usage:
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/015-megafunction-truncation/simulate-windows.ts \
 *     <prepared-runtime.js>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type { FunctionNode } from "../../src/analysis/types.js";
import { generate } from "../../src/babel-utils.js";
import { selectFunctionCode } from "../../src/rename/code-window.js";
import { collectOwnedBindingInfos } from "../../src/rename/function-bindings.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

const BATCH = 10;

import { identifierRegex } from "../../src/utils/identifier-regex.js";

const wordRegex = identifierRegex;

interface Stats {
  oversized: number;
  batches: number;
  visibleIds: number;
  invisibleIds: number;
  fallbacks: number;
  totalShownLines: number;
  invisibleSamples: string[];
}

function simulateFunction(fn: FunctionNode, stats: Stats): void {
  const loc = fn.path.node.loc;
  if (!loc) return;
  const generated = generate(fn.path.node).code;
  if (generated.split("\n").length <= 500) return;
  stats.oversized++;

  const isEligible = createIsEligible("bun", "bun");
  const bindings = collectOwnedBindingInfos(fn.path).filter((b) =>
    isEligible(b.name)
  );
  const bindingMap = new Map(bindings.map((b) => [b.name, b]));
  const names = bindings.map((b) => b.name);
  for (let i = 0; i < names.length; i += BATCH) {
    const batch = names.slice(i, i + BATCH);
    stats.batches++;
    const shown = selectFunctionCode({
      code: generated,
      sessionId: fn.sessionId,
      fnStartLine: loc.start.line,
      fnEndLine: loc.end.line,
      anchorStartLines: batch.map(
        (name) => bindingMap.get(name)?.identifier.loc?.start.line
      )
    });
    stats.totalShownLines += shown.split("\n").length;
    if (shown.includes("[truncated]")) stats.fallbacks++;
    for (const id of batch) {
      if (wordRegex(id).test(shown)) stats.visibleIds++;
      else {
        stats.invisibleIds++;
        if (stats.invisibleSamples.length < 10)
          stats.invisibleSamples.push(`${fn.sessionId}:${id}`);
      }
    }
  }
}

async function main(): Promise<void> {
  const inputPath = process.argv[2];
  if (!inputPath) {
    console.error("usage: simulate-windows.ts <prepared-runtime.js>");
    process.exit(1);
  }
  const code = fs.readFileSync(inputPath, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const isEligible = createIsEligible("bun", "bun");
  const graph = buildUnifiedGraph(ast, "input.js", undefined, isEligible, code);
  const wrapperNode = graph.wrapperPath?.node;

  const stats: Stats = {
    oversized: 0,
    batches: 0,
    visibleIds: 0,
    invisibleIds: 0,
    fallbacks: 0,
    totalShownLines: 0,
    invisibleSamples: []
  };

  for (const [, node] of graph.nodes) {
    if (node.type !== "function") continue;
    const fn = node.node;
    if (fn.path.node === wrapperNode) continue;
    const loc = fn.path.node.loc;
    if (!loc || loc.end.line - loc.start.line + 1 <= 400) continue;
    simulateFunction(fn, stats);
  }

  console.log(`oversized functions:       ${stats.oversized}`);
  console.log(`batches simulated:         ${stats.batches}`);
  console.log(`identifiers visible:       ${stats.visibleIds}`);
  console.log(`identifiers INVISIBLE:     ${stats.invisibleIds}`);
  console.log(`flat-truncation fallbacks: ${stats.fallbacks}`);
  console.log(
    `mean shown lines/batch:    ${(stats.totalShownLines / Math.max(1, stats.batches)).toFixed(0)} (old behavior: 500)`
  );
  if (stats.invisibleSamples.length) {
    console.log(`invisible samples: ${stats.invisibleSamples.join(", ")}`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
