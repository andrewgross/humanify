/**
 * exp074 verification on the REAL bundle: does tier C recover the
 * dominant churn target, and where do folders land?
 *
 *   npx tsx experiments/074-path-stability/verify.mts
 *
 * Runs the shipped matcher/placement over 2.1.86's raw bundle with
 * 2.1.85-rebased's ledger as prior — the same inputs the pipeline saw in
 * exp070-r1, so the before/after numbers are comparable.
 */
import * as fs from "node:fs";
import type * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { assignFossil } from "../../src/split/fossil-assign.js";
import { statementHash } from "../../src/split/statement-hash.js";

/**
 * The PROCESSED bundle, not the raw input: statement hashes and stems
 * must be computed on the same code the split saw, or nothing matches
 * (the exp071 basis trap — raw and processed hashes are incomparable
 * because our own pipeline rewrites the code).
 */
const INPUT = "/tmp/eval-work/exp070-r1/2.1.86/.humanify/humanified.js";
const PRIOR_LEDGER =
  "/tmp/eval-work/exp070-r1/2.1.85-rebased/.humanify/split-ledger.json";

const code = fs.readFileSync(INPUT, "utf8");
const ast = parseFileAst(code);
if (!ast) throw new Error("parse failed");
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

const prior = JSON.parse(fs.readFileSync(PRIOR_LEDGER, "utf8"));
const out = assignFossil(body, body.map(statementHash), prior);

console.log(`modules: ${out.stats.modules}`);
console.log(`inherited paths: ${out.stats.inheritedFiles}`);
console.log(`freshly named:   ${out.stats.freshNamedFiles}`);
console.log(`match tiers: ${JSON.stringify(out.stats.matchTiers)}`);
console.log(`folder signals: ${JSON.stringify(out.stats.signals)}`);

const files = [...new Set(out.fossilModules.map((m) => m.file))];
const folders = new Map<string, number>();
for (const f of files) {
  const folder = f.slice(0, f.lastIndexOf("/"));
  folders.set(folder, (folders.get(folder) ?? 0) + 1);
}
const sizes = [...folders.values()].sort((a, b) => a - b);
const biggest = [...folders.entries()].sort((a, b) => b[1] - a[1])[0];
console.log(`\nfiles: ${files.length} | folders: ${folders.size}`);
console.log(
  `median files/folder: ${sizes[Math.floor(sizes.length / 2)]} | largest: ${biggest[1]} (${biggest[0]})`
);

// A first fossil run (no prior ledger) — the only time folder inference
// actually decides the tree. With a prior, inherited paths rightly
// dominate and the folder scheme governs only freshly-named modules.
const fresh = assignFossil(body, body.map(statementHash), undefined);
const freshFiles = [...new Set(fresh.fossilModules.map((m) => m.file))];
const freshFolders = new Map<string, number>();
for (const f of freshFiles) {
  const folder = f.slice(0, f.lastIndexOf("/"));
  freshFolders.set(folder, (freshFolders.get(folder) ?? 0) + 1);
}
const freshSizes = [...freshFolders.values()].sort((a, b) => a - b);
const freshBiggest = [...freshFolders.entries()].sort((a, b) => b[1] - a[1])[0];
console.log(`\nFIRST RUN (no prior — folder inference decides):`);
console.log(`  files ${freshFiles.length} | folders ${freshFolders.size}`);
console.log(
  `  median files/folder ${freshSizes[Math.floor(freshSizes.length / 2)]} | largest ${freshBiggest[1]} (${freshBiggest[0]})`
);
console.log(`  signals: ${JSON.stringify(fresh.stats.signals)}`);

// the dominant churn target from exp073's decomposition
const target = out.fossilModules.find((m) =>
  m.file.includes("access-property")
);
const priorTarget = prior.fossilModules.find((m: { file: string }) =>
  m.file.includes("access-property")
);
console.log(`\ndominant churn target (3,204 require lines on 85→86):`);
console.log(`  prior path: ${priorTarget?.file}`);
console.log(`  fresh path: ${target?.file}`);
console.log(
  `  ${target?.file === priorTarget?.file ? "INHERITED — churn held still" : "STILL MOVED"}`
);
