/**
 * Unified rename plugin that works with any LLM provider.
 *
 * This replaces the legacy per-provider plugins (openaiRename, geminiRename, localRename)
 * with a single implementation that uses the RenameProcessor for parallel,
 * dependency-ordered function processing.
 */

import { parseSync } from "@babel/core";
import type { GeneratorOptions, GeneratorResult } from "@babel/generator";
import type * as babelTraverse from "@babel/traverse";
import * as t from "@babel/types";
import { buildUnifiedGraph } from "../analysis/function-graph.js";
import type { FunctionNode, RenameReport } from "../analysis/types.js";
import {
  findWrapperFunction,
  type WrapperFunctionResult
} from "../analysis/wrapper-detection.js";
import { matchPriorVersion } from "../cache/prior-version.js";
import { generate, traverse } from "../babel-utils.js";
import { debug } from "../debug.js";
import type { BundlerType, MinifierType } from "../detection/types.js";
import type { CommentRegion } from "../library-detection/comment-regions.js";
import { classifyFunctionsByRegion } from "../library-detection/comment-regions.js";
import type { FileContext } from "../pipeline/types.js";
import type { ProcessingMetrics } from "../llm/metrics.js";
import { MetricsTracker } from "../llm/metrics.js";
import type { LLMProvider } from "../llm/types.js";
import type { Profiler } from "../profiling/profiler.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import {
  buildCoverageSummary,
  type CoverageSummary,
  formatCoverageSummary
} from "./coverage.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { createIsEligible } from "./rename-eligibility.js";
import {
  LibraryPrefixResolver,
  sanitizeLibraryName
} from "./library-prefix-resolver.js";
import { RenameProcessor } from "./processor.js";

interface ScopeBinding {
  path: babelTraverse.NodePath;
  identifier: t.Identifier;
  referencePaths?: Array<{ node?: { loc?: { start?: { line?: number } } } }>;
}

/** Looser binding type for proximity windowing (only needs loc info, not path). */
interface ProximityBinding {
  identifier?: { loc?: { start?: { line?: number } } | null };
  referencePaths?: Array<{
    node?: { loc?: { start?: { line?: number } } | null };
  }>;
}

interface RenamePluginOptions {
  /** The LLM provider to use for name suggestions */
  provider: LLMProvider;

  /** Maximum number of concurrent function processing (default: 50) */
  concurrency?: number;

  /** Callback for progress updates (receives raw metrics) */
  onProgress?: (metrics: ProcessingMetrics) => void;

  /** Generate a source map alongside the output code */
  sourceMap?: boolean;

  /** Maximum number of module binding batches to process in parallel (default: 20) */
  moduleConcurrency?: number;

  /** Maximum identifiers per LLM batch (default: 10) */
  batchSize?: number;

  /** Per-identifier retry limit (default: 3) */
  maxRetriesPerIdentifier?: number;

  /** Cross-lane collision retry limit (default: 100) */
  maxFreeRetries?: number;

  /** Minimum bindings to enable parallel lanes (default: 25) */
  laneThreshold?: number;

  /** Profiler instance for performance instrumentation */
  profiler?: Profiler;

  /** Detected minifier type — used to select rename-eligibility rules */
  minifierType?: MinifierType;

  /** Detected bundler type — used to select rename-eligibility rules */
  bundlerType?: BundlerType;

  /**
   * When true (default), library functions in mixed files get deterministic
   * prefix renames instead of LLM processing. When false, all functions
   * (including library) go through the LLM.
   */
  skipLibraries?: boolean;

  /** Prior version humanified code for cross-version rename reuse. */
  priorVersionCode?: string;
}

/**
 * Result from the rename plugin, including output code and diagnostic reports.
 */
export interface RenamePluginResult {
  code: string;
  /** The post-rename AST, available for downstream consumers (e.g., split). */
  ast: t.File;
  reports: ReadonlyArray<RenameReport>;
  sourceMap: GeneratorResult["map"];
  coverageSummary?: string;
  coverageData?: CoverageSummary;
  /** Number of functions with renames transferred from prior version. */
  priorVersionApplied?: number;
  /** Number of functions matched but already correctly named. */
  priorVersionAlreadyNamed?: number;
  /** Number of module bindings matched from prior version. */
  priorVersionBindingsApplied?: number;
  /** Per-binding transfer stats for prior-version matching. */
  transferStats?: {
    exactMatch: TransferStats;
    closeMatch: TransferStats;
  };
}

// ---------------------------------------------------------------------------
// Internal helpers for createRenamePlugin phases
// ---------------------------------------------------------------------------

/** Mark the wrapper IIFE node as pre-done and return it if found. */
function markWrapperPreDone(
  graph: ReturnType<typeof buildUnifiedGraph>,
  preDone: FunctionNode[]
): void {
  if (!graph.wrapperPath) return;
  const wrapperNode = graph.wrapperPath.node;
  for (const [, renameNode] of graph.nodes) {
    if (
      renameNode.type === "function" &&
      renameNode.node.path.node === wrapperNode
    ) {
      renameNode.node.status = "done";
      renameNode.node.renameMapping = { names: {} };
      preDone.push(renameNode.node);
      debug.log(
        "wrapper",
        `Marked wrapper function ${renameNode.node.sessionId} as pre-done`
      );
      break;
    }
  }
}

/** Collect all FunctionNode entries from the graph. */
function collectAllFunctions(
  graph: ReturnType<typeof buildUnifiedGraph>
): FunctionNode[] {
  const result: FunctionNode[] = [];
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "function") {
      result.push(renameNode.node);
    }
  }
  return result;
}

