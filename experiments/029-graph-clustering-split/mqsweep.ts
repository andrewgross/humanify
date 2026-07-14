/**
 * Decisive test: does cutting the sequence at graph SEAMS beat naive
 * equal-spacing at the SAME cluster count K? If yes at every K, the graph
 * has exploitable module structure (and my earlier flat MQ was just the
 * granularity confound). If seam≈naive, the reference graph lacks structure
 * the order-prior doesn't already capture — and the deliverable is the size
 * win, not cohesion.
 *
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx mqsweep.ts 2.1.89
 */

import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { crossingCurve } from "./lib/cluster.js";
import { buildRefGraph } from "./lib/graph.js";
import { loadBeautified } from "./lib/io.js";
import { modularizationQuality } from "./lib/metrics.js";

function bodyOf(code: string): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper");
  const bodyNode = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(bodyNode)) throw new Error("body not block");
  return bodyNode.body;
}

/** Order[] from a sorted list of interior cut positions. */
function orderFromCuts(n: number, cuts: number[]): string[] {
  const sorted = [...cuts].sort((a, b) => a - b);
  const order = new Array<string>(n);
  let ci = 0;
  let bin = 0;
  for (let i = 0; i < n; i++) {
    while (ci < sorted.length && sorted[ci] === i) {
      bin++;
      ci++;
    }
    order[i] = `c${bin}`;
  }
  return order;
}

/** K-1 deepest crossing-curve positions, with min spacing to avoid a
 * degenerate pile-up of cuts in one valley. */
function seamCuts(x: number[], n: number, k: number): number[] {
  const gap = Math.max(1, Math.floor(n / (k * 3)));
  const cand = [];
  for (let c = 1; c < n; c++) cand.push(c);
  cand.sort((a, b) => x[a] - x[b] || a - b);
  const cuts: number[] = [];
  const taken: number[] = [];
  for (const c of cand) {
    if (cuts.length >= k - 1) break;
    if (taken.every((tc) => Math.abs(tc - c) >= gap)) {
      cuts.push(c);
      taken.push(c);
    }
  }
  return cuts;
}

function equalCuts(n: number, k: number): number[] {
  const cuts: number[] = [];
  for (let i = 1; i < k; i++) cuts.push(Math.round((i * n) / k));
  return cuts;
}

/** Deterministic pseudo-random cuts (hash of index) — a floor. */
function scatterCuts(n: number, k: number): number[] {
  const set = new Set<number>();
  let seed = 2166136261;
  while (set.size < k - 1) {
    seed = (seed ^ (seed << 13)) >>> 0;
    seed = (seed ^ (seed >> 17)) >>> 0;
    seed = (seed ^ (seed << 5)) >>> 0;
    const pos = 1 + (seed % (n - 1));
    set.add(pos);
  }
  return [...set];
}

async function main(): Promise<void> {
  const version = process.argv[2] ?? "2.1.89";
  const code = await loadBeautified(version);
  const g = buildRefGraph(bodyOf(code));
  const x = crossingCurve(g, 40);
  console.log(`statements: ${g.n}\n`);
  console.log(`     K    seam-MQ   equal-MQ  scatter-MQ   seam-advantage`);
  for (const k of [25, 50, 100, 200, 400, 800, 1600]) {
    const seam = modularizationQuality(
      g.refs,
      orderFromCuts(g.n, seamCuts(x, g.n, k))
    );
    const equal = modularizationQuality(
      g.refs,
      orderFromCuts(g.n, equalCuts(g.n, k))
    );
    const scatter = modularizationQuality(
      g.refs,
      orderFromCuts(g.n, scatterCuts(g.n, k))
    );
    const adv = (((seam - equal) / equal) * 100).toFixed(1);
    console.log(
      `${String(k).padStart(6)}   ${seam.toFixed(4)}    ${equal.toFixed(4)}    ${scatter.toFixed(4)}      ${adv.padStart(6)}%`
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
