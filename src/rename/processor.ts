import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import { performance } from "perf_hooks";
import type {
  FunctionNode,
  FunctionRenameReport,
  IdentifierOutcome,
  LLMContext,
  ModuleBindingNode,
  ProcessingProgress,
  ProcessorOptions,
  RenameDecision,
  UnifiedGraph
} from "../analysis/types.js";
import { generate } from "../babel-utils.js";
import { debug } from "../debug.js";
import {
  buildModuleLevelRenamePrompt,
  buildModuleLevelRetryPrefix,
  MODULE_LEVEL_RENAME_SYSTEM_PROMPT
} from "../llm/prompts.js";
import type { BatchRenameRequest, LLMProvider } from "../llm/types.js";
import {
  isValidIdentifier,
  RESERVED_WORDS,
  resolveConflict,
  sanitizeIdentifier
} from "../llm/validation.js";
import { getProximateUsedNames } from "../plugins/rename.js";
import { NULL_PROFILER } from "../profiling/profiler.js";
import { TRACE_TID } from "../profiling/types.js";
import { createConcurrencyLimiter } from "../utils/concurrency.js";
import { buildContext } from "./context-builder.js";
import type { LooksMinifiedFn } from "./minified-heuristic.js";
import { looksMinified as defaultLooksMinified } from "./minified-heuristic.js";

/** Failure categories from batch validation */
type Failures = {
  duplicates: string[];
  invalid: string[];
  missing: string[];
  unchanged: string[];
};

/** Per-identifier tracking for the batch-until-done loop */
interface IdentifierAttemptState {
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

/** Maximum number of retry attempts when LLM suggests a conflicting name */
const MAX_NAME_RETRIES = 9;

/** Maximum identifiers per LLM batch (adaptive — halved on truncation) */
const DEFAULT_BATCH_SIZE = 10;

/** Per-identifier retry cap for real failures */
const DEFAULT_MAX_RETRIES_PER_ID = 3;

/** Cap on "free" retries from cross-lane collisions */
const DEFAULT_MAX_FREE_RETRIES = 100;

/** Minimum number of bindings to enable parallel lanes */
const DEFAULT_LANE_THRESHOLD = 25;

/** Number of parallel lanes for large functions */
const NUM_LANES = 4;

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
  private ready = new Set<FunctionNode>();
  private processing = new Set<FunctionNode>();
  private done = new Set<FunctionNode>();
  private allRenames: RenameDecision[] = [];
  private ast: t.File;
  private metrics?: import("../llm/metrics.js").MetricsTracker;
  private _reports: FunctionRenameReport[] = [];
  private failedCount = 0;
  private paramOnly = false;
  private _skippedByHeuristic = 0;
  private options: ProcessorOptions = {};
  private isMinified: LooksMinifiedFn = defaultLooksMinified;

  /** Per-function rename reports (populated after processAll completes) */
  get reports(): ReadonlyArray<FunctionRenameReport> {
    return this._reports;
  }

  /** Number of functions that failed due to LLM errors (populated after processAll completes) */
  get failed(): number {
    return this.failedCount;
  }

  /** Number of identifiers skipped by looksMinified heuristic */
  get skippedByHeuristic(): number {
    return this._skippedByHeuristic;
  }

  constructor(ast: t.File) {
    this.ast = ast;
  }

  /**
   * Process all functions and return rename decisions for source map generation.
   */
  async processAll(
    functions: FunctionNode[],
    llm: LLMProvider,
    options: ProcessorOptions = {}
  ): Promise<RenameDecision[]> {
    const {
      concurrency = 50,
      onProgress,
      metrics,
      preDone,
      paramOnly,
      profiler: optProfiler
    } = options;
    const profiler = optProfiler ?? NULL_PROFILER;

    this.options = options;
    this.metrics = metrics;
    this.paramOnly = paramOnly ?? false;
    this.isMinified = options.looksMinified ?? defaultLooksMinified;

    if (preDone) {
      for (const fn of preDone) this.done.add(fn);
    }
    if (metrics) metrics.setFunctionTotal(functions.length);

    const initialReady = this.initializeReadySet(functions);
    if (metrics && initialReady > 0) metrics.functionsReady(initialReady);
    this.reportProgress(functions, onProgress);

    await this.runProcessAllLoop(
      functions,
      llm,
      profiler,
      metrics,
      onProgress,
      concurrency
    );

    for (const fn of functions) {
      if (fn.renameReport) this._reports.push(fn.renameReport);
    }
    metrics?.emit();
    return this.allRenames;
  }

  /** Run the main dispatch loop for processAll. */
  private async runProcessAllLoop(
    functions: FunctionNode[],
    llm: LLMProvider,
    profiler: import("../profiling/profiler.js").Profiler,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    onProgress:
      | ((p: import("../analysis/types.js").ProcessingProgress) => void)
      | undefined,
    concurrency: number
  ): Promise<void> {
    const dependents = buildDependentsMap(functions);
    const limit = createConcurrencyLimiter(concurrency);
    const inFlight = { count: 0 };
    const signals = {
      notifyCompletion: null as (() => void) | null,
      drainResolve: null as (() => void) | null
    };

    const pendingCount = () =>
      functions.filter(
        (f) =>
          !this.done.has(f) && !this.processing.has(f) && !this.ready.has(f)
      ).length;

    const readyAtMs = initReadyAtMs(profiler, this.ready);
    profiler.startConcurrencySampling(() => ({
      inFlight: inFlight.count,
      ready: this.ready.size,
      blocked: pendingCount()
    }));

    const signalDone = () => {
      if (signals.notifyCompletion) {
        const cb = signals.notifyCompletion;
        signals.notifyCompletion = null;
        cb();
      }
    };
    const decrement = () => {
      inFlight.count--;
      if (inFlight.count === 0 && signals.drainResolve) {
        const cb = signals.drainResolve;
        signals.drainResolve = null;
        cb();
      }
    };

    while (this.ready.size > 0 || this.processing.size > 0) {
      const dispatched = this.dispatchAllReady(
        llm,
        limit,
        profiler,
        dependents,
        metrics,
        readyAtMs,
        functions,
        pendingCount,
        onProgress,
        signalDone,
        decrement,
        inFlight
      );

      if (dispatched > 0) {
        debug.queueState({
          ready: this.ready.size,
          processing: this.processing.size,
          pending: pendingCount(),
          done: this.done.size,
          total: functions.length,
          inFlightLLM: inFlight.count,
          event: "dispatch",
          detail: `dispatched=${dispatched}`
        });
      }

      if (this.ready.size === 0 && this.processing.size > 0) {
        debug.queueState({
          ready: 0,
          processing: this.processing.size,
          pending: pendingCount(),
          done: this.done.size,
          total: functions.length,
          inFlightLLM: inFlight.count,
          event: "waiting-on-llm"
        });
        await new Promise<void>((resolve) => {
          signals.notifyCompletion = resolve;
        });
      }

      if (this.ready.size === 0 && this.processing.size === 0) {
        const newlyReady = this.breakDeadlocksAll(functions, pendingCount);
        if (metrics && newlyReady > 0) metrics.functionsReady(newlyReady);
      }
    }

    if (inFlight.count > 0) {
      await new Promise<void>((resolve) => {
        signals.drainResolve = resolve;
      });
    }
    profiler.stopConcurrencySampling();
  }

