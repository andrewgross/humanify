/**
 * Library-aware split: set Bun CJS factories aside in libraries/ (untouched,
 * unsplit), cluster only the app statements, and report the APP-ONLY
 * distribution. Optionally writes the browsable tree.
 *
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx libaware.ts 2.1.89 [outDir]
 *
 * Names are still mechanical placeholders — judge structure, not names.
 */

import fs from "node:fs";
import path from "node:path";
import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { loadBeautified } from "./lib/io.js";
import {
  crossFileEdgeRatio,
  folderStats,
  histogram,
  lineCountsOf,
  modularizationQuality,
  sizeStats
} from "./lib/metrics.js";
import { libraryAwareBalancedSplit } from "./lib/split.js";

function bodyOf(code: string): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper");
  const node = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(node)) throw new Error("body not block");
  return node.body;
}

async function main(): Promise<void> {
  const version = process.argv[2] ?? "2.1.89";
  const outDir = process.argv[3];
  const code = await loadBeautified(version);
  const body = bodyOf(code);
  const res = libraryAwareBalancedSplit(code, body);
  const { split, appRefs, appOrder } = res;

  // Library summary.
  const libSizes: Array<{ name: string; lines: number }> = [];
  for (const [rel, content] of split.fileContents) {
    if (rel.startsWith("libraries/")) {
      libSizes.push({
        name: rel.slice("libraries/".length),
        lines: content.split("\n").length - 1
      });
    }
  }
  libSizes.sort((a, b) => b.lines - a.lines);
  console.log(
    `=== libraries set aside: ${res.libraryFiles} files, ${res.libraryLines} lines (untouched in libraries/) ===`
  );
  for (const l of libSizes.slice(0, 8))
    console.log(`  ${String(l.lines).padStart(6)}  libraries/${l.name}`);

  // App-only distribution.
  const appFC = new Map<string, string>();
  for (const [rel, content] of split.fileContents) {
    if (!rel.startsWith("libraries/")) appFC.set(rel, content);
  }
  const counts = lineCountsOf(appFC);
  const s = sizeStats(counts);
  const fstats = folderStats([...appFC.keys()]);
  const fpf = sizeStats(fstats.filesPerFolder);
  console.log(`\n=== APP-ONLY distribution (libraries excluded) ===`);
  console.log(`files:        ${s.count}`);
  console.log(
    `folders:      ${fstats.folderCount}   maxDepth: ${fstats.maxDepth}`
  );
  console.log(
    `file lines:   median ${s.median.toFixed(0)}  mean ${s.mean.toFixed(0)}  min ${s.min}  max ${s.max}  stdev ${s.stdev.toFixed(0)}`
  );
  console.log(
    `files/folder: median ${fpf.median.toFixed(0)}  mean ${fpf.mean.toFixed(1)}  min ${fpf.min}  max ${fpf.max}`
  );
  console.log(
    `MQ:           ${modularizationQuality(appRefs, appOrder).toFixed(4)}`
  );
  console.log(
    `cross-edges:  ${(crossFileEdgeRatio(appRefs, appOrder) * 100).toFixed(1)}%`
  );
  console.log(`line histogram (bucket-lo → files):`);
  for (const b of histogram(counts, 20))
    console.log(`  ${b.lo.toFixed(0).padStart(6)}: ${b.n}`);

  if (outDir) {
    fs.rmSync(outDir, { recursive: true, force: true });
    for (const [rel, content] of split.fileContents) {
      const abs = path.join(outDir, rel);
      fs.mkdirSync(path.dirname(abs), { recursive: true });
      fs.writeFileSync(abs, content);
    }
    console.log(
      `\n=== wrote ${split.fileContents.size} files to ${outDir} (open ${outDir}) ===`
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
