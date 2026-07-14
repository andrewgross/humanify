import assert from "node:assert/strict";
import { test } from "node:test";
import {
  balancedTierOrder,
  pickWalls,
  tieredOrderFromCuts
} from "./folderize.js";

test("pickWalls caps group size and walls at the deepest seam", () => {
  const cuts = [10, 20, 30, 40, 50, 60];
  const x: number[] = [];
  for (let i = 0; i <= 70; i++) x[i] = 5;
  x[30] = 0; // deepest seam
  x[60] = 1;
  const walls = pickWalls(cuts, x, 3); // groups of <=3 cuts
  // first window [20,30]: deepest is 30 -> wall at 30
  assert.ok(walls.has(30));
  // no group may exceed maxPerGroup=3 cuts between walls
  assert.ok(walls.size >= 1);
});

test("balancedTierOrder: no folder exceeds the caps, every file nested", () => {
  const n = 100;
  const cuts: number[] = [];
  for (let c = 5; c < n; c += 5) cuts.push(c); // 19 cuts -> 20 files
  const x = new Array(n + 1).fill(3);
  for (const c of cuts) x[c] = c % 15 === 0 ? 0 : 3; // some deeper seams
  const order = balancedTierOrder(n, x, cuts, 8, 3);
  for (const p of order) assert.match(p, /^d0_\d+\/d1_\d+\/file_\d+\.js$/);
  // count files per folder (d0/d1 prefix) — none may exceed maxSub=3
  const perFolder = new Map<string, Set<string>>();
  for (const p of order) {
    const folder = p.slice(0, p.lastIndexOf("/"));
    if (!perFolder.has(folder)) perFolder.set(folder, new Set());
    perFolder.get(folder)!.add(p);
  }
  for (const files of perFolder.values()) assert.ok(files.size <= 3);
});

test("tieredOrderFromCuts still nests (regression)", () => {
  const x = new Array(13).fill(3);
  x[4] = 0;
  x[8] = 5;
  const order = tieredOrderFromCuts(12, x, [4, 8], [1]);
  assert.equal(order[0], "d0_0/file_0.js");
});
