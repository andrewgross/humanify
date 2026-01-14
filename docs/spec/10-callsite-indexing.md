# Call Site Indexing

## Problem

The current `buildContext` function traverses the entire AST for every function to find call sites. With 4500 functions and a large AST, this is O(n × m) and takes **7+ minutes**.

```
Profile results (1.8MB file, 4518 functions):
- Full AST traverse: 98ms per call
- 4518 functions × 98ms = 443 seconds = 7.4 minutes
```

## Solution

Build a call site index once during the initial analysis phase, then use O(1) lookups.

```
Current: O(n × m)
  for each function:
    traverse(entire_ast)  // 98ms

Fixed: O(m + n)
  traverse(entire_ast) once → Map<FunctionNode, CallSite[]>
  for each function:
    callsites = index.get(fn)  // O(1)
```

## Implementation

### New Type

```typescript
// src/analysis/types.ts

interface CallSiteInfo {
  /** The code of the call expression */
  code: string;
  /** Location in source for debugging */
  line: number;
  column: number;
}
```

### Add to FunctionNode

```typescript
interface FunctionNode {
  // ... existing fields

  /** Call sites where this function is invoked (populated during graph building) */
  callSites: CallSiteInfo[];
}
```

### Collect During Graph Building

Modify `buildFunctionGraph` to collect call sites in the same traversal that builds the function graph:

```typescript
// src/analysis/function-graph.ts

export function buildFunctionGraph(ast: t.File, filename: string): FunctionNode[] {
  const functions: FunctionNode[] = [];
  const nodeToFunction = new Map<t.Function, FunctionNode>();
  const callSitesByTarget = new Map<t.Function, CallSiteInfo[]>();

  // First pass: collect all functions
  traverse(ast, {
    Function(path) {
      const fn = createFunctionNode(path, filename);
      functions.push(fn);
      nodeToFunction.set(path.node, fn);
    }
  });

  // Second pass: collect dependencies AND call sites
  traverse(ast, {
    CallExpression(path) {
      const callee = path.node.callee;
      if (!t.isIdentifier(callee)) return;

      const binding = path.scope.getBinding(callee.name);
      if (!binding) return;

      const targetNode = getBindingFunctionNode(binding);
      if (!targetNode) return;

      const targetFn = nodeToFunction.get(targetNode);
      if (!targetFn) return;

      // Record call site
      const callSites = callSitesByTarget.get(targetNode) || [];
      callSites.push({
        code: generateCode(path.node),
        line: path.node.loc?.start.line ?? 0,
        column: path.node.loc?.start.column ?? 0
      });
      callSitesByTarget.set(targetNode, callSites);

      // Record dependency (existing logic)
      const callerFn = findEnclosingFunction(path, nodeToFunction);
      if (callerFn && callerFn !== targetFn) {
        callerFn.internalCallees.add(targetFn);
        targetFn.callers.add(callerFn);
      }
    }
  });

  // Attach call sites to function nodes
  for (const fn of functions) {
    fn.callSites = callSitesByTarget.get(fn.path.node) || [];
    // Limit to 5 call sites to avoid huge prompts
    if (fn.callSites.length > 5) {
      fn.callSites = fn.callSites.slice(0, 5);
    }
  }

  return functions;
}
```

### Simplify Context Builder

Remove `findCallsites` function entirely:

```typescript
// src/rename/context-builder.ts

export function buildContext(fn: FunctionNode, ast: t.File): LLMContext {
  return {
    functionCode: generateCode(fn.path.node),
    calleeSignatures: getCalleeSignatures(fn),
    callsites: fn.callSites.map(cs => cs.code),  // Just use pre-computed data
    usedIdentifiers: getUsedIdentifiers(fn.path)
  };
}

// DELETE: findCallsites function (lines 99-137)
// DELETE: getFunctionNames function (lines 139-166)
// DELETE: getBindingFunctionNode function (lines 168-188)
```

## Expected Performance

| Metric | Before | After |
|--------|--------|-------|
| Call site collection | 98ms × 4518 = 443s | ~1s (single pass) |
| Per-function context build | 70ms | <1ms |
| Total for 4518 functions | ~7 minutes | ~2 seconds |

## Testing

Update existing tests to verify:
1. `FunctionNode.callSites` is populated correctly
2. Call sites are limited to 5 per function
3. `buildContext` returns correct call site strings

```typescript
describe("call site indexing", () => {
  it("populates callSites on function nodes", () => {
    const code = `
      function target() { return 1; }
      function caller1() { return target(); }
      function caller2() { return target() + target(); }
    `;
    const ast = parseSync(code, { sourceType: "module" })!;
    const functions = buildFunctionGraph(ast, "test.js");

    const target = functions.find(f => getFunctionName(f) === "target")!;
    expect(target.callSites.length).toBe(3); // Called 3 times total
  });

  it("limits call sites to 5", () => {
    const calls = Array(10).fill("target()").join("; ");
    const code = `
      function target() {}
      function caller() { ${calls} }
    `;
    const ast = parseSync(code, { sourceType: "module" })!;
    const functions = buildFunctionGraph(ast, "test.js");

    const target = functions.find(f => getFunctionName(f) === "target")!;
    expect(target.callSites.length).toBe(5);
  });
});
```

## Migration

This is a breaking change to the internal API. No backwards compatibility needed since:
1. `FunctionNode` is internal
2. `buildContext` is internal
3. No external consumers

Simply delete the old code and add the new field.
