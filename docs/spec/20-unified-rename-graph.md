# Spec 20: Unified Rename Graph

## Status: Implementing

## Problem

Module-level renaming runs as a separate serial phase before the function pipeline. With ~6,500 bindings in a Bun CJS wrapper bundle, this creates:
1. **Serial processing** — 650+ sequential LLM requests
2. **Sparse context** — 3-line/200-char snippets insufficient for a 20B model
3. **Bloated usedNames** — thousands of names dumped into every prompt

## Solution

Unify module-level vars and functions into a single dependency graph. Process everything leaf-first in one parallel pass. Different node types get different prompt templates but share the same ready-queue + concurrency limiter.

## Architecture

### RenameNode — a tagged union

```typescript
type RenameNode =
  | { type: "function"; node: FunctionNode }
  | { type: "module-binding"; node: ModuleBindingNode }
```

Both types participate in the same dependency graph. Dependencies can cross types:
- Function -> module var (function body references a module-level class/constructor)
- Module var -> function (var initialized from a function call)
- Module var -> module var (var references another var)
- Function -> function (existing call graph)

### UnifiedGraph

```typescript
interface UnifiedGraph {
  nodes: Map<string, RenameNode>;          // sessionId -> node
  dependencies: Map<string, Set<string>>;  // sessionId -> dependency sessionIds
  dependents: Map<string, Set<string>>;    // sessionId -> dependent sessionIds
}
```

Function nodes keep existing `sessionId` format (`file:line:col`). Module binding nodes use `module:varName`.

### Processing dispatch

The coordinator dispatches based on node type:
- **Function nodes** -> existing `processFunctionBatched()` logic
- **Module binding nodes** -> module-level prompt with expanded context snippets

### Smart usedNames windowing

For module-level binding batches with >= 100 scope bindings:
1. Collect line numbers for each binding's declaration + references
2. Include only usedNames within +/-100 lines of the batch
3. Always include well-known names (exports, require, console, etc.)
4. Always exclude minified-looking names

### Pipeline change

Before:
```
Phase 1: renameModuleBindings()  ->  serial module-level
Phase 2: buildFunctionGraph() -> processAll()  ->  parallel functions
Phase 3: library param rename  ->  parallel functions
```

After:
```
Step 1: buildUnifiedGraph()  ->  combined graph
Step 2: processAll(unifiedGraph)  ->  single parallel pass, leaf-first
Step 3: library param rename  ->  parallel functions (unchanged)
```

## Key files

| File | Changes |
|------|---------|
| `src/analysis/types.ts` | Add `ModuleBindingNode`, `RenameNode`, `UnifiedGraph` types |
| `src/analysis/function-graph.ts` | Add `buildUnifiedGraph()` |
| `src/plugins/rename.ts` | Remove `renameModuleBindings()`, update plugin, add windowing |
| `src/rename/processor.ts` | Extend `processAll()` for `RenameNode[]`, add `processModuleBinding()` |
| `src/utils/concurrency.ts` | Extract shared concurrency limiter |
