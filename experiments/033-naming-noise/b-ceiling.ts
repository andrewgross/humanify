/**
 * Lever B ceiling test — DETERMINISTIC, no LLM, no rebuild.
 *
 * B (the binding-identity tier in the split's assignWithPrior) is shipped but
 * dormant: its input `priorMatchMap` is not populated in production yet, which
 * needs a rename/release-lifecycle change. This measures B's UPPER BOUND before
 * paying for that wiring — the split is deterministic when a prior ledger is
 * present (no namer/LLM), so we can run it with the map ON vs OFF on a fixed
 * humanified output and read the effect exactly.
 *
 * The map is built as an ORACLE: match the two FINAL humanified outputs
 * (215 ⇄ 216) with the same structural matcher the pipeline uses, giving
 * {216-name → 215-name} for every matched-but-renamed binding. That's a
 * best-case map (final↔final matches more than minified↔final would), so the
 * numbers are a true ceiling.
 *
 * Run:
 *   NODE_OPTIONS=--max-old-space-size=14336 npx tsx \
 *     experiments/033-naming-noise/b-ceiling.ts <priorVer> <newVer>
 *   # defaults: 2.1.215 2.1.216
 */
import * as fs from "node:fs";
import { parseSync } from "@babel/core";
import { buildUnifiedGraph } from "../../src/analysis/function-graph.js";
import type {
  FunctionNode,
  ModuleBindingNode
} from "../../src/analysis/types.js";
import { matchPriorVersion } from "../../src/prior-version/prior-version.js";
import {
  type StableSplitLedger,
  stableSplitFromCode
} from "../../src/split/stable-split.js";

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

/** First file a name maps to in a ledger, or undefined. */
function fileOf(l: StableSplitLedger, name: string): string | undefined {
  return l.nameToFiles[name]?.[0];
}

async function main() {
  console.log(`Lever B ceiling: ${priorVer} → ${newVer}\n`);

  const priorCode = humanified(priorVer);
  const newCode = humanified(newVer);
  const priorLedger = ledger(priorVer);

  // 1. Oracle map {newName → priorName} from matching final↔final.
  //    matchPriorVersion mutates `functions`: each exact-matched node gets a
  //    "transferred" state carrying {oldName:216name → newName:215name} pairs
  //    for ALL its bindings; close matches carry nameTransfers in the context;
  //    module bindings come back as moduleBindingRenames. Union all three so
  //    the oracle covers every matched-and-renamed binding B could use — not
  //    just the handful of top-level module renames.
  console.log("matching final↔final for the oracle map…");
  const { functions, bindings } = graphOf(newCode);
  const match = matchPriorVersion(priorCode, functions, bindings);
  const oracle = new Map<string, string>();
  const add = (newName: string, priorName: string) => {
    if (newName !== priorName && !oracle.has(newName)) {
      oracle.set(newName, priorName);
    }
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
  console.log(
    `  exact-matched fns: ${match.functionsMatched}, close: ${match.closeMatchCount}, ` +
      `module renames: ${(match.moduleBindingRenames ?? []).length}`
  );
  console.log(`  total oracle entries (renamed bindings): ${oracle.size}\n`);

  // 2. Split the SAME 216 code with the prior 215 ledger, map OFF then ON.
  console.log("splitting 216 without B…");
  const noB = await stableSplitFromCode(newCode, { prior: priorLedger });
  console.log("splitting 216 with B (oracle map)…");
  const withB = await stableSplitFromCode(newCode, {
    prior: priorLedger,
    priorMatchMap: oracle
  });
  if (!noB || !withB) throw new Error("split returned null");

  // 3. Assignment stats.
  console.log("\n=== split assignment stats (deterministic) ===");
  const s0 = noB.stats;
  const s1 = withB.stats;
  console.log(
    `  without B:  residueLocality=${s0.residueLocality}  viaIdentity=${s0.inheritedViaIdentity}`
  );
  console.log(
    `  with B:     residueLocality=${s1.residueLocality}  viaIdentity=${s1.inheritedViaIdentity}`
  );
  console.log(
    `  → B rescued ${s1.inheritedViaIdentity} statements from locality drift ` +
      `(residue ${s0.residueLocality} → ${s1.residueLocality})`
  );

  // 4. Actual relocation reduction: for each oracle-matched binding, did its
  //    file change vs the 215 ledger — without B vs with B?
  let relocNoB = 0;
  let relocB = 0;
  let fixedByB = 0;
  let brokenByB = 0;
  for (const [newName, priorName] of oracle) {
    const f215 = fileOf(priorLedger, priorName);
    if (!f215) continue;
    const fNoB = fileOf(noB.ledger, newName);
    const fB = fileOf(withB.ledger, newName);
    const movedNoB = fNoB !== undefined && fNoB !== f215;
    const movedB = fB !== undefined && fB !== f215;
    if (movedNoB) relocNoB++;
    if (movedB) relocB++;
    if (movedNoB && !movedB) fixedByB++;
    if (!movedNoB && movedB) brokenByB++;
  }
  console.log("\n=== file relocation of matched bindings (vs 215) ===");
  console.log(`  relocated without B: ${relocNoB}`);
  console.log(`  relocated with B:    ${relocB}`);
  console.log(
    `  → B kept ${fixedByB} bindings in their prior file; regressed ${brokenByB}` +
      ` (net ${fixedByB - brokenByB})`
  );
  const denom = relocNoB || 1;
  console.log(
    `  → relocation reduced ${(100 * (relocNoB - relocB)) / denom}% ` +
      `(${relocNoB} → ${relocB})`
  );

  // 5. Concrete diff-line impact: write both trees, diff them. The ONLY
  //    difference is the map, so this is exactly the churn B removes.
  if (process.env.WRITE_TREES) {
    const outNoB = `${process.env.WRITE_TREES}/noB`;
    const outB = `${process.env.WRITE_TREES}/withB`;
    for (const [dir, res] of [
      [outNoB, noB],
      [outB, withB]
    ] as const) {
      for (const [rel, content] of res.fileContents) {
        const full = `${dir}/src/${rel}`;
        fs.mkdirSync(full.slice(0, full.lastIndexOf("/")), { recursive: true });
        fs.writeFileSync(full, content);
      }
    }
    console.log(`\nwrote trees: ${outNoB} , ${outB}`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
