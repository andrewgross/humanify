import {
  buildBindingFullFingerprint,
  buildFullFingerprint,
  calleeShapesEqual,
  computeShingleSet,
  jaccardSimilarity
} from "./function-fingerprint.js";
import { type ExternalRefEvidence, propagate } from "./propagation.js";
import type {
  CalleeShape,
  FingerprintIndex,
  FunctionFingerprint,
  FunctionNode,
  MatchResult,
  ModuleBindingNode,
  ResolutionStats
} from "./types.js";

/**
 * Builds a fingerprint index from a function graph.
 * The index supports efficient lookup for multi-resolution matching.
 */
export function buildFingerprintIndex(
  functions: Map<string, FunctionNode>
): FingerprintIndex {
  const index: FingerprintIndex = {
    byStructuralHash: new Map(),
    fingerprints: new Map(),
    functions
  };

  for (const [sessionId, fn] of functions) {
    // Build full fingerprint with callee information
    const fingerprint = buildFullFingerprint(fn, functions);
    index.fingerprints.set(sessionId, fingerprint);

    // Index by structuralHash (uniqueHash lookup)
    const structuralHashList =
      index.byStructuralHash.get(fingerprint.structuralHash) ?? [];
    structuralHashList.push(sessionId);
    index.byStructuralHash.set(fingerprint.structuralHash, structuralHashList);
  }

  return index;
}

/**
 * Builds a fingerprint index over module bindings so they can go through
 * the same matchFunctions() cascade as functions. Bindings whose init is
 * unhashable (null fingerprint) can never match across versions — they
 * are excluded.
 */
export function buildBindingFingerprintIndex(
  bindings: ModuleBindingNode[]
): FingerprintIndex {
  const index: FingerprintIndex = {
    byStructuralHash: new Map(),
    fingerprints: new Map()
  };

  for (const binding of bindings) {
    if (!binding.fingerprint) continue;
    const fingerprint = buildBindingFullFingerprint(binding);
    index.fingerprints.set(binding.sessionId, fingerprint);

    const list = index.byStructuralHash.get(fingerprint.structuralHash) ?? [];
    list.push(binding.sessionId);
    index.byStructuralHash.set(fingerprint.structuralHash, list);
  }

  return index;
}

function filterByMemberKey(
  candidates: string[],
  oldKey: string | undefined,
  newIndex: FingerprintIndex
): string[] {
  if (oldKey === undefined) return candidates;
  // May return [] — every candidate contradicting the old memberKey is a
  // contradiction the caller must stop on, not fall through.
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    return newFp?.memberKey === oldKey;
  });
}

function filterByCalleeShapes(
  candidates: string[],
  oldShapes: CalleeShape[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return calleeShapesEqual(oldShapes, newFp.calleeShapes ?? []);
  });
}

function filterByCallerShapes(
  candidates: string[],
  oldShapes: CalleeShape[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return calleeShapesEqual(oldShapes, newFp.callerShapes ?? []);
  });
}

function filterByCalleeHashes(
  candidates: string[],
  oldHashes: string[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return arraysEqual(oldHashes, newFp.calleeHashes ?? []);
  });
}

function filterByTwoHopShapes(
  candidates: string[],
  oldShapes: string[],
  newIndex: FingerprintIndex
): string[] {
  return candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    if (!newFp) return false;
    return arraysEqual(oldShapes, newFp.twoHopShapes ?? []);
  });
}

/** Which cascade stage produced a match */
type Resolution =
  | "identity"
  | "memberKey"
  | "calleeShapes"
  | "callerShapes"
  | "calleeHashes"
  | "twoHopShapes"
  | "shingleSimilarity"
  | "ambiguous";

/** Minimum Jaccard similarity to accept a shingling tiebreaker */
const SHINGLE_THRESHOLD = 0.5;

/**
 * Try to resolve ambiguity via shingling: compute Jaccard similarity between
 * the old function's shingle set and each candidate's. Pick the best match
 * if it exceeds the threshold and is clearly better than the runner-up.
 */
