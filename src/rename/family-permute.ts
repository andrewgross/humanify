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
 *      blanked. A pair is applied only when it is MUTUAL-UNIQUE-BEST —
 *      each is the other's strict argmax support — so an ambiguous
 *      binding (two priors tie) is left to upstream naming, not guessed.
 *
 * `isEligible` gates the fresh orphan name (skip-listed names stay put).
 * The prior TARGET is additionally gated: never restore a minted-looking
 * leftover onto a fresh binding (`grepOptions → __s`) — that violates
 * "never rename to a mint" even though it would reduce the diff. Returns the moves to apply.
 */
const MIN_SUPPORT = 1;

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

  // Rule 2: masked-usage-context overlap for every orphan pair.
  const priorCtx = priorOrphans.map((m) => new Set(m.contexts));
  const support = (f: number, p: number): number => {
    let s = 0;
    for (const c of freshOrphans[f].contexts) if (priorCtx[p].has(c)) s++;
    return s;
  };

  // Rule 3: MUTUAL-UNIQUE-BEST. Pair f↔p only when p is f's strict
  // argmax support AND f is p's strict argmax — an unambiguous
  // correspondence. A fresh orphan whose two best priors tie (the
  // p2cValue / pbkdf2IterationCount case) is left to the upstream naming
  // rather than guessed; that both prevents mispairs and keeps the pass
  // self-hop-stable (ambiguous bindings never flip).
  const bestOf = (
    row: (p: number) => number,
    n: number
  ): { idx: number; unique: boolean } => {
    let idx = -1;
    let top = MIN_SUPPORT - 1;
    let second = -1;
    for (let k = 0; k < n; k++) {
      const s = row(k);
      if (s > top) {
        second = top;
        top = s;
        idx = k;
      } else if (s > second) {
        second = s;
      }
    }
    return { idx, unique: idx >= 0 && top > second };
  };

  const out: ContextAssignment[] = [];
  for (let f = 0; f < freshOrphans.length; f++) {
    const fBest = bestOf((p) => support(f, p), priorOrphans.length);
    if (!fBest.unique) continue;
    const p = fBest.idx;
    const pBest = bestOf((ff) => support(ff, p), freshOrphans.length);
    if (!pBest.unique || pBest.idx !== f) continue;
    out.push({
      fromName: freshOrphans[f].name,
      toName: priorOrphans[p].name,
      support: support(f, p)
    });
  }
  return out;
}
