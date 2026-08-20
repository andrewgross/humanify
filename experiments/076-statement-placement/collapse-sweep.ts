/**
 * 076 — sweep the folder-collapse threshold, and benchmark the resulting tree
 * against real hand-organised codebases.
 *
 *   npx tsx --max-old-space-size=32768 \
 *     experiments/076-statement-placement/collapse-sweep.ts <priorBundle> <freshBundle>
 *
 * Andrew, 2026-08-15: "What if we change the folder rule to be 1 instead of
 * 3, I think I'm fine with 2 file folders, let's see." Plus: score the
 * spread-out-ness and benchmark it.
 *
 * Reports every threshold with the SAME metrics used on the real repos below,
 * so the two are directly comparable. `Q` (Newman modularity over the import
 * graph) is the one that carries a judgement — the rest describe shape.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as t from "@babel/types";
import { parseFileAst } from "../../src/babel-utils.js";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { assignFossil } from "../../src/split/fossil-assign.js";
import { extractFossilModules } from "../../src/split/fossil-map.js";
import {
  STATEMENT_HASH_VERSION,
  statementHash
} from "../../src/split/statement-hash.js";
import type { StableSplitLedger } from "../../src/split/stable-split.js";
import { formatShape, modularity, treeShape } from "./tree-shape.js";

const [PRIOR, FRESH] = process.argv.slice(2);
if (!PRIOR || !FRESH) {
  console.error("usage: collapse-sweep.ts <priorBundle> <freshBundle>");
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

const p = load(PRIOR);
const f = load(FRESH);
const modules = extractFossilModules(p.body, p.hashes).modules;
const edges: Array<[number, number]> = [];
modules.forEach((m, i) => {
  for (const imp of new Set(m.imports)) if (imp !== i) edges.push([i, imp]);
});

console.log(
  "=== threshold sweep on 2.1.85 (release 1: rules fully in charge) ==="
);
console.log(
  "minFolderFiles         files folders     root   median   max     depth   evenness  modularity"
);
for (const minFolderFiles of [1, 2, 3, 4, 5]) {
  const out = await assignFossil(p.body, p.hashes, undefined, {
    minFolderFiles
  });
  const files = out.fossilModules.map((m) => m.file);
  const shape = treeShape([...new Set(files)]);
  const q = modularity(files, edges);
  console.log(formatShape(`  min=${minFolderFiles}`, shape, q));
}

console.log(
  "\n=== the same thresholds carried to 2.1.86 (release 2: mostly inherited) ==="
);
for (const minFolderFiles of [1, 2, 3]) {
  const first = await assignFossil(p.body, p.hashes, undefined, {
    minFolderFiles
  });
  const ledger: StableSplitLedger = {
    version: 1,
    files: [],
    nameToFiles: {},
    order: [],
    hashVersion: STATEMENT_HASH_VERSION,
    fossilModules: first.fossilModules
  };
  const second = await assignFossil(f.body, f.hashes, ledger, {
    minFolderFiles
  });
  const files = second.fossilModules.map((m) => m.file);
  const fm = extractFossilModules(f.body, f.hashes).modules;
  const e2: Array<[number, number]> = [];
  fm.forEach((m, i) => {
    for (const imp of new Set(m.imports)) if (imp !== i) e2.push([i, imp]);
  });
  console.log(
    formatShape(
      `  min=${minFolderFiles}`,
      treeShape([...new Set(files)]),
      modularity(files, e2)
    )
  );
}

/**
 * BENCHMARK — the same shape metrics on real, hand-organised source trees.
 *
 * Shape only: computing Q for these would need each repo's own import graph
 * parsed, which is a bigger job and is NOT done here. Saying so matters —
 * the shape columns are comparable, the judgement column is absent, and a
 * layout that matches a real repo's evenness has matched its LOOK, not its
 * sense. Sized as the obvious follow-on.
 */
function walkSources(root: string, limit = 20000): string[] {
  const out: string[] = [];
  const skip = new Set(["node_modules", ".git", "dist", "build", "coverage"]);
  const walk = (dir: string, rel: string) => {
    if (out.length > limit) return;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const e of entries) {
      if (e.name.startsWith(".") || skip.has(e.name)) continue;
      const abs = path.join(dir, e.name);
      const r = rel ? `${rel}/${e.name}` : e.name;
      if (e.isDirectory()) walk(abs, r);
      else if (
        /\.(ts|tsx|js|jsx|mjs)$/.test(e.name) &&
        !/\.d\.ts$/.test(e.name)
      )
        out.push(r);
    }
  };
  walk(root, "");
  return out;
}

const NM = path.resolve(import.meta.dirname, "../../node_modules");
const BENCH: Array<[string, string]> = [
  // SCALE IS THE CONFOUND. Our tree is 3,261 files; a 42-file library has no
  // reason to share its shape, and reading "we are worse than preact" off
  // that comparison would be measuring size, not layout. knip is the closest
  // same-order tree available locally; the small ones are kept as context and
  // labelled with their size so nobody quotes them as a target.
  ["knip (810 files)", path.join(NM, "knip")],
  ["humanify src/", path.resolve(import.meta.dirname, "../../src")],
  ["@babel/core lib", path.join(NM, "@babel/core/lib")],
  [
    "preact source",
    path.resolve(import.meta.dirname, "../../test/e2e/fixtures/preact/source")
  ],
  // Our OWN previous output, pre-fossil layout: what the tree looked like
  // before this arc, at full scale.
  [
    "our 2.1.191 (pre-fossil)",
    "/Users/andrewgross/Development/unpacked-claude-code/versions/claude-code-2.1.191/src"
  ]
];
console.log(
  "\n=== benchmark: real hand-organised trees (SHAPE ONLY, no Q) ==="
);
for (const [label, root] of BENCH) {
  if (!fs.existsSync(root)) {
    console.log(`  ${label.padEnd(22)} (absent)`);
    continue;
  }
  const files = walkSources(root);
  if (files.length < 10) {
    console.log(`  ${label.padEnd(22)} (only ${files.length} files — skipped)`);
    continue;
  }
  console.log(formatShape(`  ${label}`, treeShape(files)));
}
