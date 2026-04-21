import fs from "node:fs";
import type { FunctionFingerprint, FunctionNode } from "../analysis/types.js";
import { invertPlaceholderMapping } from "../analysis/structural-hash.js";

/**
 * On-disk cache format for cross-version rename reuse.
 */
export interface HumanifyCache {
  version: 1;
  sourceFile: string;
  createdAt: string;
  functions: CachedFunction[];
}

/**
 * A single cached function with its fingerprint, placeholder-keyed rename
 * mapping, and topology references for propagation reconstruction.
 */
export interface CachedFunction {
  fingerprint: FunctionFingerprint;
  renameMapping: {
    names: Record<string, string>; // placeholder-keyed: { "$0": "getUser", ... }
    model?: string;
  };
  sessionId: string;
  scopeParentId?: string;
  calleeIds?: string[];
  callerIds?: string[];
}

/**
 * Builds a cache from the completed function map.
 * Translates minified-name-keyed rename mappings to placeholder-keyed mappings.
 */
/** Translate a single function's rename mapping to placeholder keys. */
function buildCachedFunction(fn: FunctionNode): CachedFunction | null {
  if (!fn.renameMapping?.names) return null;
  if (Object.keys(fn.renameMapping.names).length === 0) return null;
  if (!fn.placeholderMapping) return null;

  const nameToPlaceholder = invertPlaceholderMapping(fn.placeholderMapping);

  const placeholderNames: Record<string, string> = {};
  for (const [minifiedName, humanName] of Object.entries(
    fn.renameMapping.names
  )) {
    const placeholder = nameToPlaceholder.get(minifiedName);
    if (placeholder) {
      placeholderNames[placeholder] = humanName;
    }
  }

  return {
    fingerprint: fn.fingerprint,
    renameMapping: {
      names: placeholderNames,
      model: fn.renameMapping.model
    },
    sessionId: fn.sessionId,
    scopeParentId: fn.scopeParent?.sessionId,
    calleeIds:
      fn.internalCallees.size > 0
        ? [...fn.internalCallees].map((c) => c.sessionId)
        : undefined,
    callerIds:
      fn.callers.size > 0 ? [...fn.callers].map((c) => c.sessionId) : undefined
  };
}

export function buildCache(
  functions: Map<string, FunctionNode>,
  sourceFile: string
): HumanifyCache {
  const cached: CachedFunction[] = [];

  for (const fn of functions.values()) {
    const entry = buildCachedFunction(fn);
    if (entry) cached.push(entry);
  }

  return {
    version: 1,
    sourceFile,
    createdAt: new Date().toISOString(),
    functions: cached
  };
}

/**
 * Writes a cache to disk as JSON.
 */
export function writeCache(cache: HumanifyCache, filePath: string): void {
  fs.writeFileSync(filePath, JSON.stringify(cache, null, 2));
}

/**
 * Reads a cache from disk. Returns null if the file doesn't exist.
 */
export function readCache(filePath: string): HumanifyCache | null {
  try {
    const data = fs.readFileSync(filePath, "utf-8");
    return JSON.parse(data) as HumanifyCache;
  } catch {
    return null;
  }
}
