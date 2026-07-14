/**
 * Weighted reference graph over wrapper-body statements (P1). Reuses the
 * splitter's own referenceIndices so the clustering scores the exact graph
 * the pipeline sees, then adds:
 *   - idf[j]: popularity down-weight of the binding(s) declared by statement
 *     j. A hub declaration referenced by everything (a shared logger/util)
 *     gets a low weight so it stops blurring module boundaries — the trick
 *     reference-cluster.ts:151 used, applied at statement granularity.
 *   - lines[i]: statement line span, for the size budgets.
 *
 * The clustering functions (cluster.ts) take the plain RefGraph struct, so
 * they unit-test on hand-built graphs with no AST.
 */

import * as t from "@babel/types";
import { referenceIndices } from "../../../src/split/stable-split.js";

export interface RefGraph {
  /** refs[i] = statement indices statement i references (edge i → j). */
  refs: Array<Set<number>>;
  /** idf[j] = log(N / (1 + indegree(j))) — target-popularity down-weight. */
  idf: number[];
  /** line span of each statement. */
  lines: number[];
  n: number;
}

export function buildRefGraph(body: t.Statement[]): RefGraph {
  const refs = referenceIndices(body);
  const n = body.length;
  const indeg = new Array<number>(n).fill(0);
  for (let i = 0; i < n; i++) {
    for (const j of refs[i]) indeg[j]++;
  }
  const idf = indeg.map((d) => Math.log(n / (1 + d)));
  const lines = body.map((s) =>
    s.loc ? s.loc.end.line - s.loc.start.line + 1 : 1
  );
  return { refs, idf, lines, n };
}
