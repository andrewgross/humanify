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

import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import {
  buildBindingFingerprintIndex,
  buildFingerprintIndex,
  matchFunctions,
  resolveAmbiguousByOrdinal
} from "../analysis/fingerprint-index.js";
import type { ExternalRefEvidence } from "../analysis/propagation.js";
import { findCloseMatches } from "../analysis/close-match.js";
import {
  computeShingleSet,
  jaccardSimilarity
} from "../analysis/function-fingerprint.js";
import {
  buildPlaceholderMapping,
  buildPlaceholderTable
} from "../analysis/structural-hash.js";
import {
  isPending,
  markTransferred,
  type TransferPair
} from "../rename/lifecycle.js";
import { computeBodyLocalTransfers } from "./statement-align.js";
import { type BindingRole, computeBindingRole } from "./binding-role.js";
import type {
  FingerprintIndex,
  FunctionNode,
  MatchResult,
  ModuleBindingNode
} from "../analysis/types.js";
import { generate, parseSourceAst } from "../babel-utils.js";
import type { Profiler } from "../profiling/profiler.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import { debug } from "../debug.js";

export interface CloseMatchInfo {
  /** Session ID of the prior close-matched function */
  priorId: string;
  /** Prior humanified code (for LLM context) */
  priorCode: string;
  /**
   * Partial name transfers. Function name + params from signature
   * position (positional pairs, binding null), body locals from
   * statement-level content alignment carrying each slot's resolved
   * Binding (see statement-align.ts) — same-named sibling bindings each
   * keep their own pair.
   */
  nameTransfers: TransferPair[];
  /** The prior function's identifier names — prompt material for reuse */
  priorNames?: string[];
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
  /**
   * Role evidence for prior module bindings whose name was NOT consumed
   * by the binding cascade or a function var-name transfer, keyed by
   * prior (humanified) name. Compact plain data — safe to hold after the
   * prior AST is released. Vote propagation uses it to corroborate
   * single-vote name pins.
   */
  priorBindingRoles: Map<string, BindingRole>;
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
        enclosingStatementResolved: 0,
        calleeShapesResolved: 0,
        callerShapesResolved: 0,
        calleeHashesResolved: 0,
        twoHopShapesResolved: 0,
        shingleSimilarityResolved: 0,
        ordinalResolved: 0,
        injectivityDemoted: 0,
        singletonRejected: 0,
        stillAmbiguous: 0,
        unmatched: 0,
        propagationResolved: 0
      }
    },
    functionsMatched: 0,
    functionsAlreadyNamed: 0,
    closeMatchContext: new Map(),
    closeMatchCount: 0,
    moduleBindingsMatched: 0,
    priorBindingRoles: new Map()
  };

  // Input contract: a prior that is empty or unparseable must fail fast.
  // Silently returning emptyResult turns a bad --prior-version argument
  // into a full-cost run that transfers nothing.
  if (!priorCode.trim()) {
    throw new Error(
      "prior version input is empty — check the --prior-version file"
    );
  }
  const hasNewFunctions = newFunctions.size > 0;
  const hasNewBindings =
    newModuleBindings !== undefined && newModuleBindings.length > 0;
  if (!hasNewFunctions && !hasNewBindings) return emptyResult;

  // Parse prior version and build its unified graph — the SAME graph the
  // new side gets (functions + module bindings with callee/caller edges,
  // Bun CJS factory classification included). Prior binding names are all
  // humanified, so every binding is a valid name source.
  const priorAst = parsePriorOrThrow(priorCode, profiler);

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
  const bindingSetup = prepareBindingMatching(priorBindings, newModuleBindings);
  let bindingMatchResult: MatchResult | null = null;

  if (priorFunctions.length > 0 && hasNewFunctions) {
    const functionMatching = matchAndApplyFunctions(
      priorFunctions,
      newFunctions,
      bindingSetup,
      profiler
    );
    ({
      matchResult,
      bindingMatchResult,
      functionsMatched,
      functionsAlreadyNamed,
      closeMatchContext
    } = functionMatching);
  }

  // Match module-level bindings through the same cascade functions use.
  // The alternation above already produced the binding result when
  // function matching ran; otherwise run the rounds now.
  const bindingSpan = profiler.startSpan(
    "prior-version:module-bindings",
    "pipeline"
  );
  const bindingCascade = resolveBindingRenames(
    bindingSetup,
    bindingMatchResult,
    matchResult.matches
  );
  const moduleBindingRenames = bindingCascade.renames;

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

  // Role evidence for still-unconsumed prior binding names, computed
  // while the prior AST is alive (roles are plain data; the AST drops
  // when this function returns).
  const priorBindingRoles = buildPriorBindingRoles(
    priorBindings,
    bindingCascade.matchedPriorIds,
    new Set(moduleBindingRenames.map((r) => r.newName))
  );

  return {
    matchResult,
    functionsMatched,
    functionsAlreadyNamed,
    closeMatchContext,
    closeMatchCount: closeMatchContext.size,
    moduleBindingsMatched: moduleBindingRenames.length,
    moduleBindingRenames,
    priorBindingRoles
  };
}

