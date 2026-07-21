import type * as t from "@babel/types";
import { performance } from "node:perf_hooks";
import type {
  FunctionNode,
  RenameReport,
  IdentifierOutcome,
  LLMContext,
  ModuleBindingNode,
  ProcessorOptions,
  RenameAttempt,
  RenameDecision,
  UnifiedGraph
} from "../analysis/types.js";
import { generate } from "../babel-utils.js";
import { debug } from "../debug.js";
import {
  buildBatchRenameRetryBody,
  buildModuleLevelRenameBody,
  buildModuleLevelRenamePrompt,
  buildModuleLevelRetryPrefix,
  MODULE_LEVEL_RENAME_SYSTEM_PROMPT
} from "../llm/prompts.js";
import type {
  BatchRenameRequest,
  BatchRenameResponse,
  LLMProvider
} from "../llm/types.js";
import { capContextCode, selectFunctionCode } from "./code-window.js";
import {
  buildPriorStemIndex,
  nameStem,
  snapSuggestionToPrior
} from "./prior-name-snap.js";
import {
  type BindingInfo,
  collectOwnedBindingInfos,
  collectShadowedBlockBindings
} from "./function-bindings.js";
import {
  isPending,
  isSettled,
  markFailed,
  markLlmDone,
  markSkipped
} from "./lifecycle.js";
import { assertUnifiedGraphClosure } from "./graph-closure.js";
import { RetryBatcher } from "./retry-batcher.js";
import { resolveConflict, sanitizeIdentifier } from "../llm/validation.js";
import { getProximateUsedNames } from "./proximity.js";
import { TRACE_TID } from "../profiling/types.js";
import { createConcurrencyLimiter } from "../utils/concurrency.js";
import { identifierRegex } from "../utils/identifier-regex.js";
import { computeDependentDepths } from "../analysis/function-graph.js";
import { buildContext } from "./context-builder.js";
import type { IsEligibleFn } from "./rename-eligibility.js";
import { resolveRunConfig } from "./run-config.js";
import {
  attemptValidatedRename,
  getRenameRejection,
  isValidRenameTarget
} from "./validated-rename.js";

/** Failure categories from batch validation */
export type Failures = {
  duplicates: string[];
  invalid: string[];
  missing: string[];
  unchanged: string[];
};

/** Per-identifier tracking for the batch-until-done loop */
export interface IdentifierAttemptState {
  /** Real failure attempts (counts against maxRetriesPerIdentifier) */
  attempts: number;
  /** Cross-lane collision retries (counts against maxFreeRetries) */
  freeRetries: number;
  /** Last LLM suggestion for this identifier */
  lastSuggestion?: string;
  /** Why the last attempt failed */
  lastFailureReason?: "duplicate" | "invalid" | "missing" | "unchanged";
  /** Last finish reason from LLM response */
  lastFinishReason?: string;
  /** Per-round attempt history (proposals + results), for diagnostics. */
  trail?: RenameAttempt[];
}

/** Append one attempt to an identifier's trail; the round is its position. */
function recordAttempt(
  state: IdentifierAttemptState,
  proposed: string | undefined,
  result: RenameAttempt["result"]
): void {
  if (!state.trail) state.trail = [];
  state.trail.push({ round: state.trail.length + 1, proposed, result });
}

/** Which failure category a validation batch assigned to `name`. */
function failureResult(
  name: string,
  dupSet: Set<string>,
  invSet: Set<string>,
  unchSet: Set<string>
): RenameAttempt["result"] {
  if (dupSet.has(name)) return "duplicate";
  if (invSet.has(name)) return "invalid";
  if (unchSet.has(name)) return "unchanged";
  return "missing";
}

/** Result from the shared batch rename loop */
interface BatchRenameLoopResult {
  outcomes: Record<string, IdentifierOutcome>;
  finishReasons: (string | undefined)[];
  remaining: Set<string>;
  totalLLMCalls: number;
  previousAttempt: Record<string, string>;
  failures: Failures;
}

/** Maximum identifiers per LLM batch (adaptive — halved on truncation) */
const DEFAULT_BATCH_SIZE = 10;

/** Per-identifier retry cap for real failures: the initial call plus ONE
 * LLM retry. Further conflicts resolve algorithmically (suffixing) — the
 * collision-retry tail dominated incremental runs and retry #2+ rarely
 * beats a suffix on an already-semantic suggestion. */
const DEFAULT_MAX_RETRIES_PER_ID = 2;

/** Cap on "free" retries from cross-lane collisions */
const DEFAULT_MAX_FREE_RETRIES = 100;

/** Minimum number of bindings to enable parallel lanes */
const DEFAULT_LANE_THRESHOLD = 25;

/**
 * Compute the number of parallel lanes for a given binding count.
 * More lanes = smaller per-lane batches = fewer collisions per lane.
 *
 * Returns 0 when bindings are below the lane threshold (no splitting).
 */
export function computeLaneCount(
  bindingCount: number,
  laneThreshold: number = DEFAULT_LANE_THRESHOLD
): number {
  if (bindingCount <= laneThreshold) return 0;
  if (bindingCount <= 200) return 4;
  if (bindingCount <= 1000) return 8;
  return 16;
}

/**
 * Compute maxFreeRetries scaled to binding count.
 * For large functions, more free retries are needed to avoid premature exhaustion.
 */
export function computeMaxFreeRetries(
  bindingCount: number,
  configuredMax?: number
): number {
  if (configuredMax !== undefined) return configuredMax;
  return Math.max(DEFAULT_MAX_FREE_RETRIES, Math.floor(bindingCount / 4));
}

/**
 * Processes functions in dependency order using a ready queue.
 *
 * Functions are processed leaf-first: those with no internal dependencies
 * are processed first, then functions that only depend on completed functions,
 * and so on. This ensures maximum context for the LLM.
 *
 * Processing happens in parallel with a configurable concurrency limit.
 */
export class RenameProcessor {
  private allRenames: RenameDecision[] = [];
  /** Nulled by releaseAst() once processing is done, so the post-naming
   * re-parse passes do not hold the whole bundle AST live (ephemeron/GC
   * fix — see plugin.ts). Only touched during processUnified, where it is
   * always set. */
  private ast: t.File | null;
  private metrics?: import("../llm/metrics.js").MetricsTracker;
  private _reports: RenameReport[] = [];
  private failedCount = 0;
  private _skippedBySkipList = 0;
  private _skipReasons = { zeroBindings: 0, allPreserved: 0, error: 0 };
  private options: ProcessorOptions = {};
  private isEligible!: IsEligibleFn;
  /** Module-level target scope (Program or wrapper IIFE) of the current graph */
  private targetScope?: import("@babel/traverse").Scope;
  /** Shared cross-function retry batching, active during processUnified */
  private retryBatcher?: RetryBatcher;

  /** Per-function rename reports (populated after processUnified completes) */
  get reports(): ReadonlyArray<RenameReport> {
    return this._reports;
  }

  /** Number of functions that failed due to LLM errors (populated after processUnified completes) */
  get failed(): number {
    return this.failedCount;
  }

  /** Drop the reference to the bundle AST. Called after processUnified
   * completes and the renamed AST has been generated, so the post-naming
   * re-parse passes (validate/reconcile/sweep) run without this holding the
   * whole 30MB tree live. The counters callers still read (failed,
   * skippedBySkipList, skipReasons) are plain numbers and survive. */
  releaseAst(): void {
    this.ast = null;
  }

  /** Number of identifiers skipped by skip-list (not eligible for rename) */
  get skippedBySkipList(): number {
    return this._skippedBySkipList;
  }

  /** Why functions were skipped during processing */
  get skipReasons() {
    return { ...this._skipReasons };
  }

  constructor(ast: t.File) {
    this.ast = ast;
  }

  /**
   * Process a single function: get LLM suggestions and apply renames.
   */
  private async processFunction(
    fn: FunctionNode,
    llm: LLMProvider,
    usedNames: Set<string>
  ): Promise<void> {
    const allBindings = collectOwnedBindingInfos(fn.path);

    // If no bindings to rename, skip
    if (allBindings.length === 0) {
      this._skipReasons.zeroBindings++;
      markSkipped(fn, "zero-bindings");
      return;
    }

    // Filter out identifiers that already have descriptive names or were
    // pre-transferred from a prior version (close-match name transfers)
    const transferred = fn.priorVersionTransferred;
    const bindings = allBindings.filter(
      (b) => this.isEligible(b.name) && !transferred?.has(b.name)
    );
    this._skippedBySkipList += allBindings.length - bindings.length;

    if (bindings.length === 0) {
      this._skipReasons.allPreserved++;
      markSkipped(fn, "all-preserved");
      return;
    }

    // Accumulate names across the main pass and the shadowed-binding pass;
    // the terminal llm-done state records the full applied map.
    const names: Record<string, string> = {};
    await this.processFunctionBatched(fn, llm, bindings, usedNames, names);

    // After the main rename, check for block-scoped bindings that were skipped
    // during initial collection because they shadowed a function-scope name.
    // Now that the function-scope binding has been renamed, these are unique.
    // Exclude bindings already processed in phase 1 (their names may have changed
    // to something that still passes isEligible, but they don't need re-renaming).
    const phase1Ids = new WeakSet(allBindings.map((b) => b.identifier));
    const shadowedBindings = collectShadowedBlockBindings(
      fn.path,
      this.isEligible
    ).filter((b) => !phase1Ids.has(b.identifier));
    if (shadowedBindings.length > 0) {
      // Minifiers reuse one tiny name across MANY sibling block scopes.
      // The batch protocol keys identifiers by name, so same-named
      // bindings collapse to one — mechanically uniquify duplicates first
      // (AST order → version-stable suffixes), then name them all.
      const uniquified = this.uniquifySameNamedBindings(
        shadowedBindings,
        fn.sessionId
      );
      await this.processFunctionBatched(fn, llm, uniquified, usedNames, names);
    }

    markLlmDone(fn, names);
  }

  /**
   * Give duplicate-named bindings unique names so each is individually
   * addressable by the name-keyed batch protocol. The k-th binding of a
   * name group becomes `<name>_<k>` (validated; suffix bumps on
   * collision). Applied through the standard validated-rename path and
   * recorded as decisions — if the LLM later fails, both legs of a
   * cross-version run still agree on the mechanical name.
   */
  private uniquifySameNamedBindings(
    bindings: BindingInfo[],
    functionId: string
  ): BindingInfo[] {
    const seen = new Map<string, number>();
    return bindings.map((binding) => {
      const count = (seen.get(binding.name) ?? 0) + 1;
      seen.set(binding.name, count);
      if (count === 1) return binding;
      const renamed = this.applyUniquifyRename(binding, count, functionId);
      return renamed ?? binding;
    });
  }

