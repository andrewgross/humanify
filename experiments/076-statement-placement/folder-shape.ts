/**
 * 076 addendum — what shape is the emitted tree, actually?
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/076-statement-placement/folder-shape.ts <priorBundle> <freshBundle>
 *
 * Andrew, 2026-08-15: "do we have top level files without a folder? How do
 * things change if a folder with only one file hoists that file up a level
 * instead of wrapping it?"
 *
 * Reports both runs, because they are NOT the same tree and only one of them
 * is governed by the folder rules:
 *   - release 1 (no prior): every folder comes from inference, so
 *     MIN_FOLDER_FILES and the collapse passes decide the shape.
 *   - release 2 (prior ledger): matched modules inherit their path VERBATIM,
 *     so the shape is inherited and the folder rules barely apply. exp074
 *     recorded this as the carry-forward finding — the folder shape arrives
 *     once and then freezes.
 */
import * as fs from "node:fs";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { assignFossil } from "../../src/split/fossil-assign.js";
import {
  STATEMENT_HASH_VERSION,
  statementHash
} from "../../src/split/statement-hash.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: folder-shape.ts <priorBundle> <freshBundle>");
  process.exit(1);
}

function load(file: string) {
  const ast = parseFileAst(fs.readFileSync(file, "utf8"));
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper IIFE");
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error("no block");
  const body = bodyNode.body;
  return { body, hashes: body.map(statementHash) };
}

function report(label: string, files: string[]): void {
  const byFolder = new Map<string, number>();
  for (const f of files) {
    const cut = f.lastIndexOf("/");
    const folder = cut <= 0 ? "src" : f.slice(0, cut);
    byFolder.set(folder, (byFolder.get(folder) ?? 0) + 1);
  }
  const sizes = [...byFolder.entries()];
  const root = byFolder.get("src") ?? 0;
  const nonRoot = sizes.filter(([f]) => f !== "src");
  const singles = nonRoot.filter(([, n]) => n === 1);
  const doubles = nonRoot.filter(([, n]) => n === 2);
  const depth = (f: string) => f.split("/").length - 1;
  const maxDepth = Math.max(0, ...nonRoot.map(([f]) => depth(f)));
  console.log(`\n=== ${label} ===`);
  console.log(`files                 ${files.length}`);
  console.log(`folders (excl. root)  ${nonRoot.length}`);
  console.log(
    `FLAT ROOT src/*.js    ${root} files ` +
      `(${((100 * root) / files.length).toFixed(1)}% of the tree)`
  );
  console.log(
    `single-file folders   ${singles.length}  ` +
      `(${singles.reduce((a, [, n]) => a + n, 0)} files could hoist up one level)`
  );
  console.log(`two-file folders      ${doubles.length}`);
  const nonRootSizes = nonRoot.map(([, n]) => n).sort((a, b) => a - b);
  console.log(
    `folder size: median ${nonRootSizes[Math.floor(nonRootSizes.length / 2)] ?? 0}` +
      `  max ${nonRootSizes[nonRootSizes.length - 1] ?? 0}  deepest nesting ${maxDepth}`
  );
  if (singles.length > 0) {
    console.log(`  examples of single-file folders:`);
    for (const [f] of singles.slice(0, 6)) {
      const only = files.find(
        (x) => x.startsWith(`${f}/`) && !x.slice(f.length + 1).includes("/")
      );
      console.log(`    ${only}`);
    }
  }
}

const p = load(PRIOR);
const f = load(FRESH);
const first = assignFossil(p.body, p.hashes, undefined);
report("release 1 — no prior, folder rules fully in charge", [
  ...new Set(first.fossilModules.map((m) => m.file))
]);

const ledger: StableSplitLedger = {
  version: 1,
  files: [],
  nameToFiles: {},
  order: [],
  hashVersion: STATEMENT_HASH_VERSION,
  fossilModules: first.fossilModules
};
const second = assignFossil(f.body, f.hashes, ledger);
report("release 2 — prior ledger, most paths inherited verbatim", [
  ...new Set(second.fossilModules.map((m) => m.file))
]);
console.log(
  `\nsignals on release 2: ${JSON.stringify(second.stats.signals)}\n` +
    `inherited ${second.stats.inheritedFiles} / fresh-named ${second.stats.freshNamedFiles}`
);
