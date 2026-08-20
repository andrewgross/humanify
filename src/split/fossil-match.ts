/**
 * Match fossil modules across releases — the OWNER of that question.
 *
 * Ported from the exp070 task-0 matcher (`experiments/070-fossil-split/
 * init-match.ts`), which measured 94.5–96.9% match rates with ZERO
 * positional tiebreaks and byte-identical match sets across independent
 * cold runs. Identity comes from WRITE-SET SHAPE (the segment's sorted
 * rename-blind hash multiset) and IMPORT-EDGE CONTEXT mapped through
 * already-made matches — NEVER from position. Duplicated modules are
 * distinct fossils distinguished only by who imports them (the i36/Pd8
 * lesson); a tie edges cannot break stays unmatched and mints fresh.
 *
 * Tiers:
 *   A  unique-signature: exact hash-multiset key, unique on both sides
 *   B  edge-corroborated (iterated to fixpoint): among candidates with
 *      Jaccard overlap ≥ 0.5, the unique best by (edge agreement, then
 *      overlap), requiring positive edge evidence — or overlap ≥ 0.8
 *      with a single candidate.
 *   C  stem-corroborated (exp074): the module's declared-name STEM is
 *      unique among unmatched modules on BOTH sides and content overlap
 *      is ≥ 0.7. Added because A and B together leave the costliest
 *      class unmatched — a module that changed slightly and has too few
 *      import edges to corroborate. Measured on 85→86: `access-property`
 *      went 17→18 statements (overlap 0.75) with ONE import edge, so B
 *      could not license it; the fresh mint then placed it in a
 *      different folder and **3,204 require lines churned**, 90% of all
 *      path-instability churn that hop. Tier C recovers 44 pairs there,
 *      median overlap 1.000 (mostly content-twins the signature tier
 *      cannot pair but whose names are distinct), and holds those 3,204
 *      lines still. A mispairing between content-twins is harmless by
 *      construction: their contents are identical.
 */

export interface FossilSignature {
  /** sorted rename-blind statement hashes of the segment. */
  hashes: string[];
  /** module indexes (same side) of import edges. */
  imports: number[];
  /**
   * The module's file stem — its declared-name kebab (fresh side) or the
   * prior ledger path's basename. Optional: callers without stems get
   * tiers A and B only, which is what every caller had before exp074.
   */
  stem?: string;
  /**
   * GRADED shape tokens (exp078) — tree-shape n-grams and literals, NOT
   * identifier names. Optional; without them the graded tier is inert and a
   * caller gets exactly the pre-exp078 behaviour.
   *
   * Why they exist: `hashes` is compared per statement as same-or-different,
   * one bit, so a statement 95% identical scores the same as one 0%
   * identical. Two enclosures sharing only `var X = {};` then score as
   * highly as two sharing most of their body. Measured on a real release,
   * the 74 pairs the 0.5 content floor rejects have median exact overlap
   * 0.30 and median token similarity 0.858 — the floor was rejecting
   * enclosures that are ~86% the same.
   */
  tokens?: string[];
  /**
   * Names the module DECLARES — its exports, post-rename (exp080).
   *
   * Deliberately name-BEARING, unlike every other field here. That is the
   * point: splitting runs AFTER naming, so a module whose functions the
   * function-matcher already paired carries its PRIOR names on the fresh side.
   * When that happened, the export sets align exactly; when it did not, they
   * do not and this tier stays silent. It cannot invent a match out of churned
   * names because it demands near-identity.
   *
   * Why it is needed: `hashes` compares statements, and a module that GREW
   * substantially has weak statement overlap however certainly it is the same
   * module. Measured on 2.1.215->216, `pr-review-artifact-template.js` went
   * 653 -> 1,006 lines and went unmatched, so it lost its filename to a
   * newcomer and moved to `-2` — 916 git lines, the largest single cross-file
   * move in the tree. Its export set overlapped the prior module 100% and the
   * new occupant 0%.
   *
   * Optional: a ledger written before this field simply gets the pre-exp080
   * behaviour, never a wrong answer — the same contract `tokens` has.
   */
  declared?: string[];
}

export interface FossilMatchResult {
  /** fresh module index → prior module index. */
  matches: Map<number, number>;
  /** per-tier counts, for stats/diagnostics. */
  tiers: Record<string, number>;
  /** fresh module index → the tier that matched it. The per-pair answer to
   * the question `tiers` only counts — without it, explaining ONE wrong
   * match means replaying the whole cascade offline (which is how exp082
   * found the export-heir chains). */
  pairTiers: Map<number, string>;
}

