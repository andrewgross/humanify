/**
 * exp036 idea 8b — diff-objective family assignment (core).
 *
 * A post-render pass over top-level statements grouped by
 * [statement hash](../split/statement-hash.ts): same hash ⇒ identical
 * structure INCLUDING literals (the hash masks only identifiers and
 * property names), so members of one bucket differ ONLY in bound names
 * and are provably interchangeable — the certificate, recomputed
 * post-render with no cross-render-boundary sessionId bridge.
 *
 * The current pipeline can leave such a bucket noisy: a fresh member
 * drew a fresh LLM name or was paired with the wrong prior member, even
 * though an assignment exists that reproduces the prior byte-exactly
 * (task A's "zeroable" class — 297 ln on 216 residual after the
 * structural anchor tier). This module computes that assignment with
 * the ACTUAL rendered text as the objective — the ground truth task C's
 * pre-render structural anchors could only approximate.
 *
 * This file is the PURE core: bucket text in, rename plan out. The AST
 * application (slot mapping + validated permutation apply + pure-rename
 * validation) wraps it in the reconcile-step harness (separate, wired
 * after this is validated offline).
 */

/** One fresh statement's assignment: adopt prior[priorIndex]'s names,
 * or -1 to keep its own (already clean, or no clean prior available). */
export interface FamilyAssignment {
  /** Index into the bucket's fresh members. */
  freshIndex: number;
  /** Index into the bucket's prior members, or -1 to leave as-is. */
  priorIndex: number;
  /** True when fresh already equals prior byte-for-byte (no rename). */
  alreadyClean: boolean;
}

/** Edited-line count from `a` to `b`: lines of `a` not present in `b`
 * (set-based, symmetric to the diff-ledger's cost). */
export function editedLineCount(a: string, b: string): number {
  const bLines = new Set(b.split("\n"));
  let n = 0;
  for (const line of a.split("\n")) if (!bLines.has(line)) n++;
  return n;
}

/**
 * Assign each fresh member of one equal-count family bucket to the prior
 * member whose names it should adopt, minimizing total rendered diff.
 *
 * Two-phase, deterministic:
 *   1. Consume byte-identical pairs first (already clean — never touch a
 *      correct member; consumes that prior so nobody else claims it).
 *   2. Greedily pair the remaining fresh↔prior by fewest edited lines,
 *      lowest cost first, ties by (freshIndex, priorIndex). Within a
 *      same-hash bucket every remaining pairing is name-only, so any
 *      bijection zeroes the bucket — the cost order just picks the
 *      permutation closest to the current state (least churn / most
 *      stable), and the deterministic tie-break makes it reparse-stable.
 *
 * Returns one entry per fresh member. Unequal counts are rejected by the
 * caller (membership churn is real change, not this tier's business).
 */
export function assignFamilyBucket(
  freshTexts: readonly string[],
  priorTexts: readonly string[]
): FamilyAssignment[] {
  if (freshTexts.length !== priorTexts.length) {
    throw new Error("assignFamilyBucket requires equal counts");
  }
  const assignments: FamilyAssignment[] = [];
  const priorUsed = new Array(priorTexts.length).fill(false);
  const freshDone = new Array(freshTexts.length).fill(false);

  // Phase 1: byte-identical pairs stay put.
  for (let f = 0; f < freshTexts.length; f++) {
    for (let p = 0; p < priorTexts.length; p++) {
      if (priorUsed[p] || freshTexts[f] !== priorTexts[p]) continue;
      assignments.push({ freshIndex: f, priorIndex: p, alreadyClean: true });
      priorUsed[p] = true;
      freshDone[f] = true;
      break;
    }
  }

  // Phase 2: min-cost greedy over the remainder.
  const pairs: Array<{ f: number; p: number; cost: number }> = [];
  for (let f = 0; f < freshTexts.length; f++) {
    if (freshDone[f]) continue;
    for (let p = 0; p < priorTexts.length; p++) {
      if (priorUsed[p]) continue;
      pairs.push({ f, p, cost: editedLineCount(freshTexts[f], priorTexts[p]) });
    }
  }
  pairs.sort((a, b) => a.cost - b.cost || a.f - b.f || a.p - b.p);
  for (const { f, p } of pairs) {
    if (freshDone[f] || priorUsed[p]) continue;
    assignments.push({ freshIndex: f, priorIndex: p, alreadyClean: false });
    freshDone[f] = true;
    priorUsed[p] = true;
  }

  return assignments.sort((a, b) => a.freshIndex - b.freshIndex);
}

/**
 * A concrete rename this tier would apply: the fresh member at
 * `freshIndex` should reassign so its rendered text matches the prior
 * member at `priorIndex`. `alreadyClean`/self assignments are dropped —
 * only reassignments that actually change a fresh member are returned.
 */
export function reassignmentsOnly(
  assignments: readonly FamilyAssignment[]
): FamilyAssignment[] {
  return assignments.filter((a) => !a.alreadyClean && a.priorIndex >= 0);
}
