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
import { buildPlaceholderMapping } from "../analysis/structural-hash.js";
import type { FunctionNode, MatchResult } from "../analysis/types.js";

export interface PriorVersionResult {
  matchResult: MatchResult;
  /** Functions matched AND renames transferred (actual LLM calls saved) */
  functionsMatched: number;
  /** Functions matched but all identifiers were already identical (e.g., exports, property keys) */
  functionsAlreadyNamed: number;
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

  // Apply matched names via placeholder mapping translation
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
      // Matched structurally but all identifiers are already the same
      // (exports, property keys, or identifiers that survived minification).
      // Mark as done with empty mapping so the LLM doesn't re-process it.
      newFn.renameMapping = { names: {} };
      functionsAlreadyNamed++;
    }
  }

  return {
    matchResult,
    functionsMatched,
    functionsAlreadyNamed,
    moduleBindingsMatched: 0 // TODO: module binding matching
  };
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