/** Mark library functions as pre-done based on comment regions and return them with library names. */
function markLibraryFunctionsPreDone(
  allFunctions: FunctionNode[],
  commentRegions: CommentRegion[] | undefined,
  preDone: FunctionNode[]
): { libraryFunctions: FunctionNode[]; libraryMap: Map<string, string> } {
  const libraryFunctions: FunctionNode[] = [];
  const emptyResult = {
    libraryFunctions,
    libraryMap: new Map<string, string>()
  };
  if (!commentRegions || commentRegions.length === 0) return emptyResult;

  const libraryMap = classifyFunctionsByRegion(allFunctions, commentRegions);
  if (libraryMap.size === 0) return emptyResult;

  for (const fn of allFunctions) {
    if (libraryMap.has(fn.sessionId)) {
      fn.status = "done";
      fn.renameMapping = { names: {} };
      preDone.push(fn);
      libraryFunctions.push(fn);
    }
  }
  debug.log("mixed-file", `Skipping ${libraryMap.size} library functions`);
  return { libraryFunctions, libraryMap };
}

/** Run the main rename pass on the unified graph. */
async function runRenamePass(
  ast: ReturnType<typeof parseSync>,
  graph: ReturnType<typeof buildUnifiedGraph>,
  provider: LLMProvider,
  options: RenamePluginOptions,
  metrics: MetricsTracker,
  preDone: FunctionNode[],
  profiler: Profiler,
  isEligible: IsEligibleFn
): Promise<{ processor: RenameProcessor; allReports: RenameReport[] }> {
  const { concurrency = 50 } = options;
  const processor = new RenameProcessor(ast as t.File);
  let allReports: RenameReport[] = [];

  if (graph.nodes.size > 0) {
    await processor.processUnified(graph, provider, {
      concurrency,
      moduleConcurrency: options.moduleConcurrency,
      metrics,
      preDone: preDone.length > 0 ? preDone : undefined,
      batchSize: options.batchSize,
      maxRetriesPerIdentifier: options.maxRetriesPerIdentifier,
      maxFreeRetries: options.maxFreeRetries,
      laneThreshold: options.laneThreshold,
      profiler,
      isEligible,
      bundlerType: options.bundlerType
    });
    allReports = [...processor.reports];
  }

  return { processor, allReports };
}

/**
 * Apply library prefix renames to library functions.
 *
 * Instead of sending library functions through the LLM, deterministically
 * rename their bindings by prefixing with the sanitized library name.
 * e.g., react-dom: Xuo -> react_dom_Xuo
 */
interface LibraryPrefixResult {
  reports: RenameReport[];
  /** Count of library functions skipped because they had no minified bindings */
  libraryNoMinified: number;
}

function runLibraryPrefixPass(
  libraryFunctions: FunctionNode[],
  libraryMap: Map<string, string>,
  isEligible: IsEligibleFn,
  existingReports: RenameReport[]
): LibraryPrefixResult {
  if (libraryFunctions.length === 0 || libraryMap.size === 0) {
    return { reports: existingReports, libraryNoMinified: 0 };
  }

  const newReports: RenameReport[] = [];
  let libraryNoMinified = 0;

  for (const fn of libraryFunctions) {
    const libName = libraryMap.get(fn.sessionId);
    if (!libName) continue;

    const prefix = sanitizeLibraryName(libName);
    const resolver = new LibraryPrefixResolver(prefix);
    const scope = fn.path.scope;
    const bindings = Object.entries(scope.bindings).filter(([name]) =>
      isEligible(name)
    );

    if (bindings.length === 0) {
      libraryNoMinified++;
      continue;
    }

    const identifiers = bindings.map(([name]) => name);
    const names = resolver.resolveNames(identifiers);
    const outcomes: Record<
      string,
      import("../analysis/types.js").IdentifierOutcome
    > = {};

    for (const [oldName, newName] of Object.entries(names)) {
      scope.rename(oldName, newName);
      outcomes[oldName] = { status: "renamed", newName, round: 1 };
    }

    fn.renameMapping = { names };
    fn.renameReport = {
      type: "function",
      strategy: "library-prefix",
      targetId: fn.sessionId,
      totalIdentifiers: identifiers.length,
      renamedCount: identifiers.length,
      outcomes
    };
    newReports.push(fn.renameReport);
  }

  debug.log(
    "library-prefix",
    `Applied library prefix to ${newReports.length} functions`
  );

  return { reports: [...existingReports, ...newReports], libraryNoMinified };
}

/** Detect library functions and mark them as pre-done. */
function detectAndMarkLibraries(
  options: RenamePluginOptions,
  graph: ReturnType<typeof buildUnifiedGraph>,
  context: FileContext | undefined,
  allFunctions: FunctionNode[],
  preDone: FunctionNode[]
): { libraryFunctions: FunctionNode[]; libraryMap: Map<string, string> } {
  const skipLibs = options.skipLibraries ?? true;
  const commentRegions =
    !skipLibs || graph.wrapperPath ? undefined : context?.commentRegions;
  return markLibraryFunctionsPreDone(allFunctions, commentRegions, preDone);
}

export interface TransferStats {
  attempted: number;
  applied: number;
  skipped: number;
}

interface ExternalRefPair {
  /** Minified name in the new version */
  oldName: string;
  /** Humanified name from the prior version */
  newName: string;
  /** Session ID of the matched function that produced this pair */
  sourceFunctionId: string;
}

type BindingMap = Map<string, babelTraverse.NodePath["scope"]>;

/** Collect bindings from a scope where binding.scope === the scope itself. */
function collectOwnScopeBindings(
  scope: babelTraverse.NodePath["scope"],
  map: BindingMap
): void {
  for (const [name, binding] of Object.entries(scope.bindings)) {
    if (binding.scope === scope && !map.has(name)) {
      map.set(name, scope);
    }
  }
}