  /** Apply one uniquify rename, bumping the suffix past collisions. */
  private applyUniquifyRename(
    binding: BindingInfo,
    ordinal: number,
    functionId: string
  ): BindingInfo | null {
    const base = binding.name;
    for (let suffix = ordinal; suffix < ordinal + 20; suffix++) {
      const candidate = `${base}_${suffix}`;
      const attempt = attemptValidatedRename(binding.scope, base, candidate);
      if (attempt.applied) {
        const loc = binding.identifier.loc;
        if (loc) {
          this.allRenames.push({
            originalPosition: {
              line: loc.start.line,
              column: loc.start.column
            },
            originalName: base,
            newName: candidate,
            functionId
          });
        }
        return { ...binding, name: candidate };
      }
      // Only name-availability rejections are retryable with a new suffix.
      if (
        attempt.reason !== "target-in-scope" &&
        attempt.reason !== "target-visible" &&
        attempt.reason !== "shadows-child"
      ) {
        debug.log(
          "processor",
          `${functionId}: uniquify ${base}→${candidate} rejected (${attempt.reason})`
        );
        return null;
      }
    }
    return null;
  }

  /**
   * Process a function using batch renaming - asks LLM for all names at once.
   * Uses the unified batch pipeline with function-specific callbacks. Applied
   * renames accumulate into `names` (the caller records them on the terminal
   * lifecycle state once all passes complete).
   */
  private async processFunctionBatched(
    fn: FunctionNode,
    llm: LLMProvider,
    bindings: BindingInfo[],
    usedNames: Set<string>,
    names: Record<string, string>
  ): Promise<void> {
    if (!this.ast) throw new Error("processor AST released before processing");
    const context = buildContext(fn, this.ast, this.isEligible);

    const makeCallbacks = this.buildFunctionCallbacks(
      fn,
      bindings,
      context,
      names,
      usedNames
    );

    const laneThreshold = this.options.laneThreshold ?? DEFAULT_LANE_THRESHOLD;
    const report = await this.processBatch(
      bindings.map((b) => b.name),
      makeCallbacks,
      llm,
      "function",
      fn.sessionId,
      laneThreshold
    );
    // The shadowed-binding second pass reuses this method; merge so the
    // main pass's outcomes stay visible to diagnostics.
    report.structuralHash = fn.fingerprint.structuralHash;
    fn.renameReport = fn.renameReport
      ? mergeRenameReports(fn.renameReport, report)
      : report;
    fn.renameReport.structuralHash = fn.fingerprint.structuralHash;
  }

  /**
   * Build batch rename callbacks for function identifiers.
   * Captures function context, binding map, and rename tracking in closures.
   */
  private buildFunctionCallbacks(
    fn: FunctionNode,
    bindings: BindingInfo[],
    context: LLMContext,
    renameMapping: Record<string, string>,
    usedNames: Set<string>
  ): (laneId: string) => BatchRenameCallbacks {
    const bindingMap = new Map(bindings.map((b) => [b.name, b]));

    // Cache proximity-windowed usedNames: same identifiers AND same usedIdentifiers size → same window.
    // usedIdentifiers only grows (via add()), so size change means another lane renamed something.
    let cachedWindowKey: string | undefined;
    let cachedUsedSize: number | undefined;
    let cachedWindowedNames: Set<string> | undefined;

    // A close-matched function's suggestions snap to same-stem prior
    // names — the LLM re-decorates (identityVal → identityVar) and every
    // such choice is a diff hunk against the prior release.
    const priorStemIndex = fn.priorVersionNames?.length
      ? buildPriorStemIndex(fn.priorVersionNames)
      : undefined;

    return buildCallbacks({
      getScope: (name) => bindingMap.get(name)?.scope,
      transformSuggestion: priorStemIndex
        ? (_oldName, suggestion) =>
            snapSuggestionToPrior(suggestion, priorStemIndex)
        : undefined,
      applyRename: (oldName, newName) => {
        const binding = bindingMap.get(oldName);
        if (binding) {
          this.applyFunctionRename(
            binding,
            oldName,
            newName,
            fn.sessionId,
            context.usedIdentifiers,
            renameMapping,
            usedNames
          );
        }
      },
      buildRequest: (remaining, round, prev, failures) => {
        const fullCode = selectRequestCode(fn, remaining, bindingMap);
        const priorContext = capPriorContext(fn);
        // Context diet: retries concern a few identifiers of an
        // already-seen function — send only the referencing lines and the
        // conflict-relevant names instead of the full first-round prompt.
        const isRetryRound = round > 1;
        const code = isRetryRound
          ? extractRetrySnippet(fullCode, remaining)
          : fullCode;

        const windowKey = remaining.join(",");
        const currentUsedSize = context.usedIdentifiers.size;
        let windowedUsedNames: Set<string>;
        if (
          windowKey === cachedWindowKey &&
          currentUsedSize === cachedUsedSize &&
          cachedWindowedNames
        ) {
          windowedUsedNames = cachedWindowedNames;
        } else {
          windowedUsedNames = computeWindowedUsedNames(
            remaining,
            bindingMap,
            fn,
            context.usedIdentifiers,
            this.isEligible
          );
          cachedWindowKey = windowKey;
          cachedUsedSize = currentUsedSize;
          cachedWindowedNames = windowedUsedNames;
        }
        const usedNamesForPrompt = isRetryRound
          ? buildRetryUsedNames(windowedUsedNames, prev)
          : windowedUsedNames;

        // Already-renamed identifiers give the LLM fixed naming context:
        // prior-version transfers on the first round (they are applied in
        // the code it sees), plus this run's earlier rounds on retries.
        let alreadyRenamed: Record<string, string> | undefined;
        const transferredPairs = fn.priorVersionTransferredPairs;
        if (transferredPairs && Object.keys(transferredPairs).length > 0) {
          alreadyRenamed = { ...transferredPairs };
        }
        if (isRetryRound && Object.keys(renameMapping).length > 0) {
          alreadyRenamed = { ...alreadyRenamed, ...renameMapping };
        }

        // The tail-less prompt body lets the retry batcher merge this
        // group with other functions' retries into one call.
        const promptBody = isRetryRound
          ? buildBatchRenameRetryBody(
              code,
              remaining,
              usedNamesForPrompt,
              prev,
              failures,
              priorContext,
              alreadyRenamed
            )
          : undefined;

        return {
          code,
          identifiers: remaining,
          usedNames: usedNamesForPrompt,
          calleeSignatures: context.calleeSignatures,
          callsites: context.callsites,
          contextVars: context.contextVars,
          priorVersionCode: priorContext,
          priorVersionNames: fn.priorVersionNames,
          priorNameHints: fn.priorNameHints,
          isRetry: isRetryRound,
          previousAttempt: isRetryRound ? prev : undefined,
          failures: isRetryRound ? failures : undefined,
          alreadyRenamed,
          promptBody
        };
      },
      getUsedNames: () => {
        const merged = new Set(context.usedIdentifiers);
        for (const n of usedNames) merged.add(n);
        return merged;
      },
      functionId: fn.sessionId,
      onUnrenamed: (name) => {
        const binding = bindingMap.get(name);
        if (binding) {
          const loc = binding.identifier.loc;
          if (loc) {
            this.allRenames.push({
              originalPosition: {
                line: loc.start.line,
                column: loc.start.column
              },
              originalName: name,
              newName: name,
              functionId: fn.sessionId
            });
          }
          renameMapping[name] = name;
        }
      }
    });
  }

  /**
   * Apply a rename to a function binding and record the decision.
   */
  private applyFunctionRename(
    binding: BindingInfo,
    oldName: string,
    newName: string,
    functionId: string,
    usedIdentifiers: Set<string>,
    renameMapping: Record<string, string>,
    usedNames: Set<string>
  ): void {
    // Defense-in-depth: the batch guard (wouldReject) should have filtered
    // unsafe names, but this is the mutation site — enforce the full
    // validated path so no caller can introduce a collision or capture.
    const attempt = attemptValidatedRename(binding.scope, oldName, newName);
    if (!attempt.applied) {
      debug.log(
        "processor",
        `${functionId}: skipping ${oldName}→${newName} — ${attempt.reason}`
      );
      return;
    }

    const loc = binding.identifier.loc;
    if (loc) {
      this.allRenames.push({
        originalPosition: { line: loc.start.line, column: loc.start.column },
        originalName: oldName,
        newName,
        functionId
      });
    }
    usedIdentifiers.delete(oldName);
    usedIdentifiers.add(newName);
    renameMapping[oldName] = newName;

    // If this binding is in the module-level scope (Program, or the wrapper
    // IIFE scope in bundles like Bun's), also register it in usedNames so
    // other lanes and the module-binding path won't collide.
    if (this.isModuleLevelScope(binding.scope)) {
      usedNames.delete(oldName);
      usedNames.add(newName);
    }
  }

  /** True for the graph's target scope (wrapper IIFE) or the Program scope. */
  private isModuleLevelScope(scope: {
    path: { isProgram: () => boolean };
  }): boolean {
    return scope === this.targetScope || scope.path.isProgram();
  }

  /**
   * Process a unified graph of function nodes and module-level bindings.
   * Both types are processed in a single parallel pass, leaf-first.
   */
  async processUnified(
    graph: UnifiedGraph,
    llm: LLMProvider,
    options: ProcessorOptions = {}
  ): Promise<RenameDecision[]> {
    const { concurrency = 50, metrics } = options;
    const { isEligible, profiler } = resolveRunConfig(options);

    this.options = options;
    this.metrics = metrics;
    this.isEligible = isEligible;
    // Retry rounds from concurrently processing functions/lanes merge into
    // shared LLM calls — the collision-retry tail used to run per-function.
    this.retryBatcher = new RetryBatcher(llm, metrics, {
      windowMs: options.retryBatchWindowMs,
      maxBatch: options.batchSize ?? DEFAULT_BATCH_SIZE
    });

    // Nodes already settled before processing (frozen functions, transferred
    // exact matches, cascade-matched module bindings) stay in the graph so
    // dependency edges keep resolving; they seed the done set instead of
    // being dispatched.
    const doneIds = new Set<string>();
    for (const [id, renameNode] of graph.nodes) {
      if (isSettled(renameNode.node)) doneIds.add(id);
    }
    assertUnifiedGraphClosure(graph, doneIds);

    const allNodeIds = [...graph.nodes.keys()].filter((id) => !doneIds.has(id));
    const { functionCount, moduleBindingCount } = countNodeTypes(
      allNodeIds,
      graph
    );
    if (metrics) {
      metrics.setFunctionTotal(functionCount);
      metrics.setModuleBindingTotal(moduleBindingCount);
    }

    await this.runProcessUnifiedLoop(
      graph,
      llm,
      profiler,
      metrics,
      concurrency,
      doneIds,
      allNodeIds,
      functionCount,
      moduleBindingCount
    );

    for (const [, renameNode] of graph.nodes) {
      if (renameNode.type === "function" && renameNode.node.renameReport)
        this._reports.push(renameNode.node.renameReport);
    }
    this.retryBatcher = undefined;
    metrics?.emit();
    return this.allRenames;
  }