function tryShingleResolve(
  oldId: string,
  candidates: string[],
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): string | null {
  const oldFn = oldIndex.functions?.get(oldId);
  if (!oldFn || !newIndex.functions) return null;

  const oldShingles = computeShingleSet(oldFn);
  if (oldShingles.size === 0) return null;

  let bestId: string | null = null;
  let bestSim = -1;
  let secondBestSim = -1;

  for (const candId of candidates) {
    const candFn = newIndex.functions.get(candId);
    if (!candFn) continue;

    const sim = jaccardSimilarity(oldShingles, computeShingleSet(candFn));
    if (sim > bestSim) {
      secondBestSim = bestSim;
      bestSim = sim;
      bestId = candId;
    } else if (sim > secondBestSim) {
      secondBestSim = sim;
    }
  }

  // Accept if above threshold and clearly better than runner-up
  if (bestId && bestSim >= SHINGLE_THRESHOLD && bestSim > secondBestSim) {
    return bestId;
  }
  return null;
}

/**
 * Resolves the best match from candidates using the disambiguation cascade.
 * Returns which cascade stage resolved the match, or "ambiguous".
 */
/**
 * Try calleeHash cascade: exact callee hashes, then two-hop shapes.
 * Returns [matchedId, resolution] or null if still ambiguous.
 */
type CalleeHashOutcome =
  | { kind: "match"; id: string; resolution: "calleeHashes" | "twoHopShapes" }
  /** A stage filtered every candidate out — stop, don't widen. */
  | { kind: "contradiction" }
  /** Narrowed (or unchanged) but still >1 candidates. */
  | { kind: "ambiguous"; pool: string[] };

function tryCalleeHashCascade(
  candidates: string[],
  oldFp: FunctionFingerprint,
  newIndex: FingerprintIndex
): CalleeHashOutcome {
  const calleeHashCandidates = filterByCalleeHashes(
    candidates,
    oldFp.calleeHashes ?? [],
    newIndex
  );

  if (calleeHashCandidates.length === 0) return { kind: "contradiction" };
  if (calleeHashCandidates.length === 1)
    return {
      kind: "match",
      id: calleeHashCandidates[0],
      resolution: "calleeHashes"
    };

  const twoHopCandidates = filterByTwoHopShapes(
    calleeHashCandidates,
    oldFp.twoHopShapes ?? [],
    newIndex
  );
  if (twoHopCandidates.length === 0) return { kind: "contradiction" };
  if (twoHopCandidates.length === 1)
    return {
      kind: "match",
      id: twoHopCandidates[0],
      resolution: "twoHopShapes"
    };

  return { kind: "ambiguous", pool: twoHopCandidates };
}

/** Run the caller-supplied identity resolver; record the match when unique. */
function tryIdentityResolve(
  oldId: string,
  candidates: string[],
  resolver:
    | ((oldId: string, candidates: string[]) => string | null)
    | undefined,
  matches: Map<string, string>
): boolean {
  if (!resolver) return false;
  const resolved = resolver(oldId, candidates);
  if (resolved && candidates.includes(resolved)) {
    matches.set(oldId, resolved);
    return true;
  }
  return false;
}

function resolveMatch(
  oldId: string,
  candidates: string[],
  oldFp: FunctionFingerprint,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  matches: Map<string, string>,
  ambiguous: Map<string, string[]>,
  maxCascadeDepth: number,
  resolveAmbiguousCandidate?: (
    oldId: string,
    candidates: string[]
  ) => string | null
): Resolution {
  // Caller-supplied identity resolution runs first — it carries evidence
  // the fingerprint fields cannot (e.g. correspondence under an existing
  // match result), which stays discriminating even when candidates wrap
  // structurally identical code.
  if (
    tryIdentityResolve(oldId, candidates, resolveAmbiguousCandidate, matches)
  ) {
    return "identity";
  }

  // memberKey disambiguation: runs before callee shapes
  const mkCandidates = filterByMemberKey(candidates, oldFp.memberKey, newIndex);
  if (mkCandidates.length === 0) {
    // Contradiction: every candidate carries a different memberKey than
    // the old function. Stop — a candidate rejected by strong evidence
    // must not win at a weaker stage.
    ambiguous.set(oldId, candidates);
    return "ambiguous";
  }
  if (mkCandidates.length === 1) {
    matches.set(oldId, mkCandidates[0]);
    return "memberKey";
  }

  if (maxCascadeDepth < 1) {
    ambiguous.set(oldId, mkCandidates);
    return "ambiguous";
  }

  // calleeShapes: blurred callee structural shapes
  const calleeShapeCandidates = filterByCalleeShapes(
    mkCandidates,
    oldFp.calleeShapes ?? [],
    newIndex
  );
  if (calleeShapeCandidates.length === 0) {
    ambiguous.set(oldId, mkCandidates);
    return "ambiguous";
  }
  if (calleeShapeCandidates.length === 1) {
    matches.set(oldId, calleeShapeCandidates[0]);
    return "calleeShapes";
  }

  // callerShapes: blurred caller structural shapes (upstream context)
  const callerShapeCandidates = filterByCallerShapes(
    calleeShapeCandidates,
    oldFp.callerShapes ?? [],
    newIndex
  );
  if (callerShapeCandidates.length === 0) {
    ambiguous.set(oldId, calleeShapeCandidates);
    return "ambiguous";
  }
  if (callerShapeCandidates.length === 1) {
    matches.set(oldId, callerShapeCandidates[0]);
    return "callerShapes";
  }

  return resolveDeepStages(
    oldId,
    callerShapeCandidates,
    oldFp,
    oldIndex,
    newIndex,
    matches,
    ambiguous,
    maxCascadeDepth
  );
}

