/**
 * True achievable ceiling for Levers B/A on a real hop — how many of the
 * NOVEL-named 216 bindings (in the 216 ledger, absent from 215) actually have a
 * recoverable 215 identity. This is the honest upper bound the production map
 * approximates; part 4's oracle-based 2,781 counted final↔final name-reuse that
 * does not correspond to a single relocating binding.
 *
 * Builds the ORACLE map (matchPriorVersion on the FINAL 216 graph vs 215 — best
 * case, more than production can match) and intersects it with the novel names.
 *
 * Run:
 *   NODE_OPTIONS=--max-old-space-size=14336 npx tsx \
 *     experiments/033-naming-noise/oracle-coverage.ts [priorVer] [newVer]
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type {
  FunctionNode,
  ModuleBindingNode
} from "../../src/analysis/types.js";
import { matchPriorVersion } from "../../src/prior-version/prior-version.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const VERSIONS = "/Users/andrewgross/Development/unpacked-claude-code/versions";
const priorVer = process.argv[2] ?? "2.1.215";
const newVer = process.argv[3] ?? "2.1.216";

function humanified(ver: string): string {
  return fs.readFileSync(
    `${VERSIONS}/claude-code-${ver}/.humanify/humanified.js`,
    "utf8"
  );
}
function ledger(ver: string): StableSplitLedger {
  return JSON.parse(
    fs.readFileSync(
      `${VERSIONS}/claude-code-${ver}/.humanify/split-ledger.json`,
      "utf8"
    )
  );
}
function graphOf(code: string): {
  functions: Map<string, FunctionNode>;
  bindings: ModuleBindingNode[];
} {
  const ast = parseSync(code, { sourceType: "unambiguous" });
  if (!ast || ast.type !== "File") throw new Error("parse failed");
  const graph = buildUnifiedGraph(ast, "new.js", undefined, undefined, code);
  const functions = new Map<string, FunctionNode>();
  const bindings: ModuleBindingNode[] = [];
  for (const [, node] of graph.nodes) {
    if (node.type === "function") functions.set(node.node.sessionId, node.node);
    else bindings.push(node.node);
  }
  return { functions, bindings };
}

async function main() {
  console.log(`oracle coverage: ${priorVer} → ${newVer}\n`);
  const priorLedger = ledger(priorVer);
  const newLedger = ledger(newVer);
  const priorNames = new Set(Object.keys(priorLedger.nameToFiles));
  const newNames = Object.keys(newLedger.nameToFiles);
  const novel = new Set(newNames.filter((n) => !priorNames.has(n)));
  console.log(
    `216 names ${newNames.length}, novel (flip/new, absent from 215): ${novel.size}`
  );

  const { functions, bindings } = graphOf(humanified(newVer));
  const match = matchPriorVersion(humanified(priorVer), functions, bindings);
  const oracle = new Map<string, string>();
  const add = (nm: string, prior: string) => {
    if (nm !== prior && !oracle.has(nm)) oracle.set(nm, prior);
  };
  for (const r of match.moduleBindingRenames ?? []) add(r.oldName, r.newName);
  for (const fn of functions.values()) {
    if (fn.state.kind === "transferred") {
      for (const p of fn.state.transfers) add(p.oldName, p.newName);
    }
  }
  for (const [, info] of match.closeMatchContext) {
    for (const p of info.nameTransfers) add(p.oldName, p.newName);
  }
  console.log(`oracle map entries (final≠prior): ${oracle.size}`);

  // How many NOVEL names does the oracle recover, with a unanimous prior home?
  let novelInOracle = 0;
  let novelUnanimous = 0;
  for (const n of novel) {
    const prior = oracle.get(n);
    if (!prior) continue;
    novelInOracle++;
    const files = priorLedger.nameToFiles[prior];
    if (files && files.length > 0 && files.every((f) => f === files[0])) {
      novelUnanimous++;
    }
  }
  console.log("\n=== A/C achievable ceiling (best case, oracle map) ===");
  console.log(
    `  novel names WITH a recoverable prior identity: ${novelInOracle}`
  );
  console.log(
    `  ...of those with a UNANIMOUS prior home (A/C-usable):   ${novelUnanimous}`
  );
  console.log(
    `\n  vs the production capture (moduleBindingRenames pins only): 5 entries.`
  );
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
