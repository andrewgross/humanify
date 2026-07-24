/**
 * exp036 idea 8b/C1 — deterministic post-render bucket assignment (core).
 *
 * Within one bucket of structurally-interchangeable members (grouped by
 * declaration [statement hash](../split/statement-hash.ts)), decide the
 * final fresh↔prior name correspondence from the evidence the rendered
 * artifact carries — NAME identity and masked USAGE CONTEXTS — and make
 * that decision DETERMINISTICALLY, overriding whatever the upstream
 * matcher did.
 *
 * Why the pass has to own the whole bucket, not just its orphans:
 * feeding the pass's output back as the next hop's prior breaks the
 * self-hop invariant. On the second hop the matcher transfers every real
 * name verbatim, but because the members are interchangeable it lands
 * each on the WRONG member (a coin-flip the matcher cannot resolve — that
 * is the very reason this pass exists). An orphan-only pass is blind to
 * that: neither swapped name is an orphan. So this core computes the
 * context-optimal assignment over ALL members and emits the moves —
 * orphan adoptions AND swap-corrections — needed to reach it, gated so a
 * correctly-placed name is never disturbed.
 *
 * The naive v1 cut ignored usage context entirely and renamed a correct
 * `getClaudeCodeOAuthToken` to an unrelated `deviceActionMap`; the guards
 * below (name-context "stay" weight, restorable-target) exist to make the
 * override safe. Pure: members in, rename plan out; the AST application
 * (including atomic swaps) wraps it in `family-permute-step.ts`.
 */

import { isBunToken, isDecoratedDescriptive } from "./minted-census.js";

/**
 * One interchangeable-bucket member, characterized by the two evidence
 * sources the rendered artifact carries: its NAME and its USAGE CONTEXTS
 * — the reference lines where the binding is used, with the member's own
 * name masked to `\x00`. A fresh mint that replaced a prior name has the
 * SAME masked usage contexts as that prior name (only the name changed),
 * which is what pairs them; a name the matcher put on the wrong member
 * has the contexts of where it SHOULD be, which is what un-swaps it.
 */
export interface BucketMember {
  name: string;
  /** Reference lines with THIS member's own name masked. */
  contexts: readonly string[];
}

/** A move this pass will apply: a fresh member's current name is replaced
 * by a prior name, justified either by usage-context overlap (an orphan
 * adopting a dead name, or a mis-placed name swapping back). */
export interface ContextAssignment {
  fromName: string;
  toName: string;
  /** Count of masked usage-context lines the fresh member shares with the
   * prior name it adopts. */
  support: number;
}

/** A prior name worth restoring: a real, descriptive word, never a minted
 * leftover the pipeline is trying to eliminate. Restoring onto a mint
 * (`grepOptions → __s`) would reduce the diff but violates "never rename
 * to a mint". */
function isRestorableTarget(name: string): boolean {
  if (name.length <= 2) return false;
  if (name.startsWith("__")) return false; // freed dunders (idea 6) are gaps
  if (isBunToken(name) && !isDecoratedDescriptive(name)) return false;
  return /[a-z]{3}/.test(name) || /[A-Z][a-z]/.test(name); // a real word
}

/**
 * Compute the deterministic set of renames that moves a bucket toward its
 * context-optimal fresh↔prior assignment. Both an orphan adopting a dead
 * prior name and two locked names swapping back are the same operation:
 * give fresh member `f` the prior name whose masked usage context it
 * matches best, but only when that strictly beats leaving `f` where it is.
 *
 * The rules, in order:
 *   1. `support(f, p)` = overlap of masked usage-context lines. This is
 *      the caller/reference evidence, read straight out of the artifact.
 *   2. `stay(f)` = the support `f` has for its OWN current name's prior
 *      counterpart (0 when `f` is a mint — no counterpart). A move
 *      `f → p` is a CANDIDATE only when `support(f, p) >= 1` AND strictly
 *      exceeds `stay(f)`. That single guard yields both behaviors: a mint
 *      (stay 0) adopts any supported dead name; a correctly-placed name
 *      (stay is its max) is never beaten, so it is left alone; a
 *      mis-placed name is beaten by its true position, so it swaps.
 *   3. The prior TARGET must be `isRestorableTarget` — never a mint.
 *   4. Ambiguity is resolved DETERMINISTICALLY: candidates are taken by
 *      (support DESC, fresh index ASC, prior index ASC) greedily, each
 *      fresh member and prior name used once. Determinism is what keeps
 *      the pass self-hop-stable — re-running on its own output reproduces
 *      the same plan. Members with no distinguishing context (a true tie)
 *      never clear the `> stay` bar, so they are left as rendered rather
 *      than guessed.
 *
 * `isEligible` gates the fresh name (skip-listed names stay put). The
 * `fresh`/`prior` arrays are expected in a stable declaration order so the
 * index tie-break is itself stable across re-parse.
 */