/** Final cascade stages: calleeHashes + twoHopShapes, then shingle tiebreak. */
function resolveDeepStages(
  oldId: string,
  candidates: string[],
  oldFp: FunctionFingerprint,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  matches: Map<string, string>,
  ambiguous: Map<string, string[]>,
  maxCascadeDepth: number
): Resolution {
  let pool = candidates;
  if (maxCascadeDepth >= 2) {
    const outcome = tryCalleeHashCascade(pool, oldFp, newIndex);
    if (outcome.kind === "match") {
      matches.set(oldId, outcome.id);
      return outcome.resolution;
    }
    if (outcome.kind === "contradiction") {
      ambiguous.set(oldId, pool);
      return "ambiguous";
    }
    pool = outcome.pool;
  }

  // shingleSimilarity: Jaccard similarity tiebreaker
  const shingleMatch = tryShingleResolve(oldId, pool, oldIndex, newIndex);
  if (shingleMatch) {
    matches.set(oldId, shingleMatch);
    return "shingleSimilarity";
  }

  // Still ambiguous
  ambiguous.set(oldId, pool);
  return "ambiguous";
}

/**
 * Options for controlling the matching cascade.
 */
export interface MatchOptions {
  /** Maximum cascade depth (0 = uniqueHash + memberKey only, 1 = also calleeShapes + callerShapes, 2 = also calleeHashes + twoHopShapes + shingle). Default: 2 */
  maxCascadeDepth?: 0 | 1 | 2;

  /** SessionIds to exclude from matching (e.g., Bun CJS wrapper functions that always change between versions) */
  excludeSessionIds?: Set<string>;

  /** Enable call-graph propagation to resolve ambiguous functions using confirmed matches as constraints. Default: false */
  enablePropagation?: boolean;

  /**
   * Caller-supplied disambiguator, tried before all fingerprint stages.
   * Given an ambiguous old-side id and its candidates, return the single
   * matching candidate or null. Used to resolve same-hash module bindings
   * by their correspondence under an existing function match result.
   */
  resolveAmbiguousCandidate?: (
    oldId: string,
    candidates: string[]
  ) => string | null;

  /**
   * Matched-binding reference evidence for propagation (see
   * ExternalRefEvidence). Lets a re-match round crack same-hash function
   * buckets whose only identity is which matched module binding they
   * reference. Only consulted when enablePropagation is set.
   */
  externalRefEvidence?: ExternalRefEvidence;
}

/**
 * Matches functions from an old version to a new version using a disambiguation cascade.
 *
 * Matching strategy:
 * 1. Try uniqueHash: exact localHash match
 * 2. If multiple candidates, try memberKey, then calleeShapes + callerShapes
 * 3. If still multiple, try calleeHashes + twoHopShapes + shingleSimilarity
 */
