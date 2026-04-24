/**
 * Prior-version matching for cross-version rename reuse.
 *
 * Instead of persisting a cache file, this module takes the prior humanified
 * code directly and matches functions between versions using the same
 * structural fingerprinting and disambiguation cascade used elsewhere.
 *
 * The prior version's humanified names are transferred to matched functions
 * in the new version via placeholder mapping translation.
 */

import { parseSync } from "@babel/core";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import {
  buildFingerprintIndex,
  matchFunctions
} from "../analysis/fingerprint-index.js";
import { findCloseMatches } from "../analysis/close-match.js";
import { buildPlaceholderMapping } from "../analysis/structural-hash.js";
import type { FunctionNode, MatchResult } from "../analysis/types.js";
import { generate } from "../babel-utils.js";

export interface CloseMatchInfo {
  /** Prior humanified code (for LLM context) */
  priorCode: string;
  /** Partial name transfers: minified name → humanified name (function name + params) */
  nameTransfers: Record<string, string>;
}

export interface PriorVersionResult {
  matchResult: MatchResult;
  /** Functions matched AND renames transferred (actual LLM calls saved) */
  functionsMatched: number;
  /** Functions matched but all identifiers were already identical (e.g., exports, property keys) */
  functionsAlreadyNamed: number;
  /** Close matches: newSessionId → close match info (prior code + partial name transfers) */
  closeMatchContext: Map<string, CloseMatchInfo>;
  closeMatchCount: number;
  moduleBindingsMatched: number;
}

/**
 * Matches functions between a prior humanified version and the current
 * minified version, transferring names to matched functions.
 *
 * @param priorCode The prior version's humanified output code
 * @param newFunctions The current version's function map (mutated: renameMapping set on matches)
 * @returns Match statistics
 */
export function matchPriorVersion(
  priorCode: string,
  newFunctions: Map<string, FunctionNode>
): PriorVersionResult {
  const emptyResult: PriorVersionResult = {
    matchResult: {
      matches: new Map(),
      ambiguous: new Map(),
      unmatched: [],
      resolutionStats: {
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
      }
    },
    functionsMatched: 0,
    functionsAlreadyNamed: 0,
    closeMatchContext: new Map(),
    closeMatchCount: 0,
    moduleBindingsMatched: 0
  };

  if (!priorCode || newFunctions.size === 0) return emptyResult;

  // Parse prior version and build its function graph
  const priorAst = parseSync(priorCode, { sourceType: "unambiguous" });
  if (!priorAst) return emptyResult;

  const priorFunctions = buildFunctionGraph(priorAst, "prior.js");
  if (priorFunctions.length === 0) return emptyResult;

  // Build function maps and fingerprint indices
  const priorFnMap = new Map<string, FunctionNode>();
  for (const fn of priorFunctions) {
    priorFnMap.set(fn.sessionId, fn);
  }

  const priorIndex = buildFingerprintIndex(priorFnMap);
  const newIndex = buildFingerprintIndex(newFunctions);

  // Run matching cascade with propagation
  const matchResult = matchFunctions(priorIndex, newIndex, {
    enablePropagation: true
  });

  const { functionsMatched, functionsAlreadyNamed } = applyExactMatches(
    matchResult,
    priorFnMap,
    newFunctions
  );

  const closeMatchContext = buildCloseMatchContext(
    matchResult,
    priorFnMap,
    newFunctions,
    priorIndex,
    newIndex
  );

  return {
    matchResult,
    functionsMatched,
    functionsAlreadyNamed,
    closeMatchContext,
    closeMatchCount: closeMatchContext.size,
    moduleBindingsMatched: 0 // TODO: module binding matching
  };
}

/** Apply exact-match renames via placeholder mapping translation. */
function applyExactMatches(
  matchResult: import("../analysis/types.js").MatchResult,
  priorFnMap: Map<string, FunctionNode>,
  newFunctions: Map<string, FunctionNode>
): { functionsMatched: number; functionsAlreadyNamed: number } {
  let functionsMatched = 0;
  let functionsAlreadyNamed = 0;

  for (const [priorId, newId] of matchResult.matches) {
    const priorFn = priorFnMap.get(priorId);
    const newFn = newFunctions.get(newId);
    if (!priorFn || !newFn) continue;

    const translated = translatePriorNames(priorFn, newFn);
    if (translated) {
      newFn.renameMapping = { names: translated };
      functionsMatched++;
    } else {
      newFn.renameMapping = { names: {} };
      functionsAlreadyNamed++;
    }
  }

  return { functionsMatched, functionsAlreadyNamed };
}