/** Jaccard over declared-name sets. 0 when either side has none. */
function declaredOverlap(a: FossilSignature, b: FossilSignature): number {
  const sa = new Set(a.declared ?? []);
  const sb = new Set(b.declared ?? []);
  if (sa.size === 0 || sb.size === 0) return 0;
  let inter = 0;
  for (const n of sa) if (sb.has(n)) inter++;
  return inter / (sa.size + sb.size - inter);
}

/** Export-set overlap this tier demands. High on purpose: the signal is only
 *  trustworthy where names were INHERITED, which shows up as near-identity.
 *  Anything lower is churned names and must not match. */
const EXPORT_SET_FLOOR = 0.6;

/**
 * TIER: a module whose EXPORT SET matches, mutually and uniquely (exp080).
 *
 * Rescues modules whose content grew or shrank enough that statement overlap
 * is weak while their identity is not in doubt. Requires MUTUAL best on both
 * sides, so two modules cannot both claim one counterpart, and a floor high
 * enough that only inherited-name alignment can clear it.
 *
 * A module that fails to match here does not merely lose a match — it loses
 * its FILENAME, because inherited paths are the only thing that keeps a file
 * where it was. That is why this runs before the weaker content tiers.
 */
function tierExportSet(state: MatchState): void {
  const bestForFresh = new Map<number, { pi: number; score: number }>();
  for (let fi = 0; fi < state.fresh.length; fi++) {
    if (state.freshToPrior.has(fi)) continue;
    const best = bestPriorByExports(state, fi);
    if (best.score >= EXPORT_SET_FLOOR) bestForFresh.set(fi, best);
  }
  // Mutual best: the prior module's own best fresh partner must be this one,
  // so two fresh modules cannot both claim one prior counterpart.
  for (const [fi, { pi, score }] of bestForFresh) {
    if (state.freshToPrior.has(fi) || state.priorToFresh.has(pi)) continue;
    const back = bestFreshByExports(state, pi);
    if (back.fi === fi && back.score === score) {
      record(state, pi, fi, "export-set");
    }
  }
}

/** Highest declared-name overlap among unmatched prior modules. */
function bestPriorByExports(
  state: MatchState,
  fi: number
): { pi: number; score: number } {
  let best = { pi: -1, score: 0 };
  for (let pi = 0; pi < state.prior.length; pi++) {
    if (state.priorToFresh.has(pi)) continue;
    const score = declaredOverlap(state.prior[pi], state.fresh[fi]);
    if (score > best.score) best = { pi, score };
  }
  return best;
}

/** Highest declared-name overlap among unmatched fresh modules. */
function bestFreshByExports(
  state: MatchState,
  pi: number
): { fi: number; score: number } {
  let best = { fi: -1, score: 0 };
  for (let fi = 0; fi < state.fresh.length; fi++) {
    if (state.freshToPrior.has(fi)) continue;
    const score = declaredOverlap(state.prior[pi], state.fresh[fi]);
    if (score > best.score) best = { fi, score };
  }
  return best;
}

/** Floors for the containment tier. Containment forgives GROWTH (Jaccard
 * punishes a module for gaining exports), but on its own it also matches a
 * tiny module absorbed into a big one — {a,b} inside a 30-name module scores
 * 1.0. The Jaccard co-floor is what rules absorption out: the measured true
 * case (create-vqs-component, 4 declared -> 6 sharing 3) sits at 0.43, the
 * absorption shape at 0.07. Both sides must declare >= 2 names so a single
 * common export can never travel. */
const CONTAINMENT_FLOOR = 0.75;
const CONTAINMENT_JACCARD_FLOOR = 0.4;

/** |intersection| / |smaller declared set|, or 0 when either side declares
 * fewer than two names. */
function declaredContainment(a: FossilSignature, b: FossilSignature): number {
  const sa = new Set(a.declared ?? []);
  const sb = new Set(b.declared ?? []);
  if (sa.size < 2 || sb.size < 2) return 0;
  let inter = 0;
  for (const n of sa) if (sb.has(n)) inter++;
  return inter / Math.min(sa.size, sb.size);
}

/** Whether a pair clears BOTH containment floors. */
function containmentEligible(a: FossilSignature, b: FossilSignature): boolean {
  return (
    declaredContainment(a, b) >= CONTAINMENT_FLOOR &&
    declaredOverlap(a, b) >= CONTAINMENT_JACCARD_FLOOR
  );
}

