/**
 * Order-respecting graph segmentation + folder agglomeration (P2/P3).
 *
 * Files (P2): divisive greedy segmentation of the statement SEQUENCE — the
 * order prior. We precompute a windowed, IDF-weighted "crossing curve"
 * x[c] = weight of short reference edges that span position c; a real module
 * seam is a valley in that curve. Within each size-budgeted window we cut at
 * the lowest-crossing position (leftmost-min, deterministic). Smaller budgets
 * than stable-split's defaults so files land near real-src size (hundreds of
 * lines, not thousands). O(E) curve + O(n) greedy — scales to 23k statements.
 *
 * Folders (P3): bottom-up agglomeration of ADJACENT files by inter-file
 * IDF-weighted reference density into a nested tree (see foldering below).
 *
 * Pure over the RefGraph struct — no AST — so it unit-tests on hand-built
 * graphs.
 */

import type { RefGraph } from "./graph.js";

export interface FileBudgets {
  /** Statements per file: floor before a cut is considered / hard cap. */
  minSeg: number;
  maxSeg: number;
  /** Line budget per file. */
  maxLines: number;
  /** Only edges within this statement span count toward seam detection —
   * long-range edges would wash out local valleys. */
  window: number;
}

export const DEFAULT_FILE_BUDGETS: FileBudgets = {
  minSeg: 6,
  maxSeg: 40,
  maxLines: 450,
  window: 40
};

/**
 * x[c] = IDF-weighted count of reference edges that SPAN position c (one
 * endpoint < c, the other >= c), limited to edges shorter than `window`.
 * Built with a difference array in O(E): an edge between a=min and b=max
 * spans positions a+1..b, so it adds its weight to that range. Weight is the
 * target binding's idf, so hub references barely dent the curve.
 */
export function crossingCurve(g: RefGraph, window: number): number[] {
  const diff = new Array<number>(g.n + 2).fill(0);
  for (let i = 0; i < g.n; i++) {
    for (const j of g.refs[i]) {
      const a = i < j ? i : j;
      const b = i < j ? j : i;
      if (b - a > window) continue;
      diff[a + 1] += g.idf[j];
      diff[b + 1] -= g.idf[j];
    }
  }
  const x = new Array<number>(g.n + 1).fill(0);
  let acc = 0;
  for (let c = 0; c <= g.n; c++) {
    acc += diff[c];
    x[c] = acc;
  }
  return x;
}

/** Furthest segment end from `start` under both budgets. */
function segmentReach(start: number, lines: number[], b: FileBudgets): number {
  let end = start + 1;
  let acc = lines[start];
  while (
    end < lines.length &&
    end - start < b.maxSeg &&
    acc + lines[end] <= b.maxLines
  ) {
    acc += lines[end];
    end++;
  }
  return end;
}

/** Divisive greedy file segmentation. Returns interior cut indices (each an
 * exclusive segment end); segments are the runs between 0, cuts…, n. */
export function segmentFiles(
  g: RefGraph,
  b: FileBudgets = DEFAULT_FILE_BUDGETS
): number[] {
  const x = crossingCurve(g, b.window);
  const cuts: number[] = [];
  let start = 0;
  while (start < g.n) {
    const reach = segmentReach(start, g.lines, b);
    if (reach >= g.n) break;
    const lo = Math.min(start + b.minSeg, reach);
    let bestCut = reach;
    let bestScore = Number.POSITIVE_INFINITY;
    for (let c = lo; c <= reach; c++) {
      if (x[c] < bestScore) {
        bestScore = x[c];
        bestCut = c;
      }
    }
    cuts.push(bestCut);
    start = bestCut;
  }
  return cuts;
}

export interface SeamOpts {
  window: number;
  /** Min statements between two seam cuts. */
  minGap: number;
  /** Target file count — the deepest (targetFiles-1) spaced seams are cut. */
  targetFiles: number;
  /** Safety caps so a seam-sparse region can't become one giant file. */
  maxLines: number;
  maxSeg: number;
}

/**
 * Global-deepest-seam segmentation (the mqsweep winner). Accept the deepest
 * crossing-curve valleys greedily with min spacing until we hit targetFiles,
 * then enforce the size caps by budget-splitting any still-oversized segment
 * at its deepest interior position. Unlike segmentFiles (budget grid), cuts
 * land at true module seams — +22–32% MQ at fine granularity.
 */
export function deepSeamCuts(g: RefGraph, o: SeamOpts): number[] {
  const x = crossingCurve(g, o.window);
  const cand: number[] = [];
  for (let c = 1; c < g.n; c++) cand.push(c);
  cand.sort((a, b) => x[a] - x[b] || a - b);
  const taken = new Set<number>();
  const accepted: number[] = [];
  for (const c of cand) {
    if (accepted.length >= o.targetFiles - 1) break;
    let ok = true;
    for (let d = 1; d < o.minGap; d++) {
      if (taken.has(c - d) || taken.has(c + d)) {
        ok = false;
        break;
      }
    }
    if (ok) {
      accepted.push(c);
      taken.add(c);
    }
  }
  accepted.sort((a, b) => a - b);
  return enforceBudgets(g, accepted, o, x);
}

/** Add budget cuts to any segment exceeding maxLines/maxSeg. */
function enforceBudgets(
  g: RefGraph,
  accepted: number[],
  o: SeamOpts,
  x: number[]
): number[] {
  const bounds = [0, ...accepted, g.n];
  const finalCuts = new Set<number>(accepted);
  for (let b = 0; b < bounds.length - 1; b++) {
    const segEnd = bounds[b + 1];
    let start = bounds[b];
    while (start < segEnd) {
      let end = start + 1;
      let acc = g.lines[start];
      while (
        end < segEnd &&
        end - start < o.maxSeg &&
        acc + g.lines[end] <= o.maxLines
      ) {
        acc += g.lines[end];
        end++;
      }
      if (end >= segEnd) break;
      let bestC = end;
      let best = Number.POSITIVE_INFINITY;
      for (let c = Math.min(start + 1, end); c <= end; c++) {
        if (x[c] < best) {
          best = x[c];
          bestC = c;
        }
      }
      finalCuts.add(bestC);
      start = bestC;
    }
  }
  return [...finalCuts].sort((a, b) => a - b);
}

/** Segment boundaries as [start,end) pairs from interior cuts. */
export function segmentsOf(n: number, cuts: number[]): Array<[number, number]> {
  const bounds = [0, ...cuts, n];
  const segs: Array<[number, number]> = [];
  for (let i = 0; i < bounds.length - 1; i++) {
    segs.push([bounds[i], bounds[i + 1]]);
  }
  return segs;
}