  /** Dispatch all currently-ready functions to the concurrency limiter. Returns count dispatched. */
  private dispatchAllReady(
    llm: LLMProvider,
    limit: ReturnType<typeof createConcurrencyLimiter>,
    profiler: import("../profiling/profiler.js").Profiler,
    dependents: Map<FunctionNode, FunctionNode[]>,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    readyAtMs: Map<FunctionNode, number> | null,
    functions: FunctionNode[],
    pendingCount: () => number,
    onProgress:
      | ((p: import("../analysis/types.js").ProcessingProgress) => void)
      | undefined,
    signalDone: () => void,
    decrement: () => void,
    inFlight: { count: number }
  ): number {
    const dispatching = [...this.ready];
    for (const fn of dispatching) {
      this.ready.delete(fn);
      this.processing.add(fn);
      fn.status = "processing";
      metrics?.functionStarted();
      inFlight.count++;

      const waitMs = readyAtMs?.has(fn)
        ? performance.now() - readyAtMs.get(fn)!
        : 0;
      readyAtMs?.delete(fn);
      const fnSpan = profiler.startSpan(
        `fn:${fn.sessionId}`,
        "rename",
        TRACE_TID.RENAME_FUNCTION,
        { waitMs }
      );

      limit(() =>
        this.runFunctionTask(
          fn,
          llm,
          fnSpan,
          dependents,
          metrics,
          readyAtMs,
          functions,
          pendingCount,
          onProgress,
          signalDone,
          decrement
        )
      );
    }
    return dispatching.length;
  }

  /** Initialize the ready set, running deadlock detection if nothing is initially ready. Returns count. */
  private initializeReadySet(functions: FunctionNode[]): number {
    let initialReady = 0;
    for (const fn of functions) {
      if (this.isReady(fn)) {
        this.ready.add(fn);
        initialReady++;
      }
    }
    debug.log(
      "processor",
      `Initial state: ${initialReady} ready, ${functions.length} total, ${this.done.size} pre-done`
    );

    if (initialReady === 0 && functions.length > 0) {
      this.logDeadlockStats(functions);
      debug.log(
        "processor",
        "Deadlock detected — relaxing scopeParent dependencies"
      );
      for (const fn of functions) {
        if (this.isReadyIgnoringScopeParent(fn)) {
          this.ready.add(fn);
          initialReady++;
        }
      }
      debug.log("processor", `After relaxing: ${initialReady} ready`);
    }
    return initialReady;
  }

  /** Log diagnostic breakdown of what is blocking functions. */
  private logDeadlockStats(functions: FunctionNode[]): void {
    let blockedByCallees = 0;
    let blockedByScopeParent = 0;
    let blockedByBoth = 0;
    for (const fn of functions) {
      let calleesBlocking = false;
      for (const c of fn.internalCallees) {
        if (!this.done.has(c)) {
          calleesBlocking = true;
          break;
        }
      }
      const parentBlocking = fn.scopeParent
        ? !this.done.has(fn.scopeParent)
        : false;
      if (calleesBlocking && parentBlocking) blockedByBoth++;
      else if (calleesBlocking) blockedByCallees++;
      else if (parentBlocking) blockedByScopeParent++;
    }
    debug.log(
      "processor",
      `Blocked by: callees=${blockedByCallees}, scopeParent=${blockedByScopeParent}, both=${blockedByBoth}`
    );
  }

  /** Run one function task async (used by the limit() call in processAll). */
  private async runFunctionTask(
    fn: FunctionNode,
    llm: LLMProvider,
    fnSpan: ReturnType<typeof NULL_PROFILER.startSpan>,
    dependents: Map<FunctionNode, FunctionNode[]>,
    metrics: import("../llm/metrics.js").MetricsTracker | undefined,
    readyAtMs: Map<FunctionNode, number> | null,
    functions: FunctionNode[],
    pendingCount: () => number,
    onProgress:
      | ((p: import("../analysis/types.js").ProcessingProgress) => void)
      | undefined,
    signalCompletion: () => void,
    decrementInflight: () => void
  ): Promise<void> {
    try {
      await this.processFunction(fn, llm);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      debug.log("processor", `Function ${fn.sessionId} failed: ${msg}`);
      this.failedCount++;
      if (!fn.renameMapping) fn.renameMapping = { names: {} };
    } finally {
      fnSpan.end({ outcome: this.failedCount > 0 ? "error" : "ok" });
      this.processing.delete(fn);
      this.done.add(fn);
      fn.status = "done";
      metrics?.functionCompleted();

      const newlyReady = this.checkNewlyReady(fn, dependents);
      if (newlyReady > 0) {
        updateReadyTimestamps(this.ready, readyAtMs);
        if (metrics) metrics.functionsReady(newlyReady);
      }

      debug.queueState({
        ready: this.ready.size,
        processing: this.processing.size,
        pending: pendingCount(),
        done: this.done.size,
        total: functions.length,
        inFlightLLM: -1,
        event: "completion",
        detail: `completed=${fn.sessionId} unlocked=${newlyReady}`
      });

      this.reportProgress(functions, onProgress);
      signalCompletion();
      decrementInflight();
    }
  }

