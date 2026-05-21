# Bun wrapper IIFE causes 25x slowdown due to function declarations becoming module bindings

## Summary

When processing Bun-compiled binaries (e.g. claude-code 2.1.116+), humanify is ~25x slower than on the equivalent esbuild-bundled code. A run that normally takes 6-8 hours has an ETA of ~200 hours. The root cause is that Bun wraps the entire bundle in a CJS IIFE, which causes `shouldSkipBinding()` to treat ~16K function declarations as module bindings instead of processing them as `FunctionNode`s in the function graph.

## The two bundle formats

### esbuild (old, fast) -- top-level ESM

```js
#!/usr/bin/env node
// Version: 2.1.87

var u = (q, K) => () => (K || q((K = { exports: {} }).exports, K), K.exports);
var y = (q, K) => () => (q && (K = q((q = 0))), K);

// ~14,726 var declarations at PROGRAM scope
var K27 = y(() => {
  q27 = M15;
});
var nJ6 = y(() => {
  JB = D15;
});

// ~14,498 function declarations at PROGRAM scope
function P15(q, K) {
  var _ = q.length;
  while (_--) if (JB(q[_][0], K)) return _;
}
```

Everything is at **program scope**. Function declarations are detected by `shouldSkipBinding()` and skipped from the module binding pool -- they get processed as `FunctionNode`s instead, each with focused context and independent LLM calls.

### Bun (new, slow) -- CJS wrapper IIFE

```js
// @bun @bytecode @bun-cjs
(function (exports, require, module, __filename, __dirname) {
  var Q = (H, _) => () => (_ || H((_ = { exports: {} }).exports, _), _.exports);
  var Z = (H, _) => () => (H && (_ = H((H = 0))), _);

  // ~16,264 var declarations inside WRAPPER scope
  var Zx8 = Q((gh3, RbK) => {
    RbK.exports = require("/$bunfs/root/image-processor.node");
  });

  // ~16,172 function declarations inside WRAPPER scope
  function WbK(H) {
    return typeof H == "function" ? H : void 0;
  }
}); // end wrapper
```

Everything is inside a **single wrapper IIFE**. The wrapper detection in `findWrapperFunction()` correctly identifies this pattern and switches the target scope from programScope to the wrapper's scope. But then `shouldSkipBinding()` **stops skipping function declarations** because it's in wrapper mode.

## The bug: `shouldSkipBinding()` in wrapper mode

**File:** `src/rename/plugin.ts`, lines 848-874

```typescript
function shouldSkipBinding(
  bindingPath: babelTraverse.NodePath,
  wrapper: WrapperFunctionResult | null
): boolean {
  // Skip function/class declarations when NOT in wrapper mode
  if (!wrapper) {
    // <-- THIS IS THE PROBLEM
    if (
      bindingPath.isFunctionDeclaration() ||
      bindingPath.isClassDeclaration()
    ) {
      return true;
    }
  }
  // ...
}
```

When `wrapper` is non-null (Bun format), **no function declarations are skipped**. All ~16K function declarations become `ModuleBindingNode`s instead of being processed as `FunctionNode`s in the function graph.

## Impact by the numbers

| Metric                    | esbuild (2.1.81 run) | Bun (2.1.120 run) | Delta      |
| ------------------------- | -------------------- | ----------------- | ---------- |
| Total items               | 67,280               | 90,676            | +35%       |
| Functions (FunctionNode)  | 54,015               | 59,537            | +10%       |
| **Module bindings**       | **13,265**           | **31,140**        | **+135%**  |
| Wrapper function bindings | 0                    | 31,146            | --         |
| Median LLM call time      | 10s                  | 128s              | 13x        |
| P90 LLM call time         | 74s                  | 2,134s            | 29x        |
| Throughput                | 8,205 items/hr       | 447 items/hr      | 18x slower |

The module binding count more than doubled (13K -> 31K). The extra ~18K bindings are function declarations that should have been `FunctionNode`s.

## Why extra module bindings are so much worse than extra FunctionNodes

1. **Batching overhead**: Module bindings are renamed in batches of 10 (or 15 for esbuild). Each batch is a single LLM call. 31K bindings = ~3,100 batch calls, vs ~1,300 with 13K bindings.

2. **Inflated prompts**: Each batch includes a `usedNames` collision-avoidance list (`getProximateUsedNames()` at plugin.ts:1017). With 31K bindings packed densely in the wrapper scope, the proximity window (+-100 lines) captures hundreds of names per batch, making prompts much larger.

3. **Less context per identifier**: A `FunctionNode` gets its own focused LLM call with the full function body, callee signatures, and parent context. A `ModuleBindingNode` in a batch of 10 only gets a declaration snippet (up to 10 lines) plus up to 10 usage snippets (800 chars each). Function declarations forced into the module binding path lose meaningful context.

4. **Dependency graph congestion**: More module bindings means more nodes in the unified dependency graph that must complete before their dependents can start, increasing the number of processing waves.

## Proposed fix

In `shouldSkipBinding()`, function/class declarations should be skipped from the module binding pool **regardless of wrapper mode**. They are already discovered and processed as `FunctionNode`s by the function graph builder. The wrapper mode flag should only affect which scope's bindings are enumerated, not whether function declarations are eligible.

```typescript
function shouldSkipBinding(
  bindingPath: babelTraverse.NodePath,
  wrapper: WrapperFunctionResult | null
): boolean {
  // Always skip function/class declarations -- they're processed as FunctionNodes
  if (bindingPath.isFunctionDeclaration() || bindingPath.isClassDeclaration()) {
    return true;
  }

  // For variable declarators, skip if init is a NAMED function/class expression
  if (bindingPath.isVariableDeclarator()) {
    const init = (bindingPath.node as t.VariableDeclarator).init;
    if (
      (t.isFunctionExpression(init) && init.id) ||
      (t.isClassExpression(init) && init.id)
    ) {
      return true;
    }
  }

  return false;
}
```

### Verification

Before applying this fix, verify that function declarations inside the wrapper IIFE are actually being picked up by the function graph builder (`src/analysis/function-graph.ts`). The graph builder should traverse the wrapper's body and create `FunctionNode`s for each function declaration regardless of scope depth. If it only looks at program-level declarations, it may also need updating to traverse the wrapper scope.

Check:

- `buildFunctionGraph()` in `src/analysis/function-graph.ts` -- does it traverse into the wrapper IIFE?
- Are functions inside the wrapper already appearing as `FunctionNode`s in the graph? (They should be, since the function visitor is scope-agnostic.)
- After the fix, confirm the total item count for Bun bundles drops from ~90K back toward ~67K (matching the esbuild profile).

### Test plan

1. Run humanify on the decompiled claude-code 2.1.120 binary (`inputs/claude-code-2.1.120/binary-decompiled/src/entrypoints/index.js`) with `--diagnostics` and compare the module binding count before and after the fix.
2. Expected: module bindings should drop from ~31K to ~15K (only var declarations, not function declarations).
3. Expected: total processing time should return to the 6-8 hour range.
4. Run on an esbuild bundle (claude-code 2.1.87 `cli.js`) to confirm no regression -- behavior should be unchanged since `wrapper` is null for those files.

## Files to modify

- `src/rename/plugin.ts` -- `shouldSkipBinding()` (lines 848-874): remove the `!wrapper` guard on function/class declaration skipping
- Possibly `src/analysis/function-graph.ts` -- verify wrapper-internal functions are traversed