/**
 * TIER: export CONTAINMENT, mutually best (exp085) — the export-set tier for
 * modules that GREW. Runs directly after `tierExportSet`, so the strict rule
 * always wins where both apply, and before the graded/content tiers, so a
 * shape-similar neighbor cannot take a prior whose grown heir is present
 * (which is exactly how create-vqs-component lost its filename: graded
 * matched an occupant at ZERO declared overlap while the heir sat at
 * containment 0.75). Mutual best uses the same declared-overlap ranking as
 * the export-set tier, restricted to pairs clearing both floors.
 */
function tierExportContainment(state: MatchState): void {
  const bestForFresh = new Map<number, { pi: number; score: number }>();
  for (let fi = 0; fi < state.fresh.length; fi++) {
    if (state.freshToPrior.has(fi)) continue;
    const best = bestContainedPrior(state, fi);
    if (best.pi >= 0) bestForFresh.set(fi, best);
  }
  for (const [fi, { pi, score }] of bestForFresh) {
    if (state.freshToPrior.has(fi) || state.priorToFresh.has(pi)) continue;
    const back = bestContainedFresh(state, pi);
    if (back.fi === fi && back.score === score) {
      record(state, pi, fi, "export-containment");
    }
  }
}

/** Best ELIGIBLE prior for one fresh module, ranked by declared overlap. */
function bestContainedPrior(
  state: MatchState,
  fi: number
): { pi: number; score: number } {
  let best = { pi: -1, score: 0 };
  for (let pi = 0; pi < state.prior.length; pi++) {
    if (state.priorToFresh.has(pi)) continue;
    if (!containmentEligible(state.prior[pi], state.fresh[fi])) continue;
    const score = declaredOverlap(state.prior[pi], state.fresh[fi]);
    if (score > best.score) best = { pi, score };
  }
  return best;
}

/** Best ELIGIBLE fresh for one prior module, ranked by declared overlap. */
function bestContainedFresh(
  state: MatchState,
  pi: number
): { fi: number; score: number } {
  let best = { fi: -1, score: 0 };
  for (let fi = 0; fi < state.fresh.length; fi++) {
    if (state.freshToPrior.has(fi)) continue;
    if (!containmentEligible(state.prior[pi], state.fresh[fi])) continue;
    const score = declaredOverlap(state.prior[pi], state.fresh[fi]);
    if (score > best.score) best = { fi, score };
  }
  return best;
}

function sigKey(m: FossilSignature): string {
  return m.hashes.join("|");
}

function overlap(a: FossilSignature, b: FossilSignature): number {
  const ca = new Map<string, number>();
  for (const h of a.hashes) ca.set(h, (ca.get(h) ?? 0) + 1);
  const cb = new Map<string, number>();
  for (const h of b.hashes) cb.set(h, (cb.get(h) ?? 0) + 1);
  let inter = 0;
  for (const [h, n] of ca) inter += Math.min(n, cb.get(h) ?? 0);
  const union = a.hashes.length + b.hashes.length - inter;
  return union === 0 ? 0 : inter / union;
}

function bySignature(mods: FossilSignature[]): Map<string, number[]> {
  const index = new Map<string, number[]>();
  mods.forEach((m, i) => {
    const k = sigKey(m);
    const list = index.get(k) ?? [];
    list.push(i);
    index.set(k, list);
  });
  return index;
}

function importersOf(mods: FossilSignature[]): Map<number, number[]> {
  const rev = new Map<number, number[]>();
  mods.forEach((m, i) => {
    for (const imp of m.imports) {
      const list = rev.get(imp) ?? [];
      list.push(i);
      rev.set(imp, list);
    }
  });
  return rev;
}

interface MatchState {
  prior: FossilSignature[];
  fresh: FossilSignature[];
  priorToFresh: Map<number, number>;
  freshToPrior: Map<number, number>;
  pImporters: Map<number, number[]>;
  fImporters: Map<number, number[]>;
  tiers: Record<string, number>;
  pairTiers: Map<number, string>;
}

/** Matched-edge agreement between a prior and fresh module: imports and
 * importers that map onto each other through already-made matches. */