  /** Run the main dispatch loop for processUnified. */
  private async runProcessUnifiedLoop(
    graph: UnifiedGraph,
    llm: LLMProvider,
    profiler: import("../profiling/profiler.js").Profiler,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    concurrency: number,
    doneIds: Set<string>,
    allNodeIds: string[],
    functionCount: number,
    moduleBindingCount: number
  ): Promise<{ doneIds: Set<string> }> {
    const processingIds = new Set<string>();
    const readyIds = new Set<string>();
    this.targetScope = graph.targetScope;
    const usedNames = collectModuleUsedNames(graph.targetScope);
    const isNodeReady = (id: string) => checkNodeReady(id, graph, doneIds);
    const isNodeReadyIgnoringScopeParent = (id: string) =>
      checkNodeReadyIgnoringScopeParent(id, graph, doneIds);
    const totalNodes = allNodeIds.length;

    const { pendingCount: initPending, blockedIds } = initUnifiedState(
      allNodeIds,
      doneIds,
      isNodeReady,
      isNodeReadyIgnoringScopeParent,
      readyIds,
      metrics,
      totalNodes,
      functionCount,
      moduleBindingCount,
      doneIds.size
    );

    const depthMap = computeDependentDepths(graph);
    const limit = createConcurrencyLimiter(concurrency);
    const isEsbuild = this.options.bundlerType === "esbuild";
    const defaultModuleConcurrency = isEsbuild ? 40 : 20;
    const moduleConcurrency =
      this.options.moduleConcurrency ?? defaultModuleConcurrency;
    const moduleLimit = createConcurrencyLimiter(moduleConcurrency);
    const signals = {
      notifyCompletion: null as (() => void) | null,
      drainResolve: null as (() => void) | null
    };
    const inFlight = { count: 0 };
    const pending = { count: initPending };

    const readyAtMs = profiler.isEnabled ? new Map<string, number>() : null;
    if (readyAtMs) {
      const now = performance.now();
      for (const id of readyIds) readyAtMs.set(id, now);
    }

    profiler.startConcurrencySampling(() => ({
      inFlight: inFlight.count,
      ready: readyIds.size,
      blocked: pending.count
    }));

    const signalCompletion = makeSignalFn(signals);
    const decrementInflight = makeDecrementFn(inFlight, signals);
    const markDone = (id: string) => {
      processingIds.delete(id);
      doneIds.add(id);
      markDoneUnblockDependents(
        id,
        graph,
        blockedIds,
        readyIds,
        readyAtMs,
        isNodeReady,
        metrics,
        (n) => {
          pending.count -= n;
        }
      );
    };

    await this.runUnifiedDispatchLoop(
      graph,
      llm,
      usedNames,
      profiler,
      metrics,
      limit,
      moduleLimit,
      depthMap,
      readyIds,
      processingIds,
      inFlight,
      readyAtMs,
      markDone,
      signalCompletion,
      decrementInflight,
      blockedIds,
      signals,
      isNodeReadyIgnoringScopeParent,
      doneIds,
      totalNodes,
      pending
    );

    profiler.stopConcurrencySampling();
    return { doneIds };
  }

  /** The while-loop body for processUnified: dispatch + wait + deadlock-break. */
  private async runUnifiedDispatchLoop(
    graph: UnifiedGraph,
    llm: LLMProvider,
    usedNames: Set<string>,
    profiler: import("../profiling/profiler.js").Profiler,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    limit: ReturnType<typeof createConcurrencyLimiter>,
    moduleLimit: ReturnType<typeof createConcurrencyLimiter>,
    depthMap: Map<string, number>,
    readyIds: Set<string>,
    processingIds: Set<string>,
    inFlight: { count: number },
    readyAtMs: Map<string, number> | null,
    markDone: (id: string) => void,
    signalCompletion: () => void,
    decrementInflight: () => void,
    blockedIds: Set<string>,
    signals: {
      notifyCompletion: (() => void) | null;
      drainResolve: (() => void) | null;
    },
    isNodeReadyIgnoringScopeParent: (id: string) => boolean,
    doneIds: Set<string>,
    totalNodes: number,
    pending: { count: number }
  ): Promise<void> {
    while (readyIds.size > 0 || processingIds.size > 0) {
      const { fnCount, mbCount } = this.dispatchUnifiedReady(
        graph,
        llm,
        usedNames,
        profiler,
        metrics,
        limit,
        moduleLimit,
        depthMap,
        readyIds,
        processingIds,
        inFlight,
        readyAtMs,
        markDone,
        signalCompletion,
        decrementInflight
      );

      if (fnCount > 0 || mbCount > 0) {
        debug.queueState({
          ready: readyIds.size,
          processing: processingIds.size,
          pending: pending.count,
          done: doneIds.size,
          total: totalNodes,
          inFlightLLM: inFlight.count,
          event: "dispatch",
          detail: `fns=${fnCount} mbs=${mbCount}`
        });
      }

      if (readyIds.size === 0 && processingIds.size > 0) {
        debug.queueState({
          ready: 0,
          processing: processingIds.size,
          pending: pending.count,
          done: doneIds.size,
          total: totalNodes,
          inFlightLLM: inFlight.count,
          event: "waiting-on-llm"
        });
        await new Promise<void>((resolve) => {
          signals.notifyCompletion = resolve;
        });
      }

      if (
        readyIds.size === 0 &&
        processingIds.size === 0 &&
        blockedIds.size > 0
      ) {
        handleMidLoopDeadlock(
          blockedIds,
          readyIds,
          isNodeReadyIgnoringScopeParent,
          doneIds,
          totalNodes,
          pending,
          metrics
        );
      }
    }

    if (inFlight.count > 0) {
      await new Promise<void>((resolve) => {
        signals.drainResolve = resolve;
      });
    }
  }

  /** Dispatch all ready nodes (functions + module binding batches). Returns counts. */
  private dispatchUnifiedReady(
    graph: UnifiedGraph,
    llm: LLMProvider,
    usedNames: Set<string>,
    profiler: import("../profiling/profiler.js").Profiler,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    limit: ReturnType<typeof createConcurrencyLimiter>,
    moduleLimit: ReturnType<typeof createConcurrencyLimiter>,
    depthMap: Map<string, number>,
    readyIds: Set<string>,
    processingIds: Set<string>,
    inFlight: { count: number },
    readyAtMs: Map<string, number> | null,
    markDone: (id: string) => void,
    signalCompletion: () => void,
    decrementInflight: () => void
  ): { fnCount: number; mbCount: number } {
    const readyFunctions: Array<[string, FunctionNode]> = [];
    const readyModuleBindings: ModuleBindingNode[] = [];
    for (const id of [...readyIds]) {
      readyIds.delete(id);
      const renameNode = graph.nodes.get(id);
      if (!renameNode) throw new Error(`Node not found in graph: ${id}`);
      if (renameNode.type === "function")
        readyFunctions.push([id, renameNode.node]);
      else readyModuleBindings.push(renameNode.node);
    }

    // Sort functions by descending dependent depth (critical path first)
    readyFunctions.sort(
      (a, b) => (depthMap.get(b[0]) ?? 0) - (depthMap.get(a[0]) ?? 0)
    );

    for (const [id, fn] of readyFunctions) {
      this.dispatchUnifiedFunction(
        id,
        fn,
        llm,
        usedNames,
        profiler,
        metrics,
        limit,
        processingIds,
        inFlight,
        readyAtMs,
        markDone,
        signalCompletion,
        decrementInflight
      );
    }
    const moduleMaxGroupSize = this.options.bundlerType === "esbuild" ? 15 : 10;
    for (const group of groupByProximity(
      readyModuleBindings,
      50,
      moduleMaxGroupSize
    )) {
      this.dispatchUnifiedModuleBatch(
        group,
        llm,
        usedNames,
        graph,
        profiler,
        metrics,
        moduleLimit,
        processingIds,
        inFlight,
        markDone,
        signalCompletion,
        decrementInflight
      );
    }

    return {
      fnCount: readyFunctions.length,
      mbCount: readyModuleBindings.length
    };
  }

  /** Dispatch a single function node in the unified processor. */
  private dispatchUnifiedFunction(
    id: string,
    fn: FunctionNode,
    llm: LLMProvider,
    usedNames: Set<string>,
    profiler: import("../profiling/profiler.js").Profiler,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    limit: ReturnType<typeof createConcurrencyLimiter>,
    processingIds: Set<string>,
    inFlight: { count: number },
    readyAtMs: Map<string, number> | null,
    markDone: (id: string) => void,
    signalCompletion: () => void,
    decrementInflight: () => void
  ): void {
    processingIds.add(id);
    inFlight.count++;
    metrics?.functionStarted();
    const waitMs = readyAtMs?.has(id)
      ? performance.now() - (readyAtMs.get(id) ?? performance.now())
      : 0;
    readyAtMs?.delete(id);
    const fnSpan = profiler.startSpan(
      `fn:${id}`,
      "rename",
      TRACE_TID.RENAME_FUNCTION,
      { waitMs }
    );
    limit(async () => {
      try {
        await this.processFunction(fn, llm, usedNames);
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        debug.log(
          "unified-processor",
          `Function ${fn.sessionId} failed: ${msg}`
        );
        this.failedCount++;
        this._skipReasons.error++;
        // If the function threw before reaching a terminal state, settle it as
        // failed. A late throw (e.g. in the shadowed-binding pass, after the
        // main pass already settled it llm-done) leaves that state in place.
        if (isPending(fn)) markFailed(fn, msg);
      } finally {
        fnSpan.end();
        metrics?.functionCompleted();
        markDone(id);
        signalCompletion();
        decrementInflight();
      }
    });
  }

  /** Dispatch a module binding batch in the unified processor. */
  private dispatchUnifiedModuleBatch(
    batch: ModuleBindingNode[],
    llm: LLMProvider,
    usedNames: Set<string>,
    graph: UnifiedGraph,
    profiler: import("../profiling/profiler.js").Profiler,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    limit: ReturnType<typeof createConcurrencyLimiter>,
    processingIds: Set<string>,
    inFlight: { count: number },
    markDone: (id: string) => void,
    signalCompletion: () => void,
    decrementInflight: () => void
  ): void {
    for (const mb of batch) {
      processingIds.add(mb.sessionId);
    }
    inFlight.count++;
    for (let i = 0; i < batch.length; i++) metrics?.moduleBindingStarted();
    const batchIds = batch.map((b) => b.sessionId).join(",");
    const mbSpan = profiler.startSpan(
      `mb:${batchIds}`,
      "rename",
      TRACE_TID.RENAME_MODULE_BINDING
    );
    limit(async () => {
      try {
        await this.processModuleBindingBatch(batch, llm, usedNames, graph);
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        debug.log("unified-processor", `Module binding batch failed: ${msg}`);
        this.failedCount += batch.length;
      } finally {
        mbSpan.end({ batchSize: batch.length });
        for (const b of batch) {
          // Settle every dispatched binding (all pending here), matching the
          // pre-refactor behavior of marking the batch done regardless of a
          // mid-batch throw.
          if (isPending(b)) markLlmDone(b);
          metrics?.moduleBindingCompleted();
          markDone(b.sessionId);
        }
        signalCompletion();
        decrementInflight();
      }
    });
  }

  /**
   * Process a batch of module-level bindings via the LLM.
   * Uses the unified batch pipeline with module-binding-specific callbacks.
   */
  private async processModuleBindingBatch(
    batch: ModuleBindingNode[],
    llm: LLMProvider,
    usedNames: Set<string>,
    graph: UnifiedGraph
  ): Promise<void> {
    const assignmentContext: Record<string, string[]> = {};
    const usageExamples: Record<string, string[]> = {};
    const suggestedNames: Record<string, string> = {};
    for (const b of batch) {
      assignmentContext[b.name] = b.assignments;
      usageExamples[b.name] = b.usages;
      if (b.suggestedName) suggestedNames[b.name] = b.suggestedName;
    }

    const batchLines = batch.map((b) => b.declarationLine);
    const totalBindings = Object.keys(graph.targetScope.bindings).length;
    const windowedNames = getProximateUsedNames(
      usedNames,
      batchLines,
      graph.targetScope.bindings,
      totalBindings,
      this.isEligible
    );

    const makeCallbacks = this.buildModuleBindingCallbacks(
      batch,
      usedNames,
      windowedNames,
      assignmentContext,
      usageExamples,
      suggestedNames
    );

    const report = await this.processBatch(
      batch.map((b) => b.name),
      makeCallbacks,
      llm,
      "module-binding",
      `module-binding-batch:${batch.map((b) => b.name).join(",")}`
    );
    this._reports.push(report);
  }