/**
 * Role evidence for prior module bindings whose humanified name is still
 * available for single-vote pinning: cascade-matched bindings and names
 * consumed by any binding rename (cascade or function var-name transfer)
 * are excluded — those names already have a destination.
 */
function buildPriorBindingRoles(
  priorBindings: ModuleBindingNode[],
  matchedPriorIds: Set<string>,
  consumedNames: Set<string>
): Map<string, BindingRole> {
  const roles = new Map<string, BindingRole>();
  for (const binding of priorBindings) {
    if (matchedPriorIds.has(binding.sessionId)) continue;
    if (consumedNames.has(binding.name)) continue;
    roles.set(binding.name, computeBindingRole(binding));
  }
  return roles;
}

/**
 * Binding renames from the alternation's binding result, or from a fresh
 * cascade run when function matching didn't run (no prior/new functions).
 * Also surfaces WHICH prior bindings the cascade matched, so role
 * evidence for single-vote pinning covers only the unmatched remainder.
 */
function resolveBindingRenames(
  bindingSetup: BindingMatchSetup | null,
  bindingMatchResult: MatchResult | null,
  fnMatches: Map<string, string>
): { renames: ModuleBindingRename[]; matchedPriorIds: Set<string> } {
  if (!bindingSetup) return { renames: [], matchedPriorIds: new Set() };
  const result =
    bindingMatchResult ??
    runBindingMatchRounds(
      bindingSetup.priorIndex,
      bindingSetup.newIndex,
      bindingSetup.priorById,
      bindingSetup.newById,
      fnMatches
    );
  return {
    renames: deriveBindingRenames(result, bindingSetup),
    matchedPriorIds: new Set(result.matches.keys())
  };
}

/**
 * Function matching for matchPriorVersion: cascade match, alternate with
 * the binding cascade when bindings are available, settle exact matches,
 * and build close-match context — with the same-program sanity check.
 */