function edgeAgreement(state: MatchState, pi: number, fi: number): number {
  let agree = 0;
  const freshImports = new Set(state.fresh[fi].imports);
  for (const imp of state.prior[pi].imports) {
    const mapped = state.priorToFresh.get(imp);
    if (mapped !== undefined && freshImports.has(mapped)) agree++;
  }
  const freshImporters = new Set(state.fImporters.get(fi) ?? []);
  for (const imp of state.pImporters.get(pi) ?? []) {
    const mapped = state.priorToFresh.get(imp);
    if (mapped !== undefined && freshImporters.has(mapped)) agree++;
  }
  return agree;
}

function record(state: MatchState, pi: number, fi: number, tier: string): void {
  state.priorToFresh.set(pi, fi);
  state.freshToPrior.set(fi, pi);
  state.tiers[tier] = (state.tiers[tier] ?? 0) + 1;
  state.pairTiers.set(fi, tier);
}

function tierUniqueSignature(state: MatchState): void {
  const bySigP = bySignature(state.prior);
  const bySigF = bySignature(state.fresh);
  for (const [k, ps] of bySigP) {
    const fsIdx = bySigF.get(k);
    if (ps.length === 1 && fsIdx?.length === 1) {
      record(state, ps[0], fsIdx[0], "unique-signature");
    }
  }
}

/**
 * A content-tier match is VETOED when the candidate pair's export sets flatly
 * contradict (both declare names, zero overlap) while some OTHER unmatched
 * fresh module clears the export-set floor for this prior — i.e. the prior's
 * rightful heir, carrying the function-matcher's verdict in its inherited
 * names, is still on the table.
 *
 * Why (exp082, measured on 2.1.215→216): prior `handle-post-tool-use-hook`
 * had 2 statements; a 1-statement neighbor shared one boilerplate hash
 * (overlap exactly 0.5, tier B's floor) plus an agreeing import edge and took
 * the module's identity — and its FILENAME. The real heir minted `-2`, and
 * the neighbor's own prior was then free to be stolen the same way: three
 * misfiled files per chain, three chains on one hop, 298 git lines of moves
 * plus every importer's require path. With the veto the whole chain
 * straightens; on the two calm hops the final match set is byte-identical
 * with and without it (0 differing pairs), so the veto only acts where the
 * name evidence actually contradicts.
 */
function exportHeirVeto(state: MatchState, pi: number, fi: number): boolean {
  const pd = state.prior[pi].declared;
  const fd = state.fresh[fi].declared;
  if (!pd?.length || !fd?.length) return false;
  if (declaredOverlap(state.prior[pi], state.fresh[fi]) > 0) return false;
  const best = bestFreshByExports(state, pi);
  return best.fi !== fi && best.score >= EXPORT_SET_FLOOR;
}

function tryEdgeMatch(
  state: MatchState,
  pi: number,
  unmatchedF: number[]
): boolean {
  interface Cand {
    fi: number;
    ov: number;
    agree: number;
  }
  const cands: Cand[] = [];
  for (const fi of unmatchedF) {
    if (state.freshToPrior.has(fi)) continue;
    const ov = overlap(state.prior[pi], state.fresh[fi]);
    if (ov < 0.5) continue;
    if (exportHeirVeto(state, pi, fi)) continue;
    cands.push({ fi, ov, agree: edgeAgreement(state, pi, fi) });
  }
  if (cands.length === 0) return false;
  cands.sort((a, b) => b.agree - a.agree || b.ov - a.ov);
  const best = cands[0];
  const second = cands[1];
  const uniqueBest =
    !second || best.agree > second.agree || best.ov > second.ov + 1e-9;
  const licensed =
    (best.agree >= 1 && uniqueBest) || (cands.length === 1 && best.ov >= 0.8);
  if (!licensed) return false;
  record(
    state,
    pi,
    best.fi,
    best.agree >= 1 ? "edge-corroborated" : "high-overlap-unique"
  );
  return true;
}

/**
 * Content overlap a stem match must clear. 0.7 measured on 85→86: it
 * admits the 0.75-overlap case worth 3,204 churned lines while the pairs
 * it admits have median overlap 1.000. At 0.8 the win disappears
 * entirely (the one module that matters sits at 0.75).
 */
const STEM_OVERLAP_FLOOR = 0.7;

/** Stem → the single unmatched module holding it, or undefined when the
 * stem is absent or shared (shared stems must never pair by position). */