  /**
   * Build batch rename callbacks for module-level bindings.
   * Captures binding map, used names, and prompt context in closures.
   */
  private buildModuleBindingCallbacks(
    batch: ModuleBindingNode[],
    usedNames: Set<string>,
    windowedNames: Set<string>,
    assignmentContext: Record<string, string[]>,
    usageExamples: Record<string, string[]>,
    suggestedNames: Record<string, string>
  ): (laneId: string) => BatchRenameCallbacks {
    const bindingMap = new Map(batch.map((b) => [b.name, b]));
    const batchId = `module-binding-batch:${batch.map((b) => b.name).join(",")}`;

    return buildCallbacks({
      getScope: (name) => bindingMap.get(name)?.scope,
      // Each binding's suggestedName is its exact prior-version name —
      // when the LLM merely re-decorates it, reuse the prior verbatim.
      transformSuggestion: (oldName, suggestion) => {
        const prior = suggestedNames[oldName];
        if (!prior || prior === suggestion) return suggestion;
        return nameStem(prior) === nameStem(suggestion) ? prior : suggestion;
      },
      applyRename: (oldName, newName) => {
        const mb = bindingMap.get(oldName);
        if (mb) {
          // Mirror applyFunctionRename: the validated path handles the
          // export-involved fallback to Babel's renamer internally, and
          // enforces collision/capture safety at the mutation site.
          const attempt = attemptValidatedRename(mb.scope, oldName, newName);
          if (!attempt.applied) {
            debug.log(
              "processor",
              `module-binding: skipping ${oldName}→${newName} — ${attempt.reason}`
            );
            return;
          }
          usedNames.delete(oldName);
          usedNames.add(newName);
        }
      },
      buildRequest: (remaining, round, prev, failures) => {
        const declarations = [
          ...new Set(
            remaining
              .map((id) => bindingMap.get(id)?.declaration)
              .filter((d): d is string => d !== undefined)
          )
        ];

        // Context diet: retry prompts carry only conflict-relevant names —
        // the module-level used list is otherwise unbounded.
        const isRetryRound = round > 1;
        const promptNames = isRetryRound
          ? buildRetryUsedNames(windowedNames, prev)
          : windowedNames;

        let userPrompt = buildModuleLevelRenamePrompt(
          declarations,
          assignmentContext,
          usageExamples,
          remaining,
          promptNames,
          this.isEligible,
          suggestedNames
        );

        let promptBody: string | undefined;
        if (isRetryRound) {
          const retryPrefix = buildModuleLevelRetryPrefix(prev, failures);
          userPrompt = `${retryPrefix}\n${userPrompt}`;
          // Tail-less body for the retry batcher (merged calls share one tail)
          promptBody = `${retryPrefix}\n${buildModuleLevelRenameBody(
            declarations,
            assignmentContext,
            usageExamples,
            remaining,
            promptNames,
            this.isEligible,
            suggestedNames
          )}`;
        }

        return {
          code: "",
          identifiers: remaining,
          usedNames: promptNames,
          calleeSignatures: [],
          callsites: [],
          systemPrompt: MODULE_LEVEL_RENAME_SYSTEM_PROMPT,
          userPrompt,
          isRetry: isRetryRound,
          previousAttempt: isRetryRound ? prev : undefined,
          failures: isRetryRound ? failures : undefined,
          promptBody
        };
      },
      getUsedNames: () => usedNames,
      functionId: batchId
    });
  }

  /**
   * Unified batch rename pipeline for both function and module-binding identifiers.
   * Handles optional lane splitting, batch rename loop, and report construction.
   */
  private async processBatch(
    identifiers: string[],
    makeCallbacks: (laneId: string) => BatchRenameCallbacks,
    llm: LLMProvider,
    reportType: RenameReport["type"],
    targetId: string,
    laneThreshold?: number
  ): Promise<RenameReport> {
    let allOutcomes: Record<string, IdentifierOutcome> = {};
    let allFinishReasons: (string | undefined)[] = [];
    let totalLLMCalls = 0;
    let totalRemaining = new Set<string>();

    const effectiveLaneThreshold = laneThreshold ?? DEFAULT_LANE_THRESHOLD;
    const numLanes = computeLaneCount(
      identifiers.length,
      effectiveLaneThreshold
    );
    if (numLanes > 0) {
      const lanes = splitByPosition(identifiers, numLanes);
      debug.log(
        "processor",
        `${targetId}: splitting ${identifiers.length} bindings into ${lanes.length} lanes`
      );

      const laneResults = await Promise.all(
        lanes.map((lane, i) =>
          this.runBatchRenameLoop(llm, lane, makeCallbacks(`:lane${i}`))
        )
      );

      for (const result of laneResults) {
        Object.assign(allOutcomes, result.outcomes);
        allFinishReasons.push(...result.finishReasons);
        totalLLMCalls += result.totalLLMCalls;
        for (const name of result.remaining) totalRemaining.add(name);
      }
    } else {
      const result = await this.runBatchRenameLoop(
        llm,
        identifiers,
        makeCallbacks("")
      );
      allOutcomes = result.outcomes;
      allFinishReasons = result.finishReasons;
      totalLLMCalls = result.totalLLMCalls;
      totalRemaining = result.remaining;
    }

    return {
      type: reportType,
      strategy: "llm",
      targetId,
      totalIdentifiers: identifiers.length,
      renamedCount: identifiers.length - totalRemaining.size,
      outcomes: allOutcomes,
      totalLLMCalls,
      finishReasons: allFinishReasons
    };
  }

  /**
   * Shared batch rename loop using batch-until-done model.
   *
   * Processes identifiers in batch windows, retrying failures within each window
   * before advancing. Each identifier tracks its own attempt count. The loop
   * terminates when the queue empties or all identifiers exhaust retries.
   *
   * Handles: per-identifier retry tracking, free retries for cross-lane
   * collisions, adaptive batch sizing, straggler pass, resolveRemaining fallback.
   */
  private async runBatchRenameLoop(
    llm: LLMProvider,
    identifierNames: string[],
    callbacks: BatchRenameCallbacks
  ): Promise<BatchRenameLoopResult> {
    const maxBatchSize = this.options.batchSize ?? DEFAULT_BATCH_SIZE;
    const maxRetriesPerIdentifier =
      this.options.maxRetriesPerIdentifier ?? DEFAULT_MAX_RETRIES_PER_ID;
    const maxFreeRetries = computeMaxFreeRetries(
      identifierNames.length,
      this.options.maxFreeRetries
    );

    const outcomes: Record<string, IdentifierOutcome> = {};
    const finishReasons: (string | undefined)[] = [];

    const idState = new Map<string, IdentifierAttemptState>();
    for (const name of identifierNames) {
      idState.set(name, { attempts: 0, freeRetries: 0 });
    }

    const queue = [...identifierNames];
    const retryExhausted: string[] = [];
    let totalLLMCalls = 0;
    let adaptiveBatchSize = maxBatchSize;
    let lastUserPrompt = "";
    let lastResponseRenames: Record<string, string> = {};
    let lastValidation: BatchValidationResult | undefined;

    while (queue.length > 0) {
      const batchResult = await this.runBatchWindow(
        llm,
        queue,
        adaptiveBatchSize,
        idState,
        finishReasons,
        outcomes,
        retryExhausted,
        callbacks,
        maxFreeRetries,
        maxRetriesPerIdentifier
      );
      adaptiveBatchSize = batchResult.adaptiveBatchSize;
      lastUserPrompt = batchResult.lastUserPrompt;
      lastResponseRenames = batchResult.lastResponseRenames;
      lastValidation = batchResult.lastValidation;
      totalLLMCalls += batchResult.llmCallsThisWindow;
    }

    await this.runStragglerPass(
      llm,
      retryExhausted,
      outcomes,
      idState,
      finishReasons,
      adaptiveBatchSize,
      callbacks,
      totalLLMCalls
    );

    totalLLMCalls += finishReasons.length - totalLLMCalls;

    const remaining = new Set(
      identifierNames.filter((name) => !outcomes[name])
    );

    if (callbacks.resolveRemaining) {
      const combinedPrev: Record<string, string> = {};
      for (const name of remaining) {
        const state = idState.get(name);
        if (state?.lastSuggestion) combinedPrev[name] = state.lastSuggestion;
      }
      callbacks.resolveRemaining(
        remaining,
        combinedPrev,
        outcomes,
        finishReasons.length
      );
    }

    const { finalFailures, finalPreviousAttempt } = buildFinalFailures(
      remaining,
      idState
    );

    recordUnrenamedOutcomes(
      remaining,
      idState,
      outcomes,
      finishReasons,
      callbacks,
      lastUserPrompt,
      lastResponseRenames,
      lastValidation,
      finishReasons.length
    );

    return {
      outcomes,
      finishReasons,
      remaining,
      totalLLMCalls: finishReasons.length,
      previousAttempt: finalPreviousAttempt,
      failures: finalFailures
    };
  }

  /** Run a single batch window (outer queue iteration), returns updated state. */
  private async runBatchWindow(
    llm: LLMProvider,
    queue: string[],
    adaptiveBatchSize: number,
    idState: Map<string, IdentifierAttemptState>,
    finishReasons: (string | undefined)[],
    outcomes: Record<string, IdentifierOutcome>,
    retryExhausted: string[],
    callbacks: BatchRenameCallbacks,
    maxFreeRetries: number,
    maxRetriesPerIdentifier: number
  ): Promise<{
    adaptiveBatchSize: number;
    lastUserPrompt: string;
    lastResponseRenames: Record<string, string>;
    lastValidation: BatchValidationResult | undefined;
    llmCallsThisWindow: number;
  }> {
    const batch = queue.splice(0, adaptiveBatchSize);
    let batchRetries = batch.slice();
    let lastUserPrompt = "";
    let lastResponseRenames: Record<string, string> = {};
    let lastValidation: BatchValidationResult | undefined;
    let llmCallsThisWindow = 0;

    while (batchRetries.length > 0) {
      const callResult = await this.runSingleBatchCall(
        llm,
        batchRetries,
        idState,
        finishReasons,
        outcomes,
        callbacks,
        adaptiveBatchSize
      );
      llmCallsThisWindow++;

      if (callResult.failed) {
        retryExhausted.push(...batchRetries);
        break;
      }

      lastUserPrompt = callResult.lastUserPrompt;
      lastResponseRenames = callResult.lastResponseRenames;
      if (callResult.validation === undefined) break;
      lastValidation = callResult.validation;
      if (callResult.newAdaptiveBatchSize !== undefined) {
        adaptiveBatchSize = callResult.newAdaptiveBatchSize;
      }

      const responseRenames = callResult.responseRenames ?? {};
      const usedNamesSnapshot =
        callResult.usedNamesSnapshot ?? new Set<string>();
      const { nextRetry, exhausted } = classifyFailedIdentifiers(
        batchRetries,
        callResult.validation,
        responseRenames,
        idState,
        callbacks,
        usedNamesSnapshot,
        maxFreeRetries,
        maxRetriesPerIdentifier
      );
      retryExhausted.push(...exhausted);

      const batchSizeBefore = batchRetries.length;
      batchRetries = nextRetry;

      if (
        callResult.validThisCall === 0 &&
        nextRetry.length === batchSizeBefore
      ) {
        retryExhausted.push(...batchRetries);
        break;
      }
    }

    return {
      adaptiveBatchSize,
      lastUserPrompt,
      lastResponseRenames,
      lastValidation,
      llmCallsThisWindow
    };
  }