function matchAndApplyFunctions(
  priorFunctions: FunctionNode[],
  newFunctions: Map<string, FunctionNode>,
  bindingSetup: BindingMatchSetup | null,
  profiler: Profiler
): {
  matchResult: MatchResult;
  bindingMatchResult: MatchResult | null;
  functionsMatched: number;
  functionsAlreadyNamed: number;
  closeMatchContext: Map<string, CloseMatchInfo>;
} {
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

  let matchResult = matchFunctions(priorIndex, newIndex, {
    enablePropagation: true
  });

  // Alternate with the binding cascade: binding AND function matches
  // become reference-identity evidence for same-hash function buckets,
  // and each round's new function matches feed the next binding round.
  // Runs even without matchable bindings — matched-function references
  // alone crack export-thunk buckets.
  const alternated = alternateFunctionAndBindingMatching(
    matchResult,
    priorIndex,
    newIndex,
    priorFnMap,
    newFunctions,
    bindingSetup
  );
  matchResult = alternated.functionResult;
  const bindingMatchResult = alternated.bindingResult;

  // Last tier, after every evidence source (cascade, propagation, binding
  // alternation) has had its chance: pair equal-count identical buckets
  // by source order so true twins keep stable names across versions.
  resolveAmbiguousByOrdinal(matchResult, priorIndex, newIndex);

  const { functionsMatched, functionsAlreadyNamed } = applyExactMatches(
    matchResult,
    priorFnMap,
    newFunctions
  );
  matchSpan.end({ matched: matchResult.matches.size });

  const closeSpan = profiler.startSpan("prior-version:close-match", "pipeline");
  const closeMatchContext = buildCloseMatchContext(
    matchResult,
    priorFnMap,
    newFunctions,
    priorIndex,
    newIndex
  );
  closeSpan.end({ closeMatches: closeMatchContext.size });

  assertPriorLooksLikeSameProgram(priorFnMap.size, matchResult);

  return {
    matchResult,
    bindingMatchResult,
    functionsMatched,
    functionsAlreadyNamed,
    closeMatchContext
  };
}

/** Parse the prior version, failing fast with a clear message. */
function parsePriorOrThrow(priorCode: string, profiler: Profiler): t.File {
  const parseSpan = profiler.startSpan("prior-version:parse", "pipeline");
  let ast: t.File | null = null;
  try {
    // preserveAstCaches: this bundle is parsed while the NEW AST is the
    // matcher's working set — clearing Babel's path/scope cache here would
    // force the new AST's scopes to re-crawl mid-matching for no hygiene
    // benefit (see ParseSourceOptions.preserveAstCaches). The analysis
    // caches are per-AST and unaffected either way.
    ast = parseSourceAst(priorCode, { preserveAstCaches: true });
  } catch (err) {
    throw new Error(
      `prior version failed to parse — check the --prior-version file: ${
        err instanceof Error ? err.message : String(err)
      }`
    );
  } finally {
    parseSpan.end();
  }
  if (!ast) {
    throw new Error(
      "prior version failed to parse — check the --prior-version file"
    );
  }
  return ast;
}

/** Minimum prior functions before the same-program sanity floor applies. */
const SAME_PROGRAM_FLOOR_MIN_FUNCTIONS = 50;
/** Minimum fraction of prior functions whose hash exists in the new version. */
const SAME_PROGRAM_PRESENCE_FLOOR = 0.05;

/**
 * A prior that shares (nearly) no structural hashes with the new version
 * is a wrong file, not an aggressive refactor — matched AND ambiguous
 * prior functions both count as presence, so even a version where nothing
 * disambiguates passes. Fails fast instead of letting a full-cost run
 * transfer nothing.
 */
