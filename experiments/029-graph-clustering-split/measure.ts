/**
 * Measure a split's intrinsic quality on a decompiled Bun bundle.
 *   tsx measure.ts <version> [algo]
 * algo: "baseline" (current stable-split) — more added in later phases.
 *
 * Needs a big heap for the 12MB bundle:
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx measure.ts 2.1.88 baseline
 */

import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import {
  referenceIndices,
  stableSplitFromCode
} from "../../src/split/stable-split.js";
import { type FileBudgets, DEFAULT_FILE_BUDGETS } from "./lib/cluster.js";
import { type HierBudgets, DEFAULT_HIER_BUDGETS } from "./lib/hier.js";
import { loadBeautified } from "./lib/io.js";
import {
  crossFileEdgeRatio,
  cyclicFileCount,
  folderStats,
  histogram,
  lineCountsOf,
  modularizationQuality,
  sizeStats
} from "./lib/metrics.js";
import {
  type Split,
  DEFAULT_SEAM_OPTS,
  clusteredSplit,
  hierClusteredSplit,
  seamBalancedSplit,
  seamTieredSplit,
  tieredClusteredSplit
} from "./lib/split.js";

async function splitBaseline(code: string): Promise<Split> {
  // The budget-grid fresh path was removed when exp029 productionized
  // clustering as the sole approach; stableSplitFromCode now always clusters.
  // Historical baseline numbers live in baseline-*.txt.
  const result = await stableSplitFromCode(code);
  if (!result) throw new Error("input is not a single wrapper IIFE");
  if (!result.wrapper) throw new Error("wrapper parse missing");
  const bodyNode = result.wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode))
    throw new Error("wrapper body not a block");
  return {
    fileContents: result.fileContents,
    order: result.ledger.order,
    body: bodyNode.body
  };
}

/** Parse + unwrap once for the clustered path (skips baseline assignment). */
function bodyOf(code: string): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("input is not a single wrapper IIFE");
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode))
    throw new Error("wrapper body not a block");
  return bodyNode.body;
}

/** Budgets overridable via env for fast iteration (fair A/B: run baseline
 * and clustered with the same EXP029_* env to compare at equal granularity). */
function envBudgets(defaults: FileBudgets): FileBudgets {
  const num = (k: string, d: number) => {
    const v = process.env[k];
    return v ? Number(v) : d;
  };
  return {
    minSeg: num("EXP029_MINSEG", defaults.minSeg),
    maxSeg: num("EXP029_MAXSEG", defaults.maxSeg),
    maxLines: num("EXP029_MAXLINES", defaults.maxLines),
    window: num("EXP029_WINDOW", defaults.window)
  };
}

function splitClustered(code: string): Split {
  const budgets = envBudgets(DEFAULT_FILE_BUDGETS);
  console.error(`[measure] budgets ${JSON.stringify(budgets)}`);
  return clusteredSplit(code, bodyOf(code), budgets);
}

function splitSeam(code: string): Split {
  const seam = {
    ...DEFAULT_SEAM_OPTS,
    targetFiles: process.env.EXP029_TARGET
      ? Number(process.env.EXP029_TARGET)
      : DEFAULT_SEAM_OPTS.targetFiles,
    maxLines: process.env.EXP029_MAXLINES
      ? Number(process.env.EXP029_MAXLINES)
      : DEFAULT_SEAM_OPTS.maxLines
  };
  const tiers = (process.env.EXP029_TIERS ?? "40,250")
    .split(",")
    .map((s) => Number(s.trim()));
  console.error(
    `[measure] seam ${JSON.stringify(seam)} tiers ${JSON.stringify(tiers)}`
  );
  return seamTieredSplit(code, bodyOf(code), seam, tiers);
}

function splitBalanced(code: string): Split {
  const seam = {
    ...DEFAULT_SEAM_OPTS,
    targetFiles: process.env.EXP029_TARGET
      ? Number(process.env.EXP029_TARGET)
      : DEFAULT_SEAM_OPTS.targetFiles,
    maxLines: process.env.EXP029_MAXLINES
      ? Number(process.env.EXP029_MAXLINES)
      : DEFAULT_SEAM_OPTS.maxLines
  };
  const maxTop = process.env.EXP029_MAXTOP
    ? Number(process.env.EXP029_MAXTOP)
    : 100;
  const maxSub = process.env.EXP029_MAXSUB
    ? Number(process.env.EXP029_MAXSUB)
    : 25;
  console.error(
    `[measure] seam ${JSON.stringify(seam)} maxTop ${maxTop} maxSub ${maxSub}`
  );
  return seamBalancedSplit(code, bodyOf(code), seam, maxTop, maxSub);
}