/** Collect body scope bindings when params have defaults/destructuring. */
function collectBodyScopeBindingsForMap(
  fnPath: babelTraverse.NodePath<t.Function>,
  map: BindingMap
): void {
  const bodyPath = fnPath.get("body");
  if (Array.isArray(bodyPath) || !bodyPath.isBlockStatement()) return;
  const bodyScope = bodyPath.scope;
  if (bodyScope === fnPath.scope) return;
  collectOwnScopeBindings(bodyScope, map);
}

/** Traverse nested block scopes and collect bindings into the map. */
function collectNestedScopeBindingsForMap(
  fnPath: babelTraverse.NodePath<t.Function>,
  map: BindingMap
): void {
  const seen = new Set(map.keys());
  fnPath.traverse({
    Function(path: babelTraverse.NodePath<t.Function>) {
      if (path !== fnPath) path.skip();
    },
    BlockStatement(path: babelTraverse.NodePath<t.BlockStatement>) {
      if (path.parentPath === fnPath) return;
      collectBlockScopeBindings(path, seen, map);
    },
    ForStatement(path: babelTraverse.NodePath<t.ForStatement>) {
      collectBlockScopeBindings(path, seen, map);
    },
    ForInStatement(path: babelTraverse.NodePath<t.ForInStatement>) {
      collectBlockScopeBindings(path, seen, map);
    },
    ForOfStatement(path: babelTraverse.NodePath<t.ForOfStatement>) {
      collectBlockScopeBindings(path, seen, map);
    },
    SwitchStatement(path: babelTraverse.NodePath<t.SwitchStatement>) {
      collectBlockScopeBindings(path, seen, map);
    },
    CatchClause(path: babelTraverse.NodePath<t.CatchClause>) {
      collectBlockScopeBindings(path, seen, map);
    }
  });
}

/** Add the function declaration's own name binding from the parent scope. */
function collectFunctionDeclNameForMap(
  fnPath: babelTraverse.NodePath<t.Function>,
  map: BindingMap
): void {
  if (!fnPath.isFunctionDeclaration() || !fnPath.node.id) return;
  const name = fnPath.node.id.name;
  const parentScope = fnPath.parentPath?.scope;
  if (parentScope?.bindings[name] && !map.has(name)) {
    map.set(name, parentScope);
  }
}

/**
 * Builds a map of ALL bindings owned by a function, including nested block scopes.
 * Returns Map<name, scope> so callers can look up and rename any function-owned binding.
 */
function buildFunctionBindingMap(fn: FunctionNode): BindingMap {
  const map: BindingMap = new Map();
  const fnPath = fn.path;

  collectOwnScopeBindings(fnPath.scope, map);
  collectBodyScopeBindingsForMap(fnPath, map);
  collectNestedScopeBindingsForMap(fnPath, map);
  collectFunctionDeclNameForMap(fnPath, map);

  return map;
}

/** Collect bindings from a block scope into the binding map. */
function collectBlockScopeBindings(
  path: babelTraverse.NodePath,
  seen: Set<string>,
  map: Map<string, babelTraverse.NodePath["scope"]>
): void {
  const blockScope = path.scope;
  for (const [name, binding] of Object.entries(blockScope.bindings)) {
    if (binding.scope === blockScope && !seen.has(name)) {
      seen.add(name);
      map.set(name, blockScope);
    }
  }
}

/** Apply matched renames to AST scopes and mark functions as pre-done. */
function applyMatchedRenames(
  allFunctions: FunctionNode[],
  preDone: FunctionNode[]
): { stats: TransferStats; externalRefs: ExternalRefPair[] } {
  const stats: TransferStats = { attempted: 0, applied: 0, skipped: 0 };
  const externalRefs: ExternalRefPair[] = [];
  for (const fn of allFunctions) {
    if (fn.status !== "done" && fn.renameMapping) {
      const bindingMap = buildFunctionBindingMap(fn);
      for (const [oldName, newName] of Object.entries(fn.renameMapping.names)) {
        if (oldName === newName) continue;
        stats.attempted++;
        const scope = bindingMap.get(oldName);
        if (!scope) {
          stats.skipped++;
          externalRefs.push({
            oldName,
            newName,
            sourceFunctionId: fn.sessionId
          });
          debug.log(
            "prior-version",
            `exact-match: skipping ${oldName}→${newName} in ${fn.sessionId}: external reference (not a function-owned binding)`
          );
          continue;
        }
        scope.rename(oldName, newName);
        stats.applied++;
      }
      fn.status = "done";
      preDone.push(fn);
    }
  }
  return { stats, externalRefs };
}

/** Apply close-match name transfers and attach prior-version context. */
function attachCloseMatchContext(
  closeMatchContext: Map<
    string,
    import("../cache/prior-version.js").CloseMatchInfo
  >,
  functionMap: Map<string, FunctionNode>
): { stats: TransferStats; externalRefs: ExternalRefPair[] } {
  const stats: TransferStats = { attempted: 0, applied: 0, skipped: 0 };
  const externalRefs: ExternalRefPair[] = [];
  for (const [newId, info] of closeMatchContext) {
    const fn = functionMap.get(newId);
    if (!fn || fn.renameMapping) continue;
    applyCloseMatchTransfers(fn, info.nameTransfers, stats, externalRefs);
    fn.priorVersionContext = info.priorCode;
  }
  return { stats, externalRefs };
}

