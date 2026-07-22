/**
 * Prior-version transfer: applies renames recovered from a matched prior
 * humanified version before the LLM pass runs.
 *
 * Phases: exact-match name transfers → close-match transfers with prior
 * context attachment → module binding renames → external-reference vote
 * propagation (module bindings and closure captures) → close-match set
 * elimination as LLM suggestions → deferred retry of collision-rejected
 * renames (phase order makes swaps/chains self-block: G→R and R→G in one
 * scope reject each other, and a token can be freed by a LATER phase).
 * Every name application goes through attemptValidatedRename; names still
 * rejected after retry fall through to the LLM pass.
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
import type { StatementTwinTransfers } from "../prior-version/statement-twin.js";
import {
  type BindingRole,
  bindingRolesAgree,
  computeBindingRole
} from "../prior-version/binding-role.js";
import { debug } from "../debug.js";
import type { Profiler } from "../profiling/profiler.js";
import { buildOwnedBindingMap } from "./function-bindings.js";
import {
  isPending,
  isSettled,
  markSkipped,
  type TransferPair
} from "./lifecycle.js";
import {
  attemptValidatedRename,
  type RenameRejectionReason
} from "./validated-rename.js";
import type { MatchedBindingRef } from "./prior-match-map.js";

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
  /**
   * True when the pair came from an EXACT-matched function's slot-resolved
   * placeholder table — byte-identical-modulo-names testimony, strong
   * enough to corroborate a single-vote pin. Close-match pairs and
   * positional name lookups are weaker and never pin alone.
   */
  exactSlotTestimony: boolean;
}

/** Where one transfer pair's rename should go, or why it can't. */
type PairTarget =
  | { kind: "scope"; scope: Scope }
  | { kind: "external"; binding: Binding }
  | { kind: "drop"; why: "phantom pair" | "stale binding" };

/** Rejection reasons that later phases can un-block (freed tokens). */
const RETRYABLE_REJECTIONS: ReadonlySet<RenameRejectionReason> = new Set([
  "target-in-scope",
  "target-visible",
  "shadows-child"
]);

/** A validated-rename rejection eligible for the deferred retry pass. */
interface RejectedTransfer {
  scope: Scope;
  /** Current name of the binding; mutated when a cycle-break temps it. */
  oldName: string;
  newName: string;
  /** Identity captured at rejection time — retry only while it holds. */
  binding: Binding;
  /** Last rejection reason; only token-collision reasons join cycles. */
  lastReason: RenameRejectionReason;
  /** Post-apply bookkeeping (settle a binding node, register with owner). */
  onApplied?: () => void;
}

/** Queue a rejection for the deferred retry pass when it is retryable. */
function queueRetry(
  queue: RejectedTransfer[],
  scope: Scope,
  oldName: string,
  newName: string,
  reason: RenameRejectionReason,
  onApplied?: () => void
): void {
  if (!RETRYABLE_REJECTIONS.has(reason)) return;
  const binding = scope.bindings[oldName];
  if (!binding) return;
  queue.push({
    scope,
    oldName,
    newName,
    binding,
    lastReason: reason,
    onApplied
  });
}

/**
 * A binding is function-owned when the function's scope is its nearest
 * function scope (params, vars, block-scoped locals incl. catch params —
 * but not nested functions' locals), or when it is the function's own
 * name binding (declarations bind in the PARENT scope).
 */
function isOwnedBinding(binding: Binding, fn: FunctionNode): boolean {
  if (binding.path.node === fn.path.node) return true;
  return binding.scope.getFunctionParent() === fn.path.scope;
}

/**
 * Resolve a pair to its rename target. Slot-resolved pairs carry the exact
 * Binding, so shadowed bindings (two bindings, one minified name) each hit
 * their own scope; a name lookup would collapse them onto one. Positional
 * pairs (close-match name/params) fall back to the owned-name map.
 */
function resolvePairTarget(
  fn: FunctionNode,
  pair: TransferPair,
  ownedScopeByName: () => Map<string, Scope>
): PairTarget {
  if (pair.binding) {
    if (!isOwnedBinding(pair.binding, fn)) {
      return { kind: "external", binding: pair.binding };
    }
    // The slot's binding must still live under the pair's old name —
    // anything else means an earlier rename touched it (stale pair).
    if (pair.binding.scope.bindings[pair.oldName] !== pair.binding) {
      return { kind: "drop", why: "stale binding" };
    }
    return { kind: "scope", scope: pair.binding.scope };
  }
  const scope = ownedScopeByName().get(pair.oldName);
  if (scope) return { kind: "scope", scope };
  const outer = resolveReferencedOuterBinding(fn, pair.oldName);
  if (outer) return { kind: "external", binding: outer };
  return { kind: "drop", why: "phantom pair" };
}