function assertPriorLooksLikeSameProgram(
  priorFunctionCount: number,
  matchResult: MatchResult
): void {
  if (priorFunctionCount < SAME_PROGRAM_FLOOR_MIN_FUNCTIONS) return;
  const present = priorFunctionCount - matchResult.unmatched.length;
  if (present / priorFunctionCount < SAME_PROGRAM_PRESENCE_FLOOR) {
    throw new Error(
      `prior version does not appear to be the same program: only ${present} of ` +
        `${priorFunctionCount} prior functions have a matching structural hash in ` +
        `the new version. Check the --prior-version file; drop the flag to run ` +
        `without transfer.`
    );
  }
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
      functionsMatched++;
    } else {
      functionsAlreadyNamed++;
    }
    // A frozen function (library / wrapper / eval-taint) is already settled;
    // the freeze wins over a prior-version match, so only claim pending ones.
    if (isPending(newFn)) {
      markTransferred(newFn, translated ?? []);
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
      // A close pair with ZERO content corroboration is a shape
      // coincidence on count features (cosine alone) — transferring the
      // signature would present a wrong name as continuity. Corroboration
      // is either an aligned statement (identical normalized content) or
      // strong rename-invariant shingle overlap (refactors where every
      // statement changed shape, e.g. `var r = X; return r` → `return X`).
      // Uncorroborated pairs still serve as LLM context. Signature-
      // position transfers (fn name + params) win over body-alignment
      // pairs on collision — both derive the same value in sane cases,
      // and position is authoritative for the signature.
      const alignment = computeBodyLocalTransfers(priorFn, newFn);
      const corroborated =
        alignment.alignedStatements >= 1 || shinglesCorroborate(priorFn, newFn);
      // Signature-position transfers (fn name + params, positional pairs)
      // win over body-alignment pairs on target-name collision downstream —
      // validated rename rejects the later duplicate — so list them first.
      const nameTransfers = corroborated
        ? [
            ...Object.entries(computePartialTransfer(priorFn, newFn)).map(
              ([oldName, newName]) => ({ oldName, newName, binding: null })
            ),
            ...alignment.transfers
          ]
        : [];
      if (!corroborated) {
        debug.log(
          "prior-version",
          `close-match ${newId}: 0/${alignment.totalNewStatements} statements aligned, shingles disagree — transfers gated, context only`
        );
      }
      const priorExternals = collectModuleScopeRefs(priorFn);
      const newExternals = collectModuleScopeRefs(newFn);
      context.set(newId, {
        priorId,
        priorCode,
        nameTransfers,
        priorNames: collectPriorNames(priorFn),
        priorExternals,
        newExternals
      });
    } catch {
      // Skip if code generation fails
    }
  }

  return context;
}

/** Minimum rename-invariant shingle overlap to corroborate a close pair. */
const CLOSE_MATCH_SHINGLE_FLOOR = 0.5;

/**
 * Shingle-overlap corroboration for close pairs whose statements all
 * changed shape. Shingle tokens (blurred callee shapes, property
 * accesses, external calls, exact string literals) are rename-invariant.
 * Empty shingle sets are missing evidence, not agreement — tiny
 * featureless functions must not pass on vacuous similarity.
 */
function shinglesCorroborate(
  priorFn: FunctionNode,
  newFn: FunctionNode
): boolean {
  const priorShingles = computeShingleSet(priorFn);
  const newShingles = computeShingleSet(newFn);
  if (priorShingles.size === 0 || newShingles.size === 0) return false;
  return (
    jaccardSimilarity(priorShingles, newShingles) >= CLOSE_MATCH_SHINGLE_FLOOR
  );
}

/** Cap on prior identifier names passed to the prompt. */
const MAX_PRIOR_NAMES = 40;

/**
 * The prior function's binding names (from its placeholder mapping —
 * binding slots only, so property names and free identifiers never
 * appear). These are the humanified names the LLM should reuse for
 * unchanged logic.
 */