/** A masked-usage-context overlap counter for one fresh×prior bucket:
 * `support(fi, pi)` = how many reference lines fresh member `fi` and prior
 * member `pi` share once each side's own name is blanked. */
type SupportFn = (fi: number, pi: number) => number;

function buildSupport(
  fresh: readonly BucketMember[],
  prior: readonly BucketMember[]
): SupportFn {
  const freshCtx = fresh.map((m) => new Set(m.contexts));
  const priorCtx = prior.map((m) => new Set(m.contexts));
  return (fi, pi) => {
    let s = 0;
    for (const c of freshCtx[fi]) if (priorCtx[pi].has(c)) s++;
    return s;
  };
}

interface Candidate {
  fi: number;
  pi: number;
  w: number;
}

/**
 * Every move worth considering, strongest-evidence-first. A fresh member
 * `f` may take restorable prior name `p` only when their context overlap
 * is positive AND STRICTLY beats `f` staying on its own name's prior
 * counterpart (`bar`, which is 0 for a mint). That strict bar is the whole
 * safety property: a mint (bar 0) adopts a supported dead name; a
 * correctly-placed name is unbeatable and left alone; a genuinely
 * cross-placed name is beaten by its true position and swaps — but a merely
 * AMBIGUOUS member (its own name is as good as any) is never moved.
 *
 * Dropping the strict bar to chase a "reclaim the prior's slot" objective
 * was measured at +50,606 noiseLn on 2.1.216: in a big bucket the context
 * weights tie, a positional tie-break dominates, and it mispairs because
 * declaration position does not correspond across versions (the same
 * failure as idea 8a's +401 on the shuffle pair). The strict bar is what
 * keeps the pass a net reducer.
 */
function collectCandidates(
  fresh: readonly BucketMember[],
  prior: readonly BucketMember[],
  support: SupportFn,
  isEligible: (name: string) => boolean
): Candidate[] {
  const priorIndexByName = new Map(prior.map((m, i) => [m.name, i]));
  const candidates: Candidate[] = [];
  for (let fi = 0; fi < fresh.length; fi++) {
    if (!isEligible(fresh[fi].name)) continue;
    const own = priorIndexByName.get(fresh[fi].name);
    const bar = own === undefined ? 0 : support(fi, own);
    for (let pi = 0; pi < prior.length; pi++) {
      if (!isRestorableTarget(prior[pi].name)) continue;
      const w = support(fi, pi);
      if (w >= 1 && w > bar) candidates.push({ fi, pi, w });
    }
  }
  candidates.sort((a, b) => b.w - a.w || a.fi - b.fi || a.pi - b.pi);
  return candidates;
}

export function assignBucket(
  fresh: readonly BucketMember[],
  prior: readonly BucketMember[],
  isEligible: (name: string) => boolean = () => true
): ContextAssignment[] {
  const support = buildSupport(fresh, prior);
  const candidates = collectCandidates(fresh, prior, support, isEligible);

  const usedFresh = new Set<number>();
  const usedPrior = new Set<number>();
  const out: ContextAssignment[] = [];
  for (const { fi, pi, w } of candidates) {
    if (usedFresh.has(fi) || usedPrior.has(pi)) continue;
    usedFresh.add(fi);
    usedPrior.add(pi);
    if (fresh[fi].name !== prior[pi].name) {
      out.push({
        fromName: fresh[fi].name,
        toName: prior[pi].name,
        support: w
      });
    }
  }
  return out;
}
