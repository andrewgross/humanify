import {
  buildFingerprintIndex,
  matchFunctions
} from "../analysis/fingerprint-index.js";
import { makeCalleeShapeKey } from "../analysis/function-fingerprint.js";
import { buildPlaceholderMapping } from "../analysis/structural-hash.js";
import type {
  FingerprintIndex,
  FunctionNode,
  MatchResult
} from "../analysis/types.js";
import type { CachedFunction, HumanifyCache } from "./cache-file.js";

/**
 * Restores cached rename mappings onto current functions by matching
 * them against cached fingerprints and translating placeholder-keyed
 * names to the new minified names.
 *
 * @returns Count of applied caches and the raw match result for stats.
 */
export function restoreFromCache(
  cache: HumanifyCache,
  currentFunctions: Map<string, FunctionNode>
): { applied: number; matchResult: MatchResult } {
  if (cache.functions.length === 0) {
    return {
      applied: 0,
      matchResult: {
        matches: new Map(),
        ambiguous: new Map(),
        unmatched: [],
        resolutionStats: {
          exactHashUnique: 0,
          memberKeyResolved: 0,
          calleeShapesResolved: 0,
          callerShapesResolved: 0,
          calleeHashesResolved: 0,
          twoHopShapesResolved: 0,
          shingleSimilarityResolved: 0,
          stillAmbiguous: 0,
          unmatched: 0,
          propagationResolved: 0
        }
      }
    };
  }

  // Build lightweight FunctionNode stubs from cache entries
  const cachedFunctions = buildCacheStubs(cache.functions);
  // Use stored fingerprints for old index (stubs have no AST paths)
  const oldIndex = buildCacheIndex(cache.functions, cachedFunctions);
  const newIndex = buildFingerprintIndex(currentFunctions);

  // Run the full matching cascade with propagation
  const matchResult = matchFunctions(oldIndex, newIndex, {
    enablePropagation: true
  });

  // Apply cached names to matched functions
  let applied = 0;
  const cachedBySessionId = new Map(
    cache.functions.map((f) => [f.sessionId, f])
  );

  for (const [oldId, newId] of matchResult.matches) {
    const cachedFn = cachedBySessionId.get(oldId);
    const newFn = currentFunctions.get(newId);
    if (!cachedFn || !newFn) continue;

    const translated = translatePlaceholderNames(
      cachedFn.renameMapping.names,
      newFn
    );
    if (!translated) continue;

    newFn.renameMapping = {
      names: translated,
      model: cachedFn.renameMapping.model
    };
    applied++;
  }

  return { applied, matchResult };
}

/**
 * Translates placeholder-keyed names to new minified names using
 * the new function's placeholder mapping.
 */
function translatePlaceholderNames(
  placeholderNames: Record<string, string>,
  newFn: FunctionNode
): Record<string, string> | null {
  // Build $N → newMinifiedName mapping
  const placeholderMap = buildPlaceholderMapping(newFn.path.node);

  const translated: Record<string, string> = {};
  let count = 0;

  for (const [placeholder, humanName] of Object.entries(placeholderNames)) {
    const newMinifiedName = placeholderMap.get(placeholder);
    if (newMinifiedName) {
      translated[newMinifiedName] = humanName;
      count++;
    }
  }

  return count > 0 ? translated : null;
}

/**
 * Builds a FingerprintIndex from cached entries using their stored fingerprints.
 * Unlike buildFingerprintIndex, this doesn't recompute fingerprints from AST paths.
 */
function buildCacheIndex(
  cached: CachedFunction[],
  stubs: Map<string, FunctionNode>
): FingerprintIndex {
  const index: FingerprintIndex = {
    byExactHash: new Map(),
    byCalleeShapeKey: new Map(),
    fingerprints: new Map(),
    functions: stubs
  };

  for (const entry of cached) {
    const fp = entry.fingerprint;
    index.fingerprints.set(entry.sessionId, fp);

    const exactHashList = index.byExactHash.get(fp.exactHash) ?? [];
    exactHashList.push(entry.sessionId);
    index.byExactHash.set(fp.exactHash, exactHashList);

    const calleeShapeKey = makeCalleeShapeKey(fp);
    const calleeShapeList = index.byCalleeShapeKey.get(calleeShapeKey) ?? [];
    calleeShapeList.push(entry.sessionId);
    index.byCalleeShapeKey.set(calleeShapeKey, calleeShapeList);
  }

  return index;
}

/**
 * Builds lightweight FunctionNode stubs from cached entries.
 * These have fingerprints and topology but no AST paths — sufficient
 * for the matching cascade and propagation.
 */
function buildCacheStubs(cached: CachedFunction[]): Map<string, FunctionNode> {
  const stubs = new Map<string, FunctionNode>();

  // First pass: create stub nodes
  for (const entry of cached) {
    const stub: FunctionNode = {
      sessionId: entry.sessionId,
      fingerprint: entry.fingerprint,
      path: null as unknown as FunctionNode["path"], // stubs don't need AST paths
      internalCallees: new Set(),
      externalCallees: new Set(),
      callers: new Set(),
      status: "done",
      callSites: []
    };
    stubs.set(entry.sessionId, stub);
  }

  // Second pass: wire topology
  for (const entry of cached) {
    const stub = stubs.get(entry.sessionId);
    if (stub) wireStubTopology(stub, entry, stubs);
  }

  return stubs;
}

/** Wire scopeParent, callees, and callers for a single stub from cache data. */
function wireStubTopology(
  stub: FunctionNode,
  entry: CachedFunction,
  stubs: Map<string, FunctionNode>
): void {
  if (entry.scopeParentId) {
    const parent = stubs.get(entry.scopeParentId);
    if (parent) stub.scopeParent = parent;
  }

  if (entry.calleeIds) {
    for (const calleeId of entry.calleeIds) {
      const callee = stubs.get(calleeId);
      if (callee) stub.internalCallees.add(callee);
    }
  }

  if (entry.callerIds) {
    for (const callerId of entry.callerIds) {
      const caller = stubs.get(callerId);
      if (caller) stub.callers.add(caller);
    }
  }
}
