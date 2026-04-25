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
import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { buildFunctionGraph } from "../analysis/function-graph.js";
import {
  buildFingerprintIndex,
  matchFunctions
} from "../analysis/fingerprint-index.js";
import { findCloseMatches } from "../analysis/close-match.js";
import {
  buildPlaceholderMapping,
  computeBindingFingerprint,
  buildBindingPlaceholderMapping
} from "../analysis/structural-hash.js";
import type {
  FunctionNode,
  MatchResult,
  ModuleBindingNode
} from "../analysis/types.js";
import { generate, traverse } from "../babel-utils.js";
import { findWrapperFunction } from "../analysis/wrapper-detection.js";
import { debug } from "../debug.js";

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
  /** Matched module binding renames to apply */
  moduleBindingRenames?: ModuleBindingRename[];
}

/** Result of a single module binding match. */
export interface ModuleBindingRename {
  /** Minified name in the new version */
  oldName: string;
  /** Humanified name from the prior version */
  newName: string;
  /** The scope containing this binding */
  scope: babelTraverse.Scope;
}

/**
 * Matches functions between a prior humanified version and the current
 * minified version, transferring names to matched functions.
 *
 * @param priorCode The prior version's humanified output code
 * @param newFunctions The current version's function map (mutated: renameMapping set on matches)
 * @param newModuleBindings Optional module binding nodes from the current graph
 * @returns Match statistics
 */
