/**
 * Hierarchical seam splitting (P2+P3 unified). One recursive mechanism:
 * within a statement range, find the PROMINENT valleys of the IDF-weighted
 * crossing curve (real module seams, spaced however the code is structured,
 * NOT on a budget grid), split the range at all of them, and recurse. The
 * recursion tree IS the folder tree: internal nodes are folders, leaves are
 * files. Depth and fanout come from the code's own structure, so the output
 * nests like a real src/ tree instead of a flat budget grid.
 *
 * Why prominence-relative-to-the-local-range and not a global threshold:
 * top-level module seams are deep, sub-module seams within a module are
 * shallower; recomputing the threshold per range lets the deep seams form
 * top folders and the shallow ones form nested subfolders.
 *
 * Order prior preserved: every range is a CONTIGUOUS run; we only choose
 * where along the sequence to cut. Deterministic (leftmost-min, fixed
 * thresholds). Pure over RefGraph — unit-tests on hand-built graphs.
 */

import { type FileBudgets, crossingCurve } from "./cluster.js";
import type { RefGraph } from "./graph.js";

export interface HierBudgets extends FileBudgets {
  /** A valley is a seam if it dips below seamFrac × the range's mean
   * crossing. Lower = only the deepest seams cut (coarser). */
  seamFrac: number;
}

export const DEFAULT_HIER_BUDGETS: HierBudgets = {
  minSeg: 4,
  maxSeg: 40,
  maxLines: 450,
  window: 40,
  seamFrac: 0.6
};

function lineSum(lines: number[], s: number, e: number): number {
  let acc = 0;
  for (let i = s; i < e; i++) acc += lines[i];
  return acc;
}

function rangeMean(x: number[], s: number, e: number): number {
  if (e <= s) return 0;
  let acc = 0;
  for (let c = s; c <= e; c++) acc += x[c];
  return acc / (e - s + 1);
}

/**
 * Prominent valleys of x within positions (lo, hi): local minima below
 * `thresh`, kept greedily left-to-right with >= minGap spacing so we don't
 * cut two seams within one statement of each other.
 */
export function prominentMinima(
  x: number[],
  lo: number,
  hi: number,
  thresh: number,
  minGap: number
): number[] {
  const seams: number[] = [];
  let last = -Infinity;
  for (let c = lo; c <= hi; c++) {
    const valley =
      x[c] <= x[c - 1] &&
      x[c] <= x[c + 1] &&
      (x[c] < x[c - 1] || x[c] < x[c + 1]);
    if (valley && x[c] < thresh && c - last >= minGap) {
      seams.push(c);
      last = c;
    }
  }
  return seams;
}

/** Flat budget cuts for a cohesive-but-oversized range (no prominent seam):
 * walk accumulating lines/statements, cut at each budget boundary. Always
 * makes progress (a single over-budget statement becomes its own chunk). */
function budgetChunks(
  g: RefGraph,
  s: number,
  e: number,
  b: HierBudgets
): Array<[number, number]> {
  const ranges: Array<[number, number]> = [];
  let start = s;
  while (start < e) {
    let end = start + 1;
    let acc = g.lines[start];
    while (
      end < e &&
      end - start < b.maxSeg &&
      acc + g.lines[end] <= b.maxLines
    ) {
      acc += g.lines[end];
      end++;
    }
    ranges.push([start, end]);
    start = end;
  }
  return ranges;
}

interface Frame {
  s: number;
  e: number;
  depth: number;
  /** path segment chain to this node, e.g. "g0_2/g1_0" */
  path: string;
}

/**
 * Build the per-statement file-path assignment (order[i]). Leaves get ".js";
 * folders extend the path. The number of distinct folder path prefixes =
 * folder count; path depth = nesting depth.
 */
export function hierSplit(
  g: RefGraph,
  b: HierBudgets = DEFAULT_HIER_BUDGETS
): string[] {
  const x = crossingCurve(g, b.window);
  const order = new Array<string>(g.n);
  const stack: Frame[] = [{ s: 0, e: g.n, depth: 0, path: "" }];
  while (stack.length) {
    const { s, e, depth, path } = stack.pop()!;
    const lines = lineSum(g.lines, s, e);
    const isLeaf = e - s <= 1 || (e - s <= b.maxSeg && lines <= b.maxLines);
    if (isLeaf) {
      const file = path === "" ? "root.js" : `${path}.js`;
      for (let i = s; i < e; i++) order[i] = file;
      continue;
    }
    const thresh = b.seamFrac * rangeMean(x, s, e);
    const seams = prominentMinima(
      x,
      s + b.minSeg,
      e - b.minSeg,
      thresh,
      b.minSeg
    );
    const ranges =
      seams.length > 0 ? partitionAt(s, e, seams) : budgetChunks(g, s, e, b);
    // Single-child (can't happen for seams; possible if budgetChunks made one)
    // — force a leaf to avoid an infinite loop.
    if (ranges.length <= 1) {
      const file = path === "" ? "root.js" : `${path}.js`;
      for (let i = s; i < e; i++) order[i] = file;
      continue;
    }
    for (let k = 0; k < ranges.length; k++) {
      const [rs, re] = ranges[k];
      const seg = `g${depth}_${k}`;
      stack.push({
        s: rs,
        e: re,
        depth: depth + 1,
        path: path === "" ? seg : `${path}/${seg}`
      });
    }
  }
  return order;
}

function partitionAt(
  s: number,
  e: number,
  seams: number[]
): Array<[number, number]> {
  const bounds = [s, ...seams, e];
  const ranges: Array<[number, number]> = [];
  for (let i = 0; i < bounds.length - 1; i++)
    ranges.push([bounds[i], bounds[i + 1]]);
  return ranges;
}
