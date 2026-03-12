import {
  buildFullFingerprint,
  calleeShapesEqual,
  makeResolution1Key
} from "./function-fingerprint.js";
import type { FingerprintIndex, FunctionNode, MatchResult } from "./types.js";

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
    fingerprints: new Map()
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

/**
 * Matches functions from an old version to a new version using multi-resolution matching.
 *
 * Matching strategy:
 * 1. Try Resolution 0: exact localHash match
 * 2. If multiple candidates, try Resolution 1: blurred callee shapes
 * 3. If still multiple, try Resolution 2: exact callee hashes
 */
export function matchFunctions(
  oldIndex: FingerprintIndex,
  newIndex: FingerprintIndex
): MatchResult {
  const matches = new Map<string, string>();
  const ambiguous = new Map<string, string[]>();
  const unmatched: string[] = [];

  for (const [oldId, oldFp] of oldIndex.fingerprints) {
    // Try Resolution 0: exact localHash match
    const candidates = newIndex.byExactHash.get(oldFp.exactHash) ?? [];

    if (candidates.length === 0) {
      unmatched.push(oldId);
      continue;
    }

    if (candidates.length === 1) {
      matches.set(oldId, candidates[0]);
      continue;
    }

    // Multiple candidates - try Resolution 1: blurred callee shapes
    const r1Candidates = candidates.filter((newId) => {
      const newFp = newIndex.fingerprints.get(newId)!;
      return calleeShapesEqual(
        oldFp.calleeShapes ?? [],
        newFp.calleeShapes ?? []
      );
    });

    if (r1Candidates.length === 1) {
      matches.set(oldId, r1Candidates[0]);
      continue;
    }

    if (r1Candidates.length > 1) {
      // Try Resolution 2: exact callee hashes
      const r2Candidates = r1Candidates.filter((newId) => {
        const newFp = newIndex.fingerprints.get(newId)!;
        return arraysEqual(oldFp.calleeHashes ?? [], newFp.calleeHashes ?? []);
      });

      if (r2Candidates.length === 1) {
        matches.set(oldId, r2Candidates[0]);
        continue;
      }

      // Try two-hop shapes as final disambiguation
      if (r2Candidates.length > 1) {
        const r2bCandidates = r2Candidates.filter((newId) => {
          const newFp = newIndex.fingerprints.get(newId)!;
          return arraysEqual(
            oldFp.twoHopShapes ?? [],
            newFp.twoHopShapes ?? []
          );
        });

        if (r2bCandidates.length === 1) {
          matches.set(oldId, r2bCandidates[0]);
          continue;
        }
      }
    }

    // Still ambiguous - record candidates for manual review
    const finalCandidates = r1Candidates.length > 0 ? r1Candidates : candidates;
    ambiguous.set(oldId, finalCandidates);
  }

  return { matches, ambiguous, unmatched };
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
function createNameCache(
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
