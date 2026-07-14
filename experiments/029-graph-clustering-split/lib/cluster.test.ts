import assert from "node:assert/strict";
import { test } from "node:test";
import { crossingCurve, segmentFiles, segmentsOf } from "./cluster.js";
import type { RefGraph } from "./graph.js";

/** Dense within-module edges: every statement references every other in its
 * module. Returns a RefGraph with uniform idf=1 and lines=1. */
function moduleGraph(moduleSizes: number[]): RefGraph {
  const n = moduleSizes.reduce((a, b) => a + b, 0);
  const refs: Array<Set<number>> = Array.from(
    { length: n },
    () => new Set<number>()
  );
  let base = 0;
  for (const size of moduleSizes) {
    for (let i = base; i < base + size; i++) {
      for (let j = base; j < base + size; j++) if (i !== j) refs[i].add(j);
    }
    base += size;
  }
  return { refs, idf: new Array(n).fill(1), lines: new Array(n).fill(1), n };
}

test("crossingCurve: a single edge spans the positions between its endpoints", () => {
  const g: RefGraph = {
    refs: [
      new Set([2]),
      new Set<number>(),
      new Set<number>(),
      new Set<number>()
    ],
    idf: [1, 1, 1, 1],
    lines: [1, 1, 1, 1],
    n: 4
  };
  // edge 0->2 spans positions 1 and 2
  assert.deepEqual(crossingCurve(g, 40), [0, 1, 1, 0, 0]);
});

test("crossingCurve: long edges past the window are ignored", () => {
  const g: RefGraph = {
    refs: [
      new Set([3]),
      new Set<number>(),
      new Set<number>(),
      new Set<number>()
    ],
    idf: [1, 1, 1, 1],
    lines: [1, 1, 1, 1],
    n: 4
  };
  // edge length 3 > window 2 -> no contribution
  assert.deepEqual(crossingCurve(g, 2), [0, 0, 0, 0, 0]);
});

test("segmentFiles cuts at module seams when budget forces a boundary", () => {
  const g = moduleGraph([4, 4, 4]);
  const cuts = segmentFiles(g, {
    minSeg: 2,
    maxSeg: 6,
    maxLines: 1000,
    window: 12
  });
  assert.deepEqual(cuts, [4, 8]);
});

test("segmentFiles: five equal modules recovered", () => {
  const g = moduleGraph([5, 5, 5, 5, 5]);
  const cuts = segmentFiles(g, {
    minSeg: 2,
    maxSeg: 8,
    maxLines: 1000,
    window: 25
  });
  assert.deepEqual(cuts, [5, 10, 15, 20]);
});

test("segmentFiles respects maxLines budget", () => {
  // one big cohesive module of 20 statements, 100 lines each = 2000 lines
  const g = moduleGraph([20]);
  g.lines = new Array(20).fill(100);
  const cuts = segmentFiles(g, {
    minSeg: 2,
    maxSeg: 40,
    maxLines: 500,
    window: 20
  });
  // 500/100 = 5 statements per file -> cuts near multiples of 5
  for (const c of cuts) assert.ok(c % 1 === 0);
  assert.ok(cuts.length >= 3, `expected several cuts, got ${cuts.length}`);
});

test("segmentsOf builds [start,end) runs", () => {
  assert.deepEqual(segmentsOf(12, [4, 8]), [
    [0, 4],
    [4, 8],
    [8, 12]
  ]);
});