  /** Execute a single LLM call for a batch, returning the response data. */
  private async runSingleBatchCall(
    llm: LLMProvider,
    batchRetries: string[],
    idState: Map<string, IdentifierAttemptState>,
    finishReasons: (string | undefined)[],
    outcomes: Record<string, IdentifierOutcome>,
    callbacks: BatchRenameCallbacks,
    adaptiveBatchSize: number
  ): Promise<{
    failed: boolean;
    validThisCall: number;
    lastUserPrompt: string;
    lastResponseRenames: Record<string, string>;
    validation?: BatchValidationResult;
    responseRenames?: Record<string, string>;
    usedNamesSnapshot?: Set<string>;
    newAdaptiveBatchSize?: number;
  }> {
    const { prev, failures } = buildPrevAndFailures(batchRetries, idState);
    const isRetry = Object.keys(prev).length > 0;
    const usedNamesSnapshot = new Set(callbacks.getUsedNames());
    const callNum = finishReasons.length + 1;

    const promptStart = Date.now();
    const request = callbacks.buildRequest(
      batchRetries,
      isRetry ? 2 : 1,
      prev,
      failures
    );
    const promptMs = Date.now() - promptStart;
    const lastUserPrompt = request.userPrompt || "";

    debug.log(
      "batch-loop",
      `${callbacks.functionId} call ${callNum}: ${batchRetries.join(", ")}`
    );

    const llmStart = Date.now();
    let response: BatchRenameResponse;
    try {
      response = await this.dispatchRenameCall(llm, request);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      debug.log(
        "batch-loop",
        `${callbacks.functionId} call ${callNum} failed: ${msg}`
      );
      return {
        failed: true,
        validThisCall: 0,
        lastUserPrompt,
        lastResponseRenames: {}
      };
    }
    const llmMs = Date.now() - llmStart;

    finishReasons.push(response.finishReason);
    if (callbacks.transformSuggestion) {
      response = {
        ...response,
        renames: Object.fromEntries(
          Object.entries(response.renames).map(([oldName, suggestion]) => [
            oldName,
            callbacks.transformSuggestion?.(oldName, suggestion) ?? suggestion
          ])
        )
      };
    }
    const lastResponseRenames = response.renames;
    let newAdaptiveBatchSize: number | undefined;
    if (response.finishReason === "length" && adaptiveBatchSize > 2) {
      newAdaptiveBatchSize = Math.max(2, Math.floor(adaptiveBatchSize / 2));
    }

    const validation = validateBatchRenames(
      response.renames,
      new Set(batchRetries),
      callbacks.getUsedNames()
    );
    debug.validation(validation);

    const renameStart = Date.now();
    const { applied: validThisCall, lateCollisions } = applyValidRenames(
      validation,
      callbacks,
      idState,
      outcomes,
      callNum,
      isRetry
    );
    if (lateCollisions.length > 0) {
      validation.duplicates.push(...lateCollisions);
    }
    const renameMs = Date.now() - renameStart;

    debug.log(
      "batch-timing",
      `${callbacks.functionId} call=${callNum} prompt=${promptMs}ms llm=${llmMs}ms rename=${renameMs}ms valid=${validThisCall}/${batchRetries.length}`
    );

    return {
      failed: false,
      validThisCall,
      lastUserPrompt,
      lastResponseRenames,
      validation,
      responseRenames: response.renames,
      usedNamesSnapshot,
      newAdaptiveBatchSize
    };
  }

  /**
   * Route a rename call: retry rounds go through the shared batcher (which
   * merges concurrent groups and records metrics per actual call); first
   * rounds call the provider directly.
   */
  private async dispatchRenameCall(
    llm: LLMProvider,
    request: BatchRenameRequest
  ): Promise<BatchRenameResponse> {
    if (request.isRetry && this.retryBatcher) {
      return this.retryBatcher.submit(request);
    }
    const done = this.metrics?.llmCallStart();
    const response = await llm.suggestAllNames(request);
    done?.();
    this.metrics?.recordTokens(
      response.usage?.totalTokens ?? 0,
      response.usage?.inputTokens,
      response.usage?.outputTokens
    );
    return response;
  }

  /**
   * Straggler pass: one final attempt for identifiers the LLM never
   * answered (provider errors, missing from every response). Identifiers
   * that already carry a suggestion are excluded — their conflict resolves
   * algorithmically in resolveRemaining (suffixing), which costs nothing.
   */
  private async runStragglerPass(
    llm: LLMProvider,
    retryExhausted: string[],
    outcomes: Record<string, IdentifierOutcome>,
    idState: Map<string, IdentifierAttemptState>,
    finishReasons: (string | undefined)[],
    adaptiveBatchSize: number,
    callbacks: BatchRenameCallbacks,
    priorLLMCalls: number
  ): Promise<void> {
    if (retryExhausted.length === 0) return;
    const stragglers = retryExhausted.filter(
      (name) => !outcomes[name] && !idState.get(name)?.lastSuggestion
    );
    if (stragglers.length === 0) return;

    debug.log(
      "batch-loop",
      `${callbacks.functionId} straggler pass: ${stragglers.length} identifiers`
    );

    for (let i = 0; i < stragglers.length; i += adaptiveBatchSize) {
      const stragBatch = stragglers.slice(i, i + adaptiveBatchSize);
      const callNum = priorLLMCalls + finishReasons.length + 1;
      await this.runOneStragglerBatch(
        llm,
        stragBatch,
        callNum,
        idState,
        finishReasons,
        outcomes,
        callbacks
      );
    }
  }

  /** Execute a single straggler batch LLM call. */
  private async runOneStragglerBatch(
    llm: LLMProvider,
    stragBatch: string[],
    callNum: number,
    idState: Map<string, IdentifierAttemptState>,
    finishReasons: (string | undefined)[],
    outcomes: Record<string, IdentifierOutcome>,
    callbacks: BatchRenameCallbacks
  ): Promise<void> {
    const { prev, failures } = buildPrevAndFailures(stragBatch, idState);
    try {
      const request = callbacks.buildRequest(stragBatch, 2, prev, failures);
      const response = await this.dispatchRenameCall(llm, request);
      finishReasons.push(response.finishReason);
      const validation = validateBatchRenames(
        response.renames,
        new Set(stragBatch),
        callbacks.getUsedNames()
      );
      applyValidRenames(
        validation,
        callbacks,
        idState,
        outcomes,
        callNum,
        false
      );
      for (const name of stragBatch) {
        if (response.renames[name]) {
          const nameState = idState.get(name);
          if (nameState) nameState.lastSuggestion = response.renames[name];
        }
      }
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      debug.log(
        "batch-loop",
        `${callbacks.functionId} straggler batch failed: ${msg}`
      );
    }
  }
}

/** Retry snippets keep this many lines around each identifier reference. */
const RETRY_SNIPPET_CONTEXT_LINES = 2;
/** Code at or under this many lines is sent whole on retries. */
const RETRY_SNIPPET_MIN_LINES = 30;
/** Hard cap on retry snippet length. */
const RETRY_SNIPPET_MAX_LINES = 80;
/** Cap on the used-names list sent with retry prompts. */
const RETRY_USED_NAMES_CAP = 25;

/**
 * Extracts the retry-relevant lines of a function: the signature plus every
 * line referencing one of the remaining identifiers, with a little context.
 * Retries concern 1-3 identifiers of an already-seen function — re-sending
 * hundreds of lines re-pays prompt processing for nothing (the retry tail
 * measured ~4M input tokens on incremental runs).
 */
export function extractRetrySnippet(
  code: string,
  identifiers: string[]
): string {
  const lines = code.split("\n");
  if (lines.length <= RETRY_SNIPPET_MIN_LINES) return code;

  const patterns = identifiers.map((id) => identifierRegex(id));
  const keep = new Set<number>([0]);
  for (let i = 0; i < lines.length; i++) {
    if (!patterns.some((p) => p.test(lines[i]))) continue;
    const from = Math.max(0, i - RETRY_SNIPPET_CONTEXT_LINES);
    const to = Math.min(lines.length - 1, i + RETRY_SNIPPET_CONTEXT_LINES);
    for (let j = from; j <= to; j++) keep.add(j);
  }

  const kept = [...keep]
    .sort((a, b) => a - b)
    .slice(0, RETRY_SNIPPET_MAX_LINES);
  const parts: string[] = [];
  let prev = -1;
  for (const i of kept) {
    if (prev !== -1 && i > prev + 1) parts.push("  // …");
    parts.push(lines[i]);
    prev = i;
  }
  if (prev < lines.length - 1) parts.push("  // …");
  return parts.join("\n");
}

/**
 * Conflict-relevant used names for a retry prompt: the previous suggestions
 * (the names that actually collided) plus proximate scope names up to a cap.
 * Validation still runs against the FULL used-names set — this only shrinks
 * what the prompt carries.
 */
export function buildRetryUsedNames(
  windowedNames: Set<string>,
  previousAttempt: Record<string, string>
): Set<string> {
  const result = new Set<string>(Object.values(previousAttempt));
  for (const name of windowedNames) {
    if (result.size >= RETRY_USED_NAMES_CAP) break;
    result.add(name);
  }
  return result;
}

/**
 * Select the code shown for one function rename request: oversized code is
 * cut to declaration-anchored windows around the batch identifiers, so
 * every requested identifier is visible (see code-window.ts).
 */
function selectRequestCode(
  fn: FunctionNode,
  remaining: string[],
  bindingMap: Map<string, BindingInfo>
): string {
  return selectFunctionCode({
    code: generate(fn.path.node).code,
    sessionId: fn.sessionId,
    fnStartLine: fn.path.node.loc?.start.line,
    fnEndLine: fn.path.node.loc?.end.line,
    anchorStartLines: remaining.map(
      (name) => bindingMap.get(name)?.identifier.loc?.start.line
    )
  });
}

/**
 * Prior context of a close-matched megafunction must be capped or the
 * prompt overflows the model context and the whole batch 400-fails.
 */
function capPriorContext(fn: FunctionNode): string | undefined {
  return fn.priorVersionContext
    ? capContextCode(fn.priorVersionContext, fn.sessionId)
    : undefined;
}

/**
 * Merge two rename reports for the same target (main pass + shadowed-
 * binding pass). Counts add up; on an outcome-name collision (a shadowed
 * binding sharing a main-pass name) the later pass wins — the counts still
 * reflect both bindings.
 */