function uniqueStems(
  mods: FossilSignature[],
  isMatched: (i: number) => boolean
): Map<string, number> {
  const counts = new Map<string, number[]>();
  mods.forEach((m, i) => {
    if (isMatched(i) || !m.stem) return;
    const list = counts.get(m.stem) ?? [];
    list.push(i);
    counts.set(m.stem, list);
  });
  const unique = new Map<string, number>();
  for (const [stem, idx] of counts)
    if (idx.length === 1) unique.set(stem, idx[0]);
  return unique;
}

/** Tier C: pair leftovers whose stems are unique on both sides and whose
 * content still substantially agrees. */
function tierStemCorroborated(state: MatchState): void {
  const priorStems = uniqueStems(state.prior, (i) => state.priorToFresh.has(i));
  const freshStems = uniqueStems(state.fresh, (i) => state.freshToPrior.has(i));
  for (const [stem, pi] of priorStems) {
    const fi = freshStems.get(stem);
    if (fi === undefined) continue;
    if (overlap(state.prior[pi], state.fresh[fi]) < STEM_OVERLAP_FLOOR)
      continue;
    record(state, pi, fi, "stem-corroborated");
  }
}

/**
 * Similarity a graded pairing must clear, and how far it must beat its rival.
 *
 * Measured, not chosen (exp078 `graded-similarity.ts`, real release
 * 2.1.215→2.1.216). Three populations sit in three separated bands:
 *
 *   confidently-matched pairs      median 1.000
 *   pairs the 0.5 exact floor cuts median 0.858   (70 of 74 above 0.5)
 *   enclosures with no counterpart        0.18 – 0.38
 *
 * 0.5 sits in the gap between the second and third bands. The margin exists
 * because a tie must mint fresh rather than guess — the same refusal every
 * other tier makes.
 */
const GRADED_FLOOR = 0.5;
const GRADED_MARGIN = 1.5;

function jaccard(a: Set<string>, b: Set<string>): number {
  if (a.size === 0 || b.size === 0) return 0;
  let inter = 0;
  const [small, big] = a.size < b.size ? [a, b] : [b, a];
  for (const x of small) if (big.has(x)) inter++;
  return inter / (a.size + b.size - inter);
}

/**
 * Graded content tier — pair leftovers whose SHAPE still agrees, even when
 * statement-level equality has collapsed.
 *
 * Runs after the exact tiers and before graph position, because content is
 * stronger evidence than position when it is available at all. Enclosures
 * without tokens are skipped entirely, so a caller that supplies none gets
 * the pre-exp078 behaviour exactly.
 */
function tierGradedContent(state: MatchState): void {
  const unmatchedP: number[] = [];
  for (let pi = 0; pi < state.prior.length; pi++) {
    if (!state.priorToFresh.has(pi) && state.prior[pi].tokens?.length) {
      unmatchedP.push(pi);
    }
  }
  if (unmatchedP.length === 0) return;
  const priorTokens = new Map<number, Set<string>>();
  for (const pi of unmatchedP) {
    priorTokens.set(pi, new Set(state.prior[pi].tokens));
  }
  for (let fi = 0; fi < state.fresh.length; fi++) {
    if (state.freshToPrior.has(fi)) continue;
    const tok = state.fresh[fi].tokens;
    if (!tok?.length) continue;
    const pick = bestGradedCandidate(
      state,
      new Set(tok),
      unmatchedP,
      priorTokens
    );
    if (pick !== undefined && !exportHeirVeto(state, pick, fi)) {
      record(state, pick, fi, "graded-content");
    }
  }
}

/** The clearly-best unmatched prior for one fresh token set, or undefined
 * when nothing clears the floor or the runner-up is too close to call. */
function bestGradedCandidate(
  state: MatchState,
  ft: Set<string>,
  unmatchedP: number[],
  priorTokens: Map<number, Set<string>>
): number | undefined {
  let best: { pi: number; s: number } | undefined;
  let secondScore = 0;
  for (const pi of unmatchedP) {
    if (state.priorToFresh.has(pi)) continue;
    const s = jaccard(ft, priorTokens.get(pi) as Set<string>);
    if (!best || s > best.s) {
      secondScore = best ? best.s : secondScore;
      best = { pi, s };
    } else if (s > secondScore) {
      secondScore = s;
    }
  }
  if (!best || best.s < GRADED_FLOOR) return undefined;
  if (secondScore > 0 && best.s < secondScore * GRADED_MARGIN) return undefined;
  return best.pi;
}

