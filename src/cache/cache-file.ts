import fs from "node:fs";
import type * as t from "@babel/types";
import type { FunctionFingerprint, FunctionNode } from "../analysis/types.js";
import {
  computeBindingFingerprint,
  invertPlaceholderMapping
} from "../analysis/structural-hash.js";

/**
 * On-disk cache format for cross-version rename reuse.
 */
export interface HumanifyCache {
  version: 1;
  sourceFile: string;
  createdAt: string;
  functions: CachedFunction[];
  moduleBindings?: CachedModuleBinding[];
}

/**
 * A cached module-level binding with its content hash and placeholder-keyed
 * name mapping for cross-version reuse.
 */
export interface CachedModuleBinding {
  /** Hash of normalized init/first-assignment AST */
  contentHash: string;
  /** Placeholder → humanified name (including $binding for the var name) */
  nameMapping: Record<string, string>;
  /** Position within the parent var declaration (for same-hash disambiguation) */
  declarationIndex: number;
  /** Whether hash came from init or first assignment */
  hashSource: "init" | "assignment";
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

/**
 * Input for caching a single module binding.
 * The caller is responsible for resolving the Babel binding to extract
 * the declarator, first-assignment RHS, and the rename mapping.
 */
export interface ModuleBindingCacheInput {
  /** The binding's minified name */
  name: string;
  /** The VariableDeclarator node (for init) */
  declarator: t.VariableDeclarator;
  /** RHS of the first assignment (for bare `var a; a = expr;` patterns) */
  firstAssignmentRHS?: t.Expression | null;
  /** Index of this declarator within its parent VariableDeclaration */
  declarationIndex: number;
  /** The humanified name this binding was renamed to */
  humanifiedName: string;
}

/** Build a single CachedModuleBinding entry, or null if unhashable. */
function buildCachedModuleBinding(
  input: ModuleBindingCacheInput
): CachedModuleBinding | null {
  const fp = computeBindingFingerprint(
    input.declarator.init,
    input.firstAssignmentRHS
  );
  if (!fp) return null;

  // For v1, we only cache the binding name itself (the var).
  // Internal identifiers within the init expression are handled by function caching.
  const nameMapping: Record<string, string> = {
    $binding: input.humanifiedName
  };

  return {
    contentHash: fp.contentHash,
    nameMapping,
    declarationIndex: input.declarationIndex,
    hashSource: fp.hashSource
  };
}

export function buildCache(
  functions: Map<string, FunctionNode>,
  sourceFile: string,
  moduleBindingInputs?: ModuleBindingCacheInput[]
): HumanifyCache {
  const cached: CachedFunction[] = [];

  for (const fn of functions.values()) {
    const entry = buildCachedFunction(fn);
    if (entry) cached.push(entry);
  }

  const moduleBindings: CachedModuleBinding[] = [];
  if (moduleBindingInputs) {
    for (const input of moduleBindingInputs) {
      const entry = buildCachedModuleBinding(input);
      if (entry) moduleBindings.push(entry);
    }
  }

  return {
    version: 1,
    sourceFile,
    createdAt: new Date().toISOString(),
    functions: cached,
    moduleBindings: moduleBindings.length > 0 ? moduleBindings : undefined
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
