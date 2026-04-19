import {
  buildFullFingerprint,
  calleeShapesEqual,
  computeShingleSet,
  jaccardSimilarity,
  makeResolution1Key
} from "./function-fingerprint.js";
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
    byExactHash: new Map(),
    byResolution1: new Map(),
    fingerprints: new Map(),
    functions
  };

  for (const [sessionId, fn] of functions) {
    // Build full fingerprint with callee information
    const fingerprint = buildFullFingerprint(fn, functions);
    index.fingerprints.set(sessionId, fingerprint);

    // Index by exactHash (Resolution 0)
    const exactHashList = index.byExactHash.get(fingerprint.exactHash) ?? [];
    exactHashList.push(sessionId);
    index.byExactHash.set(fingerprint.exactHash, exactHashList);

    // Index by Resolution 1 key (exactHash + calleeShapes)
    const r1Key = makeResolution1Key(fingerprint);
    const r1List = index.byResolution1.get(r1Key) ?? [];
    r1List.push(sessionId);
    index.byResolution1.set(r1Key, r1List);
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

/** Which resolution level produced a match */
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
 * Resolves the best match from candidates using multi-resolution disambiguation.
 * Returns which resolution level resolved the match, or "ambiguous".
 */
/**
 * Try R2 cascade: exact callee hashes, then two-hop shapes.
 * Returns [matchedId, resolution] or null if still ambiguous.
 */
function tryR2Cascade(
  r1Candidates: string[],
  oldFp: FunctionFingerprint,
  newIndex: FingerprintIndex
): [string, "calleeHashes" | "twoHopShapes"] | null {
  const r2Candidates = filterByCalleeHashes(
    r1Candidates,
    oldFp.calleeHashes ?? [],
    newIndex
  );

  if (r2Candidates.length === 1) return [r2Candidates[0], "calleeHashes"];

  if (r2Candidates.length > 1) {
    const r2bCandidates = filterByTwoHopShapes(
      r2Candidates,
      oldFp.twoHopShapes ?? [],
      newIndex
    );
    if (r2bCandidates.length === 1) return [r2bCandidates[0], "twoHopShapes"];
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
  maxResolution: number
): Resolution {
  // memberKey disambiguation: runs before callee shapes
  const mkCandidates = filterByMemberKey(candidates, oldFp.memberKey, newIndex);
  if (mkCandidates.length === 1) {
    matches.set(oldId, mkCandidates[0]);
    return "memberKey";
  }

  if (maxResolution < 1) {
    ambiguous.set(oldId, mkCandidates);
    return "ambiguous";
  }

  // Resolution 1: blurred callee shapes
  const r1Candidates = filterByCalleeShapes(
    mkCandidates,
    oldFp.calleeShapes ?? [],
    newIndex
  );

  if (r1Candidates.length === 1) {
    matches.set(oldId, r1Candidates[0]);
    return "calleeShapes";
  }

  // Resolution 1b: blurred caller shapes (upstream context)
  const r1bInput = r1Candidates.length > 0 ? r1Candidates : candidates;
  const r1bCandidates = filterByCallerShapes(
    r1bInput,
    oldFp.callerShapes ?? [],
    newIndex
  );

  if (r1bCandidates.length === 1) {
    matches.set(oldId, r1bCandidates[0]);
    return "callerShapes";
  }

  // Resolution 2 + 2b cascade
  const r2Input = r1bCandidates.length > 0 ? r1bCandidates : r1bInput;
  if (r2Input.length > 1 && maxResolution >= 2) {
    const r2Result = tryR2Cascade(r2Input, oldFp, newIndex);
    if (r2Result) {
      matches.set(oldId, r2Result[0]);
      return r2Result[1];
    }
  }

  // Shingling fallback: Jaccard similarity tiebreaker
  const shingleCandidates = r2Input.length > 0 ? r2Input : candidates;
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
  /** Maximum resolution level to use (0 = exactHash only, 1 = +callee shapes, 2 = +callee hashes + two-hop). Default: 2 */
  maxResolution?: 0 | 1 | 2;

  /** SessionIds to exclude from matching (e.g., Bun CJS wrapper functions that always change between versions) */
  excludeSessionIds?: Set<string>;
}

/**
 * Matches functions from an old version to a new version using multi-resolution matching.
 *
 * Matching strategy:
 * 1. Try Resolution 0: exact localHash match
 * 2. If multiple candidates, try Resolution 1: blurred callee shapes
 * 3. If still multiple, try Resolution 2: exact callee hashes + two-hop shapes
 */
export function matchFunctions(
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  options?: MatchOptions
): MatchResult {
  const maxResolution = options?.maxResolution ?? 2;
  const excludeIds = options?.excludeSessionIds;
  const matches = new Map<string, string>();
  const ambiguous = new Map<string, string[]>();
  const unmatched: string[] = [];
  const stats: ResolutionStats = {
    exactHashUnique: 0,
    memberKeyResolved: 0,
    calleeShapesResolved: 0,
    callerShapesResolved: 0,
    calleeHashesResolved: 0,
    twoHopShapesResolved: 0,
    shingleSimilarityResolved: 0,
    stillAmbiguous: 0,
    unmatched: 0
  };

  for (const [oldId, oldFp] of oldIndex.fingerprints) {
    // Skip excluded functions (e.g., Bun CJS wrapper)
    if (excludeIds?.has(oldId)) continue;

    // Try Resolution 0: exact localHash match
    // Filter out excluded new-side candidates too
    const rawCandidates = newIndex.byExactHash.get(oldFp.exactHash) ?? [];
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
      stats.exactHashUnique++;
      continue;
    }

    // Multiple candidates — use resolution cascade
    const resolution = resolveMatch(
      oldId,
      candidates,
      oldFp,
      oldIndex,
      newIndex,
      matches,
      ambiguous,
      maxResolution
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
 * Creates a mapping from old fingerprint exactHash to humanified names.
 * This can be used to apply cached renames to new versions.
 */
function _createNameCache(
  functions: Map<string, FunctionNode>
): Map<string, Record<string, string>> {
  const cache = new Map<string, Record<string, string>>();

  for (const fn of functions.values()) {
    if (fn.renameMapping?.names) {
      cache.set(fn.fingerprint.exactHash, fn.renameMapping.names);
    }
  }

  return cache;
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