/**
 * Tier D — GRAPH POSITION carries identity when content cannot (exp078).
 *
 * Andrew's framing, 2026-08-16: an enclosure is "the thing these 12 files
 * import and which imports these 3". Its body is what CHANGES between
 * releases; its position is what persists. Every tier above demands content
 * overlap ≥ 0.5 BEFORE edges are consulted, so an enclosure that held its
 * position and rewrote half its body was never a candidate at all — it minted
 * a fresh identity and its file appeared in git as a delete plus an add.
 *
 * Sized before building (Task 0, on a real walk 2.1.215→2.1.216): of 115
 * unmatched enclosures 98 EXISTED in the prior release, and all 74
 * unambiguous ones sat BELOW 0.5 overlap, median 0.30. No content threshold
 * could have reached them. Worth ~17,781 add+delete lines on that release.
 *
 * NO content floor, by design — that is the whole point. The licence is
 * MUTUAL UNIQUE BEST edge agreement instead:
 *
 *   - the fresh enclosure's best prior candidate must be strictly better than
 *     its runner-up, AND
 *   - that prior's best fresh candidate must be this one, strictly.
 *
 * Both halves are load-bearing. Task 0 found 24 cases where two leftovers sat
 * in the same graph position at overlap 0.00; pairing one arbitrarily carries
 * a name onto unrelated code, which is worse than an honest fresh mint. This
 * is the same refusal tier A makes for silent-edged twins (the i36/Pd8
 * lesson) and the same reason tier B requires a unique best.
 *
 * Runs LAST, so it only ever sees what every content tier declined.
 */
function unmatchedIndexes(
  count: number,
  isMatched: (i: number) => boolean
): number[] {
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    if (!isMatched(i)) out.push(i);
  }
  return out;
}

function tierGraphPosition(state: MatchState): void {
  const unmatchedP = unmatchedIndexes(state.prior.length, (pi) =>
    state.priorToFresh.has(pi)
  );
  const unmatchedF = unmatchedIndexes(state.fresh.length, (fi) =>
    state.freshToPrior.has(fi)
  );
  if (unmatchedP.length === 0 || unmatchedF.length === 0) return;

  /** The uniquely-best counterpart by edge agreement, or undefined when the
   * best is tied or has no positive evidence. */
  const bestOf = (
    others: number[],
    agree: (other: number) => number
  ): number | undefined => {
    let best: number | undefined;
    let bestScore = 0;
    let tied = false;
    for (const other of others) {
      const score = agree(other);
      if (score <= 0) continue;
      if (best === undefined || score > bestScore) {
        best = other;
        bestScore = score;
        tied = false;
      } else if (score === bestScore) {
        tied = true;
      }
    }
    return tied ? undefined : best;
  };

  for (const fi of unmatchedF) {
    if (state.freshToPrior.has(fi)) continue;
    const bestP = bestOf(unmatchedP, (pi) =>
      state.priorToFresh.has(pi) ? 0 : edgeAgreement(state, pi, fi)
    );
    if (bestP === undefined) continue;
    // …and the reverse must agree, or the pairing is one-sided.
    const bestF = bestOf(unmatchedF, (other) =>
      state.freshToPrior.has(other) ? 0 : edgeAgreement(state, bestP, other)
    );
    if (bestF !== fi) continue;
    if (exportHeirVeto(state, bestP, fi)) continue;
    record(state, bestP, fi, "graph-position");
  }
}

export function matchFossilModules(
  prior: FossilSignature[],
  fresh: FossilSignature[]
): FossilMatchResult {
  const state: MatchState = {
    prior,
    fresh,
    priorToFresh: new Map(),
    freshToPrior: new Map(),
    pImporters: importersOf(prior),
    fImporters: importersOf(fresh),
    tiers: {},
    pairTiers: new Map()
  };
  tierUniqueSignature(state);
  for (;;) {
    let made = 0;
    const unmatchedF = fresh
      .map((_, i) => i)
      .filter((i) => !state.freshToPrior.has(i));
    for (let pi = 0; pi < prior.length; pi++) {
      if (state.priorToFresh.has(pi)) continue;
      if (tryEdgeMatch(state, pi, unmatchedF)) made++;
    }
    if (made === 0) break;
  }
  tierStemCorroborated(state);
  tierExportSet(state);
  tierExportContainment(state);
  tierGradedContent(state);
  tierGraphPosition(state);
  return {
    matches: state.freshToPrior,
    tiers: state.tiers,
    pairTiers: state.pairTiers
  };
}
