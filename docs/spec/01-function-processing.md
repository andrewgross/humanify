# Function Processing

## Overview

Functions are processed in dependency order using a ready queue. Leaf functions (those that call no internal functions) are processed first, then functions that only depend on completed functions, and so on.

## Function Dependency Graph

### Building the Graph

```typescript
interface FunctionNode {
  // Stable identity within a session (position-based)
  sessionId: string;  // e.g., "file.js:142:5"

  // Content-based fingerprint for caching and cross-version matching
  fingerprint: FunctionFingerprint;  // See spec 12

  // Babel reference
  path: NodePath<t.Function>;

  // Dependencies
  internalCallees: Set<FunctionNode>;  // Functions in our code this calls
  externalCallees: Set<string>;         // Library/builtin calls

  // Reverse dependencies (who calls this)
  callers: Set<FunctionNode>;

  // Scope parent: the immediately enclosing function.
  // Used for processing order (child waits for parent) but NOT for
  // fingerprinting — scopeParent is a separate dependency axis.
  scopeParent?: FunctionNode;

  // Processing state
  status: 'pending' | 'processing' | 'done';
  renameMapping?: RenameMapping;

  // Pre-computed call site info (avoids repeated AST traversals)
  callSites: CallSiteInfo[];

  // Per-identifier rename report (populated after processing)
  renameReport?: FunctionRenameReport;
}
```

### Identifying Internal vs External Calls

```typescript
function analyzeCallees(fnPath: NodePath<t.Function>, allFunctions: Map<string, FunctionNode>) {
  const internal = new Set<FunctionNode>();
  const external = new Set<string>();

  fnPath.traverse({
    CallExpression(callPath) {
      const callee = callPath.node.callee;

      if (t.isIdentifier(callee)) {
        const binding = callPath.scope.getBinding(callee.name);

        if (binding && isFunctionBinding(binding)) {
          // Check if this function is in our graph
          const targetId = getSessionId(binding.path);
          const targetFn = allFunctions.get(targetId);

          if (targetFn) {
            internal.add(targetFn);
          } else {
            external.add(callee.name);
          }
        } else {
          // Global or unbound - treat as external
          external.add(callee.name);
        }
      }
      // Handle member expressions, etc.
    }
  });

  return { internal, external };
}
```

## Ready Queue Processing

### Core Algorithm

```typescript
class RenameProcessor {
  private ready = new Set<FunctionNode>();
  private processing = new Set<FunctionNode>();
  private done = new Set<FunctionNode>();
  private allRenames: RenameDecision[] = [];
  private failedCount = 0;

  async processAll(
    functions: FunctionNode[],
    llm: LLMProvider,
    options: ProcessorOptions = {}
  ): Promise<RenameDecision[]> {
    const { concurrency = 50, onProgress, metrics, preDone } = options;

    // Pre-seed done set with already-completed functions
    // (e.g., library functions from mixed file detection)
    if (preDone) {
      for (const fn of preDone) {
        this.done.add(fn);
      }
    }

    // Initialize: find functions whose dependencies are all satisfied.
    // With preDone, this also catches app functions whose only callees
    // are already-done library functions.
    for (const fn of functions) {
      if (this.isReady(fn)) {
        this.ready.add(fn);
      }
    }

    // Deadlock breaker: if nothing is ready, scopeParent chains are
    // blocking everything. Retry without scopeParent constraint.
    if (this.ready.size === 0 && functions.length > 0) {
      for (const fn of functions) {
        if (this.isReadyIgnoringScopeParent(fn)) {
          this.ready.add(fn);
        }
      }
    }

    // Build reverse-dependency map for efficient newly-ready checks
    const dependents = buildDependentsMap(functions);
    const limit = createConcurrencyLimiter(concurrency);

    while (this.ready.size > 0 || this.processing.size > 0) {
      for (const fn of [...this.ready]) {
        this.ready.delete(fn);
        this.processing.add(fn);
        fn.status = "processing";

        limit(async () => {
          try {
            await this.processFunction(fn, llm);
          } catch (error) {
            // Log and skip — don't crash the whole run for one function
            this.failedCount++;
            if (!fn.renameMapping) {
              fn.renameMapping = { names: {} };
            }
          } finally {
            this.processing.delete(fn);
            this.done.add(fn);
            fn.status = "done";
            this.checkNewlyReady(fn, dependents);
          }
        });
      }

      // Wait for at least one to complete if nothing is ready
      if (this.ready.size === 0 && this.processing.size > 0) {
        await waitForCompletion();
      }

      // Mid-loop deadlock breaking: if nothing is ready or processing
      // but functions remain, relax scopeParent constraints.
      if (this.ready.size === 0 && this.processing.size === 0) {
        this.checkNewlyReadyRelaxed(functions);
      }
    }

    return this.allRenames;
  }

  private isReady(fn: FunctionNode): boolean {
    for (const callee of fn.internalCallees) {
      if (!this.done.has(callee)) return false;
    }
    // Also wait for scope parent (proper variable renaming order)
    if (fn.scopeParent && !this.done.has(fn.scopeParent)) {
      return false;
    }
    return true;
  }

  private isReadyIgnoringScopeParent(fn: FunctionNode): boolean {
    for (const callee of fn.internalCallees) {
      if (!this.done.has(callee)) return false;
    }
    return true;
  }

  private checkNewlyReady(
    completedFn: FunctionNode,
    dependents: Map<FunctionNode, FunctionNode[]>
  ): number {
    // Only check direct dependents of the completed function
    const deps = dependents.get(completedFn);
    if (!deps) return 0;
    let count = 0;
    for (const fn of deps) {
      if (!this.done.has(fn) && !this.processing.has(fn) && !this.ready.has(fn)) {
        if (this.isReady(fn)) {
          this.ready.add(fn);
          count++;
        }
      }
    }
    return count;
  }
}
```