  /** Break deadlocks mid-loop in processAll. Returns count of newly readied functions. */
  private breakDeadlocksAll(
    functions: FunctionNode[],
    pendingCount: () => number
  ): number {
    let newlyReady = this.checkNewlyReadyRelaxed(functions);
    if (newlyReady > 0) {
      debug.log(
        "processor",
        `Breaking scopeParent deadlock: ${newlyReady} functions unblocked`
      );
      debug.queueState({
        ready: this.ready.size,
        processing: 0,
        pending: pendingCount(),
        done: this.done.size,
        total: functions.length,
        inFlightLLM: 0,
        event: "deadlock-break",
        detail: `tier=1-scopeParent unlocked=${newlyReady}`
      });
    } else {
      newlyReady = this.forceBreakAllDeadlocks(functions);
      if (newlyReady > 0) {
        debug.log(
          "processor",
          `Force-breaking callee deadlock: ${newlyReady} functions unblocked`
        );
        debug.queueState({
          ready: this.ready.size,
          processing: 0,
          pending: pendingCount(),
          done: this.done.size,
          total: functions.length,
          inFlightLLM: 0,
          event: "deadlock-break",
          detail: `tier=2-callee-cycle unlocked=${newlyReady}`
        });
      }
    }
    return newlyReady;
  }

  /**
   * Check if a function is ready to be processed (all dependencies done).
   */
  private isReady(fn: FunctionNode): boolean {
    for (const callee of fn.internalCallees) {
      if (!this.done.has(callee)) {
        return false;
      }
    }
    // Also wait for scope parent (needed for proper variable renaming order)
    if (fn.scopeParent && !this.done.has(fn.scopeParent)) {
      return false;
    }
    return true;
  }

  /**
   * Check if a function is ready ignoring scopeParent — used for deadlock breaking.
   * In large single-file bundles, scopeParent chains create circular dependencies
   * with internalCallees that prevent any function from becoming ready.
   */
  private isReadyIgnoringScopeParent(fn: FunctionNode): boolean {
    for (const callee of fn.internalCallees) {
      if (!this.done.has(callee)) {
        return false;
      }
    }
    return true;
  }

  /**
   * Check for functions that are newly ready after a specific function completes.
   * Only checks direct dependents of the completed function (via reverse-dep map).
   * Returns the count of newly ready functions.
   */
  private checkNewlyReady(
    completedFn: FunctionNode,
    dependents: Map<FunctionNode, FunctionNode[]>
  ): number {
    const deps = dependents.get(completedFn);
    if (!deps) return 0;

    let count = 0;
    for (const fn of deps) {
      if (
        !this.done.has(fn) &&
        !this.processing.has(fn) &&
        !this.ready.has(fn)
      ) {
        if (this.isReady(fn)) {
          this.ready.add(fn);
          count++;
        }
      }
    }
    return count;
  }

  /**
   * Like checkNewlyReady but ignores scopeParent dependencies.
   * Used to break deadlocks in large single-file bundles.
   */
  private checkNewlyReadyRelaxed(allFunctions: FunctionNode[]): number {
    let count = 0;
    for (const fn of allFunctions) {
      if (
        !this.done.has(fn) &&
        !this.processing.has(fn) &&
        !this.ready.has(fn)
      ) {
        if (this.isReadyIgnoringScopeParent(fn)) {
          this.ready.add(fn);
          count++;
        }
      }
    }
    return count;
  }

  /**
   * Force all remaining unprocessed functions into the ready queue.
   * Used as a last resort when callee cycles prevent any function from becoming ready.
   * These functions get slightly worse LLM context (missing callee signatures for
   * unprocessed callees) but will be processed rather than abandoned.
   */
  private forceBreakAllDeadlocks(allFunctions: FunctionNode[]): number {
    let count = 0;
    for (const fn of allFunctions) {
      if (
        !this.done.has(fn) &&
        !this.processing.has(fn) &&
        !this.ready.has(fn)
      ) {
        this.ready.add(fn);
        count++;
      }
    }
    return count;
  }

  /**
   * Process a single function: get LLM suggestions and apply renames.
   * Uses batch renaming when available for better semantic understanding.
   */
  private async processFunction(
    fn: FunctionNode,
    llm: LLMProvider
  ): Promise<void> {
    const allBindings = this.paramOnly
      ? getParamBindings(fn.path)
      : getOwnBindings(fn.path);

    // If no bindings to rename, skip
    if (allBindings.length === 0) {
      fn.renameMapping = { names: {} };
      return;
    }

    // Filter out identifiers that already have descriptive names
    const bindings = allBindings.filter((b) => this.isMinified(b.name));
    this._skippedByHeuristic += allBindings.length - bindings.length;

    if (bindings.length === 0) {
      fn.renameMapping = { names: {} };
      return;
    }

    // Use batch renaming if available
    if (llm.suggestAllNames) {
      await this.processFunctionBatched(fn, llm, bindings);
    } else {
      await this.processFunctionSequential(fn, llm, bindings);
    }
  }

