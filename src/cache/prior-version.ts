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
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import {
  buildBindingFingerprintIndex,
  buildFingerprintIndex,
  matchFunctions
} from "../analysis/fingerprint-index.js";
import { findCloseMatches } from "../analysis/close-match.js";
import { buildPlaceholderMapping } from "../analysis/structural-hash.js";
import type {
  FingerprintIndex,
  FunctionNode,
  MatchResult,
  ModuleBindingNode
} from "../analysis/types.js";
import { generate } from "../babel-utils.js";
import type { Profiler } from "../profiling/profiler.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import { debug } from "../debug.js";

export interface CloseMatchInfo {
  /** Session ID of the prior close-matched function */
  priorId: string;
  /** Prior humanified code (for LLM context) */
  priorCode: string;
  /** Partial name transfers: minified name → humanified name (function name + params) */
  nameTransfers: Record<string, string>;
  /** Module-scope identifiers referenced by the prior function */
  priorExternals?: Set<string>;
  /** Module-scope identifiers referenced by the new function */
  newExternals?: Set<string>;
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
  newModuleBindings?: ModuleBindingNode[],
  profiler: Profiler = NULL_PROFILER
): PriorVersionResult {
  const emptyResult: PriorVersionResult = {
    matchResult: {
      matches: new Map(),
      ambiguous: new Map(),
      unmatched: [],
      resolutionStats: {
        structuralHashUnique: 0,
        identityResolved: 0,
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

  // Parse prior version and build its unified graph — the SAME graph the
  // new side gets (functions + module bindings with callee/caller edges,
  // Bun CJS factory classification included). Prior binding names are all
  // humanified, so every binding is a valid name source.
  const parseSpan = profiler.startSpan("prior-version:parse", "pipeline");
  const priorAst = parseSync(priorCode, { sourceType: "unambiguous" });
  parseSpan.end();
  if (!priorAst) return emptyResult;

  const graphSpan = profiler.startSpan("prior-version:graph", "pipeline");
  const priorGraph = buildUnifiedGraph(
    priorAst,
    "prior.js",
    profiler,
    () => true,
    priorCode
  );
  const priorFunctions: FunctionNode[] = [];
  const priorBindings: ModuleBindingNode[] = [];
  for (const [, node] of priorGraph.nodes) {
    if (node.type === "function") {
      priorFunctions.push(node.node);
    } else {
      priorBindings.push(node.node);
    }
  }
  graphSpan.end({
    functionCount: priorFunctions.length,
    bindingCount: priorBindings.length
  });

  // Function matching
  let functionsMatched = 0;
  let functionsAlreadyNamed = 0;
  let closeMatchContext = new Map<string, CloseMatchInfo>();
  let matchResult = emptyResult.matchResult;

  if (priorFunctions.length > 0 && hasNewFunctions) {
    const priorFnMap = new Map<string, FunctionNode>();
    for (const fn of priorFunctions) {
      priorFnMap.set(fn.sessionId, fn);
    }

    const matchSpan = profiler.startSpan(
      "prior-version:match-functions",
      "pipeline"
    );
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
    matchSpan.end({ matched: matchResult.matches.size });

    const closeSpan = profiler.startSpan(
      "prior-version:close-match",
      "pipeline"
    );
    closeMatchContext = buildCloseMatchContext(
      matchResult,
      priorFnMap,
      newFunctions,
      priorIndex,
      newIndex
    );
    closeSpan.end({ closeMatches: closeMatchContext.size });
  }

  // Match module-level bindings through the same cascade functions use
  const bindingSpan = profiler.startSpan(
    "prior-version:module-bindings",
    "pipeline"
  );
  const moduleBindingRenames = matchModuleBindings(
    priorBindings,
    newModuleBindings,
    matchResult.matches
  );

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
  bindingSpan.end({ bindingRenames: moduleBindingRenames.length });

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

/**
 * Collects identifiers referenced by a function that are bound in an ancestor scope.
 * These are the function's "external references" — module-level bindings it reads/writes.
 */
function collectModuleScopeRefs(fn: FunctionNode): Set<string> {
  const refs = new Set<string>();
  const fnScope = fn.path.scope;
  fn.path.traverse({
    Identifier(
      idPath: import("@babel/traverse").NodePath<
        import("@babel/types").Identifier
      >
    ) {
      const name = idPath.node.name;
      if (refs.has(name)) return;
      if (!idPath.isReferencedIdentifier()) return;
      const binding = fnScope.getBinding(name);
      if (binding && binding.scope !== fnScope) {
        refs.add(name);
      }
    }
  });
  return refs;
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
      const priorExternals = collectModuleScopeRefs(priorFn);
      const newExternals = collectModuleScopeRefs(newFn);
      context.set(newId, {
        priorId,
        priorCode,
        nameTransfers,
        priorExternals,
        newExternals
      });
    } catch {
      // Skip if code generation fails
    }
  }

  return context;
}

/** Get a function's own name identifier (declarations/named expressions only). */
function getFunctionNameId(node: t.Function): t.Identifier | null {
  if (
    (t.isFunctionDeclaration(node) || t.isFunctionExpression(node)) &&
    node.id
  ) {
    return node.id;
  }
  return null;
}

/** Unwrap a param node to its binding identifier when it has a simple one. */
function getParamIdentifier(param: t.Node): t.Identifier | null {
  if (t.isIdentifier(param)) return param;
  if (t.isAssignmentPattern(param) && t.isIdentifier(param.left)) {
    return param.left;
  }
  if (t.isRestElement(param) && t.isIdentifier(param.argument)) {
    return param.argument;
  }
  return null;
}

/**
 * Computes partial name transfers for close-matched functions.
 *
 * Alignment is by actual AST position — function name to function name
 * (only when both sides have one), parameter i to parameter i. Placeholder
 * slots must NOT be used here: for arrows and anonymous functions there is
 * no name identifier so every slot shifts by one, and member-expression
 * property names occupy slots, letting a property name like `delete` land
 * on a parameter position. Body locals can shift when statements are
 * added/removed, so they are never transferred for close matches.
 *
 * Returns a mapping of { minifiedName → humanifiedName } for safe transfers.
 */
function computePartialTransfer(
  priorFn: FunctionNode,
  newFn: FunctionNode
): Record<string, string> {
  const transfers: Record<string, string> = {};

  const priorNameId = getFunctionNameId(priorFn.path.node);
  const newNameId = getFunctionNameId(newFn.path.node);
  if (priorNameId && newNameId && priorNameId.name !== newNameId.name) {
    transfers[newNameId.name] = priorNameId.name;
  }

  const priorParams = priorFn.path.node.params;
  const newParams = newFn.path.node.params;
  const sharedParamCount = Math.min(priorParams.length, newParams.length);
  for (let i = 0; i < sharedParamCount; i++) {
    const priorParam = getParamIdentifier(priorParams[i]);
    const newParam = getParamIdentifier(newParams[i]);
    if (priorParam && newParam && priorParam.name !== newParam.name) {
      transfers[newParam.name] = priorParam.name;
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
  // Placeholder mappings are captured at graph-build time (before any
  // renames); recomputing them per match is a full subtree walk each.
  const priorPlaceholders =
    priorFn.placeholderMapping ?? buildPlaceholderMapping(priorFn.path);
  const newPlaceholders =
    newFn.placeholderMapping ?? buildPlaceholderMapping(newFn.path);

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

    const priorFn = priorFnMap.get(info.priorId);
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

/** True when a binding can participate in hash-based cross-version matching. */
function isMatchableBinding(binding: ModuleBindingNode): boolean {
  // Unhashable bindings carry a name-derived fallback hash that can never
  // match across versions.
  if (binding.fingerprint.structuralHash.startsWith("binding:")) return false;

  // Function/class expression inits are matched by the function cascade
  // (with var-name transfers); matching them here by init hash would
  // compete with the better-informed function matcher.
  const babelBinding = binding.scope.bindings[binding.name];
  const bindingPath = babelBinding?.path;
  if (!bindingPath?.isVariableDeclarator?.()) return true;
  const init = (bindingPath.node as t.VariableDeclarator).init;
  if (!init) return true;
  return !(
    t.isFunctionExpression(init) ||
    t.isArrowFunctionExpression(init) ||
    t.isClassExpression(init)
  );
}

/**
 * Session ids of the nodes a binding's initializer references — both
 * functions and other module bindings (`var alias = OTHER_BINDING`).
 */
function calleeNeighborIds(binding: ModuleBindingNode): string[] {
  return [...binding.internalCallees].map((callee) => callee.sessionId);
}

/** Session ids of the functions that reference a binding. */
function callerFnIds(binding: ModuleBindingNode): string[] {
  return [...binding.callers].map((fn) => fn.sessionId);
}

/**
 * Identity resolver for same-hash binding buckets: a prior binding and a
 * new binding correspond when the prior's referenced (or referencing)
 * functions map exactly onto the candidate's under the function match
 * result. This stays discriminating even when candidates wrap
 * structurally identical code (`Z(()=>{wp8()})` vs `Z(()=>{VkH()})`) —
 * the neighbor functions are matched at ~99%, and their identity is the
 * one signal the structural hash erases.
 *
 * Strict by design (precision over recall): every prior neighbor must be
 * matched, the sets must be non-empty and correspond exactly, and exactly
 * one candidate may fit.
 */
/** Canonical set key for a list of session ids. */
function idsKey(ids: string[]): string {
  return [...new Set(ids)].sort().join("|");
}

/**
 * Translate prior-side neighbor function ids through the match result.
 * Returns the canonical key, or null when any neighbor is unmatched or
 * there are no neighbors (no identity evidence).
 */
function mapNeighborIds(
  priorIds: string[],
  fnMatches: Map<string, string>
): string | null {
  if (priorIds.length === 0) return null;
  const mapped: string[] = [];
  for (const id of priorIds) {
    const match = fnMatches.get(id);
    if (!match) return null;
    mapped.push(match);
  }
  return idsKey(mapped);
}

/** Find the single candidate whose neighbor key equals expectedKey. */
function findUniqueByKey(
  expectedKey: string,
  candidates: string[],
  newById: Map<string, ModuleBindingNode>,
  idsOf: (b: ModuleBindingNode) => string[]
): string | null {
  let found: string | null = null;
  for (const candId of candidates) {
    const candidate = newById.get(candId);
    if (!candidate || idsKey(idsOf(candidate)) !== expectedKey) continue;
    if (found) return null; // more than one candidate fits
    found = candId;
  }
  return found;
}

/**
 * Identity resolver for same-hash binding buckets: a prior and a new
 * binding correspond when the prior's referenced (or referencing)
 * neighbors map exactly onto the candidate's under the matches so far —
 * matched functions, plus bindings matched in earlier rounds (which
 * resolves alias patterns like `var X = OTHER_BINDING`).
 *
 * Strict by design (precision over recall): every prior neighbor must be
 * matched, the sets must be non-empty and correspond exactly, and exactly
 * one candidate may fit.
 */
function makeBindingIdentityResolver(
  priorById: Map<string, ModuleBindingNode>,
  newById: Map<string, ModuleBindingNode>,
  neighborMatches: Map<string, string>
): (oldId: string, candidates: string[]) => string | null {
  const resolveBy = (
    priorIds: string[],
    candidates: string[],
    idsOf: (b: ModuleBindingNode) => string[]
  ): string | null => {
    const expectedKey = mapNeighborIds(priorIds, neighborMatches);
    if (!expectedKey) return null;
    return findUniqueByKey(expectedKey, candidates, newById, idsOf);
  };

  return (oldId, candidates) => {
    const prior = priorById.get(oldId);
    if (!prior) return null;
    return (
      resolveBy(calleeNeighborIds(prior), candidates, calleeNeighborIds) ??
      resolveBy(callerFnIds(prior), candidates, callerFnIds)
    );
  };
}

/**
 * Runs the binding cascade, iterating identity rounds to a fixpoint:
 * bindings matched in one round become identity evidence for their
 * neighbors in the next (alias chains). Each round's resolver is
 * monotone — mappings never change, so previously resolved buckets
 * resolve identically and new evidence only adds matches.
 */
function runBindingMatchRounds(
  priorIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  priorById: Map<string, ModuleBindingNode>,
  newById: Map<string, ModuleBindingNode>,
  fnMatches: Map<string, string>
): MatchResult {
  const MAX_IDENTITY_ROUNDS = 4;
  let result = matchFunctions(priorIndex, newIndex, {
    resolveAmbiguousCandidate: makeBindingIdentityResolver(
      priorById,
      newById,
      fnMatches
    )
  });
  for (let round = 1; round < MAX_IDENTITY_ROUNDS; round++) {
    const neighborMatches = new Map([...fnMatches, ...result.matches]);
    const next = matchFunctions(priorIndex, newIndex, {
      resolveAmbiguousCandidate: makeBindingIdentityResolver(
        priorById,
        newById,
        neighborMatches
      )
    });
    const grew = next.matches.size > result.matches.size;
    result = next;
    if (!grew) break;
  }
  return result;
}

/**
 * Matches module bindings between prior and new versions using the same
 * disambiguation cascade functions get, plus an identity stage that
 * resolves same-hash buckets by correspondence under the function match
 * result. Runs after function matching for exactly that reason.
 */
function matchModuleBindings(
  priorBindings: ModuleBindingNode[],
  newModuleBindings: ModuleBindingNode[] | undefined,
  fnMatches: Map<string, string>
): ModuleBindingRename[] {
  if (!newModuleBindings || newModuleBindings.length === 0) return [];

  const matchablePrior = priorBindings.filter(isMatchableBinding);
  const matchableNew = newModuleBindings.filter(isMatchableBinding);
  if (matchablePrior.length === 0 || matchableNew.length === 0) return [];

  const priorById = new Map(matchablePrior.map((b) => [b.sessionId, b]));
  const newById = new Map(matchableNew.map((b) => [b.sessionId, b]));

  const result = runBindingMatchRounds(
    buildBindingFingerprintIndex(matchablePrior),
    buildBindingFingerprintIndex(matchableNew),
    priorById,
    newById,
    fnMatches
  );

  // Drop new-side bindings claimed by more than one prior binding — a
  // wrong reused name is worse than a fresh LLM name.
  const claimCounts = new Map<string, number>();
  for (const newId of result.matches.values()) {
    claimCounts.set(newId, (claimCounts.get(newId) ?? 0) + 1);
  }

  const renames: ModuleBindingRename[] = [];
  for (const [priorId, newId] of result.matches) {
    if ((claimCounts.get(newId) ?? 0) > 1) continue;
    const prior = priorById.get(priorId);
    const next = newById.get(newId);
    if (!prior || !next || prior.name === next.name) continue;
    renames.push({
      oldName: next.name,
      newName: prior.name,
      scope: next.scope
    });
    debug.log(
      "prior-version",
      `module-binding: matched ${next.name}→${prior.name}`
    );
  }

  const stats = result.resolutionStats;
  debug.log(
    "prior-version",
    `module-binding cascade: ${stats.structuralHashUnique} unique, ` +
      `${stats.identityResolved} identity, ${stats.calleeShapesResolved + stats.callerShapesResolved + stats.calleeHashesResolved + stats.twoHopShapesResolved} shape/hash, ` +
      `${stats.stillAmbiguous} ambiguous, ${stats.unmatched} unmatched`
  );

  return renames;
}