export function matchFunctions(
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  options?: MatchOptions
): MatchResult {
  const maxCascadeDepth = options?.maxCascadeDepth ?? 2;
  const excludeIds = options?.excludeSessionIds;
  const matches = new Map<string, string>();
  const ambiguous = new Map<string, string[]>();
  const unmatched: string[] = [];
  const stats: ResolutionStats = {
    structuralHashUnique: 0,
    identityResolved: 0,
    memberKeyResolved: 0,
    calleeShapesResolved: 0,
    callerShapesResolved: 0,
    calleeHashesResolved: 0,
    twoHopShapesResolved: 0,
    shingleSimilarityResolved: 0,
    injectivityDemoted: 0,
    singletonRejected: 0,
    stillAmbiguous: 0,
    unmatched: 0,
    propagationResolved: 0
  };

  // Resolution stage per matched old id. Stats are accumulated only after
  // injectivity enforcement so demoted matches never count as resolved.
  const resolutions = new Map<string, MatchedResolution>();

  const state: MatchingState = {
    oldIndex,
    newIndex,
    maxCascadeDepth,
    excludeIds,
    resolveAmbiguousCandidate: options?.resolveAmbiguousCandidate,
    matches,
    ambiguous,
    unmatched,
    stats,
    resolutions
  };
  runMatchingPass(state);
  demoteNonInjectiveMatches(state);

  for (const oldId of matches.keys()) {
    const resolution = resolutions.get(oldId);
    if (resolution) stats[RESOLUTION_STAT_KEY[resolution]]++;
  }
  stats.stillAmbiguous = ambiguous.size;

  // Post-pass: call-graph propagation to resolve remaining ambiguity
  if (options?.enablePropagation && ambiguous.size > 0) {
    const { resolved } = propagate(matches, ambiguous, oldIndex, newIndex, {
      externalRefEvidence: options.externalRefEvidence
    });
    stats.propagationResolved = resolved;
    stats.stillAmbiguous -= resolved;
  }

  return { matches, ambiguous, unmatched, resolutionStats: stats };
}

/** Resolution stages that produce a match (everything but "ambiguous"). */
type MatchedResolution =
  | Exclude<Resolution, "ambiguous">
  | "structuralHashUnique";

interface MatchingState {
  oldIndex: FingerprintIndex;
  newIndex: FingerprintIndex;
  maxCascadeDepth: number;
  excludeIds?: Set<string>;
  resolveAmbiguousCandidate?: (
    oldId: string,
    candidates: string[]
  ) => string | null;
  matches: Map<string, string>;
  ambiguous: Map<string, string[]>;
  unmatched: string[];
  stats: ResolutionStats;
  resolutions: Map<string, MatchedResolution>;
}

/** Stats field incremented for each resolution stage. */
const RESOLUTION_STAT_KEY: Record<MatchedResolution, keyof ResolutionStats> = {
  structuralHashUnique: "structuralHashUnique",
  identity: "identityResolved",
  memberKey: "memberKeyResolved",
  calleeShapes: "calleeShapesResolved",
  callerShapes: "callerShapesResolved",
  calleeHashes: "calleeHashesResolved",
  twoHopShapes: "twoHopShapesResolved",
  shingleSimilarity: "shingleSimilarityResolved"
};

/**
 * Contradiction check for zero-corroboration singleton accepts, using only
 * version-stable signals: memberKey (the property key a function is
 * assigned to — hash-external context), propertyAccesses, and
 * externalCalls (known globals / method names — never minified binding
 * names). A signal absent on either side is missing evidence, not an
 * opposing signal; only explicit disagreement rejects.
 */
function singletonContradicts(
  oldFp: FunctionFingerprint,
  newFp: FunctionFingerprint | undefined
): boolean {
  if (!newFp) return false;
  if (
    oldFp.memberKey !== undefined &&
    newFp.memberKey !== undefined &&
    oldFp.memberKey !== newFp.memberKey
  ) {
    return true;
  }
  const oldFeatures = oldFp.features;
  const newFeatures = newFp.features;
  if (!oldFeatures || !newFeatures) return false;
  return (
    !arraysEqual(oldFeatures.propertyAccesses, newFeatures.propertyAccesses) ||
    !arraysEqual(oldFeatures.externalCalls, newFeatures.externalCalls)
  );
}

/** New-side hash-bucket candidates for an old fingerprint, minus exclusions. */
function candidatesForHash(
  structuralHash: string,
  newIndex: FingerprintIndex,
  excludeIds?: Set<string>
): string[] {
  const raw = newIndex.byStructuralHash.get(structuralHash) ?? [];
  return excludeIds ? raw.filter((id) => !excludeIds.has(id)) : raw;
}