/**
 * Transfer rename pairs into a function's owned bindings through the
 * validated rename path. Names that are not function-owned become external
 * refs for vote propagation; rejected names are skipped and left for the
 * LLM pass. Returns the target names actually applied.
 */
function applyFunctionNameTransfers(
  fn: FunctionNode,
  pairs: TransferPair[],
  label: "exact-match" | "close-match",
  stats: TransferStats,
  externalRefs: ExternalRefPair[],
  retryQueue: RejectedTransfer[]
): Set<string> {
  // Only positional (close-match) pairs need the owned-name map; exact
  // pairs carry their binding, so skip the collection walk entirely.
  let ownedMap: Map<string, Scope> | undefined;
  const ownedScopeByName = () => {
    ownedMap ??= buildOwnedBindingMap(fn.path);
    return ownedMap;
  };
  const transferred = new Set<string>();
  for (const pair of pairs) {
    const { oldName, newName } = pair;
    if (oldName === newName) continue;
    stats.attempted++;
    const target = resolvePairTarget(fn, pair, ownedScopeByName);
    if (target.kind === "external") {
      stats.skipped++;
      externalRefs.push({
        oldName,
        newName,
        sourceFunctionId: fn.sessionId,
        binding: target.binding,
        exactSlotTestimony: label === "exact-match" && pair.binding !== null
      });
      debug.log(
        "prior-version",
        `${label}: skipping ${oldName}→${newName} in ${fn.sessionId}: external reference (not a function-owned binding)`
      );
      continue;
    }
    if (target.kind === "drop") {
      stats.skipped++;
      debug.log(
        "prior-version",
        `${label}: dropping ${oldName}→${newName} in ${fn.sessionId}: ${target.why}`
      );
      continue;
    }
    const attempt = attemptValidatedRename(target.scope, oldName, newName);
    if (attempt.applied) {
      transferred.add(newName);
      stats.applied++;
    } else {
      const reason = attempt.reason ?? "invalid-target";
      recordRejection(stats, reason);
      // A retried rename lands after the LLM-exclusion bookkeeping ran —
      // register the name with the function so the LLM does not re-rename
      // a close-matched function's recovered binding.
      queueRetry(retryQueue, target.scope, oldName, newName, reason, () => {
        fn.priorVersionTransferred ??= new Set();
        fn.priorVersionTransferred.add(newName);
      });
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
function applyMatchedRenames(
  allFunctions: FunctionNode[],
  retryQueue: RejectedTransfer[]
): {
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
        fn.state.transfers,
        "exact-match",
        stats,
        externalRefs,
        retryQueue
      );
    }
  }
  return { stats, externalRefs };
}

/** Apply close-match name transfers and attach prior-version context. */
function attachCloseMatchContext(
  closeMatchContext: Map<string, CloseMatchInfo>,
  functionMap: Map<string, FunctionNode>,
  retryQueue: RejectedTransfer[]
): { stats: TransferStats; externalRefs: ExternalRefPair[] } {
  const stats: TransferStats = { attempted: 0, applied: 0, skipped: 0 };
  const externalRefs: ExternalRefPair[] = [];
  for (const [newId, info] of closeMatchContext) {
    const fn = functionMap.get(newId);
    // Skip functions already claimed by an exact match or frozen; a close
    // match leaves the function pending so the LLM names it with context.
    if (!fn || isSettled(fn)) continue;
    // Signature pairs are positional (binding null → owned-name lookup);
    // statement-aligned body locals carry their slot's resolved Binding,
    // so same-named sibling bindings each receive their own prior name.
    const transferred = applyFunctionNameTransfers(
      fn,
      info.nameTransfers,
      "close-match",
      stats,
      externalRefs,
      retryQueue
    );
    if (transferred.size > 0) {
      fn.priorVersionTransferred = new Set([
        ...(fn.priorVersionTransferred ?? []),
        ...transferred
      ]);
      // The applied pairs feed the first-round prompt as fixed context.
      fn.priorVersionTransferredPairs = Object.fromEntries(
        info.nameTransfers
          .filter((pair) => transferred.has(pair.newName))
          .map((pair) => [pair.oldName, pair.newName])
      );
    }
    fn.priorVersionContext = info.priorCode;
    fn.priorVersionNames = info.priorNames;
    fn.priorNameHints = info.priorNameHints;
    fn.priorNameSnaps = info.priorNameSnaps;
  }
  return { stats, externalRefs };
}

