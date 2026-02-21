import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import type {
  FunctionNode,
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

/** Maximum number of retry attempts when LLM suggests a conflicting name */
const MAX_NAME_RETRIES = 9;

/** Maximum identifiers per LLM batch (adaptive — halved on truncation) */
const MAX_BATCH_SIZE = 10;

/** Maximum number of LLM rounds (covers both batching and retries) */
const MAX_ROUNDS = 5;

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

  /** Per-function rename reports (populated after processAll completes) */
  get reports(): ReadonlyArray<FunctionRenameReport> { return this._reports; }

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
    const { concurrency = 50, onProgress, metrics, preDone } = options;

    // Store metrics for use in processFunction
    this.metrics = metrics;

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

    while (this.ready.size > 0 || this.processing.size > 0) {
      // Dispatch all ready items up to concurrency limit
      for (const fn of [...this.ready]) {
        this.ready.delete(fn);
        this.processing.add(fn);
        fn.status = "processing";
        metrics?.functionStarted();
        inFlightCount++;

        limit(async () => {
          try {
            await this.processFunction(fn, llm);
          } finally {
            this.processing.delete(fn);
            this.done.add(fn);
            fn.status = "done";
            metrics?.functionCompleted();

            const newlyReady = this.checkNewlyReady(fn, dependents);
            if (metrics && newlyReady > 0) {
              metrics.functionsReady(newlyReady);
            }

            this.reportProgress(functions, onProgress);

            // Signal the main loop that a task completed
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

      // Wait for at least one to complete if nothing is ready
      if (this.ready.size === 0 && this.processing.size > 0) {
        await new Promise<void>(resolve => { notifyCompletion = resolve; });
      }

      // Mid-loop deadlock breaking: if nothing is ready or processing but
      // functions remain, scopeParent chains are blocking — relax the constraint.
      if (this.ready.size === 0 && this.processing.size === 0) {
        const newlyReady = this.checkNewlyReadyRelaxed(functions);
        if (newlyReady > 0) {
          debug.log("processor", `Breaking scopeParent deadlock: ${newlyReady} functions unblocked`);
          if (metrics) {
            metrics.functionsReady(newlyReady);
          }
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
   * Process a single function: get LLM suggestions and apply renames.
   * Uses batch renaming when available for better semantic understanding.
   */
  private async processFunction(
    fn: FunctionNode,
    llm: LLMProvider
  ): Promise<void> {
    const allBindings = getOwnBindings(fn.path);

    // If no bindings to rename, skip
    if (allBindings.length === 0) {
      fn.renameMapping = { names: {} };
      return;
    }

    // Filter out identifiers that already have descriptive names
    const bindings = allBindings.filter(b => looksMinified(b.name));

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
   */
  private async processFunctionBatched(
    fn: FunctionNode,
    llm: LLMProvider,
    bindings: BindingInfo[]
  ): Promise<void> {
    const context = buildContext(fn, this.ast);
    const renameMapping: Record<string, string> = {};
    const bindingMap = new Map(bindings.map(b => [b.name, b]));

    // Remove minified names we're about to rename from usedIdentifiers —
    // they'll be replaced, so the LLM shouldn't avoid them and conflict
    // detection shouldn't reject new names that happen to match them.
    for (const b of bindings) {
      context.usedIdentifiers.delete(b.name);
    }

    // Report tracking
    const outcomes: Record<string, IdentifierOutcome> = {};
    const finishReasons: (string | undefined)[] = [];

    let remaining = new Set(bindings.map(b => b.name));
    let round = 0;
    let maxBatchSize = MAX_BATCH_SIZE;
    let previousAttempt: Record<string, string> = {};
    let failures: { duplicates: string[]; invalid: string[]; missing: string[] } = { duplicates: [], invalid: [], missing: [] };

    while (remaining.size > 0 && round < MAX_ROUNDS) {
      round++;

      // Regenerate code from AST (reflects any renames applied in previous rounds)
      const code = generate(fn.path.node).code;

      // Batch: take up to maxBatchSize identifiers
      const batch = [...remaining].slice(0, maxBatchSize);

      // Build the batch request
      const request: BatchRenameRequest = {
        code,
        identifiers: batch,
        usedNames: context.usedIdentifiers,
        calleeSignatures: context.calleeSignatures,
        callsites: context.callsites,
        isRetry: round > 1,
        previousAttempt: round > 1 ? previousAttempt : undefined,
        failures: round > 1 ? failures : undefined
      };

      // Ask LLM for batch renames
      const done = this.metrics?.llmCallStart();
      const response = await llm.suggestAllNames!(request);
      done?.();

      finishReasons.push(response.finishReason);

      // If response was truncated, halve batch size for next round
      if (response.finishReason === "length" && maxBatchSize > 2) {
        maxBatchSize = Math.max(2, Math.floor(maxBatchSize / 2));
      }

      // Validate and categorize the response
      const validation = validateBatchRenames(
        response.renames,
        new Set(batch),
        context.usedIdentifiers
      );

      // Log validation results in debug mode
      debug.validation(validation);

      let validThisRound = 0;

      // Apply valid renames immediately
      for (const [oldName, newName] of Object.entries(validation.valid)) {
        const binding = bindingMap.get(oldName);
        if (binding) {
          // Track for source map BEFORE renaming
          const loc = binding.identifier.loc;
          if (loc) {
            this.allRenames.push({
              originalPosition: { line: loc.start.line, column: loc.start.column },
              originalName: oldName,
              newName,
              functionId: fn.sessionId
            });
          }

          // Log rename operation in debug mode
          debug.rename({
            functionId: fn.sessionId,
            oldName,
            newName,
            wasRetry: round > 1,
            attemptNumber: round
          });

          // Apply rename to AST — use the binding's own scope, not the
          // function scope, because block-scoped vars (let/const in for loops,
          // catch clauses, etc.) live in child scopes that fn.path.scope
          // can't reach.
          binding.scope.rename(oldName, newName);
          context.usedIdentifiers.add(newName);
          renameMapping[oldName] = newName;
          remaining.delete(oldName);
          outcomes[oldName] = { status: "renamed", newName, round };
          validThisRound++;
        }
      }

      // Track for retry context
      previousAttempt = response.renames;
      failures = {
        duplicates: validation.duplicates,
        invalid: validation.invalid,
        missing: validation.missing
      };

      // Everything not successfully renamed stays in remaining
      // (duplicates, invalid, and missing are already still in remaining)

      // If no progress was made this round, stop — LLM is stuck
      if (validThisRound === 0) {
        break;
      }
    }

    // Record outcomes for remaining (unrenamed) identifiers
    for (const name of remaining) {
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

        // Determine why it failed
        if (failures.duplicates.includes(name)) {
          outcomes[name] = { status: "duplicate", conflictedWith: previousAttempt[name] || "unknown", rounds: round };
        } else if (failures.invalid.includes(name)) {
          outcomes[name] = { status: "invalid", rounds: round };
        } else {
          outcomes[name] = { status: "missing", rounds: round, lastFinishReason: finishReasons[finishReasons.length - 1] };
        }
      }
    }

    fn.renameMapping = { names: renameMapping };
    fn.renameReport = {
      functionId: fn.sessionId,
      totalIdentifiers: bindings.length,
      renamedCount: bindings.length - remaining.size,
      outcomes,
      rounds: round,
      finishReasons
    };
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
      newName = resolveConflict(newName, context.usedIdentifiers);
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
 * Checks if a name looks minified (1-2 characters, not common short names).
 */
function looksMinified(name: string): boolean {
  if (name.length > 2) return false;
  const commonShort = new Set(["id", "fn", "cb", "el", "db", "io", "fs", "os", "vm", "ip"]);
  return !commonShort.has(name);
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
 * Creates a simple concurrency limiter.
 */
function createConcurrencyLimiter(
  concurrency: number
): <T>(fn: () => Promise<T>) => Promise<T> {
  let running = 0;
  const queue: Array<{
    fn: () => Promise<unknown>;
    resolve: (value: unknown) => void;
    reject: (error: unknown) => void;
  }> = [];

  async function run<T>(fn: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      queue.push({ fn, resolve: resolve as (v: unknown) => void, reject });
      processQueue();
    });
  }

  function processQueue(): void {
    while (running < concurrency && queue.length > 0) {
      const item = queue.shift()!;
      running++;

      item
        .fn()
        .then((result) => {
          running--;
          item.resolve(result);
          processQueue();
        })
        .catch((error) => {
          running--;
          item.reject(error);
          processQueue();
        });
    }
  }

  return run;
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
  const seenNewNames = new Set<string>();

  for (const [oldName, rawNewName] of Object.entries(renames)) {
    // Skip if identifier doesn't exist in expected set
    if (!expected.has(oldName)) {
      continue;
    }

    // Sanitize the name
    const newName = sanitizeIdentifier(rawNewName);

    // Skip if same as original
    if (oldName === newName) {
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

  // Find missing identifiers (not in response at all)
  const missing = [...expected].filter(
    name => !valid[name] && !duplicates.includes(name) && !invalid.includes(name)
  );

  return { valid, duplicates, invalid, missing };
}