function mergeRenameReports(a: RenameReport, b: RenameReport): RenameReport {
  return {
    ...a,
    totalIdentifiers: a.totalIdentifiers + b.totalIdentifiers,
    renamedCount: a.renamedCount + b.renamedCount,
    outcomes: { ...a.outcomes, ...b.outcomes },
    totalLLMCalls: (a.totalLLMCalls ?? 0) + (b.totalLLMCalls ?? 0),
    finishReasons: [...(a.finishReasons ?? []), ...(b.finishReasons ?? [])]
  };
}

/** Compute proximity-windowed used names for a batch of identifiers. */
function computeWindowedUsedNames(
  remaining: string[],
  bindingMap: Map<string, BindingInfo>,
  fn: FunctionNode,
  usedIdentifiers: Set<string>,
  isEligible: IsEligibleFn
): Set<string> {
  const batchLines = remaining
    .map((id) => bindingMap.get(id)?.identifier.loc?.start?.line)
    .filter((l): l is number => l !== undefined);
  if (batchLines.length === 0) return usedIdentifiers;
  const scopeBindings = fn.path.scope.bindings;
  const totalBindings = Object.keys(scopeBindings).length;
  return getProximateUsedNames(
    usedIdentifiers,
    batchLines,
    scopeBindings,
    totalBindings,
    isEligible
  );
}

/** Check if a node in the unified graph has all its dependencies done. */
function checkNodeReady(
  id: string,
  graph: UnifiedGraph,
  doneIds: Set<string>
): boolean {
  const deps = graph.dependencies.get(id);
  if (!deps) return true;
  for (const dep of deps) {
    if (!doneIds.has(dep)) return false;
  }
  return true;
}

/** Check if a node is ready ignoring scopeParent edges (Tier 1 deadlock breaking). */
function checkNodeReadyIgnoringScopeParent(
  id: string,
  graph: UnifiedGraph,
  doneIds: Set<string>
): boolean {
  const deps = graph.dependencies.get(id);
  if (!deps) return true;
  for (const dep of deps) {
    if (doneIds.has(dep) || graph.scopeParentEdges.has(`${id}->${dep}`))
      continue;
    return false;
  }
  return true;
}

/** Create a signal callback that fires the notifyCompletion in the given signals object. */
function makeSignalFn(signals: {
  notifyCompletion: (() => void) | null;
}): () => void {
  return () => {
    if (signals.notifyCompletion) {
      const cb = signals.notifyCompletion;
      signals.notifyCompletion = null;
      cb();
    }
  };
}

/** Create a decrement callback for in-flight count, resolving drainResolve when hitting zero. */
function makeDecrementFn(
  inFlight: { count: number },
  signals: { drainResolve: (() => void) | null }
): () => void {
  return () => {
    inFlight.count--;
    if (inFlight.count === 0 && signals.drainResolve) {
      const cb = signals.drainResolve;
      signals.drainResolve = null;
      cb();
    }
  };
}

/** Count function vs module-binding nodes in the unified graph. */
function countNodeTypes(
  allNodeIds: string[],
  graph: UnifiedGraph
): { functionCount: number; moduleBindingCount: number } {
  let functionCount = 0;
  let moduleBindingCount = 0;
  for (const id of allNodeIds) {
    const renameNode = graph.nodes.get(id);
    if (!renameNode) throw new Error(`Node not found in graph: ${id}`);
    if (renameNode.type === "function") functionCount++;
    else moduleBindingCount++;
  }
  return { functionCount, moduleBindingCount };
}

/** Find and populate the initial ready set for the unified processor. Returns count. */
function initUnifiedReadySet(
  allNodeIds: string[],
  isNodeReady: (id: string) => boolean,
  readyIds: Set<string>
): number {
  let count = 0;
  for (const id of allNodeIds) {
    if (isNodeReady(id)) {
      readyIds.add(id);
      count++;
    }
  }
  return count;
}

/**
 * Initialize the unified processor's ready/blocked state.
 * Populates readyIds, runs initial deadlock-breaking if needed, builds blockedIds,
 * and returns the initial pendingCount.
 */
function initUnifiedState(
  allNodeIds: string[],
  doneIds: Set<string>,
  isNodeReady: (id: string) => boolean,
  isNodeReadyIgnoringScopeParent: (id: string) => boolean,
  readyIds: Set<string>,
  metrics: import("../llm/metrics.js").MetricsTracker | undefined,
  totalNodes: number,
  functionCount: number,
  moduleBindingCount: number,
  settledAtSeedCount: number
): { pendingCount: number; blockedIds: Set<string> } {
  const initialReady = initUnifiedReadySet(allNodeIds, isNodeReady, readyIds);

  if (readyIds.size === 0 && allNodeIds.length > 0) {
    const deadlockReady = breakInitialDeadlockUnified(
      allNodeIds,
      doneIds,
      isNodeReadyIgnoringScopeParent,
      readyIds
    );
    debug.log(
      "unified-processor",
      `Initial deadlock break: readied ${deadlockReady} of ${totalNodes} total (fns=${functionCount} mbs=${moduleBindingCount})`
    );
  } else if (initialReady > 0) {
    debug.log(
      "unified-processor",
      `Initial ready: ${initialReady} of ${totalNodes} (fns=${functionCount} mbs=${moduleBindingCount})`
    );
  }

  if (metrics && readyIds.size > 0) metrics.functionsReady(readyIds.size);
  if (metrics && settledAtSeedCount > 0)
    metrics.functionsReady(settledAtSeedCount);

  const pendingCount = allNodeIds.length - readyIds.size;
  const blockedIds = new Set<string>();
  for (const id of allNodeIds) {
    if (!doneIds.has(id) && !readyIds.has(id)) blockedIds.add(id);
  }

  return { pendingCount, blockedIds };
}

/**
 * Two-tier initial deadlock breaking for the unified processor.
 * Tier 1: relax scopeParent edges. Tier 2 (if Tier 1 found nothing): force all.
 * Returns the count of nodes newly readied.
 */
function breakInitialDeadlockUnified(
  allNodeIds: string[],
  doneIds: Set<string>,
  isNodeReadyIgnoringScopeParent: (id: string) => boolean,
  readyIds: Set<string>
): number {
  let count = 0;
  for (const id of allNodeIds) {
    if (!doneIds.has(id) && isNodeReadyIgnoringScopeParent(id)) {
      readyIds.add(id);
      count++;
    }
  }
  if (count > 0) {
    debug.log(
      "unified-processor",
      `Tier 1 deadlock break: relaxed scopeParent for ${count} nodes`
    );
    return count;
  }
  for (const id of allNodeIds) {
    if (!doneIds.has(id)) {
      readyIds.add(id);
      count++;
    }
  }
  debug.log(
    "unified-processor",
    `Tier 2 deadlock break: forced ${count} nodes ready`
  );
  return count;
}

/** Mark a node done and unblock any dependents that are now ready. */
function markDoneUnblockDependents(
  id: string,
  graph: UnifiedGraph,
  blockedIds: Set<string>,
  readyIds: Set<string>,
  readyAtMs: Map<string, number> | null,
  isNodeReady: (id: string) => boolean,
  metrics: import("../llm/metrics.js").MetricsTracker | undefined,
  decrementPending: (n: number) => void
): void {
  const deps = graph.dependents.get(id);
  if (!deps) return;
  const readyNow = readyAtMs ? performance.now() : 0;
  for (const depId of deps) {
    if (blockedIds.has(depId) && isNodeReady(depId)) {
      readyIds.add(depId);
      blockedIds.delete(depId);
      decrementPending(1);
      readyAtMs?.set(depId, readyNow);
      metrics?.functionsReady(1);
    }
  }
}

/**
 * Two-tier mid-loop deadlock breaking for the unified processor.
 * Returns { newlyReady, pendingReduction } to let caller update mutable pendingCount.
 */
function breakMidLoopDeadlockUnified(
  blockedIds: Set<string>,
  readyIds: Set<string>,
  isNodeReadyIgnoringScopeParent: (id: string) => boolean,
  doneIds: Set<string>,
  totalNodes: number,
  pendingCount: number
): { newlyReady: number; pendingReduction: number } {
  let newlyReady = 0;
  for (const id of blockedIds) {
    if (isNodeReadyIgnoringScopeParent(id)) {
      readyIds.add(id);
      newlyReady++;
    }
  }
  for (const id of readyIds) blockedIds.delete(id);
  if (newlyReady > 0) {
    debug.log(
      "unified-processor",
      `Tier 1 mid-loop: relaxed scopeParent for ${newlyReady} nodes`
    );
    debug.queueState({
      ready: readyIds.size,
      processing: 0,
      pending: pendingCount - newlyReady,
      done: doneIds.size,
      total: totalNodes,
      inFlightLLM: 0,
      event: "deadlock-break",
      detail: `tier=1-scopeParent unlocked=${newlyReady}`
    });
    return { newlyReady, pendingReduction: newlyReady };
  }
  let tier2Count = 0;
  for (const id of blockedIds) {
    readyIds.add(id);
    tier2Count++;
  }
  blockedIds.clear();
  if (tier2Count > 0) {
    debug.log(
      "unified-processor",
      `Tier 2 mid-loop: forced ${tier2Count} nodes ready`
    );
    debug.queueState({
      ready: readyIds.size,
      processing: 0,
      pending: pendingCount - tier2Count,
      done: doneIds.size,
      total: totalNodes,
      inFlightLLM: 0,
      event: "deadlock-break",
      detail: `tier=2-callee-cycle unlocked=${tier2Count}`
    });
  }
  return { newlyReady: tier2Count, pendingReduction: tier2Count };
}

/** Handle mid-loop deadlock: call breakMidLoopDeadlockUnified and update pending + metrics. */
function handleMidLoopDeadlock(
  blockedIds: Set<string>,
  readyIds: Set<string>,
  isNodeReadyIgnoringScopeParent: (id: string) => boolean,
  doneIds: Set<string>,
  totalNodes: number,
  pending: { count: number },
  metrics: import("../llm/metrics.js").MetricsTracker | undefined
): void {
  const { newlyReady, pendingReduction } = breakMidLoopDeadlockUnified(
    blockedIds,
    readyIds,
    isNodeReadyIgnoringScopeParent,
    doneIds,
    totalNodes,
    pending.count
  );
  pending.count -= pendingReduction;
  if (metrics && newlyReady > 0) metrics.functionsReady(newlyReady);
}

/** Callbacks interface used by runBatchRenameLoop and helpers. */
export interface BatchRenameCallbacks {
  buildRequest(
    remaining: string[],
    round: number,
    prev: Record<string, string>,
    failures: Failures
  ): BatchRenameRequest;
  applyRename(oldName: string, newName: string): void;
  getUsedNames(): Set<string>;
  functionId: string;
  onUnrenamed?(name: string): void;
  resolveRemaining?(
    remaining: Set<string>,
    prev: Record<string, string>,
    outcomes: Record<string, IdentifierOutcome>,
    totalLLMCalls: number
  ): void;
  /** Full scope-safety check — true when the rename must not be applied. */
  wouldReject?(oldName: string, newName: string): boolean;
  /** Optional: adjust LLM suggestions before validation (prior-name snap). */
  transformSuggestion?(oldName: string, suggestion: string): string;
}