/** Find close matches among unmatched remainders and generate prior code context. */
function buildCloseMatchContext(
  matchResult: import("../analysis/types.js").MatchResult,
  priorFnMap: Map<string, FunctionNode>,
  newFunctions: Map<string, FunctionNode>,
  priorIndex: import("../analysis/types.js").FingerprintIndex,
  newIndex: import("../analysis/types.js").FingerprintIndex
): Map<string, CloseMatchInfo> {
  const matchedNewIds = new Set(matchResult.matches.values());
  const matchedPriorIds = new Set(matchResult.matches.keys());
  const unmatchedPrior = [...priorFnMap.keys()].filter(
    (id) => !matchedPriorIds.has(id)
  );
  const unmatchedNew = [...newFunctions.keys()].filter(
    (id) => !matchedNewIds.has(id)
  );

  const context = new Map<string, CloseMatchInfo>();
  if (unmatchedPrior.length === 0 || unmatchedNew.length === 0) return context;

  const { closeMatches } = findCloseMatches(
    unmatchedPrior,
    unmatchedNew,
    priorIndex,
    newIndex
  );

  for (const [priorId, newId] of closeMatches) {
    const priorFn = priorFnMap.get(priorId);
    const newFn = newFunctions.get(newId);
    if (!priorFn || !newFn) continue;
    try {
      const priorCode = generate(priorFn.path.node).code;
      const nameTransfers = computePartialTransfer(priorFn, newFn);
      context.set(newId, { priorCode, nameTransfers });
    } catch {
      // Skip if code generation fails
    }
  }

  return context;
}

/**
 * Computes partial name transfers for close-matched functions.
 *
 * For functions with different structural hashes, placeholder positions
 * are only reliable for the function name ($0) and parameters ($1..$arity).
 * Body locals can shift when statements are added/removed, so we skip them.
 *
 * Returns a mapping of { minifiedName → humanifiedName } for safe transfers.
 */
function computePartialTransfer(
  priorFn: FunctionNode,
  newFn: FunctionNode
): Record<string, string> {
  const priorPlaceholders = buildPlaceholderMapping(priorFn.path.node);
  const newPlaceholders = buildPlaceholderMapping(newFn.path.node);

  // Function name is $0, params are $1..$arity
  // Only transfer these — body locals ($arity+1, ...) may not align
  const priorArity = priorFn.path.node.params.length;
  const newArity = newFn.path.node.params.length;
  const safeCount = 1 + Math.min(priorArity, newArity); // $0 + min params

  const transfers: Record<string, string> = {};
  for (let i = 0; i < safeCount; i++) {
    const placeholder = `$${i}`;
    const priorName = priorPlaceholders.get(placeholder);
    const newMinifiedName = newPlaceholders.get(placeholder);
    if (priorName && newMinifiedName && priorName !== newMinifiedName) {
      transfers[newMinifiedName] = priorName;
    }
  }

  return transfers;
}

/**
 * Translates names from a prior humanified function to a new minified function.
 *
 * Both functions have the same structural hash, meaning they have the same
 * AST structure with different identifier names. We use placeholder mapping
 * to bridge: prior's identifiers → placeholders → new's identifiers.
 *
 * Prior: function getUser(userId) { return userId; }
 *   placeholders: $0→getUser, $1→userId
 * New:   function x(y) { return y; }
 *   placeholders: $0→x, $1→y
 * Result: x→getUser, y→userId
 */
function translatePriorNames(
  priorFn: FunctionNode,
  newFn: FunctionNode
): Record<string, string> | null {
  // Build placeholder mappings for both functions
  const priorPlaceholders = buildPlaceholderMapping(priorFn.path.node);
  const newPlaceholders = buildPlaceholderMapping(newFn.path.node);

  // priorPlaceholders: $0→"getUser", $1→"userId"
  // newPlaceholders:   $0→"x",       $1→"y"
  // We want: x→getUser, y→userId

  const translated: Record<string, string> = {};
  let count = 0;

  for (const [placeholder, priorName] of priorPlaceholders) {
    const newMinifiedName = newPlaceholders.get(placeholder);
    if (newMinifiedName && newMinifiedName !== priorName) {
      translated[newMinifiedName] = priorName;
      count++;
    }
  }

  return count > 0 ? translated : null;
}