/** Transfer individual close-match bindings for a single function. */
function applyCloseMatchTransfers(
  fn: FunctionNode,
  nameTransfers: Record<string, string>,
  stats: TransferStats,
  externalRefs: ExternalRefPair[]
): void {
  const bindingMap = buildFunctionBindingMap(fn);
  const transferred = new Set<string>();
  for (const [oldName, newName] of Object.entries(nameTransfers)) {
    if (oldName === newName) continue;
    stats.attempted++;
    const scope = bindingMap.get(oldName);
    if (!scope) {
      stats.skipped++;
      externalRefs.push({
        oldName,
        newName,
        sourceFunctionId: fn.sessionId
      });
      debug.log(
        "prior-version",
        `close-match: skipping ${oldName}→${newName} in ${fn.sessionId}: external reference (not a function-owned binding)`
      );
      continue;
    }
    if (scope.bindings[newName]) {
      stats.skipped++;
      continue;
    }
    scope.rename(oldName, newName);
    transferred.add(newName);
    stats.applied++;
  }
  if (transferred.size > 0) {
    fn.priorVersionTransferred = transferred;
  }
}

/** Apply prior-version matching and mark matched functions as pre-done. */
function applyPriorVersionIfPresent(
  priorVersionCode: string | undefined,
  allFunctions: FunctionNode[],
  graph: ReturnType<typeof buildUnifiedGraph>,
  preDone: FunctionNode[]
): {
  priorVersionApplied: number;
  priorVersionAlreadyNamed: number;
  priorVersionBindingsApplied: number;
  priorVersionCloseMatch: number;
  transferStats?: { exactMatch: TransferStats; closeMatch: TransferStats };
} {
  if (!priorVersionCode) {
    return {
      priorVersionApplied: 0,
      priorVersionAlreadyNamed: 0,
      priorVersionBindingsApplied: 0,
      priorVersionCloseMatch: 0
    };
  }

  // Extract module binding nodes from the graph
  const moduleBindings: import("../analysis/types.js").ModuleBindingNode[] = [];
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "module-binding") {
      moduleBindings.push(renameNode.node);
    }
  }

  const currentFunctionMap = new Map<string, FunctionNode>();
  for (const fn of allFunctions) {
    currentFunctionMap.set(fn.sessionId, fn);
  }
  const priorResult = matchPriorVersion(
    priorVersionCode,
    currentFunctionMap,
    moduleBindings
  );

  const { stats: exactMatchStats, externalRefs: exactExternalRefs } =
    applyMatchedRenames(allFunctions, preDone);
  const { stats: closeMatchStats, externalRefs: closeExternalRefs } =
    attachCloseMatchContext(priorResult.closeMatchContext, currentFunctionMap);

  // Apply module binding renames and remove matched bindings from the graph
  if (priorResult.moduleBindingRenames) {
    applyModuleBindingRenames(priorResult.moduleBindingRenames, graph);
  }

  // Phase 3: Propagate external references to unmatched module bindings
  // and close-matched parent function locals (closure captures)
  const allExternalRefs = [...exactExternalRefs, ...closeExternalRefs];
  const propagation = propagateExternalReferences(
    allExternalRefs,
    graph,
    allFunctions
  );

  const totalBindingsApplied =
    priorResult.moduleBindingsMatched + propagation.moduleBindingsApplied;

  debug.log(
    "prior-version",
    `Matched ${priorResult.functionsMatched} functions (${priorResult.functionsAlreadyNamed} already named), ` +
      `${priorResult.closeMatchCount} close matches, ` +
      `${priorResult.moduleBindingsMatched} bindings from prior version` +
      (propagation.moduleBindingsApplied > 0
        ? `, ${propagation.moduleBindingsApplied} propagated module bindings`
        : "") +
      (propagation.closureCapturesApplied > 0
        ? `, ${propagation.closureCapturesApplied} propagated closure captures`
        : "")
  );

  return {
    priorVersionApplied: priorResult.functionsMatched,
    priorVersionAlreadyNamed: priorResult.functionsAlreadyNamed,
    priorVersionBindingsApplied: totalBindingsApplied,
    priorVersionCloseMatch: priorResult.closeMatchCount,
    transferStats: { exactMatch: exactMatchStats, closeMatch: closeMatchStats }
  };
}

/** Apply matched module binding renames to AST scopes and mark as done. */
function applyModuleBindingRenames(
  renames: import("../cache/prior-version.js").ModuleBindingRename[],
  graph: ReturnType<typeof buildUnifiedGraph>
): void {
  for (const { oldName, newName, scope } of renames) {
    scope.rename(oldName, newName);

    // Mark the binding node as done and remove from graph
    const sessionId = `module:${oldName}`;
    const renameNode = graph.nodes.get(sessionId);
    if (renameNode && renameNode.type === "module-binding") {
      renameNode.node.status = "done";
      graph.nodes.delete(sessionId);
    }
  }
}

/** Get the top vote from a vote map, or null if tied. */
function getTopVote(votes: Map<string, number>): string | null {
  let topName: string | null = null;
  let topCount = 0;
  let tied = false;
  for (const [name, count] of votes) {
    if (count > topCount) {
      topName = name;
      topCount = count;
      tied = false;
    } else if (count === topCount) {
      tied = true;
    }
  }
  return tied ? null : topName;
}

interface PropagationResult {
  moduleBindingsApplied: number;
  closureCapturesApplied: number;
}

interface ClosureVoteEntry {
  oldName: string;
  ownerFn: FunctionNode;
  ownerScope: babelTraverse.NodePath["scope"];
  votes: Map<string, number>;
}

/** Add a vote to a nested vote map, creating entries as needed. */
function addVote(
  voteMap: Map<string, Map<string, number>>,
  key: string,
  value: string
): void {
  let votes = voteMap.get(key);
  if (!votes) {
    votes = new Map();
    voteMap.set(key, votes);
  }
  votes.set(value, (votes.get(value) || 0) + 1);
}

