# Function Processing

## Overview

Functions are processed in dependency order using a ready queue. Leaf functions (those that call no internal functions) are processed first, then functions that only depend on completed functions, and so on.

## Function Dependency Graph

### Building the Graph

```typescript
interface FunctionNode {
  // Stable identity within a session (position-based)
  sessionId: string;  // e.g., "file.js:142:5"

  // Stable identity across versions (content-based)
  structuralHash: string;

  // Babel reference
  path: NodePath<t.Function>;

  // Dependencies
  internalCallees: Set<FunctionNode>;  // Functions in our code this calls
  externalCallees: Set<string>;         // Library/builtin calls

  // Reverse dependencies (who calls this)
  callers: Set<FunctionNode>;

  // Processing state
  status: 'pending' | 'processing' | 'done';
  renameMapping?: RenameMapping;
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
  private allRenames: RenameDecision[] = [];  // For source map

  async processAll(
    functions: FunctionNode[],
    llm: LLMProvider,
    options: { concurrency?: number } = {}
  ) {
    const { concurrency = 10 } = options;
    const limit = pLimit(concurrency);

    // Initialize: find functions with no internal dependencies
    for (const fn of functions) {
      if (this.isReady(fn)) {
        this.ready.add(fn);
      }
    }

    const inFlight: Promise<void>[] = [];

    while (this.ready.size > 0 || this.processing.size > 0) {
      // Dispatch all ready items
      for (const fn of [...this.ready]) {
        this.ready.delete(fn);
        this.processing.add(fn);

        const promise = limit(async () => {
          await this.processFunction(fn, llm);
          this.processing.delete(fn);
          this.done.add(fn);
          this.checkNewlyReady(functions);
        });

        inFlight.push(promise);
      }

      // Wait for at least one to complete
      if (this.ready.size === 0 && this.processing.size > 0) {
        await Promise.race(inFlight);
      }
    }

    await Promise.all(inFlight);
  }

  private isReady(fn: FunctionNode): boolean {
    return [...fn.internalCallees].every(callee => this.done.has(callee));
  }

  private checkNewlyReady(allFunctions: FunctionNode[]) {
    for (const fn of allFunctions) {
      if (!this.done.has(fn) && !this.processing.has(fn) && !this.ready.has(fn)) {
        if (this.isReady(fn)) {
          this.ready.add(fn);
        }
      }
    }
  }
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

After each LLM call completes:

```typescript
async function processFunction(fn: FunctionNode, llm: LLMProvider) {
  const context = extractContext(fn, this.ast);
  const bindings = getOwnBindings(fn.path);

  for (const binding of bindings) {
    const suggestion = await llm.suggestName(binding.name, context);

    // Track for source map BEFORE renaming
    this.allRenames.push({
      originalPosition: binding.path.node.loc.start,
      path: binding.path,
      originalName: binding.name,
      newName: suggestion.name,
      functionId: fn.sessionId,
      confidence: suggestion.confidence
    });

    // Apply rename to AST immediately
    fn.path.scope.rename(binding.name, suggestion.name);
  }

  fn.status = 'done';
}
```

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
