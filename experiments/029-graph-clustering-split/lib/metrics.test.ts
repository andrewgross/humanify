import assert from "node:assert/strict";
import { test } from "node:test";
import {
  crossFileEdgeRatio,
  cyclicFileCount,
  folderStats,
  histogram,
  lineCountsOf,
  modularizationQuality,
  sizeStats
} from "./metrics.js";

test("lineCountsOf counts newlines (wc -l semantics)", () => {
  const fc = new Map([
    ["a.js", "x\ny\nz\n"],
    ["b.js", "one line no newline"],
    ["c.js", ""]
  ]);
  assert.deepEqual(lineCountsOf(fc), [3, 0, 0]);
});

test("sizeStats matches python statistics (sample stdev)", () => {
  const s = sizeStats([2, 4, 4, 4, 5, 5, 7, 9]);
  assert.equal(s.count, 8);
  assert.equal(s.mean, 5);
  assert.equal(s.median, 4.5);
  assert.equal(s.min, 2);
  assert.equal(s.max, 9);
  // population stdev is 2; sample (n-1) stdev ≈ 2.138
  assert.ok(Math.abs(s.stdev - 2.138) < 0.01);
});

test("histogram buckets counts across the range", () => {
  const h = histogram([0, 10, 20, 30, 40], 4);
  assert.equal(h.length, 4);
  assert.equal(
    h.reduce((a, b) => a + b.n, 0),
    5
  );
  // max value falls into the last bucket
  assert.equal(h[3].n >= 1, true);
});

test("modularizationQuality: two tight files with one cross edge", () => {
  // 0,1 in A; 2,3 in B. Edges: 0->1 (intra A), 2->3 (intra B), 1->2 (cross).
  const refs = [new Set([1]), new Set([2]), new Set([3]), new Set<number>()];
  const fileOf = ["A", "A", "B", "B"];
  // A: intra 1, inter 1 -> 1/(1+0.5)=0.6667; B same. MQ = 0.6667
  assert.ok(Math.abs(modularizationQuality(refs, fileOf) - 2 / 3) < 1e-9);
});

test("modularizationQuality: perfect separation = 1", () => {
  const refs = [
    new Set([1]),
    new Set<number>(),
    new Set([3]),
    new Set<number>()
  ];
  const fileOf = ["A", "A", "B", "B"];
  assert.equal(modularizationQuality(refs, fileOf), 1);
});

test("crossFileEdgeRatio", () => {
  const refs = [new Set([1]), new Set([2]), new Set([3]), new Set<number>()];
  const fileOf = ["A", "A", "B", "B"];
  // 3 edges, 1 crosses
  assert.ok(Math.abs(crossFileEdgeRatio(refs, fileOf) - 1 / 3) < 1e-9);
});

test("cyclicFileCount: acyclic file graph = 0", () => {
  const refs = [new Set([1]), new Set([2]), new Set([3]), new Set<number>()];
  const fileOf = ["A", "A", "B", "B"]; // A->B only
  assert.equal(cyclicFileCount(refs, fileOf), 0);
});

test("cyclicFileCount: A<->B cycle = 2 files", () => {
  // add 3->0 so B references A too
  const refs = [new Set([1]), new Set([2]), new Set([3]), new Set([0])];
  const fileOf = ["A", "A", "B", "B"];
  assert.equal(cyclicFileCount(refs, fileOf), 2);
});

test("cyclicFileCount: 3-file cycle A->B->C->A = 3", () => {
  const refs = [new Set([1]), new Set([2]), new Set([0])];
  const fileOf = ["A", "B", "C"];
  assert.equal(cyclicFileCount(refs, fileOf), 3);
});

test("folderStats: flat vs nested", () => {
  const flat = folderStats(["A/x.js", "A/y.js", "B/z.js"]);
  assert.equal(flat.fileCount, 3);
  assert.equal(flat.folderCount, 2);
  assert.equal(flat.maxDepth, 1);

  const nested = folderStats(["a/b/c/x.js", "a/b/y.js", "a/z.js"]);
  assert.equal(nested.maxDepth, 3);
  assert.equal(nested.folderCount, 3); // a/b/c, a/b, a
});