export function matchPriorVersion(
  priorCode: string,
  newFunctions: Map<string, FunctionNode>,
  newModuleBindings?: ModuleBindingNode[]
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

  if (!priorCode) return emptyResult;
  const hasNewFunctions = newFunctions.size > 0;
  const hasNewBindings =
    newModuleBindings !== undefined && newModuleBindings.length > 0;
  if (!hasNewFunctions && !hasNewBindings) return emptyResult;

  // Parse prior version and build its function graph
  const priorAst = parseSync(priorCode, { sourceType: "unambiguous" });
  if (!priorAst) return emptyResult;

  // Function matching
  let functionsMatched = 0;
  let functionsAlreadyNamed = 0;
  let closeMatchContext = new Map<string, CloseMatchInfo>();
  let matchResult = emptyResult.matchResult;

  const priorFunctions = buildFunctionGraph(priorAst, "prior.js");
  if (priorFunctions.length > 0 && hasNewFunctions) {
    const priorFnMap = new Map<string, FunctionNode>();
    for (const fn of priorFunctions) {
      priorFnMap.set(fn.sessionId, fn);
    }

    const priorIndex = buildFingerprintIndex(priorFnMap);
    const newIndex = buildFingerprintIndex(newFunctions);

    matchResult = matchFunctions(priorIndex, newIndex, {
      enablePropagation: true
    });

    ({ functionsMatched, functionsAlreadyNamed } = applyExactMatches(
      matchResult,
      priorFnMap,
      newFunctions
    ));

    closeMatchContext = buildCloseMatchContext(
      matchResult,
      priorFnMap,
      newFunctions,
      priorIndex,
      newIndex
    );
  }

  // Match module-level bindings by structural hash
  const moduleBindingRenames = matchModuleBindings(priorAst, newModuleBindings);

  // Extract variable name transfers for matched functions that are VariableDeclarator inits
  // (arrow/function expressions whose variable name isn't covered by function matching)
  if (hasNewFunctions && newModuleBindings) {
    const fnVarRenames = collectFunctionVarNameTransfers(
      matchResult,
      priorFunctions,
      newFunctions,
      closeMatchContext
    );
    moduleBindingRenames.push(...fnVarRenames);
  }

  return {
    matchResult,
    functionsMatched,
    functionsAlreadyNamed,
    closeMatchContext,
    closeMatchCount: closeMatchContext.size,
    moduleBindingsMatched: moduleBindingRenames.length,
    moduleBindingRenames
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

// ---------------------------------------------------------------------------
// Function variable name transfers
// ---------------------------------------------------------------------------

/**
 * Extracts variable name transfers for matched functions whose AST node
 * is the init of a VariableDeclarator (arrow/function expressions).
 *
 * Function matching transfers inner bindings (params, body locals) but not
 * the variable name at module scope. This fills that gap.
 */
/** Find the prior FunctionNode whose generated code matches the close match's priorCode. */
function findPriorFnByCode(
  priorFunctions: FunctionNode[],
  priorCode: string
): FunctionNode | null {
  for (const priorFn of priorFunctions) {
    try {
      if (generate(priorFn.path.node).code === priorCode) return priorFn;
    } catch {
      // Skip if code generation fails
    }
  }
  return null;
}

function collectFunctionVarNameTransfers(
  matchResult: import("../analysis/types.js").MatchResult,
  priorFunctions: FunctionNode[],
  newFunctions: Map<string, FunctionNode>,
  closeMatchContext: Map<string, CloseMatchInfo>
): ModuleBindingRename[] {
  const priorFnMap = new Map<string, FunctionNode>();
  for (const fn of priorFunctions) {
    priorFnMap.set(fn.sessionId, fn);
  }

  const renames: ModuleBindingRename[] = [];

  // Exact matches
  for (const [priorId, newId] of matchResult.matches) {
    const rename = extractVarNameRename(
      priorFnMap.get(priorId),
      newFunctions.get(newId)
    );
    if (rename) renames.push(rename);
  }

  // Close matches — also transfer variable name
  for (const [newId, info] of closeMatchContext) {
    const newFn = newFunctions.get(newId);
    if (!newFn || !getVarDeclName(newFn)) continue;

    const priorFn = findPriorFnByCode(priorFunctions, info.priorCode);
    if (priorFn) {
      const rename = extractVarNameRename(priorFn, newFn);
      if (rename) renames.push(rename);
    }
  }

  return renames;
}

/** Get the variable name if a function node is a VariableDeclarator init. */
function getVarDeclName(fn: FunctionNode): string | null {
  const parentPath = fn.path.parentPath;
  if (!parentPath?.isVariableDeclarator?.()) return null;
  const id = (parentPath.node as t.VariableDeclarator).id;
  return t.isIdentifier(id) ? id.name : null;
}

/** Extract a ModuleBindingRename for a pair of matched functions if both are var declarator inits. */
function extractVarNameRename(
  priorFn: FunctionNode | undefined,
  newFn: FunctionNode | undefined
): ModuleBindingRename | null {
  if (!priorFn || !newFn) return null;

  const priorVarName = getVarDeclName(priorFn);
  const newVarName = getVarDeclName(newFn);
  if (!priorVarName || !newVarName) return null;
  if (priorVarName === newVarName) return null;

  // Get the scope from the variable declarator's parent (VariableDeclaration)
  const declaratorPath = newFn.path.parentPath;
  if (!declaratorPath?.isVariableDeclarator?.()) return null;
  const scope = declaratorPath.scope;

  debug.log(
    "prior-version",
    `fn-var-name: matched ${newVarName}→${priorVarName}`
  );

  return { oldName: newVarName, newName: priorVarName, scope };
}

// ---------------------------------------------------------------------------
// Module binding matching
// ---------------------------------------------------------------------------

/** A prior binding with its humanified name and structural hash. */
interface PriorBindingInfo {
  name: string;
  structuralHash: string;
  initExpr: t.Expression;
}

/**
 * Collects module-level bindings from the prior humanified AST.
 * Does NOT filter by isEligible — prior names are humanified.
 * Handles wrapper IIFE scope.
 */
function collectPriorModuleBindings(priorAst: t.File): PriorBindingInfo[] {
  const wrapper = findWrapperFunction(priorAst);

  let targetScope: babelTraverse.Scope | null = null;
  if (wrapper) {
    targetScope = wrapper.scope;
  } else {
    traverse(priorAst, {
      Program(path: babelTraverse.NodePath<t.Program>) {
        targetScope = path.scope;
        path.stop();
      }
    });
  }

  if (!targetScope) return [];

  const results: PriorBindingInfo[] = [];
  for (const [name, binding] of Object.entries(
    (targetScope as babelTraverse.Scope).bindings
  )) {
    const bindingPath = binding.path;
    if (!bindingPath.isVariableDeclarator?.()) continue;

    const declarator = bindingPath.node as t.VariableDeclarator;
    if (!declarator.init) continue;

    // Skip function/class expressions — those are handled by function matching
    if (
      t.isFunctionExpression(declarator.init) ||
      t.isArrowFunctionExpression(declarator.init) ||
      t.isClassExpression(declarator.init)
    ) {
      continue;
    }

    const fp = computeBindingFingerprint(declarator.init);
    if (!fp) continue;

    results.push({
      name,
      structuralHash: fp.structuralHash,
      initExpr: declarator.init
    });
  }

  return results;
}

/** Build a hash → items index, grouping items by a key function. */
function buildHashIndex<T>(
  items: T[],
  getHash: (item: T) => string | null
): Map<string, T[]> {
  const index = new Map<string, T[]>();
  for (const item of items) {
    const hash = getHash(item);
    if (!hash) continue;
    let list = index.get(hash);
    if (!list) {
      list = [];
      index.set(hash, list);
    }
    list.push(item);
  }
  return index;
}

/**
 * Matches module bindings between prior and new versions by unique structural hash.
 * Only matches when both sides have exactly one binding with a given hash (unique-unique).
 * Translates names via placeholder mapping.
 */
function matchModuleBindings(
  priorAst: t.File,
  newModuleBindings?: ModuleBindingNode[]
): ModuleBindingRename[] {
  if (!newModuleBindings || newModuleBindings.length === 0) return [];

  const priorBindings = collectPriorModuleBindings(priorAst);
  if (priorBindings.length === 0) return [];

  const priorByHash = buildHashIndex(priorBindings, (b) => b.structuralHash);
  const newByHash = buildHashIndex(newModuleBindings, (b) => {
    const hash = b.fingerprint.structuralHash;
    return hash.startsWith("binding:") ? null : hash;
  });

  const renames: ModuleBindingRename[] = [];

  for (const [hash, priorList] of priorByHash) {
    if (priorList.length !== 1) continue;
    const newList = newByHash.get(hash);
    if (!newList || newList.length !== 1) continue;

    const prior = priorList[0];
    const newBinding = newList[0];
    if (prior.name === newBinding.name) continue;

    const translatedName = translateBindingName(prior, newBinding);
    if (translatedName && translatedName !== newBinding.name) {
      renames.push({
        oldName: newBinding.name,
        newName: translatedName,
        scope: newBinding.scope
      });
      debug.log(
        "prior-version",
        `module-binding: matched ${newBinding.name}→${translatedName} (hash: ${hash.slice(0, 8)})`
      );
    }
  }

  return renames;
}

/**
 * Translates a module binding name from prior to new version using placeholder mapping.
 * The binding name is stored as $binding in the placeholder map.
 */
function translateBindingName(
  prior: PriorBindingInfo,
  newBinding: ModuleBindingNode
): string | null {
  // Get the init expression from the new binding's scope
  const babelBinding = newBinding.scope.bindings[newBinding.name];
  if (!babelBinding) return null;
  const bindingPath = babelBinding.path;
  if (!bindingPath.isVariableDeclarator?.()) return null;
  const newInit = (bindingPath.node as t.VariableDeclarator).init;
  if (!newInit) return null;

  const priorMapping = buildBindingPlaceholderMapping(
    prior.initExpr,
    prior.name
  );
  const newMapping = buildBindingPlaceholderMapping(newInit, newBinding.name);

  // The binding's own name is stored as $binding
  const priorName = priorMapping.get("$binding");
  const newName = newMapping.get("$binding");

  if (!priorName || !newName) return null;
  if (priorName === newName) return null; // already named

  return priorName;
}