/** Classify an external ref as a closure capture and add to closureVotes. */
function classifyClosureCapture(
  ref: ExternalRefPair,
  functionMap: Map<string, FunctionNode>,
  scopeToFunction: Map<babelTraverse.NodePath["scope"], FunctionNode>,
  closureVotes: Map<string, ClosureVoteEntry>
): void {
  const sourceFn = functionMap.get(ref.sourceFunctionId);
  if (!sourceFn) return;

  const binding = sourceFn.path.scope.getBinding(ref.oldName);
  if (!binding) return;

  const ownerFn = scopeToFunction.get(binding.scope);
  if (!ownerFn || !ownerFn.priorVersionContext) return;

  const key = `${ownerFn.sessionId}:${ref.oldName}`;
  let entry = closureVotes.get(key);
  if (!entry) {
    entry = {
      oldName: ref.oldName,
      ownerFn,
      ownerScope: binding.scope,
      votes: new Map()
    };
    closureVotes.set(key, entry);
  }
  entry.votes.set(ref.newName, (entry.votes.get(ref.newName) || 0) + 1);
}

/** Apply propagated module binding renames via voting. */
function applyPropagatedModuleBindings(
  moduleVotes: Map<string, Map<string, number>>,
  unmatchedModuleBindings: Map<
    string,
    import("../analysis/types.js").ModuleBindingNode
  >,
  graph: ReturnType<typeof buildUnifiedGraph>
): number {
  let applied = 0;
  for (const [minifiedName, votes] of moduleVotes) {
    const bindingNode = unmatchedModuleBindings.get(minifiedName);
    if (!bindingNode) continue;

    const topName = getTopVote(votes);
    if (!topName) {
      debug.log(
        "prior-version",
        `propagated: module-binding ${minifiedName} skipped (tied votes)`
      );
      continue;
    }

    if (bindingNode.scope.bindings[topName]) {
      debug.log(
        "prior-version",
        `propagated: module-binding ${minifiedName}→${topName} skipped (name collision)`
      );
      continue;
    }

    const voteCount = votes.get(topName) ?? 0;
    bindingNode.scope.rename(minifiedName, topName);
    bindingNode.status = "done";
    graph.nodes.delete(bindingNode.sessionId);
    applied++;
    debug.log(
      "prior-version",
      `propagated: module-binding ${minifiedName}→${topName} (${voteCount} vote${voteCount > 1 ? "s" : ""} from matched functions)`
    );
  }
  return applied;
}

/** Apply propagated closure capture renames via voting. */
function applyPropagatedClosureCaptures(
  closureVotes: Map<string, ClosureVoteEntry>
): number {
  let applied = 0;
  for (const [, entry] of closureVotes) {
    const topName = getTopVote(entry.votes);
    if (!topName) continue;

    if (entry.ownerFn.priorVersionTransferred?.has(topName)) continue;
    if (!entry.ownerScope.bindings[entry.oldName]) continue;
    if (entry.ownerScope.bindings[topName]) continue;

    entry.ownerScope.rename(entry.oldName, topName);
    if (!entry.ownerFn.priorVersionTransferred) {
      entry.ownerFn.priorVersionTransferred = new Set();
    }
    entry.ownerFn.priorVersionTransferred.add(topName);
    applied++;
    debug.log(
      "prior-version",
      `propagated: closure-capture ${entry.oldName}→${topName} in ${entry.ownerFn.sessionId}`
    );
  }
  return applied;
}

/**
 * Propagates external reference pairs from matched functions to:
 * (a) unmatched module bindings (via voting from referencing functions)
 * (b) close-matched parent function locals (closure captures from exact-matched children)
 */
function propagateExternalReferences(
  externalRefs: ExternalRefPair[],
  graph: ReturnType<typeof buildUnifiedGraph>,
  allFunctions: FunctionNode[]
): PropagationResult {
  if (externalRefs.length === 0) {
    return { moduleBindingsApplied: 0, closureCapturesApplied: 0 };
  }

  const unmatchedModuleBindings = new Map<
    string,
    import("../analysis/types.js").ModuleBindingNode
  >();
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "module-binding") {
      unmatchedModuleBindings.set(renameNode.node.name, renameNode.node);
    }
  }

  const scopeToFunction = new Map<
    babelTraverse.NodePath["scope"],
    FunctionNode
  >();
  const functionMap = new Map<string, FunctionNode>();
  for (const fn of allFunctions) {
    scopeToFunction.set(fn.path.scope, fn);
    functionMap.set(fn.sessionId, fn);
  }

  const moduleVotes = new Map<string, Map<string, number>>();
  const closureVotes = new Map<string, ClosureVoteEntry>();

  for (const ref of externalRefs) {
    if (unmatchedModuleBindings.has(ref.oldName)) {
      addVote(moduleVotes, ref.oldName, ref.newName);
    } else {
      classifyClosureCapture(ref, functionMap, scopeToFunction, closureVotes);
    }
  }

  return {
    moduleBindingsApplied: applyPropagatedModuleBindings(
      moduleVotes,
      unmatchedModuleBindings,
      graph
    ),
    closureCapturesApplied: applyPropagatedClosureCaptures(closureVotes)
  };
}

/**
 * Creates a rename plugin that processes all functions in dependency order
 * using the provided LLM provider.
 *
 * @param options Configuration options for the rename plugin
 * @returns An async function that transforms code and returns reports
 */
