import type { NodePath } from "@babel/core";
import * as t from "@babel/types";
import type {
  FunctionNode,
  RenameDecision,
  ProcessorOptions,
  ProcessingProgress
} from "../analysis/types.js";
import { buildContext } from "./context-builder.js";
import type { LLMProvider } from "../llm/types.js";
import {
  sanitizeIdentifier,
  resolveConflict,
  validateSuggestion,
  RESERVED_WORDS
} from "../llm/validation.js";

// Re-export LLMProvider for backward compatibility
export type { LLMProvider } from "../llm/types.js";

/** Maximum number of retry attempts when LLM suggests a conflicting name */
const MAX_NAME_RETRIES = 9;

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
    const { concurrency = 50, onProgress, metrics } = options;

    // Initialize metrics if provided
    if (metrics) {
      metrics.setFunctionTotal(functions.length);
    }

    // Initialize: find functions with no internal dependencies (leaves)
    let initialReady = 0;
    for (const fn of functions) {
      if (this.isReady(fn)) {
        this.ready.add(fn);
        initialReady++;
      }
    }

    // Update metrics with initial ready count
    if (metrics && initialReady > 0) {
      metrics.functionsReady(initialReady);
    }

    this.reportProgress(functions, onProgress);

    const limit = createConcurrencyLimiter(concurrency);
    const pending = new Set<Promise<void>>();

    while (this.ready.size > 0 || this.processing.size > 0) {
      // Dispatch all ready items up to concurrency limit
      for (const fn of [...this.ready]) {
        this.ready.delete(fn);
        this.processing.add(fn);
        fn.status = "processing";
        metrics?.functionStarted();

        const promise = limit(async () => {
          try {
            await this.processFunction(fn, llm);
          } finally {
            this.processing.delete(fn);
            this.done.add(fn);
            fn.status = "done";
            metrics?.functionCompleted();

            const newlyReady = this.checkNewlyReady(functions);
            if (metrics && newlyReady > 0) {
              metrics.functionsReady(newlyReady);
            }

            this.reportProgress(functions, onProgress);
          }
        });

        // Track promise and remove when done
        pending.add(promise);
        promise.finally(() => pending.delete(promise));
      }

      // Wait for at least one to complete if nothing is ready
      if (this.ready.size === 0 && this.processing.size > 0 && pending.size > 0) {
        await Promise.race([...pending]);
      }
    }

    // Wait for all remaining to complete
    await Promise.all([...pending]);

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
    return true;
  }

  /**
   * Check for functions that are newly ready after a completion.
   * Returns the count of newly ready functions.
   */
  private checkNewlyReady(allFunctions: FunctionNode[]): number {
    let count = 0;
    for (const fn of allFunctions) {
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
   * Process a single function: get LLM suggestions and apply renames.
   */
  private async processFunction(
    fn: FunctionNode,
    llm: LLMProvider
  ): Promise<void> {
    const context = buildContext(fn, this.ast);
    const bindings = getOwnBindings(fn.path);
    const renameMapping: Record<string, string> = {};

    for (const binding of bindings) {
      const newName = await this.suggestNameWithRetry(
        binding.name,
        context,
        llm
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

      // Apply rename to AST immediately
      fn.path.scope.rename(binding.name, newName);
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
    context: { usedIdentifiers: Set<string>; functionCode: string; calleeSignatures: Array<{ name: string; params: string[]; snippet?: string }>; callsites: string[] },
    llm: LLMProvider
  ): Promise<string> {
    let suggestion = await llm.suggestName(currentName, context);
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
 * Binding info with the identifier node and its location.
 */
interface BindingInfo {
  name: string;
  identifier: t.Identifier;
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
        identifier: binding.identifier
      });
    }
  }

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
          identifier: nameBinding.identifier
        });
      }
    }
  }

  return bindings;
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