function splitTiered(code: string): Split {
  const fb = envBudgets(DEFAULT_FILE_BUDGETS);
  const tiers = (process.env.EXP029_TIERS ?? "40,250")
    .split(",")
    .map((s) => Number(s.trim()));
  console.error(
    `[measure] budgets ${JSON.stringify(fb)} tiers ${JSON.stringify(tiers)}`
  );
  return tieredClusteredSplit(code, bodyOf(code), fb, tiers);
}

function splitHier(code: string): Split {
  const fb = envBudgets(DEFAULT_HIER_BUDGETS);
  const seamFrac = process.env.EXP029_SEAMFRAC
    ? Number(process.env.EXP029_SEAMFRAC)
    : DEFAULT_HIER_BUDGETS.seamFrac;
  const budgets: HierBudgets = { ...fb, seamFrac };
  console.error(`[measure] budgets ${JSON.stringify(budgets)}`);
  return hierClusteredSplit(code, bodyOf(code), budgets);
}

function reportMetrics(label: string, split: Split): void {
  const { fileContents, order, body } = split;
  const refs = referenceIndices(body);
  const counts = lineCountsOf(fileContents);
  const s = sizeStats(counts);
  const files = [...fileContents.keys()];
  const fstats = folderStats(files);
  const fpf = sizeStats(fstats.filesPerFolder);
  const mq = modularizationQuality(refs, order);
  const cross = crossFileEdgeRatio(refs, order);
  const cyclic = cyclicFileCount(refs, order);

  console.log(`\n===== ${label} =====`);
  console.log(`statements:   ${body.length}`);
  console.log(`files:        ${s.count}`);
  console.log(
    `folders:      ${fstats.folderCount}   maxDepth: ${fstats.maxDepth}`
  );
  console.log(
    `file lines:   median ${s.median.toFixed(0)}  mean ${s.mean.toFixed(0)}  ` +
      `min ${s.min}  max ${s.max}  stdev ${s.stdev.toFixed(0)}`
  );
  console.log(
    `files/folder: median ${fpf.median.toFixed(0)}  mean ${fpf.mean.toFixed(1)}  ` +
      `min ${fpf.min}  max ${fpf.max}`
  );
  console.log(`MQ:           ${mq.toFixed(4)}   (higher = more cohesive)`);
  console.log(`cross-edges:  ${(cross * 100).toFixed(1)}%   (lower = tighter)`);
  console.log(
    `cyclic files: ${cyclic}   (files in a >1 SCC of the import graph)`
  );
  console.log(`depth histogram (folders-above → files):`);
  for (const d of [...fstats.depthHistogram.keys()].sort((a, b) => a - b)) {
    console.log(`  depth ${d}: ${fstats.depthHistogram.get(d)}`);
  }
  console.log(`line histogram (bucket-lo → files):`);
  for (const b of histogram(counts, 20)) {
    console.log(`  ${b.lo.toFixed(0).padStart(6)}: ${b.n}`);
  }
}

async function main(): Promise<void> {
  const version = process.argv[2] ?? "2.1.88";
  const algo = process.argv[3] ?? "baseline";
  const start = Date.now();
  console.error(`[measure] loading + beautifying ${version} …`);
  const code = await loadBeautified(version);
  console.error(
    `[measure] beautified ${(code.length / 1e6).toFixed(1)}MB in ${Date.now() - start}ms; splitting (${algo}) …`
  );
  const splitStart = Date.now();
  const split =
    algo === "clustered"
      ? splitClustered(code)
      : algo === "hier"
        ? splitHier(code)
        : algo === "tiered"
          ? splitTiered(code)
          : algo === "seam"
            ? splitSeam(code)
            : algo === "balanced"
              ? splitBalanced(code)
              : await splitBaseline(code);
  console.error(`[measure] split in ${Date.now() - splitStart}ms`);
  reportMetrics(`${version} — ${algo}`, split);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
