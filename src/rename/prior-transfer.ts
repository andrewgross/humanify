/**
 * Prior-version transfer: applies renames recovered from a matched prior
 * humanified version before the LLM pass runs.
 *
 * Phases: exact-match name transfers → close-match transfers with prior
 * context attachment → module binding renames → external-reference vote
 * propagation (module bindings and closure captures) → close-match set
 * elimination as LLM suggestions. Every name application goes through
 * attemptValidatedRename; rejected names fall through to the LLM pass.
 */
import type { Binding, Scope } from "@babel/traverse";
import type * as t from "@babel/types";
import type {
  FunctionNode,
  ModuleBindingNode,
  UnifiedGraph
} from "../analysis/types.js";
import type {
  CloseMatchInfo,
  ModuleBindingRename
} from "../prior-version/prior-version.js";
import { matchPriorVersion } from "../prior-version/prior-version.js";
import { debug } from "../debug.js";
import type { Profiler } from "../profiling/profiler.js";
import { buildOwnedBindingMap } from "./function-bindings.js";
import { isPending, isSettled, markSkipped } from "./lifecycle.js";
import {
  attemptValidatedRename,
  type RenameRejectionReason
} from "./validated-rename.js";

export interface TransferStats {
  attempted: number;
  applied: number;
  skipped: number;
  /** Skip counts broken down by validation rejection reason */
  rejected?: Partial<Record<RenameRejectionReason, number>>;
}

/** Record a validation rejection on transfer stats. */
function recordRejection(
  stats: TransferStats,
  reason: RenameRejectionReason
): void {
  stats.skipped++;
  stats.rejected ??= {};
  stats.rejected[reason] = (stats.rejected[reason] ?? 0) + 1;
}

interface ExternalRefPair {
  /** Minified name in the new version */
  oldName: string;
  /** Humanified name from the prior version */
  newName: string;
  /** Session ID of the matched function that produced this pair */
  sourceFunctionId: string;
  /**
   * The binding the old name resolves to from the source function's scope.
   * Votes are keyed by this identity — two scopes can bind the same
   * minified name, and a name-string key would let one rename the other.
   */
  binding: Binding;
}

/**
 * Transfer a set of renames into a function's owned bindings through the
 * validated rename path. Names that are not function-owned become external
 * refs for vote propagation; rejected names are skipped and left for the
 * LLM pass. Returns the target names actually applied.
 */
function applyFunctionNameTransfers(
  fn: FunctionNode,
  names: Record<string, string>,
  label: "exact-match" | "close-match",
  stats: TransferStats,
  externalRefs: ExternalRefPair[]
): Set<string> {
  const bindingMap = buildOwnedBindingMap(fn.path);
  const transferred = new Set<string>();
  for (const [oldName, newName] of Object.entries(names)) {
    if (oldName === newName) continue;
    stats.attempted++;
    const scope = bindingMap.get(oldName);
    if (!scope) {
      stats.skipped++;
      const binding = resolveReferencedOuterBinding(fn, oldName);
      if (binding) {
        externalRefs.push({
          oldName,
          newName,
          sourceFunctionId: fn.sessionId,
          binding
        });
        debug.log(
          "prior-version",
          `${label}: skipping ${oldName}→${newName} in ${fn.sessionId}: external reference (not a function-owned binding)`
        );
      } else {
        debug.log(
          "prior-version",
          `${label}: dropping ${oldName}→${newName} in ${fn.sessionId}: phantom pair (no referenced outer binding)`
        );
      }
      continue;
    }
    const attempt = attemptValidatedRename(scope, oldName, newName);
    if (attempt.applied) {
      transferred.add(newName);
      stats.applied++;
    } else {
      recordRejection(stats, attempt.reason ?? "invalid-target");
      debug.log(
        "prior-version",
        `${label}: rejected ${oldName}→${newName} in ${fn.sessionId} (${attempt.reason})`
      );
    }
  }
  return transferred;
}

