/**
 * 073 — what share of the ceiling can the MECHANISM reach?
 *
 * The ceiling (944 lines) is churn inside provably-identical modules. But
 * the mechanism only proposes pairs for statements whose hash is AMBIGUOUS
 * tree-wide; statements already unique belong to the existing unique-twin
 * tier and are not new reach. Conflating "population" with "what the gates
 * admit" is exactly how exp069 published a 224-line ceiling worth zero.
 *
 * Runs on raw shipped bundles — no LLM, no naming.
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { extractFossilModules } from "../../src/split/fossil-map.js";
import { statementHash } from "../../src/split/statement-hash.js";

const ROOT = "/Users/andrewgross/Development/claude-code-versions/inputs";
function sideOf(version: string) {
  const code = fs.readFileSync(
    `${ROOT}/claude-code-${version}/binary-decompiled/src/entrypoints/index.js`,
    "utf8"
  );
  const ast = parseFileAst(code);
  if (!ast) throw new Error(`parse failed for ${version}`);
  let body: t.Statement[] = [];
  (function walk(n: unknown): void {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) {
      for (const c of n) walk(c);
      return;
    }
    const node = n as { type?: string; body?: unknown };
    if (
      node.type === "BlockStatement" &&
      Array.isArray(node.body) &&
      node.body.length > body.length
    ) {
      body = node.body as t.Statement[];
    }
    for (const k of Object.keys(node)) {
      if (k !== "loc") walk((node as Record<string, unknown>)[k]);
    }
  })(ast.program);
  const hashes = body.map((s) => statementHash(s));
  const counts = new Map<string, number>();
  for (const h of hashes) counts.set(h, (counts.get(h) ?? 0) + 1);
  return { body, hashes, counts, ex: extractFossilModules(body, hashes) };
}

const prior = sideOf(process.argv[2] ?? "2.1.85");
const fresh = sideOf(process.argv[3] ?? "2.1.86");
const sig = (m: { hashes: string[] }) => m.hashes.join("|");
const countSigs = (mods: Array<{ hashes: string[] }>) => {
  const c = new Map<string, number>();
  for (const m of mods) c.set(sig(m), (c.get(sig(m)) ?? 0) + 1);
  return c;
};
const cp = countSigs(prior.ex.modules);
const cf = countSigs(fresh.ex.modules);

let matchedModules = 0;
let ambiguousModules = 0;
let stmtsInMatched = 0;
let stmtsAmbiguousHash = 0;
let stmtsUniqueHash = 0;
for (const fm of fresh.ex.modules) {
  const k = sig(fm);
  if (cf.get(k) !== 1 || cp.get(k) !== 1) {
    ambiguousModules++;
    continue;
  }
  matchedModules++;
  for (const i of fm.statements) {
    stmtsInMatched++;
    if (fresh.counts.get(fresh.hashes[i]) === 1) stmtsUniqueHash++;
    else stmtsAmbiguousHash++;
  }
}
const pct = (n: number, d: number) => (d ? `${((100 * n) / d).toFixed(1)}%` : "n/a");
console.log("=== 073 addressable set (raw bundles, no LLM) ===");
console.log(`  fresh modules ${fresh.ex.modules.length} | prior ${prior.ex.modules.length}`);
console.log(`  modules matched 1:1 on signature     ${matchedModules}`);
console.log(`  modules ambiguous (skipped)          ${ambiguousModules}`);
console.log(`  statements inside matched modules    ${stmtsInMatched}`);
console.log(
  `  ...hash UNIQUE tree-wide (existing tier owns) ${stmtsUniqueHash}  ${pct(stmtsUniqueHash, stmtsInMatched)}`
);
console.log(
  `  ...hash AMBIGUOUS (THIS mechanism's new reach) ${stmtsAmbiguousHash}  ${pct(stmtsAmbiguousHash, stmtsInMatched)}`
);
