import assert from "node:assert/strict";
import { test } from "node:test";
import { deepSeamCuts } from "./cluster.js";
import { tieredOrderFromCuts } from "./folderize.js";
import type { RefGraph } from "./graph.js";

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

test("deepSeamCuts picks the deepest seams up to targetFiles", () => {
  const g = moduleGraph([4, 4, 4]);
  const cuts = deepSeamCuts(g, {
    window: 12,
    minGap: 2,
    targetFiles: 3,
    maxLines: 1000,
    maxSeg: 100
  });
  assert.deepEqual(cuts, [4, 8]);
});

test("deepSeamCuts enforces maxLines in seam-sparse regions", () => {
  // one cohesive module, no interior seam, 20 stmts x 100 lines
  const g = moduleGraph([20]);
  g.lines = new Array(20).fill(100);
  const cuts = deepSeamCuts(g, {
    window: 20,
    minGap: 1,
    targetFiles: 2, // asks for ~1 cut, but budget forces several
    maxLines: 500,
    maxSeg: 40
  });
  assert.ok(cuts.length >= 3, `budget should force splits, got ${cuts.length}`);
});

test("tieredOrderFromCuts nests deepest seams as folders, shallow as files", () => {
  // n=12, cuts at 4 (deep, x=0) and 8 (shallow, x=5)
  const x = new Array(13).fill(3);
  x[4] = 0;
  x[8] = 5;
  const order = tieredOrderFromCuts(12, x, [4, 8], [1]); // 1 folder tier
  // 4 is the deepest -> a folder wall; 8 -> a file wall inside that folder
  assert.equal(order[0], "d0_0/file_0.js");
  assert.equal(order[4], "d0_1/file_0.js");
  assert.equal(order[8], "d0_1/file_1.js");
});

test("tieredOrderFromCuts: every statement lands inside a folder (no root dump)", () => {
  const x = new Array(21).fill(2);
  for (const c of [5, 10, 15]) x[c] = 0;
  const order = tieredOrderFromCuts(20, x, [5, 10, 15], [2]);
  for (const p of order) assert.match(p, /^d0_\d+\/file_\d+\.js$/);
});