/** Strategy object for the parts that differ between function and module callback builders. */
export interface RenameStrategy {
  /** Look up a binding's scope by name */
  getScope(name: string): import("@babel/traverse").Scope | undefined;
  /** Apply the actual AST rename */
  applyRename(oldName: string, newName: string): void;
  /** Build the LLM request */
  buildRequest(
    remaining: string[],
    round: number,
    prev: Record<string, string>,
    failures: Failures
  ): BatchRenameRequest;
  /** What set of names to check for collisions */
  getUsedNames(): Set<string>;
  /** ID for logging/metrics */
  functionId: string;
  /** Optional: record identity renames (function-only) */
  onUnrenamed?(name: string): void;
  /** Optional: adjust LLM suggestions before validation (prior-name snap). */
  transformSuggestion?(oldName: string, suggestion: string): string;
}

/**
 * Build BatchRenameCallbacks from a RenameStrategy.
 * Shared implementation of wouldReject and resolveRemaining — no divergence possible.
 */
export function buildCallbacks(
  strategy: RenameStrategy
): (laneId: string) => BatchRenameCallbacks {
  // Full scope-safety rejection (same-scope collision, outer-reference
  // capture, child-scope shadowing, free-name capture) — the same set the
  // transfer paths enforce. Checking only child-scope shadowing here let
  // an LLM suggestion capture an outer binding's references (the 2.1.166
  // transport bug).
  const wouldReject = (oldName: string, newName: string) => {
    const scope = strategy.getScope(oldName);
    if (!scope) return false;
    return getRenameRejection(scope, oldName, newName) !== null;
  };

  return (laneId: string) => ({
    buildRequest: strategy.buildRequest,
    applyRename: strategy.applyRename,
    getUsedNames: strategy.getUsedNames,
    functionId: `${strategy.functionId}${laneId}`,
    onUnrenamed: strategy.onUnrenamed,
    wouldReject,
    transformSuggestion: strategy.transformSuggestion,

    resolveRemaining: (
      remaining: Set<string>,
      prev: Record<string, string>,
      outcomes: Record<string, IdentifierOutcome>,
      totalLLMCalls: number
    ) => {
      resolveRemainingIdentifiers(
        remaining,
        prev,
        outcomes,
        totalLLMCalls,
        strategy.getUsedNames(),
        strategy.functionId,
        strategy.applyRename,
        wouldReject
      );
    }
  });
}

/**
 * Apply validated renames with an atomic check-and-claim guard.
 *
 * Why this guard is sufficient: JavaScript is single-threaded, so the race
 * between parallel lanes only occurs across `await` boundaries. This loop is
 * fully synchronous — no `await` between `getUsedNames().has(newName)` and
 * `applyRename()` (which calls `usedIdentifiers.add(newName)`). The check-and-add
 * executes atomically within a single microtask, preventing interleaving.
 */
export function applyValidRenames(
  validation: BatchValidationResult,
  callbacks: BatchRenameCallbacks,
  idState: Map<string, IdentifierAttemptState>,
  outcomes: Record<string, IdentifierOutcome>,
  callNum: number,
  isRetry: boolean
): { applied: number; lateCollisions: string[] } {
  let applied = 0;
  const lateCollisions: string[] = [];
  for (const [oldName, newName] of Object.entries(validation.valid)) {
    if (callbacks.getUsedNames().has(newName)) {
      lateCollisions.push(oldName);
      continue;
    }
    if (callbacks.wouldReject?.(oldName, newName)) {
      lateCollisions.push(oldName);
      continue;
    }
    debug.rename({
      functionId: callbacks.functionId,
      oldName,
      newName,
      wasRetry: isRetry,
      attemptNumber: (idState.get(oldName)?.attempts ?? 0) + 1
    });
    callbacks.applyRename(oldName, newName);
    const successState = idState.get(oldName);
    if (successState) recordAttempt(successState, newName, "applied");
    outcomes[oldName] = {
      status: "renamed",
      newName,
      round: callNum,
      trail: successState?.trail
    };
    applied++;
  }
  return { applied, lateCollisions };
}

/**
 * Classify failed identifiers after a batch call into nextRetry and exhausted lists.
 * Updates idState in place.
 */
/** Route a single failed identifier to nextRetry or exhausted. */
function classifySingleFailure(
  name: string,
  state: IdentifierAttemptState,
  isFreeRetry: boolean,
  dupSet: Set<string>,
  invSet: Set<string>,
  unchSet: Set<string>,
  maxRetriesPerIdentifier: number,
  nextRetry: string[],
  exhausted: string[]
): void {
  if (!isFreeRetry) {
    updateFailureState(name, state, dupSet, invSet, unchSet);
    if (state.attempts < maxRetriesPerIdentifier) {
      nextRetry.push(name);
    } else {
      exhausted.push(name);
    }
  } else if (state.freeRetries >= 2 && state.lastSuggestion) {
    // Fast collision resolution: after 2+ cross-lane collisions,
    // resolve algorithmically instead of doing another LLM call.
    // Push to exhausted so resolveRemaining handles it with suffix logic.
    exhausted.push(name);
  } else {
    nextRetry.push(name);
  }
}

function classifyFailedIdentifiers(
  batchRetries: string[],
  validation: BatchValidationResult,
  responseRenames: Record<string, string>,
  idState: Map<string, IdentifierAttemptState>,
  callbacks: BatchRenameCallbacks,
  usedNamesSnapshot: Set<string>,
  maxFreeRetries: number,
  maxRetriesPerIdentifier: number
): { nextRetry: string[]; exhausted: string[] } {
  const successes = new Set(Object.keys(validation.valid));
  const dupSet = new Set(validation.duplicates);
  const invSet = new Set(validation.invalid);
  const unchSet = new Set(validation.unchanged);
  const nextRetry: string[] = [];
  const exhausted: string[] = [];

  for (const name of batchRetries) {
    if (successes.has(name)) continue;
    const state = idState.get(name);
    if (!state) throw new Error(`Identifier state not found: ${name}`);
    if (responseRenames[name]) state.lastSuggestion = responseRenames[name];

    const isFreeRetry =
      dupSet.has(name) &&
      isFreeDuplicateRetry(
        name,
        responseRenames,
        callbacks,
        usedNamesSnapshot,
        state,
        maxFreeRetries
      );

    recordAttempt(
      state,
      responseRenames[name],
      failureResult(name, dupSet, invSet, unchSet)
    );

    classifySingleFailure(
      name,
      state,
      isFreeRetry,
      dupSet,
      invSet,
      unchSet,
      maxRetriesPerIdentifier,
      nextRetry,
      exhausted
    );
  }

  return { nextRetry, exhausted };
}

/**
 * Determine if a duplicate failure qualifies as a free (cross-lane) retry.
 * Side-effect: increments state.freeRetries when returning true.
 */
function isFreeDuplicateRetry(
  name: string,
  responseRenames: Record<string, string>,
  callbacks: BatchRenameCallbacks,
  usedNamesSnapshot: Set<string>,
  state: IdentifierAttemptState,
  maxFreeRetries: number
): boolean {
  const suggestedName = sanitizeIdentifier(responseRenames[name] || "");
  if (
    suggestedName &&
    callbacks.getUsedNames().has(suggestedName) &&
    !usedNamesSnapshot.has(suggestedName)
  ) {
    state.freeRetries++;
    return state.freeRetries < maxFreeRetries;
  }
  return false;
}

/** Update state failure reason and attempts count for a non-free-retry failure. */
function updateFailureState(
  name: string,
  state: IdentifierAttemptState,
  dupSet: Set<string>,
  invSet: Set<string>,
  unchSet: Set<string>
): void {
  if (dupSet.has(name)) {
    state.lastFailureReason = "duplicate";
    state.attempts++;
  } else if (invSet.has(name)) {
    state.lastFailureReason = "invalid";
    state.attempts++;
  } else if (unchSet.has(name)) {
    state.lastFailureReason = "unchanged";
    state.attempts++;
  } else {
    state.lastFailureReason = "missing";
    state.attempts++;
  }
}

/** Build final failures and previousAttempt from remaining identifiers. */
function buildFinalFailures(
  remaining: Set<string>,
  idState: Map<string, IdentifierAttemptState>
): { finalFailures: Failures; finalPreviousAttempt: Record<string, string> } {
  const finalFailures: Failures = {
    duplicates: [],
    invalid: [],
    missing: [],
    unchanged: []
  };
  const finalPreviousAttempt: Record<string, string> = {};
  for (const name of remaining) {
    const state = idState.get(name);
    if (state?.lastFailureReason === "duplicate")
      finalFailures.duplicates.push(name);
    else if (state?.lastFailureReason === "invalid")
      finalFailures.invalid.push(name);
    else if (state?.lastFailureReason === "unchanged")
      finalFailures.unchanged.push(name);
    else finalFailures.missing.push(name);
    if (state?.lastSuggestion)
      finalPreviousAttempt[name] = state.lastSuggestion;
  }
  return { finalFailures, finalPreviousAttempt };
}

/** Record outcome entries and debug logs for all unrenamed identifiers. */
function recordUnrenamedOutcomes(
  remaining: Set<string>,
  idState: Map<string, IdentifierAttemptState>,
  outcomes: Record<string, IdentifierOutcome>,
  finishReasons: (string | undefined)[],
  callbacks: BatchRenameCallbacks,
  lastUserPrompt: string,
  lastResponseRenames: Record<string, string>,
  lastValidation: BatchValidationResult | undefined,
  totalLLMCalls: number
): void {
  for (const name of remaining) {
    callbacks.onUnrenamed?.(name);
    const state = idState.get(name);
    if (!state) throw new Error(`Identifier state not found: ${name}`);
    const totalAttempts = state.attempts + (state.freeRetries > 0 ? 1 : 0);
    outcomes[name] = buildUnrenamedOutcome(state, totalAttempts, finishReasons);
    debugLogUnrenamed(
      name,
      state,
      outcomes[name],
      callbacks,
      lastUserPrompt,
      lastResponseRenames,
      lastValidation,
      totalLLMCalls
    );
  }
}

/** Build an IdentifierOutcome for an unrenamed identifier based on its failure reason. */
function buildUnrenamedOutcome(
  state: IdentifierAttemptState,
  totalAttempts: number,
  finishReasons: (string | undefined)[]
): IdentifierOutcome {
  const trail = state.trail;
  if (state.lastFailureReason === "duplicate") {
    return {
      status: "duplicate",
      conflictedWith: state.lastSuggestion || "unknown",
      attempts: totalAttempts,
      suggestion: state.lastSuggestion,
      trail
    };
  }
  if (state.lastFailureReason === "invalid") {
    return {
      status: "invalid",
      attempts: totalAttempts,
      suggestion: state.lastSuggestion,
      trail
    };
  }
  if (state.lastFailureReason === "unchanged") {
    return {
      status: "unchanged",
      attempts: totalAttempts,
      suggestion: state.lastSuggestion,
      trail
    };
  }
  return {
    status: "missing",
    attempts: totalAttempts,
    lastFinishReason: finishReasons[finishReasons.length - 1],
    trail
  };
}