export function createRenamePlugin(options: RenamePluginOptions) {
  const { provider, onProgress } = options;
  const profiler = options.profiler ?? NULL_PROFILER;
  const isEligible: IsEligibleFn = createIsEligible(
    options.bundlerType,
    options.minifierType
  );

  return async (
    code: string,
    context?: FileContext
  ): Promise<RenamePluginResult> => {
    const originalCode = code;

    const parseSpan = profiler.startSpan("parse", "pipeline");
    const ast = parseSync(code, {
      sourceType: "unambiguous"
    });
    parseSpan.end({ codeLength: code.length });

    if (!ast) {
      throw new Error("Failed to parse code");
    }

    const metrics = new MetricsTracker({
      onMetrics: (m) => onProgress?.(m)
    });

    const genOpts: GeneratorOptions = options.sourceMap
      ? { sourceMaps: true, sourceFileName: "input.js" }
      : {};
    const genSource = options.sourceMap ? originalCode : undefined;

    // Step 1: Build unified graph (functions + module-level bindings)
    metrics.setStage("building-graph");
    const graphSpan = profiler.startSpan("graph-build", "pipeline");
    const graph = buildUnifiedGraph(ast, "input.js", profiler, isEligible);
    graphSpan.end({ nodeCount: graph.nodes.size });

    if (graph.nodes.size === 0) {
      const output = generate(ast, genOpts, genSource);
      return {
        code: output.code,
        ast: ast as t.File,
        reports: [],
        sourceMap: output.map
      };
    }

    // Collect pre-done nodes (library + wrapper IIFE)
    const preDone: FunctionNode[] = [];

    // Mark wrapper IIFE as pre-done so its children can process without deadlock
    markWrapperPreDone(graph, preDone);

    // Collect all function nodes for library detection
    const allFunctions = collectAllFunctions(graph);

    // Filter out library functions from mixed files (Layer 3)
    const { libraryFunctions, libraryMap } = detectAndMarkLibraries(
      options,
      graph,
      context,
      allFunctions,
      preDone
    );

    // Apply prior-version matching if provided
    const {
      priorVersionApplied,
      priorVersionAlreadyNamed,
      priorVersionBindingsApplied,
      priorVersionCloseMatch,
      transferStats
    } = applyPriorVersionIfPresent(
      options.priorVersionCode,
      allFunctions,
      graph,
      preDone
    );

    // Remove pre-done function nodes from the graph's active set
    // (they'll be in preDone for dependency tracking but won't be processed)
    for (const fn of preDone) {
      graph.nodes.delete(fn.sessionId);
    }

    // Step 2: Process unified graph in a single parallel pass
    metrics.setStage("renaming");
    const renameSpan = profiler.startSpan("rename:functions", "pipeline");
    const { processor, allReports: renameReports } = await runRenamePass(
      ast,
      graph,
      provider,
      options,
      metrics,
      preDone,
      profiler,
      isEligible
    );
    renameSpan.end({ processedCount: renameReports.length });

    // Step 3: Apply library prefix renames (deterministic, no LLM calls)
    metrics.setStage("library-prefix");
    const libPrefixSpan = profiler.startSpan(
      "rename:library-prefix",
      "pipeline"
    );
    const { reports: allReports, libraryNoMinified } = runLibraryPrefixPass(
      libraryFunctions,
      libraryMap,
      isEligible,
      renameReports
    );
    libPrefixSpan.end();

    const totalSkippedBySkipList = processor.skippedBySkipList;
    const coverage = buildCoverageSummary(
      allReports,
      allFunctions.length,
      metrics.getMetrics(),
      totalSkippedBySkipList,
      processor.skipReasons,
      libraryNoMinified,
      priorVersionApplied,
      priorVersionAlreadyNamed,
      priorVersionBindingsApplied,
      priorVersionCloseMatch
    );
    const coverageSummary = formatCoverageSummary(coverage);

    metrics.setStage("generating");
    const generateSpan = profiler.startSpan("generate", "pipeline");
    const output = generate(ast, genOpts, genSource);
    generateSpan.end({ codeLength: output.code.length });
    metrics.setStage("done");
    return {
      code: output.code,
      ast: ast as t.File,
      reports: allReports,
      sourceMap: output.map,
      coverageSummary,
      coverageData: coverage,
      priorVersionApplied,
      priorVersionAlreadyNamed,
      priorVersionBindingsApplied,
      transferStats
    };
  };
}

/** Maximum identifiers per batch for module-level renaming */
const _MODULE_BATCH_SIZE = 5;

interface ModuleBinding {
  name: string;
  identifier: t.Identifier;
  declaration: string;
}

/**
 * Result of collecting module-level bindings.
 */
interface ModuleLevelBindingsResult {
  bindings: ModuleBinding[];
  /** The scope used for renaming (program scope or wrapper IIFE scope) */
  targetScope: babelTraverse.Scope;
  /** If a wrapper IIFE was detected, the path to it */
  wrapperPath?: babelTraverse.NodePath<t.Function>;
}

/**
 * Returns the declaration text for a function/class declaration binding path.
 */
function getFunctionOrClassDeclarationText(
  name: string,
  bindingPath: babelTraverse.NodePath
): string {
  try {
    const fullCode = generate(bindingPath.node).code;
    const lines = fullCode.split("\n");
    if (lines.length > 10) {
      return `${lines.slice(0, 10).join("\n")}\n  // ...`;
    }
    return fullCode;
  } catch {
    const params = ((bindingPath.node as t.FunctionDeclaration).params ?? [])
      .map((p: t.Node) => generate(p).code)
      .join(", ");
    return `function ${name}(${params}) { ... }`;
  }
}

/**
 * Returns the declaration text for a variable declarator binding path.
 */
function getVariableDeclaratorText(
  bindingPath: babelTraverse.NodePath
): string {
  const declPath = bindingPath.parentPath;
  if (declPath) {
    return generate(declPath.node).code;
  }
  return "";
}

/**
 * Returns the declaration text for an import specifier binding path.
 */
function getImportSpecifierText(bindingPath: babelTraverse.NodePath): string {
  const importPath = bindingPath.parentPath;
  if (importPath) {
    return generate(importPath.node).code;
  }
  return "";
}

/**
 * Derives a human-readable declaration string for a binding path.
 */