/** The per-old-id matching loop: uniqueHash accept or disambiguation cascade. */
function runMatchingPass(state: MatchingState): void {
  for (const [oldId, oldFp] of state.oldIndex.fingerprints) {
    // Skip excluded functions (e.g., Bun CJS wrapper)
    if (state.excludeIds?.has(oldId)) continue;

    const candidates = candidatesForHash(
      oldFp.structuralHash,
      state.newIndex,
      state.excludeIds
    );

    if (candidates.length === 0) {
      state.unmatched.push(oldId);
      state.stats.unmatched++;
      continue;
    }

    if (candidates.length === 1) {
      // A singleton bucket matches with zero cascade corroboration —
      // exactly where a deleted helper and an unrelated added helper
      // auto-match. Reject when a version-stable signal contradicts.
      if (
        singletonContradicts(
          oldFp,
          state.newIndex.fingerprints.get(candidates[0])
        )
      ) {
        state.unmatched.push(oldId);
        state.stats.unmatched++;
        state.stats.singletonRejected++;
        continue;
      }
      state.matches.set(oldId, candidates[0]);
      state.resolutions.set(oldId, "structuralHashUnique");
      continue;
    }

    // Multiple candidates — use disambiguation cascade
    const resolution = resolveMatch(
      oldId,
      candidates,
      oldFp,
      state.oldIndex,
      state.newIndex,
      state.matches,
      state.ambiguous,
      state.maxCascadeDepth,
      state.resolveAmbiguousCandidate
    );
    if (resolution !== "ambiguous") state.resolutions.set(oldId, resolution);
  }
}

/**
 * Enforces injectivity: a new-side function claimed by more than one
 * old-side function is a contradiction — at most one claimant can be the
 * true origin, and picking one by iteration order would silently transfer
 * wrong names. Demote every claimant back to ambiguous with its full
 * hash-bucket candidate list; propagation may later re-resolve claimants
 * that have positive call-graph evidence.
 */
function demoteNonInjectiveMatches(state: MatchingState): void {
  const claimants = new Map<string, string[]>();
  for (const [oldId, newId] of state.matches) {
    const list = claimants.get(newId) ?? [];
    list.push(oldId);
    claimants.set(newId, list);
  }

  for (const [, oldIds] of claimants) {
    if (oldIds.length <= 1) continue;
    for (const oldId of oldIds) {
      state.matches.delete(oldId);
      const oldFp = state.oldIndex.fingerprints.get(oldId);
      const candidates = oldFp
        ? candidatesForHash(
            oldFp.structuralHash,
            state.newIndex,
            state.excludeIds
          )
        : [];
      state.ambiguous.set(oldId, candidates);
      state.stats.injectivityDemoted++;
    }
  }
}

/**
 * Simple array equality check for sorted string arrays.
 */
function arraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((val, i) => val === b[i]);
}

/**
 * Statistics about matching results. Not used by the production pipeline
 * — kept for the experiment harnesses (experiments/012's analyze scripts).
 */
export function getMatchStats(result: MatchResult): {
  matched: number;
  ambiguous: number;
  unmatched: number;
  total: number;
  matchRate: number;
} {
  const matched = result.matches.size;
  const ambiguous = result.ambiguous.size;
  const unmatched = result.unmatched.length;
  const total = matched + ambiguous + unmatched;
  const matchRate = total > 0 ? matched / total : 0;

  return { matched, ambiguous, unmatched, total, matchRate };
}

/**
 * Functions in the new index with no match in the old index — likely
 * added in this version. Not used by the production pipeline — kept for
 * the e2e harness (test/e2e/harness/validate.ts).
 */
export function findNewFunctions(
  _oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  matchResult: MatchResult
): string[] {
  const matchedNewIds = new Set(matchResult.matches.values());
  for (const candidates of matchResult.ambiguous.values()) {
    for (const c of candidates) {
      matchedNewIds.add(c);
    }
  }

  const newFunctions: string[] = [];
  for (const newId of newIndex.fingerprints.keys()) {
    if (!matchedNewIds.has(newId)) {
      newFunctions.push(newId);
    }
  }

  return newFunctions;
}