/**
 * Resolves a placeholder-pair name to a binding OUTSIDE the function that
 * the function actually references. Pairs that reach this point can come
 * from a nested function's locals (handled by the nested function's own
 * match); the local's minified name may coincide with an unrelated outer
 * binding, so binding resolution alone is not enough — the resolved
 * binding must have a reference or write inside this function's subtree
 * to count as evidence.
 */
function resolveReferencedOuterBinding(
  fn: FunctionNode,
  name: string
): Binding | null {
  const binding = fn.path.scope.getBinding(name);
  if (!binding) return null;
  const paths = [
    ...binding.referencePaths,
    ...(binding.constantViolations ?? [])
  ];
  for (const p of paths) {
    if (p.isDescendant(fn.path)) return binding;
  }
  return null;
}

/** Apply matched renames to AST scopes and mark functions as pre-done. */
function applyMatchedRenames(allFunctions: FunctionNode[]): {
  stats: TransferStats;
  externalRefs: ExternalRefPair[];
} {
  const stats: TransferStats = { attempted: 0, applied: 0, skipped: 0 };
  const externalRefs: ExternalRefPair[] = [];
  for (const fn of allFunctions) {
    // "transferred" is the settled state matchPriorVersion assigns to an
    // exact match; frozen functions are "skipped" and excluded here.
    if (fn.state.kind === "transferred") {
      applyFunctionNameTransfers(
        fn,
        fn.state.names,
        "exact-match",
        stats,
        externalRefs
      );
    }
  }
  return { stats, externalRefs };
}

/** Apply close-match name transfers and attach prior-version context. */
function attachCloseMatchContext(
  closeMatchContext: Map<string, CloseMatchInfo>,
  functionMap: Map<string, FunctionNode>
): { stats: TransferStats; externalRefs: ExternalRefPair[] } {
  const stats: TransferStats = { attempted: 0, applied: 0, skipped: 0 };
  const externalRefs: ExternalRefPair[] = [];
  for (const [newId, info] of closeMatchContext) {
    const fn = functionMap.get(newId);
    // Skip functions already claimed by an exact match or frozen; a close
    // match leaves the function pending so the LLM names it with context.
    if (!fn || isSettled(fn)) continue;
    const transferred = applyFunctionNameTransfers(
      fn,
      info.nameTransfers,
      "close-match",
      stats,
      externalRefs
    );
    if (transferred.size > 0) {
      fn.priorVersionTransferred = new Set([
        ...(fn.priorVersionTransferred ?? []),
        ...transferred
      ]);
      // The applied pairs feed the first-round prompt as fixed context.
      fn.priorVersionTransferredPairs = Object.fromEntries(
        Object.entries(info.nameTransfers).filter(([, newName]) =>
          transferred.has(newName)
        )
      );
    }
    fn.priorVersionContext = info.priorCode;
    fn.priorVersionNames = info.priorNames;
  }
  return { stats, externalRefs };
}

