import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import type {
  FunctionNode,
  ModuleBindingNode,
  RenameNode,
  UnifiedGraph,
  FunctionRenameReport,
  IdentifierOutcome,
  LLMContext,
  RenameDecision,
  ProcessorOptions,
  ProcessingProgress
} from "../analysis/types.js";
import { buildContext } from "./context-builder.js";
import type { LLMProvider, BatchRenameRequest } from "../llm/types.js";
import {
  sanitizeIdentifier,
  resolveConflict,
  isValidIdentifier,
  RESERVED_WORDS
} from "../llm/validation.js";
import { debug } from "../debug.js";
import { generate } from "../babel-utils.js";
import { looksMinified } from "./minified-heuristic.js";
import { createConcurrencyLimiter } from "../utils/concurrency.js";
import {
  MODULE_LEVEL_RENAME_SYSTEM_PROMPT,
  buildModuleLevelRenamePrompt,
  buildModuleLevelRetryPrefix
} from "../llm/prompts.js";
import { getProximateUsedNames } from "../plugins/rename.js";

/** Failure categories from batch validation */
type Failures = { duplicates: string[]; invalid: string[]; missing: string[]; unchanged: string[] };

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

  /** Per-function rename reports (populated after processAll completes) */
  get reports(): ReadonlyArray<FunctionRenameReport> { return this._reports; }

  /** Number of functions that failed due to LLM errors (populated after processAll completes) */
  get failed(): number { return this.failedCount; }

  /** Number of identifiers skipped by looksMinified heuristic */
  get skippedByHeuristic(): number { return this._skippedByHeuristic; }

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
    const { concurrency = 50, onProgress, metrics, preDone, paramOnly } = options;

    // Store options and metrics for use in processFunction
    this.options = options;
    this.metrics = metrics;
    this.paramOnly = paramOnly ?? false;

    // Pre-seed done set with already-completed functions (e.g., library functions)
    if (preDone) {
      for (const fn of preDone) {
        this.done.add(fn);
      }
    }

    // Initialize metrics if provided
    if (metrics) {
      metrics.setFunctionTotal(functions.length);
    }

    // Initialize: find functions whose dependencies are all satisfied.
    // When preDone is empty this is equivalent to findLeafFunctions();
    // when preDone contains library functions, this also finds app functions
    // whose only callees are already-done library functions.
    let initialReady = 0;
    for (const fn of functions) {
      if (this.isReady(fn)) {
        this.ready.add(fn);
        initialReady++;
      }
    }

    debug.log("processor", `Initial state: ${initialReady} ready, ${functions.length} total, ${this.done.size} pre-done`);

    // Deadlock breaker: if nothing is ready, scopeParent chains are blocking everything.
    // Retry without scopeParent constraint to unblock processing.
    if (initialReady === 0 && functions.length > 0) {
      let blockedByCallees = 0;
      let blockedByScopeParent = 0;
      let blockedByBoth = 0;
      for (const fn of functions) {
        let calleesBlocking = false;
        for (const c of fn.internalCallees) {
          if (!this.done.has(c)) { calleesBlocking = true; break; }
        }
        const parentBlocking = fn.scopeParent ? !this.done.has(fn.scopeParent) : false;
        if (calleesBlocking && parentBlocking) blockedByBoth++;
        else if (calleesBlocking) blockedByCallees++;
        else if (parentBlocking) blockedByScopeParent++;
      }
      debug.log("processor", `Blocked by: callees=${blockedByCallees}, scopeParent=${blockedByScopeParent}, both=${blockedByBoth}`);

      debug.log("processor", "Deadlock detected — relaxing scopeParent dependencies");
      for (const fn of functions) {
        if (this.isReadyIgnoringScopeParent(fn)) {
          this.ready.add(fn);
          initialReady++;
        }
      }
      debug.log("processor", `After relaxing: ${initialReady} ready`);
    }

    // Update metrics with initial ready count
    if (metrics && initialReady > 0) {
      metrics.functionsReady(initialReady);
    }

    this.reportProgress(functions, onProgress);

    // Build reverse-dependency map: for each function, which functions depend on it?
    const dependents = new Map<FunctionNode, FunctionNode[]>();
    for (const fn of functions) {
      for (const callee of fn.internalCallees) {
        let list = dependents.get(callee);
        if (!list) { list = []; dependents.set(callee, list); }
        list.push(fn);
      }
      if (fn.scopeParent) {
        let list = dependents.get(fn.scopeParent);
        if (!list) { list = []; dependents.set(fn.scopeParent, list); }
        list.push(fn);
      }
    }

    const limit = createConcurrencyLimiter(concurrency);

    // Notification signal: replaces Promise.race over all pending promises
    let notifyCompletion: (() => void) | null = null;

    // Track in-flight count for clean shutdown
    let inFlightCount = 0;
    let drainResolve: (() => void) | null = null;

    const pendingCount = () => functions.filter(
      f => !this.done.has(f) && !this.processing.has(f) && !this.ready.has(f)
    ).length;

    while (this.ready.size > 0 || this.processing.size > 0) {
      // Dispatch all ready items up to concurrency limit
      const dispatching = [...this.ready];
      for (const fn of dispatching) {
        this.ready.delete(fn);
        this.processing.add(fn);
        fn.status = "processing";
        metrics?.functionStarted();
        inFlightCount++;

        limit(async () => {
          try {
            await this.processFunction(fn, llm);
          } catch (error) {
            const msg = error instanceof Error ? error.message : String(error);
            debug.log("processor", `Function ${fn.sessionId} failed: ${msg}`);
            this.failedCount++;
            if (!fn.renameMapping) {
              fn.renameMapping = { names: {} };
            }
          } finally {
            this.processing.delete(fn);
            this.done.add(fn);
            fn.status = "done";
            metrics?.functionCompleted();

            const newlyReady = this.checkNewlyReady(fn, dependents);
            if (metrics && newlyReady > 0) {
              metrics.functionsReady(newlyReady);
            }

            debug.queueState({
              ready: this.ready.size, processing: this.processing.size,
              pending: pendingCount(), done: this.done.size, total: functions.length,
              inFlightLLM: inFlightCount - 1, event: "completion",
              detail: `completed=${fn.sessionId} unlocked=${newlyReady}`
            });

            this.reportProgress(functions, onProgress);

            if (notifyCompletion) {
              const cb = notifyCompletion;
              notifyCompletion = null;
              cb();
            }

            inFlightCount--;
            if (inFlightCount === 0 && drainResolve) {
              const cb = drainResolve;
              drainResolve = null;
              cb();
            }
          }
        });
      }

      if (dispatching.length > 0) {
        debug.queueState({
          ready: this.ready.size, processing: this.processing.size,
          pending: pendingCount(), done: this.done.size, total: functions.length,
          inFlightLLM: inFlightCount, event: "dispatch",
          detail: `dispatched=${dispatching.length}`
        });
      }

      // Wait for at least one to complete if nothing is ready
      if (this.ready.size === 0 && this.processing.size > 0) {
        debug.queueState({
          ready: 0, processing: this.processing.size,
          pending: pendingCount(), done: this.done.size, total: functions.length,
          inFlightLLM: inFlightCount, event: "waiting-on-llm"
        });
        await new Promise<void>(resolve => { notifyCompletion = resolve; });
      }

      // Mid-loop deadlock breaking
      if (this.ready.size === 0 && this.processing.size === 0) {
        let newlyReady = this.checkNewlyReadyRelaxed(functions);
        if (newlyReady > 0) {
          debug.log("processor", `Breaking scopeParent deadlock: ${newlyReady} functions unblocked`);
          debug.queueState({
            ready: this.ready.size, processing: 0,
            pending: pendingCount(), done: this.done.size, total: functions.length,
            inFlightLLM: 0, event: "deadlock-break",
            detail: `tier=1-scopeParent unlocked=${newlyReady}`
          });
        } else {
          newlyReady = this.forceBreakAllDeadlocks(functions);
          if (newlyReady > 0) {
            debug.log("processor", `Force-breaking callee deadlock: ${newlyReady} functions unblocked`);
            debug.queueState({
              ready: this.ready.size, processing: 0,
              pending: pendingCount(), done: this.done.size, total: functions.length,
              inFlightLLM: 0, event: "deadlock-break",
              detail: `tier=2-callee-cycle unlocked=${newlyReady}`
            });
          }
        }
        if (metrics && newlyReady > 0) {
          metrics.functionsReady(newlyReady);
        }
      }
    }

    // Wait for all in-flight tasks to complete
    if (inFlightCount > 0) {
      await new Promise<void>(resolve => { drainResolve = resolve; });
    }

    // Collect reports from all functions
    for (const fn of functions) {
      if (fn.renameReport) {
        this._reports.push(fn.renameReport);
      }
    }

    // Final metrics emit
    metrics?.emit();

    return this.allRenames;
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
    const allBindings = this.paramOnly ? getParamBindings(fn.path) : getOwnBindings(fn.path);

    // If no bindings to rename, skip
    if (allBindings.length === 0) {
      fn.renameMapping = { names: {} };
      return;
    }

    // Filter out identifiers that already have descriptive names
    const bindings = allBindings.filter(b => looksMinified(b.name));
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
    const context = buildContext(fn, this.ast);
    const renameMapping: Record<string, string> = {};
    const bindingMap = new Map(bindings.map(b => [b.name, b]));

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
      buildRequest: (remaining: string[], round: number, prev: Record<string, string>, failures: Failures) => {
        // Regenerate code from AST (reflects any renames applied in previous rounds)
        let code = generate(fn.path.node).code;

        // Truncate very large functions to avoid exceeding LLM context window
        const MAX_CODE_LINES = 500;
        const lines = code.split('\n');
        if (lines.length > MAX_CODE_LINES) {
          code = lines.slice(0, MAX_CODE_LINES).join('\n') + '\n  // ... [truncated] ...\n}';
          debug.log("processor", `Truncated function ${fn.sessionId} from ${lines.length} to ${MAX_CODE_LINES} lines`);
        }

        // Compute proximity-windowed usedNames, cached when batch identifiers
        // AND usedIdentifiers set are unchanged (size is monotonically increasing)
        const windowKey = remaining.join(",");
        const currentUsedSize = context.usedIdentifiers.size;
        let windowedUsedNames: Set<string>;
        if (windowKey === cachedWindowKey && currentUsedSize === cachedUsedSize && cachedWindowedNames) {
          windowedUsedNames = cachedWindowedNames;
        } else {
          const batchLines = remaining
            .map(id => bindingMap.get(id)?.identifier.loc?.start?.line)
            .filter((l): l is number => l !== undefined);
          const scopeBindings = fn.path.scope.bindings;
          const totalBindings = Object.keys(scopeBindings).length;
          windowedUsedNames = batchLines.length > 0
            ? getProximateUsedNames(context.usedIdentifiers, batchLines, scopeBindings, totalBindings)
            : context.usedIdentifiers;
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
          this.applyFunctionRename(binding, oldName, newName, fn.sessionId, context.usedIdentifiers, renameMapping);
        }
      },
      getUsedNames: () => context.usedIdentifiers,
      functionId: `${fn.sessionId}${laneId}`,
      resolveRemaining: (remaining: Set<string>, prev: Record<string, string>, outcomes: Record<string, IdentifierOutcome>, totalLLMCalls: number) => {
        resolveRemainingIdentifiers(remaining, prev, outcomes, totalLLMCalls, context.usedIdentifiers, fn.sessionId,
          (name, newName) => {
            const binding = bindingMap.get(name);
            if (binding) {
              this.applyFunctionRename(binding, name, newName, fn.sessionId, context.usedIdentifiers, renameMapping);
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
              originalPosition: { line: loc.start.line, column: loc.start.column },
              originalName: name,
              newName: name,
              functionId: fn.sessionId
            });
          }
          renameMapping[name] = name;
        }
      }
    }; };

    // Decide whether to use parallel lanes
    let allOutcomes: Record<string, IdentifierOutcome> = {};
    let allFinishReasons: (string | undefined)[] = [];
    let totalLLMCalls = 0;
    let totalRemaining = new Set<string>();

    if (bindings.length > laneThreshold) {
      // Split bindings into lanes by position
      const lanes = splitByPosition(bindings.map(b => b.name), NUM_LANES);
      debug.log("processor", `${fn.sessionId}: splitting ${bindings.length} bindings into ${lanes.length} lanes`);

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
      const result = await this.runBatchRenameLoop(llm, bindings.map(b => b.name), makeCallbacks(""));
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
    const context = buildContext(fn, this.ast);
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
      const rejection = this.getRejectionReason(newName, context.usedIdentifiers);

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
    const { concurrency = 50, metrics, preDone } = options;

    this.options = options;
    this.metrics = metrics;

    // Track done/processing/ready by sessionId
    const doneIds = new Set<string>();
    const processingIds = new Set<string>();
    const readyIds = new Set<string>();

    // Pre-seed done set
    if (preDone) {
      for (const fn of preDone) {
        doneIds.add(fn.sessionId);
        this.done.add(fn);
      }
    }

    const allNodeIds = [...graph.nodes.keys()].filter(id => !doneIds.has(id));

    // Count functions and module bindings separately for metrics
    let functionCount = 0;
    let moduleBindingCount = 0;
    for (const id of allNodeIds) {
      const renameNode = graph.nodes.get(id)!;
      if (renameNode.type === "function") functionCount++;
      else moduleBindingCount++;
    }

    if (metrics) {
      metrics.setFunctionTotal(functionCount);
      metrics.setModuleBindingTotal(moduleBindingCount);
    }

    // Shared usedNames for module-level bindings
    const usedNames = new Set<string>();
    for (const name of Object.keys(graph.targetScope.bindings)) {
      usedNames.add(name);
    }

    // Helper: check if a node's deps are all done
    const isNodeReady = (id: string): boolean => {
      const deps = graph.dependencies.get(id);
      if (!deps) return true;
      for (const dep of deps) {
        if (!doneIds.has(dep)) return false;
      }
      return true;
    };

    // Helper: check if a node is ready ignoring scopeParent edges (Tier 1 deadlock breaking)
    const isNodeReadyIgnoringScopeParent = (id: string): boolean => {
      const deps = graph.dependencies.get(id);
      if (!deps) return true;
      for (const dep of deps) {
        if (doneIds.has(dep)) continue;
        if (graph.scopeParentEdges.has(`${id}->${dep}`)) continue;
        return false;
      }
      return true;
    };

    // Helper: mark a node done and check dependents
    const markDone = (id: string) => {
      processingIds.delete(id);
      doneIds.add(id);

      const deps = graph.dependents.get(id);
      if (deps) {
        for (const depId of deps) {
          if (blockedIds.has(depId)) {
            if (isNodeReady(depId)) {
              readyIds.add(depId);
              blockedIds.delete(depId);
              pendingCount--;
              metrics?.functionsReady(1);
            }
          }
        }
      }
    };

    // Pending count: nodes not yet ready, processing, or done
    let pendingCount = allNodeIds.length;

    // Find initial ready set
    let initialReady = 0;
    for (const id of allNodeIds) {
      if (isNodeReady(id)) {
        readyIds.add(id);
        initialReady++;
      }
    }
    pendingCount -= initialReady;

    const totalNodes = allNodeIds.length;
    debug.log("unified-processor", `Initial: ${initialReady} ready, ${totalNodes} total (${functionCount} fns, ${moduleBindingCount} mbs), ${doneIds.size} pre-done`);

    // Two-tier deadlock breaking if nothing is ready
    if (initialReady === 0 && totalNodes > 0) {
      // Tier 1: relax scopeParent edges only (still respects callee ordering)
      for (const id of allNodeIds) {
        if (!doneIds.has(id) && isNodeReadyIgnoringScopeParent(id)) {
          readyIds.add(id);
          initialReady++;
        }
      }
      pendingCount -= initialReady;
      if (initialReady > 0) {
        debug.log("unified-processor", `Tier 1 deadlock break: relaxed scopeParent for ${initialReady} nodes`);
      } else {
        // Tier 2: force all remaining (true callee cycles)
        for (const id of allNodeIds) {
          if (!doneIds.has(id)) {
            readyIds.add(id);
            initialReady++;
          }
        }
        pendingCount -= initialReady;
        debug.log("unified-processor", `Tier 2 deadlock break: forced ${initialReady} nodes ready`);
      }
    }

    if (metrics && initialReady > 0) {
      metrics.functionsReady(initialReady);
    }

    // Blocked set: nodes not yet ready, processing, or done (for efficient deadlock breaking)
    const blockedIds = new Set<string>();
    for (const id of allNodeIds) {
      if (!readyIds.has(id) && !doneIds.has(id)) {
        blockedIds.add(id);
      }
    }

    const limit = createConcurrencyLimiter(concurrency);
    let notifyCompletion: (() => void) | null = null;
    let inFlightCount = 0;
    let drainResolve: (() => void) | null = null;

    const signalCompletion = () => {
      if (notifyCompletion) {
        const cb = notifyCompletion;
        notifyCompletion = null;
        cb();
      }
    };

    const decrementInflight = () => {
      inFlightCount--;
      if (inFlightCount === 0 && drainResolve) {
        const cb = drainResolve;
        drainResolve = null;
        cb();
      }
    };

    // Dispatch a single function node
    const dispatchFunction = (id: string, fn: FunctionNode) => {
      fn.status = "processing";
      processingIds.add(id);
      inFlightCount++;
      metrics?.functionStarted();

      limit(async () => {
        try {
          await this.processFunction(fn, llm);
        } catch (error) {
          const msg = error instanceof Error ? error.message : String(error);
          debug.log("unified-processor", `Function ${fn.sessionId} failed: ${msg}`);
          this.failedCount++;
          if (!fn.renameMapping) fn.renameMapping = { names: {} };
        } finally {
          fn.status = "done";
          this.done.add(fn);
          metrics?.functionCompleted();
          markDone(id);
          signalCompletion();
          decrementInflight();
        }
      });
    };

    // Dispatch a batch of module binding nodes (one concurrency slot per batch)
    const dispatchModuleBindingBatch = (batch: ModuleBindingNode[]) => {
      for (const mb of batch) {
        mb.status = "processing";
        processingIds.add(mb.sessionId);
      }
      inFlightCount++;
      // Count all items as started for metrics
      for (let i = 0; i < batch.length; i++) metrics?.moduleBindingStarted();

      limit(async () => {
        try {
          await this.processModuleBindingBatch(batch, llm, usedNames, graph);
        } catch (error) {
          const msg = error instanceof Error ? error.message : String(error);
          debug.log("unified-processor", `Module binding batch failed: ${msg}`);
          this.failedCount += batch.length;
        } finally {
          for (const b of batch) {
            b.status = "done";
            metrics?.moduleBindingCompleted();
            markDone(b.sessionId);
          }
          signalCompletion();
          decrementInflight();
        }
      });
    };

    while (readyIds.size > 0 || processingIds.size > 0) {
      // Partition ready items into functions and module bindings
      const readyFunctions: Array<[string, FunctionNode]> = [];
      const readyModuleBindings: ModuleBindingNode[] = [];

      for (const id of [...readyIds]) {
        readyIds.delete(id);
        const renameNode = graph.nodes.get(id)!;
        if (renameNode.type === "function") {
          readyFunctions.push([id, renameNode.node]);
        } else {
          readyModuleBindings.push(renameNode.node);
        }
      }

      // Dispatch function nodes individually
      for (const [id, fn] of readyFunctions) {
        dispatchFunction(id, fn);
      }

      // Group and dispatch module bindings by proximity
      const groups = groupByProximity(readyModuleBindings);
      for (const group of groups) {
        dispatchModuleBindingBatch(group);
      }

      if (readyFunctions.length > 0 || readyModuleBindings.length > 0) {
        debug.queueState({
          ready: readyIds.size, processing: processingIds.size,
          pending: pendingCount, done: doneIds.size, total: totalNodes,
          inFlightLLM: inFlightCount, event: "dispatch",
          detail: `fns=${readyFunctions.length} mbs=${readyModuleBindings.length}`
        });
      }

      // Wait for at least one to complete if nothing is ready
      if (readyIds.size === 0 && processingIds.size > 0) {
        debug.queueState({
          ready: 0, processing: processingIds.size,
          pending: pendingCount, done: doneIds.size, total: totalNodes,
          inFlightLLM: inFlightCount, event: "waiting-on-llm"
        });
        await new Promise<void>(resolve => { notifyCompletion = resolve; });
      }

      // Two-tier deadlock breaking mid-loop
      if (readyIds.size === 0 && processingIds.size === 0 && blockedIds.size > 0) {
        let newlyReady = 0;
        for (const id of blockedIds) {
          if (isNodeReadyIgnoringScopeParent(id)) {
            readyIds.add(id);
            newlyReady++;
          }
        }
        for (const id of readyIds) blockedIds.delete(id);
        pendingCount -= newlyReady;
        if (newlyReady > 0) {
          debug.log("unified-processor", `Tier 1 mid-loop: relaxed scopeParent for ${newlyReady} nodes`);
          debug.queueState({
            ready: readyIds.size, processing: 0,
            pending: pendingCount, done: doneIds.size, total: totalNodes,
            inFlightLLM: 0, event: "deadlock-break",
            detail: `tier=1-scopeParent unlocked=${newlyReady}`
          });
        } else {
          for (const id of blockedIds) {
            readyIds.add(id);
            newlyReady++;
          }
          blockedIds.clear();
          pendingCount -= newlyReady;
          if (newlyReady > 0) {
            debug.log("unified-processor", `Tier 2 mid-loop: forced ${newlyReady} nodes ready`);
            debug.queueState({
              ready: readyIds.size, processing: 0,
              pending: pendingCount, done: doneIds.size, total: totalNodes,
              inFlightLLM: 0, event: "deadlock-break",
              detail: `tier=2-callee-cycle unlocked=${newlyReady}`
            });
          }
        }
        if (metrics && newlyReady > 0) metrics.functionsReady(newlyReady);
      }
    }

    // Wait for all in-flight tasks to complete
    if (inFlightCount > 0) {
      await new Promise<void>(resolve => { drainResolve = resolve; });
    }

    // Collect reports from function nodes
    for (const [, renameNode] of graph.nodes) {
      if (renameNode.type === "function" && renameNode.node.renameReport) {
        this._reports.push(renameNode.node.renameReport);
      }
    }

    metrics?.emit();

    return this.allRenames;
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

    const bindingMap = new Map(batch.map(b => [b.name, b]));

    // Build assignment and usage context maps for this batch
    const assignmentContext: Record<string, string[]> = {};
    const usageExamples: Record<string, string[]> = {};
    for (const b of batch) {
      assignmentContext[b.name] = b.assignments;
      usageExamples[b.name] = b.usages;
    }

    // Compute windowed usedNames for prompts
    const batchLines = batch.map(b => b.declarationLine);
    const totalBindings = Object.keys(graph.targetScope.bindings).length;
    const windowedNames = getProximateUsedNames(
      usedNames,
      batchLines,
      graph.targetScope.bindings,
      totalBindings
    );

    const result = await this.runBatchRenameLoop(llm, batch.map(b => b.name), {
      buildRequest: (remaining, round, prev, failures) => {
        const declarations = [...new Set(
          remaining.map(id => bindingMap.get(id)!.declaration)
        )];

        let userPrompt = buildModuleLevelRenamePrompt(
          declarations,
          assignmentContext,
          usageExamples,
          remaining,
          windowedNames
        );

        // For retries, prepend rejection context so the LLM knows what was tried
        if (round > 1) {
          userPrompt = buildModuleLevelRetryPrefix(prev, failures) + "\n" + userPrompt;
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
      functionId: `module-binding-batch:${batch.map(b => b.name).join(",")}`,
      resolveRemaining: (remaining, prev, outcomes, totalLLMCalls) => {
        resolveRemainingIdentifiers(remaining, prev, outcomes, totalLLMCalls, usedNames, "module-binding",
          (name, newName) => {
            const mb = bindingMap.get(name);
            if (mb) {
              this.applyModuleRename(mb.scope, name, newName);
              usedNames.add(newName);
            }
          }
        );
      }
    });

    const report: FunctionRenameReport = {
      functionId: `module-binding-batch:${batch.map(b => b.name).join(",")}`,
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
    callbacks: {
      buildRequest(remaining: string[], round: number, prev: Record<string, string>, failures: Failures): BatchRenameRequest;
      applyRename(oldName: string, newName: string): void;
      getUsedNames(): Set<string>;
      functionId: string;
      onUnrenamed?(name: string): void;
      resolveRemaining?(remaining: Set<string>, prev: Record<string, string>, outcomes: Record<string, IdentifierOutcome>, totalLLMCalls: number): void;
    }
  ): Promise<BatchRenameLoopResult> {
    const maxBatchSize = this.options.batchSize ?? DEFAULT_BATCH_SIZE;
    const maxRetriesPerIdentifier = this.options.maxRetriesPerIdentifier ?? DEFAULT_MAX_RETRIES_PER_ID;
    const maxFreeRetries = this.options.maxFreeRetries ?? DEFAULT_MAX_FREE_RETRIES;

    const outcomes: Record<string, IdentifierOutcome> = {};
    const finishReasons: (string | undefined)[] = [];

    // Per-identifier attempt tracking
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
      const batch = queue.splice(0, adaptiveBatchSize);

      // Inner retry loop for THIS batch
      let batchRetries = batch.slice();
      while (batchRetries.length > 0) {
        totalLLMCalls++;

        const { prev, failures } = buildPrevAndFailures(batchRetries, idState);
        const isRetry = Object.keys(prev).length > 0;
        const usedNamesSnapshot = new Set(callbacks.getUsedNames());

        const promptStart = Date.now();
        const request = callbacks.buildRequest(batchRetries, isRetry ? 2 : 1, prev, failures);
        const promptMs = Date.now() - promptStart;
        lastUserPrompt = request.userPrompt || "";

        debug.log("batch-loop", `${callbacks.functionId} call ${totalLLMCalls}: ${batchRetries.join(", ")}`);

        let response: import("../llm/types.js").BatchRenameResponse;
        const llmStart = Date.now();
        try {
          const done = this.metrics?.llmCallStart();
          response = await llm.suggestAllNames!(request);
          done?.();
          this.metrics?.recordTokens(response.usage?.totalTokens ?? 0, response.usage?.inputTokens, response.usage?.outputTokens);
        } catch (error) {
          const msg = error instanceof Error ? error.message : String(error);
          debug.log("batch-loop", `${callbacks.functionId} call ${totalLLMCalls} failed: ${msg}`);
          retryExhausted.push(...batchRetries);
          break;
        }
        const llmMs = Date.now() - llmStart;

        finishReasons.push(response.finishReason);
        lastResponseRenames = response.renames;

        if (response.finishReason === "length" && adaptiveBatchSize > 2) {
          adaptiveBatchSize = Math.max(2, Math.floor(adaptiveBatchSize / 2));
        }

        const validation = validateBatchRenames(
          response.renames,
          new Set(batchRetries),
          callbacks.getUsedNames()
        );
        lastValidation = validation;

        debug.validation(validation);

        let validThisCall = 0;
        const renameStart = Date.now();

        for (const [oldName, newName] of Object.entries(validation.valid)) {
          debug.rename({
            functionId: callbacks.functionId,
            oldName,
            newName,
            wasRetry: isRetry,
            attemptNumber: (idState.get(oldName)?.attempts ?? 0) + 1
          });

          callbacks.applyRename(oldName, newName);
          outcomes[oldName] = { status: "renamed", newName, round: totalLLMCalls };
          validThisCall++;
        }
        const renameMs = Date.now() - renameStart;

        debug.log("batch-timing", `${callbacks.functionId} call=${totalLLMCalls} prompt=${promptMs}ms llm=${llmMs}ms rename=${renameMs}ms valid=${validThisCall}/${batchRetries.length}`);

        // Classify failures and update per-identifier state
        const successes = new Set(Object.keys(validation.valid));
        const dupSet = new Set(validation.duplicates);
        const invSet = new Set(validation.invalid);
        const unchSet = new Set(validation.unchanged);
        const nextRetry: string[] = [];

        for (const name of batchRetries) {
          if (successes.has(name)) continue;

          const state = idState.get(name)!;
          if (response.renames[name]) {
            state.lastSuggestion = response.renames[name];
          }

          if (dupSet.has(name)) {
            state.lastFailureReason = "duplicate";
            const suggestedName = sanitizeIdentifier(response.renames[name] || "");
            if (suggestedName && callbacks.getUsedNames().has(suggestedName) && !usedNamesSnapshot.has(suggestedName)) {
              state.freeRetries++;
              if (state.freeRetries < maxFreeRetries) {
                nextRetry.push(name);
                continue;
              }
            } else {
              state.attempts++;
            }
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

          if (state.attempts < maxRetriesPerIdentifier) {
            nextRetry.push(name);
          } else {
            retryExhausted.push(name);
          }
        }

        const batchSizeBefore = batchRetries.length;
        batchRetries = nextRetry;

        // No progress at all → break inner retry loop to avoid infinite loop
        if (validThisCall === 0 && nextRetry.length === batchSizeBefore) {
          retryExhausted.push(...batchRetries);
          break;
        }
      }
    }

    // Straggler pass: one final attempt on all retryExhausted
    if (retryExhausted.length > 0) {
      const stragglers = retryExhausted.filter(name => !outcomes[name]);
      if (stragglers.length > 0) {
        debug.log("batch-loop", `${callbacks.functionId} straggler pass: ${stragglers.length} identifiers`);

        for (let i = 0; i < stragglers.length; i += adaptiveBatchSize) {
          const stragBatch = stragglers.slice(i, i + adaptiveBatchSize);
          totalLLMCalls++;

          const { prev, failures } = buildPrevAndFailures(stragBatch, idState);

          try {
            const request = callbacks.buildRequest(stragBatch, 2, prev, failures);
            const done = this.metrics?.llmCallStart();
            const response = await llm.suggestAllNames!(request);
            done?.();
            this.metrics?.recordTokens(response.usage?.totalTokens ?? 0, response.usage?.inputTokens, response.usage?.outputTokens);
            finishReasons.push(response.finishReason);

            const validation = validateBatchRenames(
              response.renames,
              new Set(stragBatch),
              callbacks.getUsedNames()
            );

            for (const [oldName, newName] of Object.entries(validation.valid)) {
              callbacks.applyRename(oldName, newName);
              outcomes[oldName] = { status: "renamed", newName, round: totalLLMCalls };
            }

            // Update idState with straggler suggestions
            for (const name of stragBatch) {
              if (response.renames[name]) {
                idState.get(name)!.lastSuggestion = response.renames[name];
              }
            }
          } catch (error) {
            const msg = error instanceof Error ? error.message : String(error);
            debug.log("batch-loop", `${callbacks.functionId} straggler batch failed: ${msg}`);
          }
        }
      }
    }

    // Compute final remaining set
    const remaining = new Set(identifierNames.filter(name => !outcomes[name]));

    // Universal fallback via resolveRemaining
    if (callbacks.resolveRemaining) {
      const combinedPrev: Record<string, string> = {};
      for (const name of remaining) {
        const state = idState.get(name);
        if (state?.lastSuggestion) combinedPrev[name] = state.lastSuggestion;
      }
      callbacks.resolveRemaining(remaining, combinedPrev, outcomes, totalLLMCalls);
    }

    // Derive final failures from idState (no redundant global tracking)
    const finalFailures: Failures = { duplicates: [], invalid: [], missing: [], unchanged: [] };
    const finalPreviousAttempt: Record<string, string> = {};
    for (const name of remaining) {
      const state = idState.get(name);
      if (state?.lastFailureReason === "duplicate") finalFailures.duplicates.push(name);
      else if (state?.lastFailureReason === "invalid") finalFailures.invalid.push(name);
      else if (state?.lastFailureReason === "unchanged") finalFailures.unchanged.push(name);
      else finalFailures.missing.push(name);
      if (state?.lastSuggestion) finalPreviousAttempt[name] = state.lastSuggestion;
    }

    // Record outcomes for remaining (unrenamed) identifiers
    for (const name of remaining) {
      callbacks.onUnrenamed?.(name);

      const state = idState.get(name)!;
      const totalAttempts = state.attempts + (state.freeRetries > 0 ? 1 : 0);

      if (state.lastFailureReason === "duplicate") {
        outcomes[name] = { status: "duplicate", conflictedWith: state.lastSuggestion || "unknown", attempts: totalAttempts, suggestion: state.lastSuggestion };
      } else if (state.lastFailureReason === "invalid") {
        outcomes[name] = { status: "invalid", attempts: totalAttempts, suggestion: state.lastSuggestion };
      } else if (state.lastFailureReason === "unchanged") {
        outcomes[name] = { status: "unchanged", attempts: totalAttempts, suggestion: state.lastSuggestion };
      } else {
        outcomes[name] = { status: "missing", attempts: totalAttempts, lastFinishReason: finishReasons[finishReasons.length - 1] };
      }

      const reason = outcomes[name].status === "duplicate"
        ? `duplicate (collided with ${state.lastSuggestion || "unknown"})`
        : outcomes[name].status === "invalid" ? "invalid identifier"
        : outcomes[name].status === "unchanged" ? "LLM returned original name"
        : "not returned by LLM";

      // Build rich context for debug log
      const usedSample = [...callbacks.getUsedNames()].slice(0, 50);
      const contextParts = [
        `lastPrompt(${lastUserPrompt.length}chars): ${lastUserPrompt.slice(0, 300)}`,
        `lastResponse: ${JSON.stringify(lastResponseRenames)}`,
        lastValidation ? `validation: valid=${Object.keys(lastValidation.valid).length} dup=${lastValidation.duplicates.length} inv=${lastValidation.invalid.length} miss=${lastValidation.missing.length} unch=${lastValidation.unchanged.length}` : "",
        `usedNames(${callbacks.getUsedNames().size} total, sample): ${usedSample.join(", ")}`
      ].filter(Boolean).join("\n");

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

    return { outcomes, finishReasons, remaining, totalLLMCalls, previousAttempt: finalPreviousAttempt, failures: finalFailures };
  }
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

  // Get all bindings in this function's scope
  for (const [name, binding] of Object.entries(scope.bindings)) {
    // Only include bindings that are declared in this function
    // (not in a parent scope)
    if (binding.scope === scope) {
      bindings.push({
        name,
        identifier: binding.identifier,
        scope: binding.scope
      });
    }
  }

  // When parameters have defaults/destructuring/rest, Babel creates a separate
  // scope for the function body. Check for body-scope bindings we missed.
  const bodyPath = fnPath.get("body");
  if (!Array.isArray(bodyPath) && bodyPath.isBlockStatement()) {
    const bodyScope = bodyPath.scope;
    if (bodyScope !== scope) {
      for (const [name, binding] of Object.entries(bodyScope.bindings)) {
        if (binding.scope === bodyScope && !bindings.some(b => b.name === name)) {
          bindings.push({ name, identifier: binding.identifier, scope: binding.scope });
        }
      }
    }
  }

  // Traverse nested block scopes to collect let/const bindings inside
  // for/while/if/try blocks that are owned by this function but live in
  // child block scopes.
  const seen = new Set(bindings.map(b => b.name));
  fnPath.traverse({
    // Skip into nested functions — their bindings belong to them, not us
    Function(path: NodePath<t.Function>) {
      if (path !== fnPath) path.skip();
    },
    BlockStatement(path: NodePath<t.BlockStatement>) {
      // Skip the function's own body block (already handled above)
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

  // Also include the function's own name if it's a named function expression
  if (fnPath.isFunctionExpression() || fnPath.isFunctionDeclaration()) {
    const id = fnPath.node.id;
    if (id) {
      // The function name binding is in the parent scope for declarations,
      // or in the function's own scope for named function expressions
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
  }

  return bindings;
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
    for (const element of node.elements) {
      if (element) collectParamNames(element, names);
    }
  } else if (t.isObjectPattern(node)) {
    for (const prop of node.properties) {
      if (t.isObjectProperty(prop)) {
        collectParamNames(prop.value, names);
      } else if (t.isRestElement(prop)) {
        collectParamNames(prop.argument, names);
      }
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
      bindings.push({ name, identifier: binding.identifier, scope: binding.scope });
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
    // Skip if identifier doesn't exist in expected set
    if (!expected.has(oldName)) {
      continue;
    }

    // Sanitize the name
    const newName = sanitizeIdentifier(rawNewName);

    // Track if LLM returned the original name back
    if (oldName === newName) {
      unchanged.push(oldName);
      continue;
    }

    // Skip if invalid syntax
    if (!isValidIdentifier(newName)) {
      invalid.push(oldName);
      continue;
    }

    // Skip if reserved word
    if (RESERVED_WORDS.has(newName)) {
      invalid.push(oldName);
      continue;
    }

    // Check for duplicates within this batch
    if (seenNewNames.has(newName)) {
      // Find and remove the previous mapping that used this name
      for (const [k, v] of Object.entries(valid)) {
        if (v === newName) {
          delete valid[k];
          duplicates.push(k);
          break;
        }
      }
      duplicates.push(oldName);
      continue;
    }

    // Check for conflict with existing used names
    if (usedNames.has(newName)) {
      duplicates.push(oldName);
      continue;
    }

    valid[oldName] = newName;
    seenNewNames.add(newName);
  }

  // Find missing identifiers (not in response at all) — use Sets for O(1) lookup
  const dupSet = new Set(duplicates);
  const invSet = new Set(invalid);
  const unchSet = new Set(unchanged);
  const missing = [...expected].filter(
    name => !valid[name] && !dupSet.has(name) && !invSet.has(name) && !unchSet.has(name)
  );

  return { valid, duplicates, invalid, missing, unchanged };
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

  const sorted = [...bindings].sort((a, b) => a.declarationLine - b.declarationLine);
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
  const failures: Failures = { duplicates: [], invalid: [], missing: [], unchanged: [] };
  for (const name of batch) {
    const state = idState.get(name)!;
    if (state.lastSuggestion) prev[name] = state.lastSuggestion;
    if (state.lastFailureReason === "duplicate") failures.duplicates.push(name);
    else if (state.lastFailureReason === "invalid") failures.invalid.push(name);
    else if (state.lastFailureReason === "missing") failures.missing.push(name);
    else if (state.lastFailureReason === "unchanged") failures.unchanged.push(name);
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
    if (!isValidIdentifier(sanitized) || RESERVED_WORDS.has(sanitized)) continue;
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
      outcomes[name] = { status: "renamed", newName: resolved, round: totalLLMCalls + 1 };
    } else {
      applyRename(name, sanitized);
      remaining.delete(name);
      outcomes[name] = { status: "renamed", newName: sanitized, round: totalLLMCalls + 1 };
    }
  }
}