```typescript
interface ProcessorOptions {
  /** Maximum number of functions to process in parallel */
  concurrency?: number;

  /** Progress callback */
  onProgress?: ProgressCallback;

  /** Metrics tracker for detailed observability */
  metrics?: MetricsTracker;

  /**
   * Functions to treat as already completed (e.g., library functions,
   * or previously-checkpointed functions on resume).
   * Added to the done set before processing begins so dependents
   * can become ready.
   */
  preDone?: FunctionNode[];

  /**
   * Called after each function completes (for checkpoint persistence).
   */
  onFunctionDone?: (fn: FunctionNode) => void;
}
```

### Why Immediate Rename Application is Safe

Each function only renames its **own bindings**:
- Parameters declared in the function signature
- Variables declared inside the function body (var, let, const)
- The function's own name (if it's a named function expression)

Parent scope variables are NOT renamed - they belong to the parent function.

```javascript
function outer() {
  const x = 1;  // Renamed when processing `outer`

  function inner(y) {  // `y` renamed when processing `inner`
    const z = 2;       // `z` renamed when processing `inner`
    return x + y + z;  // `x` reference updated when `outer` renames it
  }
}
```

Since parallel functions operate on disjoint binding sets, there are no conflicts.

## Context Extraction

When processing a function, we extract context to help the LLM:

```typescript
interface LLMContext {
  // The function being processed (current minified names)
  functionCode: string;

  // Functions this calls (already humanified)
  calleeSignatures: Array<{
    name: string;           // Humanified name
    params: string[];       // Humanified param names
    snippet: string;        // First few lines of body
  }>;

  // Where this function is called from (still minified)
  callsites: string[];

  // Names already used in scope (to avoid conflicts)
  usedIdentifiers: Set<string>;
}

function extractContext(fn: FunctionNode, ast: t.File): LLMContext {
  return {
    // Generate code from current AST state (includes previous renames)
    functionCode: generate(fn.path.node).code,

    calleeSignatures: [...fn.internalCallees].map(callee => ({
      name: callee.path.node.id?.name || 'anonymous',
      params: callee.path.node.params.map(p => generate(p).code),
      snippet: getFirstLines(callee.path.node.body, 3)
    })),

    callsites: findCallsites(fn, ast).map(site => generate(site.node).code),

    usedIdentifiers: new Set(fn.path.scope.getAllBindings().keys())
  };
}
```

## Tracking Renames for Source Maps

Every rename is tracked with its original position:

```typescript
interface RenameDecision {
  // Original position in source (for source map)
  originalPosition: { line: number; column: number };

  // The binding path
  path: NodePath<t.Identifier>;

  // Names
  originalName: string;
  newName: string;

  // Metadata
  functionId: string;
  confidence?: number;
}
```

After each function's dependencies are met, it's processed via batch renaming (see [spec 11](./11-batched-renaming.md) for details):

```typescript
async function processFunction(fn: FunctionNode, llm: LLMProvider) {
  const bindings = getOwnBindings(fn.path).filter(b => looksMinified(b.name));
  if (bindings.length === 0) {
    fn.renameMapping = { names: {} };
    return;
  }

  // Batch mode: ask LLM for all names at once
  if (llm.suggestAllNames) {
    await processFunctionBatched(fn, llm, bindings);
  } else {
    await processFunctionSequential(fn, llm, bindings);
  }
}
```

**Batched renaming** (the primary path) sends all minified identifiers in a single LLM call. After each round, valid renames are applied to the AST immediately and the code is regenerated — remaining identifiers are re-sent with updated context. This progressive approach gives the LLM better signal on each retry round. See [spec 11](./11-batched-renaming.md) for the full algorithm.

## Error Resilience

The processor uses a catch-and-continue pattern: if a single function fails (LLM timeout, API error, malformed response), it is logged and skipped rather than crashing the entire run.

```typescript
try {
  await this.processFunction(fn, llm);
} catch (error) {
  // Log and skip — don't crash the whole run for one function
  this.failedCount++;
  fn.renameMapping = { names: {} };  // empty mapping = keep minified names
}
```

Key behaviors:
- **Failed functions get empty mappings**: Their minified names are preserved in the output. This is better than crashing — the user gets partial results.
- **`failedCount` tracking**: The processor exposes a `failed` count so callers can report how many functions were skipped.
- **Dependents still unblock**: Failed functions are marked `done` so their dependents don't get stuck waiting.
- **API retry logic**: Individual LLM calls are retried at the HTTP level (via `--retries` flag, default 3) before the function-level catch fires. The catch handles errors that persist after all retries.

## Handling Cycles

In rare cases, functions may have circular dependencies:

```javascript
function a() { return b(); }
function b() { return a(); }
```

Detection:
```typescript
function detectCycles(functions: FunctionNode[]): FunctionNode[][] {
  // Tarjan's algorithm or similar
  // Returns strongly connected components
}
```

Resolution options:
1. **Process together** - Treat the cycle as a single unit, process all functions in it at once
2. **Break arbitrarily** - Pick one function to process first, accept slightly degraded context
3. **User prompt** - Ask user which function to prioritize

For MVP, option 2 is simplest - just add one of the cycle members to the ready queue and proceed.

## Progress Reporting

```typescript
interface ProcessingProgress {
  total: number;
  done: number;
  processing: number;
  ready: number;
  pending: number;

  currentFunction?: string;
  estimatedTimeRemaining?: number;
}

// Callback on each state change
onProgress?: (progress: ProcessingProgress) => void;
```