/** Apply prior-version matching and mark matched functions as pre-done. */
export function applyPriorVersionIfPresent(
  priorVersionCode: string | undefined,
  allFunctions: FunctionNode[],
  graph: UnifiedGraph,
  profiler: Profiler
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
  const moduleBindings: ModuleBindingNode[] = [];
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
    moduleBindings,
    profiler
  );

  const applySpan = profiler.startSpan("prior-version:apply", "pipeline");
  const { stats: exactMatchStats, externalRefs: exactExternalRefs } =
    applyMatchedRenames(allFunctions);
  const { stats: closeMatchStats, externalRefs: closeExternalRefs } =
    attachCloseMatchContext(priorResult.closeMatchContext, currentFunctionMap);

  // Apply module binding renames and remove matched bindings from the graph
  const nodeToFunction = new Map<t.Node, FunctionNode>();
  for (const fn of allFunctions) {
    nodeToFunction.set(fn.path.node, fn);
  }
  const appliedBindingRenames = priorResult.moduleBindingRenames
    ? applyModuleBindingRenames(
        priorResult.moduleBindingRenames,
        graph,
        nodeToFunction
      )
    : new Map<string, string>();

  // Phase 3: Propagate external references to unmatched module bindings
  // and close-matched parent function locals (closure captures)
  const allExternalRefs = [...exactExternalRefs, ...closeExternalRefs];
  const propagation = propagateExternalReferences(
    allExternalRefs,
    graph,
    allFunctions
  );

  // Phase 4: Close-match set elimination → LLM suggestions
  // Build resolved bindings map from all prior phases
  const resolvedBindings = new Map<string, string>(appliedBindingRenames);
  for (const [oldName, newName] of propagation.appliedModuleRenames) {
    resolvedBindings.set(oldName, newName);
  }
  const suggestionsApplied = suggestFromCloseMatchExternals(
    priorResult.closeMatchContext,
    resolvedBindings,
    graph
  );
  applySpan.end({
    fnRenames: exactMatchStats.applied + closeMatchStats.applied,
    bindingRenames: appliedBindingRenames.size,
    propagated: propagation.moduleBindingsApplied
  });

  const totalBindingsApplied =
    appliedBindingRenames.size + propagation.moduleBindingsApplied;

  debug.log(
    "prior-version",
    `Matched ${priorResult.functionsMatched} functions (${priorResult.functionsAlreadyNamed} already named), ` +
      `${priorResult.closeMatchCount} close matches, ` +
      `${appliedBindingRenames.size} bindings from prior version` +
      (propagation.moduleBindingsApplied > 0
        ? `, ${propagation.moduleBindingsApplied} propagated module bindings`
        : "") +
      (propagation.closureCapturesApplied > 0
        ? `, ${propagation.closureCapturesApplied} propagated closure captures`
        : "") +
      (suggestionsApplied > 0
        ? `, ${suggestionsApplied} close-match suggestions`
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

/**
 * Apply matched module binding renames to AST scopes and mark as done.
 * Rejected renames leave the binding in the graph so the LLM pass names it.
 * Returns the renames actually applied (oldName → newName).
 */
function applyModuleBindingRenames(
  renames: ModuleBindingRename[],
  graph: UnifiedGraph,
  nodeToFunction: Map<t.Node, FunctionNode>
): Map<string, string> {
  const applied = new Map<string, string>();
  for (const { oldName, newName, scope } of renames) {
    const attempt = attemptValidatedRename(scope, oldName, newName);
    if (!attempt.applied) {
      debug.log(
        "prior-version",
        `module-binding: rejected ${oldName}→${newName} (${attempt.reason})`
      );
      continue;
    }
    applied.set(oldName, newName);
    registerTransferredWithOwner(scope, newName, nodeToFunction);

    // Settle the binding node. It stays in the graph — deleting it leaves
    // dangling dependency edges that block every dependent until the
    // deadlock force-break releases them unordered.
    const sessionId = `module:${oldName}`;
    const renameNode = graph.nodes.get(sessionId);
    if (
      renameNode &&
      renameNode.type === "module-binding" &&
      isPending(renameNode.node)
    ) {
      markSkipped(renameNode.node, "prior-version-match");
    }
  }
  return applied;
}

/**
 * Register a transferred name with the function that owns the binding's
 * scope, so the LLM pass does not re-rename it. Function-var-name transfers
 * rename locals of functions that still go through LLM processing (close
 * matches, unmatched parents) — without this, the LLM overwrites the
 * transferred name and reintroduces cross-version drift.
 */
function registerTransferredWithOwner(
  scope: Scope,
  newName: string,
  nodeToFunction: Map<t.Node, FunctionNode>
): void {
  const fnPath = scope.path.isFunction()
    ? scope.path
    : scope.path.getFunctionParent();
  if (!fnPath) return;
  const ownerFn = nodeToFunction.get(fnPath.node);
  if (!ownerFn) return;
  ownerFn.priorVersionTransferred ??= new Set();
  ownerFn.priorVersionTransferred.add(newName);
}

/**
 * Get the top vote from a vote map, or null if tied or below the floor.
 * Module binding renames require ≥2 agreeing votes: a single placeholder
 * pair is one function's testimony, and a wrong single vote renames an
 * unrelated binding with no corroboration.
 */
function getTopVote(votes: Map<string, number>, minVotes = 1): string | null {
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
  if (tied || topCount < minVotes) return null;
  return topName;
}

/** Minimum agreeing votes to rename a module binding via propagation. */
const MIN_MODULE_BINDING_VOTES = 2;

interface PropagationResult {
  moduleBindingsApplied: number;
  closureCapturesApplied: number;
  /** Map of minified→humanified for module bindings applied via voting */
  appliedModuleRenames: Map<string, string>;
}

interface ClosureVoteEntry {
  oldName: string;
  ownerFn: FunctionNode;
  ownerScope: Scope;
  votes: Map<string, number>;
}

/** Add a vote to a nested vote map, creating entries as needed. */
function addVote<K>(
  voteMap: Map<K, Map<string, number>>,
  key: K,
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
  scopeToFunction: Map<Scope, FunctionNode>,
  closureVotes: Map<string, ClosureVoteEntry>
): void {
  const ownerFn = scopeToFunction.get(ref.binding.scope);
  if (!ownerFn || !ownerFn.priorVersionContext) return;

  const key = `${ownerFn.sessionId}:${ref.oldName}`;
  let entry = closureVotes.get(key);
  if (!entry) {
    entry = {
      oldName: ref.oldName,
      ownerFn,
      ownerScope: ref.binding.scope,
      votes: new Map()
    };
    closureVotes.set(key, entry);
  }
  entry.votes.set(ref.newName, (entry.votes.get(ref.newName) || 0) + 1);
}

/** Apply propagated module binding renames via voting. */
function applyPropagatedModuleBindings(
  moduleVotes: Map<ModuleBindingNode, Map<string, number>>
): { applied: number; renames: Map<string, string> } {
  let applied = 0;
  const renames = new Map<string, string>();
  for (const [bindingNode, votes] of moduleVotes) {
    const minifiedName = bindingNode.name;
    const topName = getTopVote(votes, MIN_MODULE_BINDING_VOTES);
    if (!topName) {
      debug.log(
        "prior-version",
        `propagated: module-binding ${minifiedName} skipped (tied or below ${MIN_MODULE_BINDING_VOTES}-vote floor)`
      );
      continue;
    }

    const attempt = attemptValidatedRename(
      bindingNode.scope,
      minifiedName,
      topName
    );
    if (!attempt.applied) {
      debug.log(
        "prior-version",
        `propagated: module-binding ${minifiedName}→${topName} skipped (${attempt.reason})`
      );
      continue;
    }

    const voteCount = votes.get(topName) ?? 0;
    if (isPending(bindingNode)) markSkipped(bindingNode, "propagated");
    applied++;
    renames.set(minifiedName, topName);
    debug.log(
      "prior-version",
      `propagated: module-binding ${minifiedName}→${topName} (${voteCount} vote${voteCount > 1 ? "s" : ""} from matched functions)`
    );
  }
  return { applied, renames };
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

    const attempt = attemptValidatedRename(
      entry.ownerScope,
      entry.oldName,
      topName
    );
    if (!attempt.applied) {
      debug.log(
        "prior-version",
        `propagated: closure-capture ${entry.oldName}→${topName} skipped (${attempt.reason})`
      );
      continue;
    }
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

/** Filter a set to only names present in a lookup map. */
function filterToUnmatched(
  externals: Set<string>,
  unmatchedNames: Map<string, unknown>
): string[] {
  const result: string[] = [];
  for (const name of externals) {
    if (unmatchedNames.has(name)) result.push(name);
  }
  return result;
}

/** Filter a set by removing names that have already been resolved. */
function filterOutResolved(
  externals: Set<string>,
  resolved: Set<string>
): string[] {
  const result: string[] = [];
  for (const name of externals) {
    if (!resolved.has(name)) result.push(name);
  }
  return result;
}

/** Collect unmatched module binding nodes from the graph. */
function collectUnmatchedModuleBindings(
  graph: UnifiedGraph
): Map<string, ModuleBindingNode> {
  const result = new Map<string, ModuleBindingNode>();
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "module-binding" && isPending(renameNode.node)) {
      result.set(renameNode.node.name, renameNode.node);
    }
  }
  return result;
}

/**
 * Phase 4: Set elimination on close-match external references.
 *
 * For each close-matched function pair, compare sets of module-scope identifiers.
 * After eliminating pairs already resolved by earlier phases, a 1:1 remainder
 * means the binding should probably have the prior name. We set suggestedName
 * on the module binding node (NOT auto-rename) so the LLM can validate.
 */
function suggestFromCloseMatchExternals(
  closeMatchContext: Map<string, CloseMatchInfo>,
  resolvedBindings: Map<string, string>,
  graph: UnifiedGraph
): number {
  const resolvedHumanified = new Set(resolvedBindings.values());
  const unmatchedBindings = collectUnmatchedModuleBindings(graph);
  let suggestionsApplied = 0;

  for (const [, info] of closeMatchContext) {
    if (!info.priorExternals || !info.newExternals) continue;

    const newRemaining = filterToUnmatched(
      info.newExternals,
      unmatchedBindings
    );
    const priorRemaining = filterOutResolved(
      info.priorExternals,
      resolvedHumanified
    );

    if (newRemaining.length !== 1 || priorRemaining.length !== 1) continue;

    const bindingNode = unmatchedBindings.get(newRemaining[0]);
    if (!bindingNode || bindingNode.suggestedName) continue;

    bindingNode.suggestedName = priorRemaining[0];
    suggestionsApplied++;
    debug.log(
      "prior-version",
      `close-match-suggest: ${newRemaining[0]}→${priorRemaining[0]} (set elimination)`
    );
  }

  return suggestionsApplied;
}

/**
 * Propagates external reference pairs from matched functions to:
 * (a) unmatched module bindings (via voting from referencing functions)
 * (b) close-matched parent function locals (closure captures from exact-matched children)
 */
function propagateExternalReferences(
  externalRefs: ExternalRefPair[],
  graph: UnifiedGraph,
  allFunctions: FunctionNode[]
): PropagationResult {
  if (externalRefs.length === 0) {
    return {
      moduleBindingsApplied: 0,
      closureCapturesApplied: 0,
      appliedModuleRenames: new Map()
    };
  }

  // Unmatched module binding nodes keyed by their RESOLVED binding — a
  // ref votes for a node only when it references that exact binding, not
  // one that happens to share the minified name.
  const moduleNodeByBinding = new Map<Binding, ModuleBindingNode>();
  for (const [, renameNode] of graph.nodes) {
    if (renameNode.type === "module-binding" && isPending(renameNode.node)) {
      const binding = renameNode.node.scope.getBinding(renameNode.node.name);
      if (binding) moduleNodeByBinding.set(binding, renameNode.node);
    }
  }

  const scopeToFunction = new Map<Scope, FunctionNode>();
  for (const fn of allFunctions) {
    scopeToFunction.set(fn.path.scope, fn);
  }

  const moduleVotes = new Map<ModuleBindingNode, Map<string, number>>();
  const closureVotes = new Map<string, ClosureVoteEntry>();

  for (const ref of externalRefs) {
    const moduleNode = moduleNodeByBinding.get(ref.binding);
    if (moduleNode) {
      addVote(moduleVotes, moduleNode, ref.newName);
    } else {
      classifyClosureCapture(ref, scopeToFunction, closureVotes);
    }
  }

  const moduleResult = applyPropagatedModuleBindings(moduleVotes);
  return {
    moduleBindingsApplied: moduleResult.applied,
    closureCapturesApplied: applyPropagatedClosureCaptures(closureVotes),
    appliedModuleRenames: moduleResult.renames
  };
}