  /**
   * Process a function using batch renaming - asks LLM for all names at once.
   *
   * Uses progressive rename: after each LLM round, valid renames are applied
   * to the AST immediately and code is regenerated. Remaining identifiers are
   * re-sent with the updated code context, giving the LLM better signal.
   *
   * For large functions (>laneThreshold bindings), splits identifiers into
   * parallel lanes that share the AST and usedIdentifiers set.
   */
  private async processFunctionBatched(
    fn: FunctionNode,
    llm: LLMProvider,
    bindings: BindingInfo[]
  ): Promise<void> {
    const context = buildContext(fn, this.ast, this.isMinified);
    const renameMapping: Record<string, string> = {};
    const bindingMap = new Map(bindings.map((b) => [b.name, b]));

    const laneThreshold = this.options.laneThreshold ?? DEFAULT_LANE_THRESHOLD;

    // Remove minified names we're about to rename from usedIdentifiers —
    // they'll be replaced, so the LLM shouldn't avoid them and conflict
    // detection shouldn't reject new names that happen to match them.
    for (const b of bindings) {
      context.usedIdentifiers.delete(b.name);
    }

    // Shared callbacks for all lanes
    const makeCallbacks = (laneId: string) => {
      // Cache proximity-windowed usedNames: same identifiers AND same usedIdentifiers size → same window.
      // usedIdentifiers only grows (via add()), so size change means another lane renamed something.
      let cachedWindowKey: string | undefined;
      let cachedUsedSize: number | undefined;
      let cachedWindowedNames: Set<string> | undefined;

      return {
        buildRequest: (
          remaining: string[],
          round: number,
          prev: Record<string, string>,
          failures: Failures
        ) => {
          const code = truncateFunctionCode(
            generate(fn.path.node).code,
            fn.sessionId
          );

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
              this.isMinified
            );
            cachedWindowKey = windowKey;
            cachedUsedSize = currentUsedSize;
            cachedWindowedNames = windowedUsedNames;
          }

          return {
            code,
            identifiers: remaining,
            usedNames: windowedUsedNames,
            calleeSignatures: context.calleeSignatures,
            callsites: context.callsites,
            contextVars: context.contextVars,
            isRetry: round > 1,
            previousAttempt: round > 1 ? prev : undefined,
            failures: round > 1 ? failures : undefined
          };
        },
        applyRename: (oldName: string, newName: string) => {
          const binding = bindingMap.get(oldName);
          if (binding) {
            this.applyFunctionRename(
              binding,
              oldName,
              newName,
              fn.sessionId,
              context.usedIdentifiers,
              renameMapping
            );
          }
        },
        getUsedNames: () => context.usedIdentifiers,
        functionId: `${fn.sessionId}${laneId}`,
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
            context.usedIdentifiers,
            fn.sessionId,
            (name, newName) => {
              const binding = bindingMap.get(name);
              if (binding) {
                this.applyFunctionRename(
                  binding,
                  name,
                  newName,
                  fn.sessionId,
                  context.usedIdentifiers,
                  renameMapping
                );
              }
            }
          );
        },
        onUnrenamed: (name: string) => {
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
      };
    };

    // Decide whether to use parallel lanes
    let allOutcomes: Record<string, IdentifierOutcome> = {};
    let allFinishReasons: (string | undefined)[] = [];
    let totalLLMCalls = 0;
    let totalRemaining = new Set<string>();

    if (bindings.length > laneThreshold) {
      // Split bindings into lanes by position
      const lanes = splitByPosition(
        bindings.map((b) => b.name),
        NUM_LANES
      );
      debug.log(
        "processor",
        `${fn.sessionId}: splitting ${bindings.length} bindings into ${lanes.length} lanes`
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
        bindings.map((b) => b.name),
        makeCallbacks("")
      );
      allOutcomes = result.outcomes;
      allFinishReasons = result.finishReasons;
      totalLLMCalls = result.totalLLMCalls;
      totalRemaining = result.remaining;
    }

    fn.renameMapping = { names: renameMapping };
    fn.renameReport = {
      functionId: fn.sessionId,
      totalIdentifiers: bindings.length,
      renamedCount: bindings.length - totalRemaining.size,
      outcomes: allOutcomes,
      totalLLMCalls,
      finishReasons: allFinishReasons
    };
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
    renameMapping: Record<string, string>
  ): void {
    const loc = binding.identifier.loc;
    if (loc) {
      this.allRenames.push({
        originalPosition: { line: loc.start.line, column: loc.start.column },
        originalName: oldName,
        newName,
        functionId
      });
    }
    binding.scope.rename(oldName, newName);
    usedIdentifiers.add(newName);
    renameMapping[oldName] = newName;
  }

  /**
   * Process a function sequentially - one identifier at a time (fallback).
   */
  private async processFunctionSequential(
    fn: FunctionNode,
    llm: LLMProvider,
    bindings: BindingInfo[]
  ): Promise<void> {
    const context = buildContext(fn, this.ast, this.isMinified);
    const renameMapping: Record<string, string> = {};

    for (const binding of bindings) {
      const newName = await this.suggestNameWithRetry(
        binding.name,
        context,
        llm,
        this.metrics
      );

      // Track for source map BEFORE renaming
      const loc = binding.identifier.loc;
      if (loc) {
        this.allRenames.push({
          originalPosition: { line: loc.start.line, column: loc.start.column },
          originalName: binding.name,
          newName,
          functionId: fn.sessionId
        });
      }

      // Apply rename to AST — use binding's own scope for block-scoped vars
      binding.scope.rename(binding.name, newName);
      context.usedIdentifiers.add(newName);
      renameMapping[binding.name] = newName;
    }

    fn.renameMapping = { names: renameMapping };
  }

  /**
   * Get a name suggestion from the LLM, retrying if the suggestion conflicts.
   * Falls back to algorithmic resolution after MAX_NAME_RETRIES.
   */
  private async suggestNameWithRetry(
    currentName: string,
    context: LLMContext,
    llm: LLMProvider,
    metrics?: import("../llm/metrics.js").MetricsTracker
  ): Promise<string> {
    const done = metrics?.llmCallStart();
    let suggestion = await llm.suggestName(currentName, context);
    done?.();

    let newName = sanitizeIdentifier(suggestion.name);
    let attempts = 0;

    // Retry loop: ask LLM for alternatives when name conflicts
    while (attempts < MAX_NAME_RETRIES) {
      const rejection = this.getRejectionReason(
        newName,
        context.usedIdentifiers
      );

      if (!rejection) {
        // Name is valid and available
        return newName;
      }

      attempts++;

      const retryDone = metrics?.llmCallStart();
      // Try to get a new suggestion via retry
      if (llm.retrySuggestName) {
        suggestion = await llm.retrySuggestName(
          currentName,
          newName,
          rejection,
          context
        );
      } else {
        // Fallback for providers without retry support:
        // Re-call suggestName with the rejected name added to used set
        const updatedContext = {
          ...context,
          usedIdentifiers: new Set([...context.usedIdentifiers, newName])
        };
        suggestion = await llm.suggestName(currentName, updatedContext);
      }
      retryDone?.();

      newName = sanitizeIdentifier(suggestion.name);
    }

    // Final fallback: algorithmic resolution after exhausting retries
    const rejection = this.getRejectionReason(newName, context.usedIdentifiers);
    if (rejection) {
      const beforeResolve = newName;
      newName = resolveConflict(newName, context.usedIdentifiers);
      debug.renameFallback({
        functionId: currentName,
        identifier: currentName,
        suggestedName: beforeResolve,
        rejectionReason: rejection,
        fallbackResult: newName
      });
    }

    return newName;
  }

  /**
   * Check if a name should be rejected, returning the reason or null if valid.
   */
  private getRejectionReason(
    name: string,
    usedIdentifiers: Set<string>
  ): string | null {
    if (usedIdentifiers.has(name)) {
      return `"${name}" is already in use in this scope`;
    }
    if (RESERVED_WORDS.has(name)) {
      return `"${name}" is a JavaScript reserved word`;
    }
    if (name.length > 50) {
      return `"${name}" exceeds the 50 character limit`;
    }
    return null;
  }

  /**
   * Report progress to the callback if provided.
   */
  private reportProgress(
    allFunctions: FunctionNode[],
    onProgress?: (progress: ProcessingProgress) => void
  ): void {
    if (!onProgress) return;

    const pending = allFunctions.filter(
      (fn) =>
        !this.done.has(fn) && !this.processing.has(fn) && !this.ready.has(fn)
    ).length;

    onProgress({
      total: allFunctions.length,
      done: this.done.size,
      processing: this.processing.size,
      ready: this.ready.size,
      pending
    });
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
    const {
      concurrency = 50,
      metrics,
      preDone,
      profiler: optProfiler
    } = options;
    const profiler = optProfiler ?? NULL_PROFILER;

    this.options = options;
    this.metrics = metrics;
    this.isMinified = options.looksMinified ?? defaultLooksMinified;

    const doneIds = new Set<string>();
    if (preDone) {
      for (const fn of preDone) {
        doneIds.add(fn.sessionId);
        this.done.add(fn);
      }
    }

    const allNodeIds = [...graph.nodes.keys()].filter((id) => !doneIds.has(id));
    const { functionCount, moduleBindingCount } = countNodeTypes(
      allNodeIds,
      graph
    );
    if (metrics) {
      metrics.setFunctionTotal(functionCount);
      metrics.setModuleBindingTotal(moduleBindingCount);
    }

    const { doneIds: finalDoneIds } = await this.runProcessUnifiedLoop(
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
    const usedNames = new Set<string>(Object.keys(graph.targetScope.bindings));
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

    const limit = createConcurrencyLimiter(concurrency);
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

    const signalCompletion = makeSignalFn(signals, "notifyCompletion");
    const decrementInflight = makeDecrementFn(inFlight, signals);
    const markDone = (id: string) => {
      processingIds.delete(id);
      doneIds.add(id);
      markDoneUnblockDependents(
        id,
        graph,
        doneIds,
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
      const renameNode = graph.nodes.get(id)!;
      if (renameNode.type === "function")
        readyFunctions.push([id, renameNode.node]);
      else readyModuleBindings.push(renameNode.node);
    }

    for (const [id, fn] of readyFunctions) {
      this.dispatchUnifiedFunction(
        id,
        fn,
        llm,
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
    for (const group of groupByProximity(readyModuleBindings)) {
      this.dispatchUnifiedModuleBatch(
        group,
        llm,
        usedNames,
        graph,
        profiler,
        metrics,
        limit,
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
    fn.status = "processing";
    processingIds.add(id);
    inFlight.count++;
    metrics?.functionStarted();
    const waitMs = readyAtMs?.has(id)
      ? performance.now() - readyAtMs.get(id)!
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
        await this.processFunction(fn, llm);
      } catch (error) {
        const msg = error instanceof Error ? error.message : String(error);
        debug.log(
          "unified-processor",
          `Function ${fn.sessionId} failed: ${msg}`
        );
        this.failedCount++;
        if (!fn.renameMapping) fn.renameMapping = { names: {} };
      } finally {
        fnSpan.end();
        fn.status = "done";
        this.done.add(fn);
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
      mb.status = "processing";
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
          b.status = "done";
          metrics?.moduleBindingCompleted();
          markDone(b.sessionId);
        }
        signalCompletion();
        decrementInflight();
      }
    });
  }

  /**
   * Rename a module-level binding by directly updating its references,
   * avoiding a full AST traversal that scope.rename() would perform.
   * Safe because Babel's binding.referencePaths already excludes shadowed refs.
   */
  private applyModuleRename(
    scope: { bindings: Record<string, any> },
    oldName: string,
    newName: string
  ): void {
    const binding = scope.bindings[oldName];
    if (!binding) return;

    // Update the declaration identifier
    binding.identifier.name = newName;

    // Update all references (Babel tracks only those resolving to THIS binding)
    for (const refPath of binding.referencePaths) {
      if (refPath.isIdentifier()) {
        refPath.node.name = newName;
      }
    }

    // Update assignment targets (constant violations)
    for (const vPath of binding.constantViolations) {
      if (
        vPath.isAssignmentExpression() &&
        t.isIdentifier(vPath.node.left) &&
        vPath.node.left.name === oldName
      ) {
        vPath.node.left.name = newName;
      }
    }

    // Update scope binding table
    scope.bindings[newName] = binding;
    delete scope.bindings[oldName];
  }

  /**
   * Process a batch of module-level bindings via the LLM.
   * Uses the shared batch rename loop with module-binding-specific callbacks.
   */
  private async processModuleBindingBatch(
    batch: ModuleBindingNode[],
    llm: LLMProvider,
    usedNames: Set<string>,
    graph: UnifiedGraph
  ): Promise<void> {
    if (!llm.suggestAllNames) return;

    const bindingMap = new Map(batch.map((b) => [b.name, b]));

    // Build assignment and usage context maps for this batch
    const assignmentContext: Record<string, string[]> = {};
    const usageExamples: Record<string, string[]> = {};
    for (const b of batch) {
      assignmentContext[b.name] = b.assignments;
      usageExamples[b.name] = b.usages;
    }

    // Compute windowed usedNames for prompts
    const batchLines = batch.map((b) => b.declarationLine);
    const totalBindings = Object.keys(graph.targetScope.bindings).length;
    const windowedNames = getProximateUsedNames(
      usedNames,
      batchLines,
      graph.targetScope.bindings,
      totalBindings,
      this.isMinified
    );

    const result = await this.runBatchRenameLoop(
      llm,
      batch.map((b) => b.name),
      {
        buildRequest: (remaining, round, prev, failures) => {
          const declarations = [
            ...new Set(remaining.map((id) => bindingMap.get(id)!.declaration))
          ];

          let userPrompt = buildModuleLevelRenamePrompt(
            declarations,
            assignmentContext,
            usageExamples,
            remaining,
            windowedNames,
            this.isMinified
          );

          // For retries, prepend rejection context so the LLM knows what was tried
          if (round > 1) {
            userPrompt =
              buildModuleLevelRetryPrefix(prev, failures) + "\n" + userPrompt;
          }

          return {
            code: "",
            identifiers: remaining,
            usedNames: windowedNames,
            calleeSignatures: [],
            callsites: [],
            systemPrompt: MODULE_LEVEL_RENAME_SYSTEM_PROMPT,
            userPrompt,
            isRetry: round > 1,
            previousAttempt: round > 1 ? prev : undefined,
            failures: round > 1 ? failures : undefined
          };
        },
        applyRename: (oldName, newName) => {
          const mb = bindingMap.get(oldName);
          if (mb) {
            this.applyModuleRename(mb.scope, oldName, newName);
            usedNames.add(newName);
          }
        },
        getUsedNames: () => usedNames,
        functionId: `module-binding-batch:${batch.map((b) => b.name).join(",")}`,
        resolveRemaining: (remaining, prev, outcomes, totalLLMCalls) => {
          resolveRemainingIdentifiers(
            remaining,
            prev,
            outcomes,
            totalLLMCalls,
            usedNames,
            "module-binding",
            (name, newName) => {
              const mb = bindingMap.get(name);
              if (mb) {
                this.applyModuleRename(mb.scope, name, newName);
                usedNames.add(newName);
              }
            }
          );
        }
      }
    );

    const report: FunctionRenameReport = {
      functionId: `module-binding-batch:${batch.map((b) => b.name).join(",")}`,
      totalIdentifiers: batch.length,
      renamedCount: batch.length - result.remaining.size,
      outcomes: result.outcomes,
      totalLLMCalls: result.totalLLMCalls,
      finishReasons: result.finishReasons
    };
    this._reports.push(report);
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
    const maxFreeRetries =
      this.options.maxFreeRetries ?? DEFAULT_MAX_FREE_RETRIES;

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
      lastValidation = callResult.validation!;
      if (callResult.newAdaptiveBatchSize !== undefined) {
        adaptiveBatchSize = callResult.newAdaptiveBatchSize;
      }

      const { nextRetry, exhausted } = classifyFailedIdentifiers(
        batchRetries,
        callResult.validation!,
        callResult.responseRenames!,
        idState,
        callbacks,
        callResult.usedNamesSnapshot!,
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
    let response: import("../llm/types.js").BatchRenameResponse;
    try {
      const done = this.metrics?.llmCallStart();
      response = await llm.suggestAllNames!(request);
      done?.();
      this.metrics?.recordTokens(
        response.usage?.totalTokens ?? 0,
        response.usage?.inputTokens,
        response.usage?.outputTokens
      );
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
    const validThisCall = applyValidRenames(
      validation,
      callbacks,
      idState,
      outcomes,
      callNum,
      isRetry
    );
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

  /** Straggler pass: one final attempt on all retry-exhausted identifiers. */
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
    const stragglers = retryExhausted.filter((name) => !outcomes[name]);
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
      const done = this.metrics?.llmCallStart();
      const response = await llm.suggestAllNames!(request);
      done?.();
      this.metrics?.recordTokens(
        response.usage?.totalTokens ?? 0,
        response.usage?.inputTokens,
        response.usage?.outputTokens
      );
      finishReasons.push(response.finishReason);
      const validation = validateBatchRenames(
        response.renames,
        new Set(stragBatch),
        callbacks.getUsedNames()
      );
      for (const [oldName, newName] of Object.entries(validation.valid)) {
        callbacks.applyRename(oldName, newName);
        outcomes[oldName] = { status: "renamed", newName, round: callNum };
      }
      for (const name of stragBatch) {
        if (response.renames[name]) {
          idState.get(name)!.lastSuggestion = response.renames[name];
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

/** Truncate function code to MAX_CODE_LINES to avoid exceeding LLM context window. */
function truncateFunctionCode(code: string, sessionId: string): string {
  const MAX_CODE_LINES = 500;
  const lines = code.split("\n");
  if (lines.length <= MAX_CODE_LINES) return code;
  debug.log(
    "processor",
    `Truncated function ${sessionId} from ${lines.length} to ${MAX_CODE_LINES} lines`
  );
  return (
    lines.slice(0, MAX_CODE_LINES).join("\n") + "\n  // ... [truncated] ...\n}"
  );
}

/** Compute proximity-windowed used names for a batch of identifiers. */
function computeWindowedUsedNames(
  remaining: string[],
  bindingMap: Map<string, BindingInfo>,
  fn: FunctionNode,
  usedIdentifiers: Set<string>,
  isMinified: LooksMinifiedFn
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
    isMinified
  );
}

/** Initialize the readyAtMs profiling map if profiling is enabled. Returns null if not enabled. */
function initReadyAtMs(
  profiler: import("../profiling/profiler.js").Profiler,
  ready: Set<FunctionNode>
): Map<FunctionNode, number> | null {
  if (!profiler.isEnabled) return null;
  const now = performance.now();
  const map = new Map<FunctionNode, number>();
  for (const fn of ready) map.set(fn, now);
  return map;
}

/** Update ready-timestamp map for all newly-ready functions that don't have a timestamp yet. */
function updateReadyTimestamps(
  ready: Set<FunctionNode>,
  readyAtMs: Map<FunctionNode, number> | null
): void {
  if (!readyAtMs) return;
  const now = performance.now();
  for (const readyFn of ready) {
    if (!readyAtMs.has(readyFn)) readyAtMs.set(readyFn, now);
  }
}

/** Build reverse-dependency map: for each function, which functions depend on it? */
function buildDependentsMap(
  functions: FunctionNode[]
): Map<FunctionNode, FunctionNode[]> {
  const dependents = new Map<FunctionNode, FunctionNode[]>();
  for (const fn of functions) {
    for (const callee of fn.internalCallees) {
      let list = dependents.get(callee);
      if (!list) {
        list = [];
        dependents.set(callee, list);
      }
      list.push(fn);
    }
    if (fn.scopeParent) {
      let list = dependents.get(fn.scopeParent);
      if (!list) {
        list = [];
        dependents.set(fn.scopeParent, list);
      }
      list.push(fn);
    }
  }
  return dependents;
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
function makeSignalFn(
  signals: { notifyCompletion: (() => void) | null },
  _key: "notifyCompletion"
): () => void {
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
    const renameNode = graph.nodes.get(id)!;
    if (renameNode.type === "function") functionCount++;
    else moduleBindingCount++;
  }
  return { functionCount, moduleBindingCount };
}

/** Find and populate the initial ready set for the unified processor. Returns count. */
function initUnifiedReadySet(
  allNodeIds: string[],
  doneIds: Set<string>,
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
  preDoneSize: number
): { pendingCount: number; blockedIds: Set<string> } {
  const initialReady = initUnifiedReadySet(
    allNodeIds,
    doneIds,
    isNodeReady,
    readyIds
  );

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
  if (metrics && preDoneSize > 0) metrics.functionsReady(preDoneSize);

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
  doneIds: Set<string>,
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
interface BatchRenameCallbacks {
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
}

/**
 * Apply all valid renames from a validation result, recording outcomes.
 * Returns the count of valid renames applied.
 */
function applyValidRenames(
  validation: BatchValidationResult,
  callbacks: BatchRenameCallbacks,
  idState: Map<string, IdentifierAttemptState>,
  outcomes: Record<string, IdentifierOutcome>,
  callNum: number,
  isRetry: boolean
): number {
  let count = 0;
  for (const [oldName, newName] of Object.entries(validation.valid)) {
    debug.rename({
      functionId: callbacks.functionId,
      oldName,
      newName,
      wasRetry: isRetry,
      attemptNumber: (idState.get(oldName)?.attempts ?? 0) + 1
    });
    callbacks.applyRename(oldName, newName);
    outcomes[oldName] = { status: "renamed", newName, round: callNum };
    count++;
  }
  return count;
}

/**
 * Classify failed identifiers after a batch call into nextRetry and exhausted lists.
 * Updates idState in place.
 */
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
    const state = idState.get(name)!;
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

    if (!isFreeRetry) {
      updateFailureState(name, state, dupSet, invSet, unchSet);
      if (state.attempts < maxRetriesPerIdentifier) {
        nextRetry.push(name);
      } else {
        exhausted.push(name);
      }
    } else {
      nextRetry.push(name);
    }
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
    const state = idState.get(name)!;
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
  if (state.lastFailureReason === "duplicate") {
    return {
      status: "duplicate",
      conflictedWith: state.lastSuggestion || "unknown",
      attempts: totalAttempts,
      suggestion: state.lastSuggestion
    };
  }
  if (state.lastFailureReason === "invalid") {
    return {
      status: "invalid",
      attempts: totalAttempts,
      suggestion: state.lastSuggestion
    };
  }
  if (state.lastFailureReason === "unchanged") {
    return {
      status: "unchanged",
      attempts: totalAttempts,
      suggestion: state.lastSuggestion
    };
  }
  return {
    status: "missing",
    attempts: totalAttempts,
    lastFinishReason: finishReasons[finishReasons.length - 1]
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
 * Binding info with the identifier node, its location, and owning scope.
 */
interface BindingInfo {
  name: string;
  identifier: t.Identifier;
  /** The scope that owns this binding (needed for block-scoped vars in child scopes) */
  scope: NodePath<t.Function>["scope"];
}

/**
 * Gets all bindings owned by this function (not inherited from parent scope).
 */
function getOwnBindings(fnPath: NodePath<t.Function>): BindingInfo[] {
  const bindings: BindingInfo[] = [];
  const scope = fnPath.scope;

  collectScopeOwnBindings(scope, bindings);
  collectBodyScopeBindings(fnPath, scope, bindings);
  collectNestedBlockBindings(fnPath, bindings);
  collectFunctionNameBinding(fnPath, scope, bindings);

  return bindings;
}

/** Collect bindings declared directly in the function's own scope. */
function collectScopeOwnBindings(
  scope: NodePath<t.Function>["scope"],
  bindings: BindingInfo[]
): void {
  for (const [name, binding] of Object.entries(scope.bindings)) {
    if (binding.scope === scope) {
      bindings.push({
        name,
        identifier: binding.identifier,
        scope: binding.scope
      });
    }
  }
}

/**
 * When parameters have defaults/destructuring/rest, Babel creates a separate
 * scope for the function body. Collect any bindings from that body scope.
 */
function collectBodyScopeBindings(
  fnPath: NodePath<t.Function>,
  scope: NodePath<t.Function>["scope"],
  bindings: BindingInfo[]
): void {
  const bodyPath = fnPath.get("body");
  if (Array.isArray(bodyPath) || !bodyPath.isBlockStatement()) return;
  const bodyScope = bodyPath.scope;
  if (bodyScope === scope) return;
  for (const [name, binding] of Object.entries(bodyScope.bindings)) {
    if (binding.scope === bodyScope && !bindings.some((b) => b.name === name)) {
      bindings.push({
        name,
        identifier: binding.identifier,
        scope: binding.scope
      });
    }
  }
}

/**
 * Traverse nested block scopes to collect let/const bindings inside
 * for/while/if/try blocks owned by this function but in child block scopes.
 */
function collectNestedBlockBindings(
  fnPath: NodePath<t.Function>,
  bindings: BindingInfo[]
): void {
  const seen = new Set(bindings.map((b) => b.name));
  fnPath.traverse({
    Function(path: NodePath<t.Function>) {
      if (path !== fnPath) path.skip();
    },
    BlockStatement(path: NodePath<t.BlockStatement>) {
      if (path.parentPath === fnPath) return;
      collectBlockBindings(path, seen, bindings);
    },
    ForStatement(path: NodePath<t.ForStatement>) {
      collectBlockBindings(path, seen, bindings);
    },
    ForInStatement(path: NodePath<t.ForInStatement>) {
      collectBlockBindings(path, seen, bindings);
    },
    ForOfStatement(path: NodePath<t.ForOfStatement>) {
      collectBlockBindings(path, seen, bindings);
    },
    SwitchStatement(path: NodePath<t.SwitchStatement>) {
      collectBlockBindings(path, seen, bindings);
    },
    CatchClause(path: NodePath<t.CatchClause>) {
      collectBlockBindings(path, seen, bindings);
    }
  });
}

/** Include the function's own name binding for named function expressions/declarations. */
function collectFunctionNameBinding(
  fnPath: NodePath<t.Function>,
  scope: NodePath<t.Function>["scope"],
  bindings: BindingInfo[]
): void {
  if (!fnPath.isFunctionExpression() && !fnPath.isFunctionDeclaration()) return;
  const id = fnPath.node.id;
  if (!id) return;
  const nameBinding = fnPath.isFunctionDeclaration()
    ? fnPath.parentPath.scope.getBinding(id.name)
    : scope.getBinding(id.name);
  if (nameBinding && !bindings.some((b) => b.name === id.name)) {
    bindings.push({
      name: id.name,
      identifier: nameBinding.identifier,
      scope: nameBinding.scope
    });
  }
}

/**
 * Gets only parameter bindings for a function (not body locals).
 * Used for lightweight "params-only" processing of library functions.
 */
function getParamBindings(fnPath: NodePath<t.Function>): BindingInfo[] {
  const bindings: BindingInfo[] = [];
  const scope = fnPath.scope;
  const params = fnPath.node.params;

  // Collect all names introduced by parameters (including destructured)
  const paramNames = new Set<string>();
  for (const param of params) {
    collectParamNames(param, paramNames);
  }

  // Match against scope bindings for proper identifier references
  for (const name of paramNames) {
    const binding = scope.getBinding(name);
    if (binding) {
      bindings.push({
        name,
        identifier: binding.identifier,
        scope: binding.scope
      });
    }
  }

  return bindings;
}

/**
 * Recursively collects identifier names from a parameter pattern.
 */
function collectParamNames(node: t.Node, names: Set<string>): void {
  if (t.isIdentifier(node)) {
    names.add(node.name);
  } else if (t.isAssignmentPattern(node)) {
    collectParamNames(node.left, names);
  } else if (t.isRestElement(node)) {
    collectParamNames(node.argument, names);
  } else if (t.isArrayPattern(node)) {
    collectArrayPatternParamNames(node, names);
  } else if (t.isObjectPattern(node)) {
    collectObjectPatternParamNames(node, names);
  }
}

function collectArrayPatternParamNames(
  node: t.ArrayPattern,
  names: Set<string>
): void {
  for (const element of node.elements) {
    if (element) collectParamNames(element, names);
  }
}

function collectObjectPatternParamNames(
  node: t.ObjectPattern,
  names: Set<string>
): void {
  for (const prop of node.properties) {
    if (t.isObjectProperty(prop)) {
      collectParamNames(prop.value, names);
    } else if (t.isRestElement(prop)) {
      collectParamNames(prop.argument, names);
    }
  }
}

/**
 * Collects bindings from a block scope that are declared directly in that scope.
 */
function collectBlockBindings(
  path: NodePath,
  seen: Set<string>,
  bindings: BindingInfo[]
): void {
  const blockScope = path.scope;
  for (const [name, binding] of Object.entries(blockScope.bindings)) {
    if (binding.scope === blockScope && !seen.has(name)) {
      seen.add(name);
      bindings.push({
        name,
        identifier: binding.identifier,
        scope: binding.scope
      });
    }
  }
}

/**
 * Result of validating batch rename suggestions.
 */
interface BatchValidationResult {
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

  for (const [oldName, rawNewName] of Object.entries(renames)) {
    if (!expected.has(oldName)) continue;
    const newName = sanitizeIdentifier(rawNewName);
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
  if (!isValidIdentifier(newName) || RESERVED_WORDS.has(newName)) {
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
    const state = idState.get(name)!;
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
function resolveRemainingIdentifiers(
  remaining: Set<string>,
  prev: Record<string, string>,
  outcomes: Record<string, IdentifierOutcome>,
  totalLLMCalls: number,
  usedNames: Set<string>,
  functionId: string,
  applyRename: (oldName: string, newName: string) => void
): void {
  for (const name of [...remaining]) {
    const suggestedName = prev[name];
    if (!suggestedName) continue;

    const sanitized = sanitizeIdentifier(suggestedName);
    if (!isValidIdentifier(sanitized) || RESERVED_WORDS.has(sanitized))
      continue;
    if (sanitized === name) continue;

    if (usedNames.has(sanitized)) {
      const resolved = resolveConflict(sanitized, usedNames);
      debug.renameFallback({
        functionId,
        identifier: name,
        suggestedName: sanitized,
        rejectionReason: `collision with existing name "${sanitized}"`,
        fallbackResult: resolved,
        round: totalLLMCalls
      });
      applyRename(name, resolved);
      remaining.delete(name);
      outcomes[name] = {
        status: "renamed",
        newName: resolved,
        round: totalLLMCalls + 1
      };
    } else {
      applyRename(name, sanitized);
      remaining.delete(name);
      outcomes[name] = {
        status: "renamed",
        newName: sanitized,
        round: totalLLMCalls + 1
      };
    }
  }
}
