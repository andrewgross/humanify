/**
 * P5 hold-out experiment: does reference-affinity place NEW bindings in the
 * right cluster better than textual locality? On real data, with ground
 * truth, no LLM.
 *
 * Cluster v fresh → the ground-truth assignment `order`. Hold out a
 * deterministic fraction of statements (pretend they're new code): build a
 * ledger from the REST, inherit the full body, and for the held-out
 * statements that abstain, compare the placement to their true file/folder
 * under textual-locality vs reference-affinity.
 *
 *   NODE_OPTIONS=--max-old-space-size=8192 tsx stability.ts 2.1.89
 */

import * as t from "@babel/types";
import { findWrapperFunction } from "../../src/analysis/wrapper-detection.js";
import { parseFileAst } from "../../src/babel-utils.js";
import { buildRefGraph } from "./lib/graph.js";
import { loadBeautified } from "./lib/io.js";
import { modularizationQuality } from "./lib/metrics.js";
import { seamTieredSplit } from "./lib/split.js";
import {
  buildLedger,
  inherit,
  referenceAffinity,
  reverseRefsOf,
  textualLocality
} from "./lib/stability.js";

function bodyOf(code: string): t.Statement[] {
  const ast = parseFileAst(code);
  if (!ast) throw new Error("parse failed");
  const wrapper = findWrapperFunction(ast);
  if (!wrapper) throw new Error("no wrapper");
  const node = wrapper.functionPath.node.body;
  if (!t.isBlockStatement(node)) throw new Error("body not block");
  return node.body;
}

/** Deterministic hold-out mask: ~pct% of indices, xorshift-hashed (no RNG). */
function holdout(n: number, pct: number): Set<number> {
  const held = new Set<number>();
  for (let i = 0; i < n; i++) {
    let h = (i * 2654435761) >>> 0;
    h = (h ^ (h >>> 15)) >>> 0;
    h = Math.imul(h, 2246822519) >>> 0;
    h = (h ^ (h >>> 13)) >>> 0;
    if (h % 100 < pct) held.add(i);
  }
  return held;
}

/** Ledger from only the kept (non-held-out) statements. */
function ledgerWithout(
  body: t.Statement[],
  order: string[],
  held: Set<number>
) {
  const keepBody: t.Statement[] = [];
  const keepOrder: string[] = [];
  for (let i = 0; i < body.length; i++) {
    if (!held.has(i)) {
      keepBody.push(body[i]);
      keepOrder.push(order[i]);
    }
  }
  return buildLedger(keepBody, keepOrder);
}

const folderOf = (p: string) => p.slice(0, p.lastIndexOf("/"));

async function main(): Promise<void> {
  const version = process.argv[2] ?? "2.1.89";
  const code = await loadBeautified(version);
  const body = bodyOf(code);
  const g = buildRefGraph(body);
  const rev = reverseRefsOf(g);
  const order = seamTieredSplit(code, body).order;
  const fallback = order[0];
  console.log(`statements: ${body.length}\n`);

  // Self-stability sanity: full ledger reproduces the assignment exactly.
  const full = inherit(
    body,
    buildLedger(body, order),
    textualLocality(fallback)
  );
  const identical = full.order.every((f, i) => f === order[i]);
  console.log(
    `self-stability (full ledger): ${identical ? "IDENTICAL" : "DRIFT!"} — ${full.stats.placed} placed\n`
  );

  console.log(`hold-out placement accuracy (of the abstaining statements):`);
  console.log(
    `  pct   placed   locality-file  affinity-file    locality-folder  affinity-folder`
  );
  for (const pct of [5, 10, 20, 40]) {
    const held = holdout(body.length, pct);
    const ledger = ledgerWithout(body, order, held);
    const loc = inherit(body, ledger, textualLocality(fallback));
    const aff = inherit(body, ledger, referenceAffinity(g, rev, fallback));

    let placed = 0;
    let locFile = 0;
    let affFile = 0;
    let locFolder = 0;
    let affFolder = 0;
    for (let i = 0; i < body.length; i++) {
      if (!loc.placedMask[i]) continue; // only score genuine placements
      placed++;
      if (loc.order[i] === order[i]) locFile++;
      if (aff.order[i] === order[i]) affFile++;
      if (folderOf(loc.order[i]) === folderOf(order[i])) locFolder++;
      if (folderOf(aff.order[i]) === folderOf(order[i])) affFolder++;
    }
    const pctf = (x: number) =>
      `${((100 * x) / placed).toFixed(1)}%`.padStart(8);
    console.log(
      `${String(pct).padStart(5)}%  ${String(placed).padStart(6)}   ${pctf(locFile)}       ${pctf(affFile)}      ${pctf(locFolder)}         ${pctf(affFolder)}`
    );
  }

  // Cohesion of the whole tree after a 20% churn, each strategy.
  const held = holdout(body.length, 20);
  const ledger = ledgerWithout(body, order, held);
  const locMQ = modularizationQuality(
    g.refs,
    inherit(body, ledger, textualLocality(fallback)).order
  );
  const affMQ = modularizationQuality(
    g.refs,
    inherit(body, ledger, referenceAffinity(g, rev, fallback)).order
  );
  console.log(
    `\nMQ after 20% churn — locality ${locMQ.toFixed(4)}  affinity ${affMQ.toFixed(4)}  (fresh ${modularizationQuality(g.refs, order).toFixed(4)})`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