function getDeclarationText(
  name: string,
  bindingPath: babelTraverse.NodePath
): string {
  if (bindingPath.isFunctionDeclaration() || bindingPath.isClassDeclaration()) {
    return getFunctionOrClassDeclarationText(name, bindingPath);
  }
  if (bindingPath.isVariableDeclarator()) {
    return getVariableDeclaratorText(bindingPath);
  }
  if (
    bindingPath.isImportSpecifier() ||
    bindingPath.isImportDefaultSpecifier() ||
    bindingPath.isImportNamespaceSpecifier()
  ) {
    return getImportSpecifierText(bindingPath);
  }
  return generate(bindingPath.node).code;
}

/**
 * Returns true if a binding should be skipped (function/class declarations
 * when NOT in wrapper mode, or named function/class expressions stored in variables).
 */
function shouldSkipBinding(
  bindingPath: babelTraverse.NodePath,
  wrapper: WrapperFunctionResult | null
): boolean {
  // Skip function/class declarations when NOT in wrapper mode
  if (!wrapper) {
    if (
      bindingPath.isFunctionDeclaration() ||
      bindingPath.isClassDeclaration()
    ) {
      return true;
    }
  }

  // For variable declarators, skip if init is a NAMED function/class expression
  if (bindingPath.isVariableDeclarator()) {
    const init = (bindingPath.node as t.VariableDeclarator).init;
    if (
      (t.isFunctionExpression(init) && init.id) ||
      (t.isClassExpression(init) && init.id)
    ) {
      return true;
    }
  }

  return false;
}

/**
 * Collects module-level bindings that look minified and aren't functions/classes.
 * When a giant wrapper IIFE is detected, uses the wrapper's scope instead of programScope.
 */
export function getModuleLevelBindings(
  ast: t.File,
  isEligibleOverride?: IsEligibleFn
): ModuleLevelBindingsResult | null {
  let programScope: babelTraverse.Scope | null = null;
  const bindings: ModuleBinding[] = [];

  traverse(ast, {
    Program(path: babelTraverse.NodePath<t.Program>) {
      programScope = path.scope;
      path.stop();
    }
  });

  if (!programScope) return null;

  // Check for wrapper function — use its scope instead of programScope when detected
  const wrapper = findWrapperFunction(ast);
  const targetScope = wrapper ? wrapper.scope : programScope;

  const isEligible = isEligibleOverride ?? createIsEligible();
  for (const [name, binding] of Object.entries(targetScope.bindings) as [
    string,
    ScopeBinding
  ][]) {
    if (!isEligible(name)) continue;

    const bindingPath = binding.path;

    if (shouldSkipBinding(bindingPath, wrapper)) continue;

    const declaration = getDeclarationText(name, bindingPath);

    bindings.push({
      name,
      identifier: binding.identifier,
      declaration
    });
  }

  if (bindings.length === 0) return null;

  return {
    bindings,
    targetScope,
    wrapperPath: wrapper?.functionPath
  };
}

/** Maximum number of usage/assignment context snippets per identifier */
const MAX_CONTEXT_SNIPPETS = 10;
/** Maximum character length for a single context snippet */
const MAX_SNIPPET_CHARS = 800;
/** Maximum lines to take from a single statement */
const MAX_SNIPPET_LINES = 10;

/** Well-known names that should always appear in usedNames regardless of proximity */
const WELL_KNOWN_NAMES = new Set([
  "exports",
  "require",
  "module",
  "__filename",
  "__dirname",
  "console",
  "process",
  "Buffer",
  "Promise",
  "Object",
  "Array",
  "Map",
  "Set",
  "Error",
  "JSON",
  "Math",
  "undefined",
  "null",
  "NaN",
  "Infinity",
  "setTimeout",
  "setInterval",
  "clearTimeout",
  "clearInterval",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "encodeURI",
  "decodeURI",
  "encodeURIComponent",
  "decodeURIComponent"
]);

/** Minimum scope bindings before activating proximity windowing */
const WINDOWING_THRESHOLD = 100;

/** Line proximity radius for usedNames windowing */
const PROXIMITY_RADIUS = 100;

/**
 * Returns true if a name is within the proximity window given the binding info.
 */
function isNameInProximityWindow(
  _name: string,
  binding: ProximityBinding | undefined,
  minLine: number,
  maxLine: number,
  alreadyIncluded: boolean
): boolean {
  if (alreadyIncluded) return false;
  if (!binding) return true; // include if binding not found, to be safe

  const declLine = binding.identifier?.loc?.start?.line;
  if (declLine !== undefined && declLine >= minLine && declLine <= maxLine) {
    return true;
  }

  if (binding.referencePaths) {
    for (const refPath of binding.referencePaths) {
      const refLine = refPath.node?.loc?.start?.line;
      if (refLine !== undefined && refLine >= minLine && refLine <= maxLine) {
        return true;
      }
    }
  }

  return false;
}

/**
 * Computes a proximity-windowed subset of usedNames for module-level prompts.
 *
 * When the scope has >= WINDOWING_THRESHOLD bindings, only returns names whose
 * own declarations/references fall within +-PROXIMITY_RADIUS lines of the batch's
 * relevant lines. Always includes well-known names and excludes minified-looking names.
 *
 * When the scope has fewer bindings, returns all non-minified names.
 */
