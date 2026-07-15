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

  return odometerOrder(n, cuts, levelOf, D);
}

/** Multi-level odometer: walk the sequence, and at each cut bump its level's
 * counter and reset all finer counters. Every statement lands at folder
 * depth D-1 (no root dump). */
function odometerOrder(
  n: number,
  cuts: number[],
  levelOf: Map<number, number>,
  D: number
): string[] {
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

/**
 * Group a sorted cut list into runs of <= maxPerGroup, placing each group
 * WALL at the deepest seam (lowest x) within the size window. Caps folder
 * size AND puts boundaries at real seams — fixes global-depth tiering's one
 * oversized folder + singleton spray.
 */
export function pickWalls(
  cuts: number[],
  x: number[],
  maxPerGroup: number
): Set<number> {
  const walls = new Set<number>();
  let start = 0;
  while (start < cuts.length) {
    const end = Math.min(start + maxPerGroup, cuts.length);
    if (end >= cuts.length) break; // trailing group needs no wall
    let best = end;
    let bestD = Number.POSITIVE_INFINITY;
    for (let k = start + 1; k <= end && k < cuts.length; k++) {
      if (x[cuts[k]] < bestD) {
        bestD = x[cuts[k]];
        best = k;
      }
    }
    walls.add(cuts[best]);
    start = best;
  }
  return walls;
}

/**
 * Balanced foldering: top-level walls every <= maxTop files (at the deepest
 * seam in each window), then sub-walls every <= maxSub files within each top
 * group. Guarantees folder size <= maxTop / <= maxSub, so no 877-file folder
 * and far fewer singletons than global-depth tiering.
 */
export function balancedTierOrder(
  n: number,
  x: number[],
  cutsIn: number[],
  maxTop: number,
  maxSub: number
): string[] {
  const cuts = [...cutsIn].sort((a, b) => a - b);
  const topWalls = pickWalls(cuts, x, maxTop);
  const subWalls = new Set<number>();
  let group: number[] = [];
  for (const c of cuts) {
    if (topWalls.has(c)) {
      for (const w of pickWalls(group, x, maxSub)) subWalls.add(w);
      group = [];
    } else {
      group.push(c);
    }
  }
  for (const w of pickWalls(group, x, maxSub)) subWalls.add(w); // last group
  const levelOf = new Map<number, number>();
  for (const c of cuts) {
    levelOf.set(c, topWalls.has(c) ? 1 : subWalls.has(c) ? 2 : 3);
  }
  return odometerOrder(n, cuts, levelOf, 3);
}