/** Emit a renameFallback debug log for an unrenamed identifier. */
function debugLogUnrenamed(
  name: string,
  state: IdentifierAttemptState,
  outcome: IdentifierOutcome,
  callbacks: BatchRenameCallbacks,
  lastUserPrompt: string,
  lastResponseRenames: Record<string, string>,
  lastValidation: BatchValidationResult | undefined,
  totalLLMCalls: number
): void {
  const reason =
    outcome.status === "duplicate"
      ? `duplicate (collided with ${state.lastSuggestion || "unknown"})`
      : outcome.status === "invalid"
        ? "invalid identifier"
        : outcome.status === "unchanged"
          ? "LLM returned original name"
          : "not returned by LLM";

  const usedSample = [...callbacks.getUsedNames()].slice(0, 50);
  const contextParts = [
    `lastPrompt(${lastUserPrompt.length}chars): ${lastUserPrompt.slice(0, 300)}`,
    `lastResponse: ${JSON.stringify(lastResponseRenames)}`,
    lastValidation
      ? `validation: valid=${Object.keys(lastValidation.valid).length} dup=${lastValidation.duplicates.length} inv=${lastValidation.invalid.length} miss=${lastValidation.missing.length} unch=${lastValidation.unchanged.length}`
      : "",
    `usedNames(${callbacks.getUsedNames().size} total, sample): ${usedSample.join(", ")}`
  ]
    .filter(Boolean)
    .join("\n");

  debug.renameFallback({
    functionId: callbacks.functionId,
    identifier: name,
    suggestedName: state.lastSuggestion,
    rejectionReason: reason,
    fallbackResult: name,
    context: contextParts,
    round: totalLLMCalls
  });
}

/**
 * Result of validating batch rename suggestions.
 */
export interface BatchValidationResult {
  /** Valid mappings that can be applied */
  valid: Record<string, string>;
  /** Identifiers whose suggested names were duplicated */
  duplicates: string[];
  /** Identifiers whose suggested names were invalid */
  invalid: string[];
  /** Identifiers that weren't in the response */
  missing: string[];
  /** Identifiers where LLM returned the original name */
  unchanged: string[];
}

/**
 * Validates batch rename suggestions from the LLM.
 *
 * Checks for:
 * - Identifiers that don't exist in the expected set
 * - Names that are the same as the original
 * - Invalid identifier syntax
 * - Duplicate names within the batch
 * - Conflicts with already-used names
 */
function validateBatchRenames(
  renames: Record<string, string>,
  expected: Set<string>,
  usedNames: Set<string>
): BatchValidationResult {
  const valid: Record<string, string> = {};
  const duplicates: string[] = [];
  const invalid: string[] = [];
  const unchanged: string[] = [];
  const seenNewNames = new Set<string>();

  for (const [oldName, newName] of Object.entries(renames)) {
    if (!expected.has(oldName)) continue;
    // Classify the RAW model name. An invalid/reserved/builtin name lands in
    // the `invalid` bucket, which the retry machinery turns into a follow-up
    // LLM call with feedback — instead of being silently sanitized and applied.
    classifyRenameEntry(
      oldName,
      newName,
      valid,
      duplicates,
      invalid,
      unchanged,
      seenNewNames,
      usedNames
    );
  }

  const missing = findMissingIdentifiers(
    expected,
    valid,
    duplicates,
    invalid,
    unchanged
  );
  return { valid, duplicates, invalid, missing, unchanged };
}

/** Classify a single rename entry into the appropriate result bucket. */
function classifyRenameEntry(
  oldName: string,
  newName: string,
  valid: Record<string, string>,
  duplicates: string[],
  invalid: string[],
  unchanged: string[],
  seenNewNames: Set<string>,
  usedNames: Set<string>
): void {
  if (oldName === newName) {
    unchanged.push(oldName);
    return;
  }
  if (!isValidRenameTarget(newName)) {
    invalid.push(oldName);
    return;
  }
  if (seenNewNames.has(newName)) {
    evictDuplicateEntry(newName, valid, duplicates);
    duplicates.push(oldName);
    return;
  }
  if (usedNames.has(newName)) {
    duplicates.push(oldName);
    return;
  }
  valid[oldName] = newName;
  seenNewNames.add(newName);
}

/** Remove the first valid entry with the given new name, moving it to duplicates. */
function evictDuplicateEntry(
  newName: string,
  valid: Record<string, string>,
  duplicates: string[]
): void {
  for (const [k, v] of Object.entries(valid)) {
    if (v === newName) {
      delete valid[k];
      duplicates.push(k);
      break;
    }
  }
}

/** Find identifiers from expected set not present in any result bucket. */
function findMissingIdentifiers(
  expected: Set<string>,
  valid: Record<string, string>,
  duplicates: string[],
  invalid: string[],
  unchanged: string[]
): string[] {
  const dupSet = new Set(duplicates);
  const invSet = new Set(invalid);
  const unchSet = new Set(unchanged);
  return [...expected].filter(
    (name) =>
      !valid[name] &&
      !dupSet.has(name) &&
      !invSet.has(name) &&
      !unchSet.has(name)
  );
}

/**
 * Groups module bindings by proximity (declaration line distance).
 * Bindings within ±radius lines of the group's first member form a group.
 */
/**
 * Names already taken at module level: the target scope's own bindings
 * plus the file's free names, so LLM suggestions can't shadow either.
 * Free names live on the PROGRAM scope's globals — the target scope is a
 * wrapper IIFE scope in Bun bundles, whose own `.globals` is always
 * empty (review C1).
 */
export function collectModuleUsedNames(targetScope: {
  bindings: Record<string, unknown>;
  getProgramParent: () => { globals?: Record<string, unknown> };
}): Set<string> {
  const usedNames = new Set<string>(Object.keys(targetScope.bindings));
  for (const name of Object.keys(
    targetScope.getProgramParent().globals ?? {}
  )) {
    usedNames.add(name);
  }
  return usedNames;
}

function groupByProximity(
  bindings: ModuleBindingNode[],
  radius = 50,
  maxSize = 10
): ModuleBindingNode[][] {
  if (bindings.length === 0) return [];

  const sorted = [...bindings].sort(
    (a, b) => a.declarationLine - b.declarationLine
  );
  const groups: ModuleBindingNode[][] = [];
  let current: ModuleBindingNode[] = [];

  for (const mb of sorted) {
    if (current.length === 0) {
      current.push(mb);
    } else if (
      mb.declarationLine - current[0].declarationLine <= radius * 2 &&
      current.length < maxSize
    ) {
      current.push(mb);
    } else {
      groups.push(current);
      current = [mb];
    }
  }
  if (current.length > 0) groups.push(current);
  return groups;
}

/**
 * Splits identifier names into N lanes by contiguous chunks.
 * Preserves locality within each lane for better proximity windowing.
 */
function splitByPosition(identifiers: string[], numLanes: number): string[][] {
  const chunkSize = Math.ceil(identifiers.length / numLanes);
  const lanes: string[][] = [];
  for (let i = 0; i < identifiers.length; i += chunkSize) {
    lanes.push(identifiers.slice(i, i + chunkSize));
  }
  return lanes;
}

/**
 * Builds previousAttempt and failures from per-identifier state tracking.
 * Used by both the main retry loop and straggler pass.
 */
function buildPrevAndFailures(
  batch: string[],
  idState: Map<string, IdentifierAttemptState>
): { prev: Record<string, string>; failures: Failures } {
  const prev: Record<string, string> = {};
  const failures: Failures = {
    duplicates: [],
    invalid: [],
    missing: [],
    unchanged: []
  };
  for (const name of batch) {
    const state = idState.get(name);
    if (!state) throw new Error(`Identifier state not found: ${name}`);
    if (state.lastSuggestion) prev[name] = state.lastSuggestion;
    if (state.lastFailureReason === "duplicate") failures.duplicates.push(name);
    else if (state.lastFailureReason === "invalid") failures.invalid.push(name);
    else if (state.lastFailureReason === "missing") failures.missing.push(name);
    else if (state.lastFailureReason === "unchanged")
      failures.unchanged.push(name);
  }
  return { prev, failures };
}

/**
 * Shared fallback resolution for remaining identifiers after the batch loop.
 * Applies valid LLM suggestions directly or resolves collisions via suffix.
 */
/** Apply a resolved rename and record the outcome. */
function applyResolvedRename(
  name: string,
  newName: string,
  remaining: Set<string>,
  outcomes: Record<string, IdentifierOutcome>,
  round: number,
  applyRename: (oldName: string, newName: string) => void
): void {
  applyRename(name, newName);
  remaining.delete(name);
  outcomes[name] = { status: "renamed", newName, round };
}

interface RemainingResolutionContext {
  remaining: Set<string>;
  outcomes: Record<string, IdentifierOutcome>;
  totalLLMCalls: number;
  usedNames: Set<string>;
  functionId: string;
  applyRename: (oldName: string, newName: string) => void;
  wouldReject?: (oldName: string, newName: string) => boolean;
}

function resolveRemainingIdentifiers(
  remaining: Set<string>,
  prev: Record<string, string>,
  outcomes: Record<string, IdentifierOutcome>,
  totalLLMCalls: number,
  usedNames: Set<string>,
  functionId: string,
  applyRename: (oldName: string, newName: string) => void,
  wouldReject?: (oldName: string, newName: string) => boolean
): void {
  const ctx: RemainingResolutionContext = {
    remaining,
    outcomes,
    totalLLMCalls,
    usedNames,
    functionId,
    applyRename,
    wouldReject
  };
  for (const name of [...remaining]) {
    const suggestedName = prev[name];
    if (!suggestedName) continue;

    // Terminal safety: after retries are exhausted, only a valid suggestion is
    // applied. An invalid/reserved/builtin name (or the unchanged original) is
    // left unrenamed — the minified name stays, which is honest and precise —
    // rather than being silently sanitized into a legal identifier. This
    // mirrors how unchanged/missing exhaustion is already handled; only a valid
    // name that merely collides is repaired algorithmically below.
    if (!isValidRenameTarget(suggestedName) || suggestedName === name) continue;
    resolveOneRemaining(name, suggestedName, ctx);
  }
}

/**
 * Apply one exhausted identifier's suggestion. A suggestion that collides
 * with a used name OR is scope-unsafe (capture, merge, child shadow) gets
 * the same algorithmic repair: a suffixed variant, which must itself pass
 * the scope check.
 */
function resolveOneRemaining(
  name: string,
  suggestedName: string,
  ctx: RemainingResolutionContext
): void {
  const round = ctx.totalLLMCalls + 1;
  const scopeRejected = ctx.wouldReject?.(name, suggestedName) ?? false;
  if (!ctx.usedNames.has(suggestedName) && !scopeRejected) {
    applyResolvedRename(
      name,
      suggestedName,
      ctx.remaining,
      ctx.outcomes,
      round,
      ctx.applyRename
    );
    return;
  }

  const resolved = resolveConflict(suggestedName, ctx.usedNames);
  if (ctx.wouldReject?.(name, resolved)) return;
  debug.renameFallback({
    functionId: ctx.functionId,
    identifier: name,
    suggestedName,
    rejectionReason: scopeRejected
      ? `scope-unsafe suggestion "${suggestedName}"`
      : `collision with existing name "${suggestedName}"`,
    fallbackResult: resolved,
    round: ctx.totalLLMCalls
  });
  applyResolvedRename(
    name,
    resolved,
    ctx.remaining,
    ctx.outcomes,
    round,
    ctx.applyRename
  );
}
