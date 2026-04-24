import {
  buildFullFingerprint,
  calleeShapesEqual,
  computeShingleSet,
  jaccardSimilarity,
  makeCalleeShapeKey
} from "./function-fingerprint.js";
import { propagate } from "./propagation.js";
import type {
  CalleeShape,
  FingerprintIndex,
  FunctionFingerprint,
  FunctionNode,
  MatchResult,
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
    byCalleeShapeKey: new Map(),
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

    // Index by calleeShapeKey (structuralHash + calleeShapes)
    const calleeShapeKey = makeCalleeShapeKey(fingerprint);
    const calleeShapeList = index.byCalleeShapeKey.get(calleeShapeKey) ?? [];
    calleeShapeList.push(sessionId);
    index.byCalleeShapeKey.set(calleeShapeKey, calleeShapeList);
  }

  return index;
}

function filterByMemberKey(
  candidates: string[],
  oldKey: string | undefined,
  newIndex: FingerprintIndex
): string[] {
  if (oldKey === undefined) return candidates;
  const filtered = candidates.filter((newId) => {
    const newFp = newIndex.fingerprints.get(newId);
    return newFp?.memberKey === oldKey;
  });
  // If filter yields 0, fall through with original candidates
  return filtered.length > 0 ? filtered : candidates;
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
function tryCalleeHashCascade(
  candidates: string[],
  oldFp: FunctionFingerprint,
  newIndex: FingerprintIndex
): [string, "calleeHashes" | "twoHopShapes"] | null {
  const calleeHashCandidates = filterByCalleeHashes(
    candidates,
    oldFp.calleeHashes ?? [],
    newIndex
  );

  if (calleeHashCandidates.length === 1)
    return [calleeHashCandidates[0], "calleeHashes"];

  if (calleeHashCandidates.length > 1) {
    const twoHopCandidates = filterByTwoHopShapes(
      calleeHashCandidates,
      oldFp.twoHopShapes ?? [],
      newIndex
    );
    if (twoHopCandidates.length === 1)
      return [twoHopCandidates[0], "twoHopShapes"];
  }

  return null;
}

function resolveMatch(
  oldId: string,
  candidates: string[],
  oldFp: FunctionFingerprint,
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  matches: Map<string, string>,
  ambiguous: Map<string, string[]>,
  maxCascadeDepth: number
): Resolution {
  // memberKey disambiguation: runs before callee shapes
  const mkCandidates = filterByMemberKey(candidates, oldFp.memberKey, newIndex);
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

  if (calleeShapeCandidates.length === 1) {
    matches.set(oldId, calleeShapeCandidates[0]);
    return "calleeShapes";
  }

  // callerShapes: blurred caller structural shapes (upstream context)
  const callerShapeInput =
    calleeShapeCandidates.length > 0 ? calleeShapeCandidates : candidates;
  const callerShapeCandidates = filterByCallerShapes(
    callerShapeInput,
    oldFp.callerShapes ?? [],
    newIndex
  );

  if (callerShapeCandidates.length === 1) {
    matches.set(oldId, callerShapeCandidates[0]);
    return "callerShapes";
  }

  // calleeHashes + twoHopShapes cascade
  const calleeHashInput =
    callerShapeCandidates.length > 0 ? callerShapeCandidates : callerShapeInput;
  if (calleeHashInput.length > 1 && maxCascadeDepth >= 2) {
    const calleeHashResult = tryCalleeHashCascade(
      calleeHashInput,
      oldFp,
      newIndex
    );
    if (calleeHashResult) {
      matches.set(oldId, calleeHashResult[0]);
      return calleeHashResult[1];
    }
  }

  // shingleSimilarity: Jaccard similarity tiebreaker
  const shingleCandidates =
    calleeHashInput.length > 0 ? calleeHashInput : candidates;
  const shingleMatch = tryShingleResolve(
    oldId,
    shingleCandidates,
    oldIndex,
    newIndex
  );
  if (shingleMatch) {
    matches.set(oldId, shingleMatch);
    return "shingleSimilarity";
  }

  // Still ambiguous
  ambiguous.set(oldId, shingleCandidates);
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
    memberKeyResolved: 0,
    calleeShapesResolved: 0,
    callerShapesResolved: 0,
    calleeHashesResolved: 0,
    twoHopShapesResolved: 0,
    shingleSimilarityResolved: 0,
    stillAmbiguous: 0,
    unmatched: 0,
    propagationResolved: 0
  };

  for (const [oldId, oldFp] of oldIndex.fingerprints) {
    // Skip excluded functions (e.g., Bun CJS wrapper)
    if (excludeIds?.has(oldId)) continue;

    // uniqueHash: exact localHash match
    // Filter out excluded new-side candidates too
    const rawCandidates =
      newIndex.byStructuralHash.get(oldFp.structuralHash) ?? [];
    const candidates = excludeIds
      ? rawCandidates.filter((id) => !excludeIds.has(id))
      : rawCandidates;

    if (candidates.length === 0) {
      unmatched.push(oldId);
      stats.unmatched++;
      continue;
    }

    if (candidates.length === 1) {
      matches.set(oldId, candidates[0]);
      stats.structuralHashUnique++;
      continue;
    }

    // Multiple candidates — use disambiguation cascade
    const resolution = resolveMatch(
      oldId,
      candidates,
      oldFp,
      oldIndex,
      newIndex,
      matches,
      ambiguous,
      maxCascadeDepth
    );
    switch (resolution) {
      case "memberKey":
        stats.memberKeyResolved++;
        break;
      case "calleeShapes":
        stats.calleeShapesResolved++;
        break;
      case "callerShapes":
        stats.callerShapesResolved++;
        break;
      case "calleeHashes":
        stats.calleeHashesResolved++;
        break;
      case "twoHopShapes":
        stats.twoHopShapesResolved++;
        break;
      case "shingleSimilarity":
        stats.shingleSimilarityResolved++;
        break;
      case "ambiguous":
        stats.stillAmbiguous++;
        break;
    }
  }

  // Post-pass: call-graph propagation to resolve remaining ambiguity
  if (options?.enablePropagation && ambiguous.size > 0) {
    const { resolved } = propagate(matches, ambiguous, oldIndex, newIndex);
    stats.propagationResolved = resolved;
    stats.stillAmbiguous -= resolved;
  }

  return { matches, ambiguous, unmatched, resolutionStats: stats };
}

/**
 * Simple array equality check for sorted string arrays.
 */
function arraysEqual(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((val, i) => val === b[i]);
}

/**
 * Gets statistics about matching results.
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
 * Finds functions in the new index that have no match in the old index.
 * These are likely new functions added in this version.
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

/**
 * Applies cached names to functions in a new version using match results.
 */
export function applyCachedNames(
  matchResult: MatchResult,
  oldFunctions: Map<string, FunctionNode>,
  newFunctions: Map<string, FunctionNode>
): number {
  let applied = 0;

  for (const [oldId, newId] of matchResult.matches) {
    const oldFn = oldFunctions.get(oldId);
    const newFn = newFunctions.get(newId);

    if (oldFn?.renameMapping?.names && newFn) {
      newFn.renameMapping = { ...oldFn.renameMapping };
      applied++;
    }
  }

  return applied;
}
