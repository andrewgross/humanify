/**
 * exp036 idea 8b — evidence-based interchangeable assignment (core).
 *
 * Within one bucket of structurally-interchangeable members (grouped by
 * declaration [statement hash](../split/statement-hash.ts)), decide
 * fresh↔prior name correspondence from the evidence the rendered
 * artifact carries — NAME identity and masked USAGE CONTEXTS — not a
 * blind pool pick. The naive first cut ignored both and renamed a
 * correct `getClaudeCodeOAuthToken` to an unrelated `deviceActionMap`;
 * this core locks round-tripping names and moves only orphans backed by
 * matching call-site context. Pure: members in, rename plan out; the
 * AST application wraps it in `family-permute-step.ts`.
 */

import { isBunToken, isDecoratedDescriptive } from "./minted-census.js";

/**
 * One interchangeable-bucket member, characterized by the two evidence
 * sources the rendered artifact actually carries: its NAME and its
 * USAGE CONTEXTS — the reference lines where the binding is used, with
 * the member's own name masked to `\x00`. A fresh mint that replaced a
 * prior name has the SAME masked usage contexts as that prior name
 * (only the name changed), which is what pairs them.
 */
export interface BucketMember {
  name: string;
  /** Reference lines with THIS member's own name masked. */
  contexts: readonly string[];
}

/** A reassignment this pass will apply: a fresh ORPHAN name (present
 * only on the fresh side — a mint / re-draw) adopts a prior ORPHAN name
 * (present only on the prior side — went dead), justified by matching
 * usage contexts. */
export interface ContextAssignment {
  fromName: string;
  toName: string;
  /** Count of masked usage-context lines the two share. */
  support: number;
}

/**
 * Assign fresh↔prior names within one interchangeable bucket using the
 * evidence in the rendered artifact, NOT a blind pool pick. Two hard
 * rules, in order:
 *
 *   1. NAME IDENTITY IS LOCKED. A name present on BOTH sides round-trips
 *      — it is already correct, so it is never reassigned and its prior
 *      counterpart is never handed to anyone else. (Skipping this is
 *      what made the naive v1 rename `getClaudeCodeOAuthToken` →
 *      `deviceActionMap`: a correct name got grabbed from the pool.)
 *   2. Only the ORPHANS move — a fresh name absent from the prior (a
 *      mint / re-draw) may adopt a prior name absent from the fresh (a
 *      name that went dead) — and only when their MASKED USAGE CONTEXTS
 *      overlap: the reference lines match once each side's own name is
 *      blanked. Greedy by support, highest first; each prior orphan used
 *      once; zero-overlap pairs are refused (no evidence ⇒ no move).
 *
 * `isEligible` gates the fresh orphan name (skip-listed names stay put).
 * The prior TARGET is additionally gated: never restore a minted-looking
 * leftover onto a fresh binding (`grepOptions → __s`) — that violates
 * "never rename to a mint" even though it would reduce the diff. A pair
 * also needs `MIN_SUPPORT` matching context lines, so a single
 * coincidental reference cannot drive a rename. Returns the moves to
 * apply.
 */
const MIN_SUPPORT = 2;

/** A prior name worth restoring: a real, descriptive name, never a
 * minted leftover the pipeline is trying to eliminate. */
function isRestorableTarget(name: string): boolean {
  if (name.length <= 2) return false;
  if (name.startsWith("__")) return false; // freed dunders (idea 6) are gaps
  if (isBunToken(name) && !isDecoratedDescriptive(name)) return false;
  return /[a-z]{3}/.test(name) || /[A-Z][a-z]/.test(name); // a real word
}

export function assignByContext(
  fresh: readonly BucketMember[],
  prior: readonly BucketMember[],
  isEligible: (name: string) => boolean = () => true
): ContextAssignment[] {
  const priorNames = new Set(prior.map((m) => m.name));
  const freshNames = new Set(fresh.map((m) => m.name));
  // Rule 1: locked names are exactly the ones present on both sides;
  // orphans are what remain.
  const freshOrphans = fresh.filter(
    (m) => !priorNames.has(m.name) && isEligible(m.name)
  );
  const priorOrphans = prior.filter(
    (m) => !freshNames.has(m.name) && isRestorableTarget(m.name)
  );
  if (freshOrphans.length === 0 || priorOrphans.length === 0) return [];

  // Rule 2: score every orphan pair by masked-usage-context overlap.
  const priorCtx = priorOrphans.map((m) => new Set(m.contexts));
  const scored: Array<{ f: number; p: number; support: number }> = [];
  for (let f = 0; f < freshOrphans.length; f++) {
    for (let p = 0; p < priorOrphans.length; p++) {
      let support = 0;
      for (const c of freshOrphans[f].contexts)
        if (priorCtx[p].has(c)) support++;
      if (support >= MIN_SUPPORT) scored.push({ f, p, support });
    }
  }
  scored.sort((a, b) => b.support - a.support || a.f - b.f || a.p - b.p);
  const usedF = new Set<number>();
  const usedP = new Set<number>();
  const out: ContextAssignment[] = [];
  for (const { f, p, support } of scored) {
    if (usedF.has(f) || usedP.has(p)) continue;
    usedF.add(f);
    usedP.add(p);
    out.push({
      fromName: freshOrphans[f].name,
      toName: priorOrphans[p].name,
      support
    });
  }
  return out;
}