/**
 * Pair every module binding the matcher mapped with its live declaration
 * identifier, keyed by the binding's current (minified) name. Call while the
 * identifiers still carry minified names (before the transfer phases rename
 * them); the final shipped name is read later by `buildPriorMatchMap`.
 */
function collectMatchedModuleBindings(
  moduleBindings: ModuleBindingNode[],
  renames: ModuleBindingRename[] | undefined
): MatchedBindingRef[] {
  const identByMinified = new Map<string, t.Identifier>();
  for (const binding of moduleBindings) {
    identByMinified.set(binding.identifier.name, binding.identifier);
  }
  const refs: MatchedBindingRef[] = [];
  for (const rename of renames ?? []) {
    const identifier = identByMinified.get(rename.oldName);
    if (identifier) refs.push({ identifier, priorName: rename.newName });
  }
  return refs;
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
  transferStats?: {
    exactMatch: TransferStats;
    closeMatch: TransferStats;
    statementTwin?: TransferStats;
    retry?: TransferStats;
  };
  /**
   * The module bindings the matcher mapped across versions, each carrying its
   * live declaration identifier (whose `.name` becomes the final shipped name
   * once every rename pass runs) and the prior name it matched. Drives the
   * split's binding-identity tier — see `buildPriorMatchMap`.
   */
  matchedModuleBindings: MatchedBindingRef[];
} {
  if (!priorVersionCode) {
    return {
      priorVersionApplied: 0,
      priorVersionAlreadyNamed: 0,
      priorVersionBindingsApplied: 0,
      priorVersionCloseMatch: 0,
      matchedModuleBindings: []
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
    profiler,
    graph
  );

  // matchPriorVersion only computes matches (no rename), so the bindings' live
  // identifiers still carry minified names — pair them with their prior names
  // now; the final shipped name is read later (buildPriorMatchMap).
  const matchedModuleBindings = collectMatchedModuleBindings(
    moduleBindings,
    priorResult.moduleBindingRenames
  );

  const applySpan = profiler.startSpan("prior-version:apply", "pipeline");
  const retryQueue: RejectedTransfer[] = [];
  const nodeToFunction = new Map<t.Node, FunctionNode>();
  for (const fn of allFunctions) {
    nodeToFunction.set(fn.path.node, fn);
  }
  // Statement-twin transfers land FIRST: a unique statement twin is
  // whole-statement identity (literals included), which outranks both an
  // ordinal/identity exact match that cross-paired same-shaped siblings
  // under a bundle reorder AND a close-match similarity guess. The finer
  // tiers' pairs for twin-renamed bindings then drop as stale; everything
  // the twin gates abstained from proceeds exactly as before.
  const { stats: statementTwinStats, externalRefs: twinExternalRefs } =
    applyStatementTwinTransfers(
      priorResult.statementTwins,
      graph,
      nodeToFunction,
      retryQueue
    );
  const { stats: exactMatchStats, externalRefs: exactExternalRefs } =
    applyMatchedRenames(allFunctions, retryQueue);
  const { stats: closeMatchStats, externalRefs: closeExternalRefs } =
    attachCloseMatchContext(
      priorResult.closeMatchContext,
      currentFunctionMap,
      retryQueue
    );
  const appliedBindingRenames = priorResult.moduleBindingRenames
    ? applyModuleBindingRenames(
        priorResult.moduleBindingRenames,
        graph,
        nodeToFunction,
        retryQueue
      )
    : new Map<string, string>();

  // Phase 3: Propagate external references to unmatched module bindings
  // and close-matched parent function locals (closure captures).
  // Statement-twin outer refs carry exact-grade testimony: the whole
  // statement is byte-identical-modulo-names, same strength as an exact
  // match's slot table.
  const allExternalRefs = [
    ...exactExternalRefs,
    ...closeExternalRefs,
    ...twinExternalRefs
  ];
  const propagation = propagateExternalReferences(
    allExternalRefs,
    graph,
    allFunctions,
    retryQueue,
    {
      priorBindingRoles: priorResult.priorBindingRoles,
      fnMatches: priorResult.matchResult.matches
    }
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

  // Phase 5: deferred retry — phase order makes swaps/chains self-block
  // (both directions of a token swap reject each other; a wanted token can
  // be freed by a later phase's rename).
  const retryStats = retryRejectedTransfers(retryQueue);
  applySpan.end({
    fnRenames: exactMatchStats.applied + closeMatchStats.applied,
    statementTwinRenames: statementTwinStats.applied,
    bindingRenames: appliedBindingRenames.size,
    propagated: propagation.moduleBindingsApplied
  });

  const totalBindingsApplied =
    appliedBindingRenames.size +
    propagation.moduleBindingsApplied +
    propagation.singleVotePins;

  debug.log(
    "prior-version",
    `Matched ${priorResult.functionsMatched} functions (${priorResult.functionsAlreadyNamed} already named), ` +
      `${priorResult.closeMatchCount} close matches, ` +
      `${appliedBindingRenames.size} bindings from prior version` +
      (statementTwinStats.applied > 0
        ? `, ${statementTwinStats.applied} statement-twin transfers`
        : "") +
      (propagation.moduleBindingsApplied > 0
        ? `, ${propagation.moduleBindingsApplied} propagated module bindings`
        : "") +
      (propagation.singleVotePins > 0
        ? `, ${propagation.singleVotePins} single-vote pins`
        : "") +
      (propagation.closureCapturesApplied > 0
        ? `, ${propagation.closureCapturesApplied} propagated closure captures`
        : "") +
      (propagation.functionNamesApplied > 0
        ? `, ${propagation.functionNamesApplied} propagated function names`
        : "") +
      (suggestionsApplied > 0
        ? `, ${suggestionsApplied} close-match suggestions`
        : "") +
      (retryStats.applied > 0
        ? `, ${retryStats.applied} retried renames (swaps/chains)`
        : "")
  );

  return {
    priorVersionApplied: priorResult.functionsMatched,
    priorVersionAlreadyNamed: priorResult.functionsAlreadyNamed,
    priorVersionBindingsApplied: totalBindingsApplied,
    priorVersionCloseMatch: priorResult.closeMatchCount,
    transferStats: {
      exactMatch: exactMatchStats,
      closeMatch: closeMatchStats,
      statementTwin: statementTwinStats,
      retry: retryStats
    },
    matchedModuleBindings
  };
}

/**
 * Apply statement-twin transfer pairs (Lever 1). Pairs arrive fully gated
 * (unique twin + callee/role/structural corroboration, pending owners
 * only); this phase adds the runtime checks: the binding must still live
 * under its computed old name (an earlier tier renaming it means the pair
 * is stale and simply drops), and every application goes through the
 * validated rename path. Applied names are registered so the LLM pass
 * skips them; module-level bindings settle their graph node.
 */
function applyStatementTwinTransfers(
  twins: StatementTwinTransfers,
  graph: UnifiedGraph,
  nodeToFunction: Map<t.Node, FunctionNode>,
  retryQueue: RejectedTransfer[]
): { stats: TransferStats; externalRefs: ExternalRefPair[] } {
  const stats: TransferStats = { attempted: 0, applied: 0, skipped: 0 };
  const externalRefs: ExternalRefPair[] = [];
  for (const ref of twins.outerRefs) {
    if (!ref.binding || ref.oldName === ref.newName) continue;
    externalRefs.push({
      oldName: ref.oldName,
      newName: ref.newName,
      sourceFunctionId: "statement-twin",
      binding: ref.binding,
      exactSlotTestimony: true
    });
  }
  for (const pair of twins.pairs) {
    const binding = pair.binding;
    if (!binding) continue;
    stats.attempted++;
    if (binding.scope.bindings[pair.oldName] !== binding) {
      stats.skipped++; // stale: a finer tier already renamed this binding
      continue;
    }
    const bookkeep = () => {
      settleModuleBindingNode(graph, pair.oldName);
      registerTransferredWithOwner(binding.scope, pair.newName, nodeToFunction);
      // A function declaration's name is owned by BOTH its parent scope's
      // function and the declared function itself — register with the
      // declared one too so its own LLM pass never re-renames it.
      if (binding.path.isFunctionDeclaration()) {
        const declaredFn = nodeToFunction.get(binding.path.node);
        if (declaredFn) {
          declaredFn.priorVersionTransferred ??= new Set();
          declaredFn.priorVersionTransferred.add(pair.newName);
        }
      }
    };
    const attempt = attemptValidatedRename(
      binding.scope,
      pair.oldName,
      pair.newName
    );
    if (attempt.applied) {
      bookkeep();
      stats.applied++;
    } else {
      const reason = attempt.reason ?? "invalid-target";
      recordRejection(stats, reason);
      queueRetry(
        retryQueue,
        binding.scope,
        pair.oldName,
        pair.newName,
        reason,
        bookkeep
      );
      debug.log(
        "prior-version",
        `statement-twin: rejected ${pair.oldName}→${pair.newName} (${attempt.reason})`
      );
    }
  }
  return { stats, externalRefs };
}

/**
 * Apply matched module binding renames to AST scopes and mark as done.
 * Rejected renames leave the binding in the graph so the LLM pass names it.
 * Returns the renames actually applied (oldName → newName).
 */
function applyModuleBindingRenames(
  renames: ModuleBindingRename[],
  graph: UnifiedGraph,
  nodeToFunction: Map<t.Node, FunctionNode>,
  retryQueue: RejectedTransfer[]
): Map<string, string> {
  const applied = new Map<string, string>();
  for (const { oldName, newName, scope } of renames) {
    const attempt = attemptValidatedRename(scope, oldName, newName);
    if (!attempt.applied) {
      queueRetry(
        retryQueue,
        scope,
        oldName,
        newName,
        attempt.reason ?? "invalid-target",
        () => {
          registerTransferredWithOwner(scope, newName, nodeToFunction);
          settleModuleBindingNode(graph, oldName);
        }
      );
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
    settleModuleBindingNode(graph, oldName);
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
  functionNamesApplied: number;
  /** Below-floor names pinned via single exact-match testimony + role gate */
  singleVotePins: number;
  /** Map of minified→humanified for module bindings applied via voting */
  appliedModuleRenames: Map<string, string>;
}

/** Per-name module binding vote tally, exact-sourced votes tracked. */
interface ModuleVoteCount {
  total: number;
  exact: number;
}

/**
 * Cross-version identity context for single-vote pinning: role evidence
 * for unconsumed prior names, and the function match map that translates
 * prior callee ids for the role's callee veto.
 */
interface SingleVotePinContext {
  priorBindingRoles: ReadonlyMap<string, BindingRole>;
  fnMatches: ReadonlyMap<string, string>;
}

interface ClosureVoteEntry {
  oldName: string;
  ownerFn: FunctionNode;
  ownerScope: Scope;
  votes: Map<string, number>;
}

interface FunctionNameVoteEntry {
  oldName: string;
  /** The pending FunctionNode owning this declaration name. */
  fn: FunctionNode;
  votes: Map<string, number>;
}

/**
 * Collect a vote for an unmatched function declaration's NAME. A drifted
 * function has no exact match, often no close match — nothing match-based
 * can pin its name, and both legs of a cross-version run re-invent it
 * every time (serializeWithHelper: five names in five runs). Its
 * exact/close-matched CALLERS carry the prior name as external refs;
 * agreeing votes transfer it mechanically.
 */
function collectFunctionNameVote(
  ref: ExternalRefPair,
  fnByNode: Map<t.Node, FunctionNode>,
  fnNameVotes: Map<Binding, FunctionNameVoteEntry>
): void {
  const fn = fnByNode.get(ref.binding.path.node);
  // Only pending functions: a settled one already carries its own name.
  if (!fn || isSettled(fn)) return;
  let entry = fnNameVotes.get(ref.binding);
  if (!entry) {
    entry = { oldName: ref.oldName, fn, votes: new Map() };
    fnNameVotes.set(ref.binding, entry);
  }
  entry.votes.set(ref.newName, (entry.votes.get(ref.newName) || 0) + 1);
}

/** Apply function-name votes: same agreement floor as module bindings. */
function applyPropagatedFunctionNames(
  fnNameVotes: Map<Binding, FunctionNameVoteEntry>,
  retryQueue: RejectedTransfer[]
): number {
  let applied = 0;
  for (const [binding, entry] of fnNameVotes) {
    const topName = getTopVote(entry.votes, MIN_MODULE_BINDING_VOTES);
    if (!topName) continue;

    const markTransferredName = () => {
      entry.fn.priorVersionTransferred ??= new Set();
      entry.fn.priorVersionTransferred.add(topName);
    };
    const attempt = attemptValidatedRename(
      binding.scope,
      entry.oldName,
      topName
    );
    if (!attempt.applied) {
      queueRetry(
        retryQueue,
        binding.scope,
        entry.oldName,
        topName,
        attempt.reason ?? "invalid-target",
        markTransferredName
      );
      debug.log(
        "prior-version",
        `propagated: function-name ${entry.oldName}→${topName} skipped (${attempt.reason})`
      );
      continue;
    }
    // Keep the LLM pass from re-renaming the vote-transferred name.
    markTransferredName();
    applied++;
    debug.log(
      "prior-version",
      `propagated: function-name ${entry.oldName}→${topName} (${entry.votes.get(topName)} votes from matched callers)`
    );
  }
  return applied;
}

/** Classify an external ref as a closure capture and add to closureVotes. */
function classifyClosureCapture(
  ref: ExternalRefPair,
  scopeToFunction: Map<Scope, FunctionNode>,
  closureVotes: Map<Binding, ClosureVoteEntry>
): void {
  // The captured binding may live in a BLOCK scope (catch clause, if/for
  // block) — the owning function is its nearest function parent, not the
  // binding's own scope. The rename still applies against the binding's
  // own scope.
  const ownerFnScope = ref.binding.scope.getFunctionParent();
  const ownerFn = ownerFnScope ? scopeToFunction.get(ownerFnScope) : undefined;
  if (!ownerFn || !ownerFn.priorVersionContext) return;

  let entry = closureVotes.get(ref.binding);
  if (!entry) {
    entry = {
      oldName: ref.oldName,
      ownerFn,
      ownerScope: ref.binding.scope,
      votes: new Map()
    };
    closureVotes.set(ref.binding, entry);
  }
  entry.votes.set(ref.newName, (entry.votes.get(ref.newName) || 0) + 1);
}

/** Mark a module-binding graph node settled under its ORIGINAL name. */
function settleModuleBindingNode(graph: UnifiedGraph, oldName: string): void {
  const renameNode = graph.nodes.get(`module:${oldName}`);
  if (
    renameNode &&
    renameNode.type === "module-binding" &&
    isPending(renameNode.node)
  ) {
    markSkipped(renameNode.node, "prior-version-match");
  }
}

/**
 * Deferred retry of collision-rejected renames, run after every transfer
 * phase has applied. A scan pass re-attempts each entry — chains unwind as
 * later phases free tokens. When a scan makes no progress, a detected
 * blocked-by cycle (each member's subject holds the token the previous
 * member wants — a pure swap) is broken by temping one member; the next
 * scans then unwind the rest and finally land the temped entry. Entries
 * whose binding was renamed by another path meanwhile are dropped.
 */
function retryRejectedTransfers(queue: RejectedTransfer[]): TransferStats {
  const stats: TransferStats = {
    attempted: queue.length,
    applied: 0,
    skipped: 0
  };
  let pending = queue.filter(entryStillPending);
  let tempCounter = 0;
  while (pending.length > 0) {
    const scan = retryScanPass(pending, stats);
    pending = scan.remaining;
    if (scan.applied > 0 || pending.length === 0) continue;
    const cycleEntry = findRetryCycleEntry(pending);
    if (!cycleEntry) break;
    const temp = `__hf_retry_${tempCounter++}`;
    const hop = attemptValidatedRename(
      cycleEntry.scope,
      cycleEntry.oldName,
      temp
    );
    if (!hop.applied) break;
    debug.log(
      "prior-version",
      `retry: cycle break ${cycleEntry.oldName}→${temp} (wants ${cycleEntry.newName})`
    );
    cycleEntry.oldName = temp;
  }
  stats.skipped = stats.attempted - stats.applied;
  return stats;
}

/** True while the entry's binding still lives under its (current) old name. */
function entryStillPending(entry: RejectedTransfer): boolean {
  return entry.scope.bindings[entry.oldName] === entry.binding;
}

/** One retry sweep: apply what now validates, keep the rest. */
function retryScanPass(
  pending: RejectedTransfer[],
  stats: TransferStats
): { applied: number; remaining: RejectedTransfer[] } {
  let applied = 0;
  const remaining: RejectedTransfer[] = [];
  for (const entry of pending) {
    if (!entryStillPending(entry)) continue;
    const attempt = attemptValidatedRename(
      entry.scope,
      entry.oldName,
      entry.newName
    );
    if (attempt.applied) {
      applied++;
      stats.applied++;
      entry.onApplied?.();
      debug.log(
        "prior-version",
        `retry: applied ${entry.oldName}→${entry.newName}`
      );
    } else {
      entry.lastReason = attempt.reason ?? entry.lastReason;
      remaining.push(entry);
    }
  }
  return { applied, remaining };
}

/**
 * Find an entry on a closed blocked-by cycle: follow, from each start,
 * the pending entry whose SUBJECT binding currently holds the token the
 * previous entry wants. Only token-collision rejections participate — a
 * shadows-child block is positional; temping its subject frees nothing.
 */
function findRetryCycleEntry(
  pending: RejectedTransfer[]
): RejectedTransfer | null {
  const bySubject = new Map<Binding, RejectedTransfer>();
  for (const entry of pending) {
    if (entry.lastReason === "shadows-child") continue;
    const subject = entry.scope.bindings[entry.oldName];
    if (subject) bySubject.set(subject, entry);
  }
  for (const start of bySubject.values()) {
    if (walkClosesCycle(start, bySubject)) return start;
  }
  return null;
}

/** Follow blocked-by hops from `start`; true when they loop back to it. */
function walkClosesCycle(
  start: RejectedTransfer,
  bySubject: Map<Binding, RejectedTransfer>
): boolean {
  const seen = new Set<RejectedTransfer>([start]);
  let current = start;
  for (;;) {
    const holder = current.scope.getBinding(current.newName);
    const nextEntry = holder ? bySubject.get(holder) : undefined;
    if (!nextEntry || seen.has(nextEntry)) return nextEntry === start;
    seen.add(nextEntry);
    current = nextEntry;
  }
}

/** Apply propagated module binding renames via voting. */
function applyPropagatedModuleBindings(
  moduleVotes: Map<ModuleBindingNode, Map<string, ModuleVoteCount>>,
  retryQueue: RejectedTransfer[],
  pinContext: SingleVotePinContext
): { applied: number; pinned: number; renames: Map<string, string> } {
  let applied = 0;
  let pinned = 0;
  const renames = new Map<string, string>();
  const nameClaimants = countNameClaimants(moduleVotes);
  for (const [bindingNode, votes] of moduleVotes) {
    const minifiedName = bindingNode.name;
    const totals = new Map(
      [...votes].map(([name, count]) => [name, count.total])
    );
    const topName = getTopVote(totals, MIN_MODULE_BINDING_VOTES);
    if (!topName) {
      if (
        trySingleVotePin(bindingNode, votes, nameClaimants, pinContext, renames)
      ) {
        pinned++;
      } else {
        debug.log(
          "prior-version",
          `propagated: module-binding ${minifiedName} skipped (tied or below ${MIN_MODULE_BINDING_VOTES}-vote floor)`
        );
      }
      continue;
    }

    const attempt = attemptValidatedRename(
      bindingNode.scope,
      minifiedName,
      topName
    );
    if (!attempt.applied) {
      queueRetry(
        retryQueue,
        bindingNode.scope,
        minifiedName,
        topName,
        attempt.reason ?? "invalid-target",
        () => {
          if (isPending(bindingNode)) markSkipped(bindingNode, "propagated");
        }
      );
      debug.log(
        "prior-version",
        `propagated: module-binding ${minifiedName}→${topName} skipped (${attempt.reason})`
      );
      continue;
    }

    const voteCount = totals.get(topName) ?? 0;
    if (isPending(bindingNode)) markSkipped(bindingNode, "propagated");
    applied++;
    renames.set(minifiedName, topName);
    debug.log(
      "prior-version",
      `propagated: module-binding ${minifiedName}→${topName} (${voteCount} vote${voteCount > 1 ? "s" : ""} from matched functions)`
    );
  }
  return { applied, pinned, renames };
}

/** How many distinct binding nodes each proposed name has votes on. */
function countNameClaimants(
  moduleVotes: Map<ModuleBindingNode, Map<string, ModuleVoteCount>>
): Map<string, number> {
  const claimants = new Map<string, number>();
  for (const [, votes] of moduleVotes) {
    for (const name of votes.keys()) {
      claimants.set(name, (claimants.get(name) ?? 0) + 1);
    }
  }
  return claimants;
}

/**
 * Below-floor single-vote pin: a prior name recovered by exactly one
 * exact-matched function's slot testimony inherits mechanically when the
 * prior and new binding provably play the same role. Precision gates, in
 * order — testimony strength (exact slot pairs only), injectivity (the
 * name must have exactly one claimant), role corroboration (content
 * agreement + callee veto), validated rename (collisions reject, no
 * retry: a held token means the name has a better owner elsewhere).
 */
function trySingleVotePin(
  bindingNode: ModuleBindingNode,
  votes: Map<string, ModuleVoteCount>,
  nameClaimants: Map<string, number>,
  pinContext: SingleVotePinContext,
  renames: Map<string, string>
): boolean {
  if (votes.size !== 1) return false;
  const [name, count] = [...votes][0];
  const blocked = (reason: string): false => {
    debug.log(
      "prior-version",
      `propagated: module-binding ${bindingNode.name} single-vote pin blocked (${reason})`
    );
    return false;
  };
  if (count.total !== 1 || count.exact !== 1) {
    return blocked("non-exact-source");
  }
  if (nameClaimants.get(name) !== 1) return blocked("name-conflict");
  const priorRole = pinContext.priorBindingRoles.get(name);
  if (!priorRole) return blocked("no-prior-role");
  const agreement = bindingRolesAgree(
    priorRole,
    computeBindingRole(bindingNode),
    pinContext.fnMatches
  );
  if (!agreement.agrees) {
    return blocked(`role-mismatch:${agreement.reason}`);
  }
  const attempt = attemptValidatedRename(
    bindingNode.scope,
    bindingNode.name,
    name
  );
  if (!attempt.applied) return blocked(`validation:${attempt.reason}`);
  if (isPending(bindingNode)) markSkipped(bindingNode, "propagated");
  renames.set(bindingNode.name, name);
  debug.log(
    "prior-version",
    `propagated: module-binding ${bindingNode.name}→${name} (single exact-match vote, role ${agreement.reason})`
  );
  return true;
}

/** Apply propagated closure capture renames via voting. */
function applyPropagatedClosureCaptures(
  closureVotes: Map<Binding, ClosureVoteEntry>,
  retryQueue: RejectedTransfer[]
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
      const ownerFn = entry.ownerFn;
      queueRetry(
        retryQueue,
        entry.ownerScope,
        entry.oldName,
        topName,
        attempt.reason ?? "invalid-target",
        () => {
          ownerFn.priorVersionTransferred ??= new Set();
          ownerFn.priorVersionTransferred.add(topName);
        }
      );
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
  allFunctions: FunctionNode[],
  retryQueue: RejectedTransfer[],
  pinContext: SingleVotePinContext
): PropagationResult {
  if (externalRefs.length === 0) {
    return {
      moduleBindingsApplied: 0,
      closureCapturesApplied: 0,
      functionNamesApplied: 0,
      singleVotePins: 0,
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
  const fnByNode = new Map<t.Node, FunctionNode>();
  for (const fn of allFunctions) {
    scopeToFunction.set(fn.path.scope, fn);
    fnByNode.set(fn.path.node, fn);
  }

  const moduleVotes = new Map<
    ModuleBindingNode,
    Map<string, ModuleVoteCount>
  >();
  const closureVotes = new Map<Binding, ClosureVoteEntry>();
  const fnNameVotes = new Map<Binding, FunctionNameVoteEntry>();

  for (const ref of externalRefs) {
    const moduleNode = moduleNodeByBinding.get(ref.binding);
    if (moduleNode) {
      addModuleVote(moduleVotes, moduleNode, ref);
    } else if (ref.binding.path.isFunctionDeclaration()) {
      collectFunctionNameVote(ref, fnByNode, fnNameVotes);
    } else {
      classifyClosureCapture(ref, scopeToFunction, closureVotes);
    }
  }

  const moduleResult = applyPropagatedModuleBindings(
    moduleVotes,
    retryQueue,
    pinContext
  );
  return {
    moduleBindingsApplied: moduleResult.applied,
    closureCapturesApplied: applyPropagatedClosureCaptures(
      closureVotes,
      retryQueue
    ),
    functionNamesApplied: applyPropagatedFunctionNames(fnNameVotes, retryQueue),
    singleVotePins: moduleResult.pinned,
    appliedModuleRenames: moduleResult.renames
  };
}

/** Tally a module-binding vote, tracking exact-slot-testimony counts. */
function addModuleVote(
  moduleVotes: Map<ModuleBindingNode, Map<string, ModuleVoteCount>>,
  node: ModuleBindingNode,
  ref: ExternalRefPair
): void {
  let votes = moduleVotes.get(node);
  if (!votes) {
    votes = new Map();
    moduleVotes.set(node, votes);
  }
  let count = votes.get(ref.newName);
  if (!count) {
    count = { total: 0, exact: 0 };
    votes.set(ref.newName, count);
  }
  count.total++;
  if (ref.exactSlotTestimony) count.exact++;
}