function collectPriorNames(priorFn: FunctionNode): string[] {
  const mapping =
    priorFn.placeholderMapping ?? buildPlaceholderMapping(priorFn.path);
  const names = new Set<string>();
  for (const name of mapping.values()) {
    names.add(name);
    if (names.size >= MAX_PRIOR_NAMES) break;
  }
  return [...names];
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
 *
 * Pairs stay keyed by SLOT, each carrying the new version's resolved
 * Binding: two distinct bindings can share one minified name (a catch
 * param shadowing a function-scope binding), so collapsing pairs into a
 * name-keyed record loses one — and can hand its name to the other binding.
 */
function translatePriorNames(
  priorFn: FunctionNode,
  newFn: FunctionNode
): TransferPair[] | null {
  // Placeholder mappings are captured at graph-build time (before any
  // renames); recomputing them per match is a full subtree walk each.
  const priorPlaceholders =
    priorFn.placeholderMapping ?? buildPlaceholderMapping(priorFn.path);
  const newPlaceholders = resolveNewPlaceholders(newFn);

  assertPlaceholderAlignment(priorPlaceholders, newPlaceholders.names, newFn);

  const pairs: TransferPair[] = [];
  for (const [placeholder, priorName] of priorPlaceholders) {
    const newMinifiedName = newPlaceholders.names.get(placeholder);
    if (newMinifiedName && newMinifiedName !== priorName) {
      pairs.push({
        oldName: newMinifiedName,
        newName: priorName,
        binding: newPlaceholders.bindings.get(placeholder) ?? null
      });
    }
  }

  return pairs.length > 0 ? pairs : null;
}

/**
 * The new function's placeholder table. Graph-built nodes carry both views
 * cached; when a node lacks them (hand-built in tests), recompute BOTH from
 * one walk — mixing a cached names view with freshly computed bindings
 * could pair stale names with post-rename bindings.
 */
function resolveNewPlaceholders(newFn: FunctionNode): {
  names: Map<string, string>;
  bindings: Map<string, babelTraverse.Binding>;
} {
  if (newFn.placeholderMapping && newFn.placeholderBindings) {
    return {
      names: newFn.placeholderMapping,
      bindings: newFn.placeholderBindings
    };
  }
  return buildPlaceholderTable(newFn.path);
}

/**
 * Matched functions have equal structural hashes, and slot ordinals are
 * assigned by the same serialization walk — so their placeholder maps must
 * cover the same slot set. A divergence means a stale or corrupt mapping,
 * and translating through it would assign names to the WRONG identifiers
 * (silent, unfixable downstream). Fail fast instead.
 */
function assertPlaceholderAlignment(
  priorPlaceholders: Map<string, string>,
  newPlaceholders: Map<string, string>,
  newFn: FunctionNode
): void {
  if (priorPlaceholders.size === newPlaceholders.size) {
    let aligned = true;
    for (const slot of priorPlaceholders.keys()) {
      if (!newPlaceholders.has(slot)) {
        aligned = false;
        break;
      }
    }
    if (aligned) return;
  }
  throw new Error(
    `placeholder maps misaligned for matched pair at ${newFn.sessionId}: ` +
      `prior has ${priorPlaceholders.size} slots, new has ${newPlaceholders.size} — ` +
      `equal hashes guarantee equal slot sets, so a mapping is stale or corrupt`
  );
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
  if (!parentPath?.isVariableDeclarator()) return null;
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

  // Resolve the scope that OWNS the binding — a `var` inside a block
  // hoists past the declarator's own scope, and the validated rename
  // checks own bindings, not the scope chain.
  const declaratorPath = newFn.path.parentPath;
  if (!declaratorPath?.isVariableDeclarator()) return null;
  const binding = declaratorPath.scope.getBinding(newVarName);
  if (!binding) return null;
  const scope = binding.scope;

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
  // Unhashable bindings (null fingerprint) can never match across versions.
  if (!binding.fingerprint) return false;

  // Function/class expression inits are matched by the function cascade
  // (with var-name transfers); matching them here by init hash would
  // compete with the better-informed function matcher.
  const babelBinding = binding.scope.bindings[binding.name];
  const bindingPath = babelBinding?.path;
  if (!bindingPath?.isVariableDeclarator()) return true;
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

/** Prepared inputs for the binding cascade, shared across alternation rounds. */
interface BindingMatchSetup {
  priorIndex: FingerprintIndex;
  newIndex: FingerprintIndex;
  priorById: Map<string, ModuleBindingNode>;
  newById: Map<string, ModuleBindingNode>;
}

/**
 * Filters to matchable bindings and builds the cascade inputs, or null
 * when either side has nothing matchable.
 */
function prepareBindingMatching(
  priorBindings: ModuleBindingNode[],
  newModuleBindings: ModuleBindingNode[] | undefined
): BindingMatchSetup | null {
  if (!newModuleBindings || newModuleBindings.length === 0) return null;

  const matchablePrior = priorBindings.filter(isMatchableBinding);
  const matchableNew = newModuleBindings.filter(isMatchableBinding);
  if (matchablePrior.length === 0 || matchableNew.length === 0) return null;

  return {
    priorIndex: buildBindingFingerprintIndex(matchablePrior),
    newIndex: buildBindingFingerprintIndex(matchableNew),
    priorById: new Map(matchablePrior.map((b) => [b.sessionId, b])),
    newById: new Map(matchableNew.map((b) => [b.sessionId, b]))
  };
}

/** Cap on function↔binding alternation rounds (first round included). */
const MAX_ALTERNATION_ROUNDS = 3;

/**
 * Alternates the function and binding cascades to a capped fixpoint.
 * Binding matches give ambiguous same-hash function buckets their only
 * remaining identity signal — WHICH matched binding a member references
 * (module-scope export getters have no callees, callers, or matched
 * parents). Each round's new function matches then strengthen the next
 * binding round. Growth is checked on the function side; the binding
 * result always reflects the final function matches.
 */
function alternateFunctionAndBindingMatching(
  initialFunctionResult: MatchResult,
  priorIndex: FingerprintIndex,
  newIndex: FingerprintIndex,
  priorFnMap: Map<string, FunctionNode>,
  newFunctions: Map<string, FunctionNode>,
  setup: BindingMatchSetup | null
): { functionResult: MatchResult; bindingResult: MatchResult | null } {
  let functionResult = initialFunctionResult;
  let bindingResult = setup
    ? runBindingMatchRounds(
        setup.priorIndex,
        setup.newIndex,
        setup.priorById,
        setup.newById,
        functionResult.matches
      )
    : null;

  for (let round = 1; round < MAX_ALTERNATION_ROUNDS; round++) {
    if (functionResult.ambiguous.size === 0) break;
    const evidence = buildExternalRefEvidence(
      functionResult.ambiguous,
      priorFnMap,
      newFunctions,
      setup,
      bindingResult?.matches ?? new Map(),
      functionResult.matches
    );
    if (!evidence) break;

    const next = matchFunctions(priorIndex, newIndex, {
      enablePropagation: true,
      externalRefEvidence: evidence
    });
    if (next.matches.size <= functionResult.matches.size) break;
    debug.log(
      "prior-version",
      `alternation round ${round}: +${next.matches.size - functionResult.matches.size} function matches from reference-identity evidence`
    );
    functionResult = next;
    if (setup) {
      bindingResult = runBindingMatchRounds(
        setup.priorIndex,
        setup.newIndex,
        setup.priorById,
        setup.newById,
        functionResult.matches
      );
    }
  }

  return { functionResult, bindingResult };
}

/**
 * Builds reference-identity evidence for the ambiguous functions and
 * their candidates, or null when there is nothing to build on. A
 * reference can hit a matchable module binding OR a binding that holds a
 * graph function (Bun's export thunks reference functions without
 * calling them, so callee edges never see them). Refs are collected per
 * binding IDENTITY (the resolved Binding object), the same precision
 * standard vote propagation uses.
 */
function buildExternalRefEvidence(
  ambiguous: Map<string, string[]>,
  priorFnMap: Map<string, FunctionNode>,
  newFunctions: Map<string, FunctionNode>,
  setup: BindingMatchSetup | null,
  bindingMatches: Map<string, string>,
  fnMatches: Map<string, string>
): ExternalRefEvidence | null {
  if (ambiguous.size === 0) return null;
  if (bindingMatches.size === 0 && fnMatches.size === 0) return null;

  const priorIdsByBinding = referenceIdsByBinding(
    setup?.priorById,
    priorFnMap.values()
  );
  const newIdsByBinding = referenceIdsByBinding(
    setup?.newById,
    newFunctions.values()
  );
  if (priorIdsByBinding.size === 0 || newIdsByBinding.size === 0) return null;

  const refMatches = new Map([...bindingMatches, ...fnMatches]);
  const oldRefs = new Map<string, Set<string>>();
  const newRefs = new Map<string, Set<string>>();
  const candidateIds = new Set<string>();
  for (const [oldId, candidates] of ambiguous) {
    const fn = priorFnMap.get(oldId);
    if (!fn) continue;
    oldRefs.set(oldId, collectReferencedBindingIds(fn, priorIdsByBinding));
    for (const candidate of candidates) candidateIds.add(candidate);
  }
  for (const candId of candidateIds) {
    const fn = newFunctions.get(candId);
    if (fn)
      newRefs.set(candId, collectReferencedBindingIds(fn, newIdsByBinding));
  }
  return { oldRefs, newRefs, refMatches };
}

/**
 * One side's reference-identity map: module-binding ids first,
 * function-holder ids second — a binding that is both (var t = () => ...)
 * resolves to the function id, whose match set grows through alternation.
 * Same merge order on both sides.
 */
function referenceIdsByBinding(
  bindingsById: Map<string, ModuleBindingNode> | undefined,
  functions: Iterable<FunctionNode>
): Map<babelTraverse.Binding, string> {
  const ids = bindingsById
    ? bindingIdsByBindingObject(bindingsById)
    : new Map<babelTraverse.Binding, string>();
  for (const [binding, fnId] of functionIdsByBinding(functions)) {
    ids.set(binding, fnId);
  }
  return ids;
}

/**
 * The binding that holds each function's value — a declaration's name
 * binding or the var declarator it is assigned to. Identity-guarded: the
 * resolved binding's declaration site must be this exact function, or it
 * is dropped (a shadowing name must not alias unrelated evidence).
 */
function functionIdsByBinding(
  functions: Iterable<FunctionNode>
): Map<babelTraverse.Binding, string> {
  const map = new Map<babelTraverse.Binding, string>();
  for (const fn of functions) {
    const binding = holdingBinding(fn);
    if (binding) map.set(binding, fn.sessionId);
  }
  return map;
}

/** Resolve a function's holding binding, or null when it has none. */
function holdingBinding(fn: FunctionNode): babelTraverse.Binding | null {
  const path = fn.path;
  if (path.isFunctionDeclaration()) {
    const id = path.node.id;
    if (!id) return null;
    const binding =
      path.parentPath?.scope.getBinding(id.name) ??
      path.scope.getBinding(id.name);
    return binding && binding.path.node === path.node ? binding : null;
  }
  const parent = path.parentPath;
  if (parent?.isVariableDeclarator() && t.isIdentifier(parent.node.id)) {
    const binding = parent.scope.getBinding(parent.node.id.name);
    return binding && binding.path.node === parent.node ? binding : null;
  }
  return null;
}

/** Resolve each module-binding node to its Binding object, keyed by it. */
function bindingIdsByBindingObject(
  byId: Map<string, ModuleBindingNode>
): Map<babelTraverse.Binding, string> {
  const map = new Map<babelTraverse.Binding, string>();
  for (const [sessionId, node] of byId) {
    const binding = node.scope.getBinding(node.name);
    if (binding) map.set(binding, sessionId);
  }
  return map;
}

/**
 * The module-binding sessionIds a function references, resolved per
 * occurrence (a name lookup from the function root would mis-resolve
 * shadowed occurrences).
 */
function collectReferencedBindingIds(
  fn: FunctionNode,
  idsByBinding: Map<babelTraverse.Binding, string>
): Set<string> {
  const refs = new Set<string>();
  fn.path.traverse({
    Identifier(idPath: babelTraverse.NodePath<t.Identifier>) {
      if (!idPath.isReferencedIdentifier()) return;
      const binding = idPath.scope.getBinding(idPath.node.name);
      if (!binding) return;
      const bindingId = idsByBinding.get(binding);
      if (bindingId) refs.add(bindingId);
    }
  });
  return refs;
}

/**
 * Derives rename records from the binding cascade's final result.
 * Injectivity (no new-side binding claimed twice) is enforced inside
 * matchFunctions — multi-claimed targets never reach here.
 */
function deriveBindingRenames(
  result: MatchResult,
  setup: BindingMatchSetup
): ModuleBindingRename[] {
  const renames: ModuleBindingRename[] = [];
  for (const [priorId, newId] of result.matches) {
    const prior = setup.priorById.get(priorId);
    const next = setup.newById.get(newId);
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