export function getProximateUsedNames(
  allUsedNames: Set<string>,
  batchLines: number[],
  scopeBindings: Record<string, ProximityBinding>,
  totalBindings: number,
  isEligibleOverride?: IsEligibleFn
): Set<string> {
  const result = new Set<string>();

  // Always include well-known names that are in scope
  for (const name of allUsedNames) {
    if (WELL_KNOWN_NAMES.has(name)) {
      result.add(name);
    }
  }

  // Filter out eligible names (they'll be renamed) — keep only preserved names
  const isEligible = isEligibleOverride ?? createIsEligible();
  const preserved = [...allUsedNames].filter((n) => !isEligible(n));

  // If below threshold, return all non-minified names
  if (totalBindings < WINDOWING_THRESHOLD) {
    for (const name of preserved) {
      result.add(name);
    }
    return result;
  }

  // Compute the proximity window
  const minLine = Math.min(...batchLines) - PROXIMITY_RADIUS;
  const maxLine = Math.max(...batchLines) + PROXIMITY_RADIUS;

  for (const name of preserved) {
    if (
      isNameInProximityWindow(
        name,
        scopeBindings[name],
        minLine,
        maxLine,
        result.has(name)
      )
    ) {
      result.add(name);
    }
  }

  return result;
}

/**
 * Truncates generated code to up to MAX_SNIPPET_LINES lines and MAX_SNIPPET_CHARS chars.
 * Returns null if the result is empty.
 */
function truncateSnippet(code: string): string | null {
  const lines = code.split("\n").slice(0, MAX_SNIPPET_LINES);
  let snippet = lines.join("\n").trim();
  if (snippet.length > MAX_SNIPPET_CHARS) {
    snippet = `${snippet.slice(0, MAX_SNIPPET_CHARS)}…`;
  }
  return snippet || null;
}

/**
 * Extracts the identifier name from the LHS of an assignment expression, or null.
 * Handles: direct assignment (x = ...), property (x.foo = ...), prototype (x.prototype.m = ...).
 */
function extractAssignmentTargetName(
  left: t.LVal | t.OptionalMemberExpression,
  identifiers: Set<string>
): string | null {
  // Direct assignment: x = ...
  if (t.isIdentifier(left) && identifiers.has(left.name)) {
    return left.name;
  }

  // Property / prototype assignment: x.foo = ... or x.prototype.m = ...
  if (t.isMemberExpression(left)) {
    const obj = left.object;
    // x.prototype.method — drill through one level
    if (
      t.isMemberExpression(obj) &&
      t.isIdentifier(obj.object) &&
      identifiers.has(obj.object.name)
    ) {
      return obj.object.name;
    }
    if (t.isIdentifier(obj) && identifiers.has(obj.name)) {
      return obj.name;
    }
  }

  return null;
}

/**
 * Collects assignment context for module-level identifiers.
 * Finds direct assignments (Cj = ...), property assignments (Cj.create = ...),
 * and prototype assignments (Cj.prototype.method = ...).
 */
export function collectAssignmentContext(
  ast: t.File,
  identifiers: Set<string>
): Record<string, string[]> {
  const assignments: Record<string, string[]> = {};
  for (const id of identifiers) {
    assignments[id] = [];
  }

  traverse(ast, {
    AssignmentExpression(path: babelTraverse.NodePath<t.AssignmentExpression>) {
      const name = extractAssignmentTargetName(path.node.left, identifiers);

      if (!name) return;
      if (assignments[name].length >= MAX_CONTEXT_SNIPPETS) return;

      // Get the containing expression statement for cleaner output
      const statement = path.findParent((p: babelTraverse.NodePath) =>
        p.isStatement()
      );
      const node = statement ? statement.node : path.node;

      try {
        const code = generate(node).code;
        const snippet = truncateSnippet(code);
        if (snippet && !assignments[name].includes(snippet)) {
          assignments[name].push(snippet);
        }
      } catch {
        // Skip if generation fails
      }
    }
  });

  return assignments;
}

/**
 * Returns the containing statement/declaration for an Identifier path, for usage context.
 * Returns null if this identifier should be skipped (declaration, assignment LHS, etc.).
 */
function getIdentifierUsageStatement(
  path: babelTraverse.NodePath<t.Identifier>,
  name: string,
  identifiers: Set<string>,
  assignmentCounts: Record<string, number>,
  examples: Record<string, string[]>
): babelTraverse.NodePath | null {
  if (!identifiers.has(name)) return null;

  // Cap total context: assignments + usages ≤ MAX_CONTEXT_SNIPPETS
  const remaining = MAX_CONTEXT_SNIPPETS - (assignmentCounts[name] || 0);
  if (examples[name].length >= remaining) return null;

  // Skip the declaration itself
  if (path.isBindingIdentifier()) return null;

  // Cast needed: after isBindingIdentifier() narrows to `never`, TS loses parent/findParent
  const p = path as babelTraverse.NodePath<t.Identifier>;

  // Skip if this is an assignment LHS (already captured by collectAssignmentContext)
  const parent = p.parent;
  if (t.isAssignmentExpression(parent) && parent.left === p.node) return null;

  return p.findParent(
    (pp: babelTraverse.NodePath) => pp.isStatement() || pp.isDeclaration()
  );
}

/**
 * Collects usage examples for module-level identifiers (up to MAX_CONTEXT_SNIPPETS per identifier).
 */
export function collectUsageExamples(
  ast: t.File,
  identifiers: Set<string>,
  assignmentCounts: Record<string, number>
): Record<string, string[]> {
  const examples: Record<string, string[]> = {};
  for (const id of identifiers) {
    examples[id] = [];
  }

  traverse(ast, {
    Identifier(path: babelTraverse.NodePath<t.Identifier>) {
      const name = path.node.name;
      const statement = getIdentifierUsageStatement(
        path,
        name,
        identifiers,
        assignmentCounts,
        examples
      );
      if (!statement) return;

      try {
        const code = generate(statement.node).code;
        if (code) {
          const snippet = truncateSnippet(code);
          if (snippet && !examples[name].includes(snippet)) {
            examples[name].push(snippet);
          }
        }
      } catch {
        // Skip if generation fails for this node
      }
    }
  });

  return examples;
}
