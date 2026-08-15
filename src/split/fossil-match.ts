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
 */

export interface FossilSignature {
  /** sorted rename-blind statement hashes of the segment. */
  hashes: string[];
  /** module indexes (same side) of import edges. */
  imports: number[];
}

export interface FossilMatchResult {
  /** fresh module index → prior module index. */
  matches: Map<number, number>;
  /** per-tier counts, for stats/diagnostics. */
  tiers: Record<string, number>;
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
    if (ov >= 0.5) cands.push({ fi, ov, agree: edgeAgreement(state, pi, fi) });
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
    tiers: {}
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
  return { matches: state.freshToPrior, tiers: state.tiers };
}
