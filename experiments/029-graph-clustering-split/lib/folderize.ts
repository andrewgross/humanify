/**
 * Foldering by seam-depth tiering (the agglomerative direction). P2 gives
 * file-boundary positions; each boundary's crossing depth x[c] says how
 * strong a seam it is. We TIER the boundaries: the deepest seams become
 * top-level folder walls, the next tier subfolder walls, the rest mere
 * file walls. Then a multi-level odometer walks the sequence — every file
 * lands INSIDE a folder (no root dump), and coarser seams nest finer ones.
 *
 * Fixed folder depth D-1 (tiers.length), which matches real src/'s modal
 * depth of ~2; variable depth is a later refinement. Deterministic:
 * boundaries tiered by (depth, position). Order prior preserved — folders
 * are contiguous runs of the sequence.
 */

import { type FileBudgets, crossingCurve, segmentFiles } from "./cluster.js";
import type { RefGraph } from "./graph.js";

/**
 * @param tiers boundary counts per folder level, coarsest first. E.g.
 *   [40, 250] → 40 deepest seams are top-folder walls, next 250 are
 *   subfolder walls, remaining cuts are file walls. Folder depth = 2.
 */
export function tieredSplitOrder(
  g: RefGraph,
  fileBudgets: FileBudgets,
  tiers: number[]
): string[] {
  const x = crossingCurve(g, fileBudgets.window);
  const cuts = segmentFiles(g, fileBudgets).sort((a, b) => a - b);
  return tieredOrderFromCuts(g.n, x, cuts, tiers);
}

/**
 * Tier a precomputed set of file-boundary cuts into a folder hierarchy by
 * seam depth. Decoupled from how the cuts were produced, so it works on the
 * deep-seam cuts (better) as well as the budget-grid cuts.
 */
export function tieredOrderFromCuts(
  n: number,
  x: number[],
  cutsIn: number[],
  tiers: number[]
): string[] {
  const cuts = [...cutsIn].sort((a, b) => a - b);
  const D = tiers.length + 1; // folder levels + the file level

  // Tier each cut by seam depth: deepest (lowest x) first.
  const byDepth = [...cuts].sort((a, b) => x[a] - x[b] || a - b);
  const levelOf = new Map<number, number>();
  let idx = 0;
  for (let L = 0; L < tiers.length; L++) {
    for (let k = 0; k < tiers[L] && idx < byDepth.length; k++, idx++) {
      levelOf.set(byDepth[idx], L + 1);
    }
  }
  for (; idx < byDepth.length; idx++) levelOf.set(byDepth[idx], D); // file walls

  const p = new Array<number>(D).fill(0);
  const pathFrom = (): string => {
    const parts: string[] = [];
    for (let k = 0; k < D - 1; k++) parts.push(`d${k}_${p[k]}`);
    parts.push(`file_${p[D - 1]}.js`);
    return parts.join("/");
  };

  const order = new Array<string>(n);
  let ci = 0;
  let cur = pathFrom();
  for (let i = 0; i < n; i++) {
    while (ci < cuts.length && cuts[ci] === i) {
      const L = levelOf.get(cuts[ci]) ?? D; // 1..D
      p[L - 1]++;
      for (let k = L; k < D; k++) p[k] = 0;
      cur = pathFrom();
      ci++;
    }
    order[i] = cur;
  }
  return order;
}
