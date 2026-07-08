/**
 * Sizing: how many eligible bindings are SKIPPED because a sibling block
 * binding with the same name was already collected? collectOwnedBindingInfos
 * dedups by name (the LLM protocol keys identifiers by name), so for N
 * same-named sibling-block bindings only the first is collected; the
 * shadowed second pass recovers at most one more. The rest stay minified in
 * BOTH legs of a cross-version run — Bun rerolls the token and every
 * reference becomes reroll noise (the `$_→w_` / `v6→X6` families).
 *
 * Usage:
 *   NODE_OPTIONS=--max-old-space-size=16384 npx tsx \
 *     experiments/015-megafunction-truncation/count-duplicate-names.ts \
 *     <prepared-runtime.js>
 */
import fs from "node:fs";
import { parseSync } from "@babel/core";
import type { NodePath } from "@babel/traverse";
import type * as t from "@babel/types";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import { createIsEligible } from "../../src/rename/rename-eligibility.js";

/** Count bindings per name across all block scopes owned by fn (excl. nested fns). */
function countOwnedBindingsByName(
  fnPath: NodePath<t.Function>
): Map<string, number> {
  const counts = new Map<string, number>();
  const bump = (name: string) => counts.set(name, (counts.get(name) ?? 0) + 1);
  for (const [name, binding] of Object.entries(fnPath.scope.bindings)) {
    if (binding.scope === fnPath.scope) bump(name);
  }
  const visited = new WeakSet();
  fnPath.traverse({
    Function(path: NodePath<t.Function>) {
      if (path !== fnPath) path.skip();
    },
    Scope(path: NodePath) {
      const scope = path.scope;
      if (scope === fnPath.scope || visited.has(scope)) return;
      if (scope.path.isFunction() && scope.path !== fnPath) return;
      visited.add(scope);
      for (const [name, binding] of Object.entries(scope.bindings)) {
        if (binding.scope !== scope) continue;
        bump(name);
      }
    }
  });
  return counts;
}

async function main(): Promise<void> {
  const inputPath = process.argv[2];
  const code = fs.readFileSync(inputPath, "utf-8");
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast) throw new Error("parse failed");
  const isEligible = createIsEligible("bun", "bun");
  const graph = buildUnifiedGraph(ast, "input.js", undefined, isEligible, code);
  const wrapperNode = graph.wrapperPath?.node;

  let fnsWithDupes = 0;
  let totalDupes = 0; // bindings beyond the first occurrence of each name
  let dupesBeyondSecond = 0; // beyond what main+shadowed pass can recover
  const perFn: Array<{ id: string; dupes: number; top: string }> = [];

  for (const [, node] of graph.nodes) {
    if (node.type !== "function") continue;
    const fn = node.node;
    if (fn.path.node === wrapperNode) continue;
    const counts = countOwnedBindingsByName(fn.path as NodePath<t.Function>);
    let dupes = 0;
    let beyond = 0;
    let top = "";
    let topN = 0;
    for (const [name, n] of counts) {
      if (n < 2 || !isEligible(name)) continue;
      dupes += n - 1;
      beyond += Math.max(0, n - 2);
      if (n > topN) {
        topN = n;
        top = `${name}×${n}`;
      }
    }
    if (dupes > 0) {
      fnsWithDupes++;
      totalDupes += dupes;
      dupesBeyondSecond += beyond;
      perFn.push({ id: fn.sessionId, dupes, top });
    }
  }

  perFn.sort((a, b) => b.dupes - a.dupes);
  console.log(`functions with same-named sibling bindings: ${fnsWithDupes}`);
  console.log(`duplicate bindings (beyond first):          ${totalDupes}`);
  console.log(
    `unreachable today (beyond first two):       ${dupesBeyondSecond}`
  );
  console.log(`\ntop 15 functions:`);
  for (const f of perFn.slice(0, 15)) {
    console.log(
      `  ${f.id.padEnd(26)} dupes ${String(f.dupes).padStart(4)}  worst ${f.top}`
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
