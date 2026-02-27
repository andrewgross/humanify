# Fix Parallelism Bottleneck and Rename Coverage Gaps

## Context

A test run of the unified rename pipeline on a 728K-line Bun CJS bundle (68,601 functions + ~20K module bindings) revealed two problems:

1. **Serialization bottleneck**: Only 35% of nodes (24,294) were initially ready. After those drained, `processUnified()` stalled for ~7 hours until a nuclear deadlock break force-unlocked 28,136 nodes (41% of total) all at once with no ordering. Total run: ~9 hours.

2. **~4,000 un-renamed variables**: Module binding renames had a 74% success rate (15,280 / ~20,560). Failures were primarily name collisions — the LLM suggests a name already used by a previous batch, validation rejects it, and with no retry the binding is silently skipped.

---

## Deadlock Analysis: What Causes the Stall

The Bun CJS bundle has one giant wrapper IIFE containing ~68K functions. The wrapper is marked `pre-done`, so all direct children (depth-1) are immediately ready. The problem is **depth-2+ nesting** — functions defined inside lazy initializers and arrow functions:

### Pattern: Lazy initializer with nested functions

```javascript
// Depth 1: inside wrapper (ready immediately)
var initializeConfig = lazyInitializer(() => {
  // Depth 2: nested inside initializeConfig (blocked by scopeParent)
  function parseConfigValue(str) {
    // Depth 3: nested deeper (blocked by parseConfigValue)
    const validate = (v) => { ... };
    return validate(str);
  }
  parseConfigValue("test");
});
```

- `initializeConfig` (depth 1): ready immediately — scopeParent is the pre-done wrapper
- `parseConfigValue` (depth 2): blocked — scopeParent is `initializeConfig`
- `validate` (depth 3): blocked — scopeParent is `parseConfigValue`

This creates chains where 44,307 nodes (65%) wait for their parent function to complete before they can start. The current `processUnified()` deadlock breaker only fires when **all** ready and processing queues are empty, then force-unlocks everything — destroying leaf-first ordering.

Meanwhile `processAll()` (the old function-only path) has a two-tier system:
- **Tier 1**: Relax scopeParent constraints only (still respects callee ordering)
- **Tier 2**: Nuclear force-break (only for true cycles)

### Why scopeParent matters (and when it doesn't)

scopeParent ordering means "process the parent function first so its locals are already renamed when we process the child." This provides better context but is **not required for correctness** — each function only renames its own bindings. The two-tier approach treats scopeParent as a soft preference: respect it when possible, relax it when it blocks progress.

---

## Change 1: Two-Tier Deadlock Breaking in `processUnified()`

**Files:** `src/analysis/types.ts`, `src/analysis/function-graph.ts`, `src/rename/processor.ts`

### 1a. Track scopeParent edges separately

Add `scopeParentEdges: Set<string>` to `UnifiedGraph` (types.ts). In `buildUnifiedGraph()` (function-graph.ts), when adding scopeParent dependencies at line 494, also record the edge:
```typescript
scopeParentEdges.add(`${fn.sessionId}->${fn.scopeParent.sessionId}`);
```

### 1b. Add `isNodeReadyIgnoringScopeParent` in processUnified

```typescript
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
```

### 1c. Replace both deadlock breakers with two-tier

**Initial deadlock check** (lines 797-805) and **mid-loop deadlock breaker** (lines 920-932) both get the same pattern:
- Tier 1: scan remaining nodes with `isNodeReadyIgnoringScopeParent`
- Tier 2: only if Tier 1 yields zero, force-break all remaining

**Expected impact:** The 7-hour stall is eliminated. After the initial 24K nodes process, Tier 1 unblocks the 28K depth-2+ nodes progressively while still respecting callee ordering. Tier 2 is only needed for true callee cycles (rare).

---

## Change 2: Unify Module Binding Processing with Function Processing

Module bindings and function bindings should use the same LLM calling, retry, and error handling logic. The only difference is prompt generation.

**File:** `src/rename/processor.ts`

### 2a. Refactor `processModuleBindingBatch` to use `processFunctionBatched` patterns

Replace the current single-shot `processModuleBindingBatch()` (lines 955-1060) with logic that mirrors `processFunctionBatched()` (lines 406-575):

1. **Retry loop**: `MAX_ROUNDS` attempts (currently 3 for functions), same as functions
2. **Progressive rename**: After each round, apply valid renames immediately. Re-send remaining identifiers on retry with updated context.
3. **Validation**: Use `validateBatchRenames()` (same function used by `processFunctionBatched` at line 475) instead of inline validation
4. **Conflict tracking**: Track `previousAttempt` and `failures` for retry context, same as functions
5. **Reporting**: Generate `FunctionRenameReport` with per-identifier outcomes, same as functions
6. **resolveConflict fallback**: After all retry rounds exhausted, use `resolveConflict()` (from `src/llm/validation.ts:156`) for any remaining bindings where the LLM suggested a valid-but-colliding name — same as the function path at line 670

The only differences from function processing:
- **Prompt generation**: Module bindings use `MODULE_LEVEL_RENAME_SYSTEM_PROMPT` + `buildModuleLevelRenamePrompt()` instead of the function code + callee signatures prompt
- **Scope**: Module bindings use the shared `graph.targetScope` instead of per-function scope
- **usedNames**: Module bindings use proximity-windowed `getProximateUsedNames()` (the windowed names are only for the prompt — collision validation still checks against the full `usedNames` set)

### 2b. On usedNames visibility

The collision check at line 1043 already uses the full `usedNames` set (which includes names from all previous batches). The windowed names are only sent to the LLM as context in the prompt. This means:
- The LLM might suggest a name it doesn't know is taken (outside the proximity window)
- The validation catches the collision and rejects it
- With the new retry logic (2a), the LLM gets a second chance with the collision reported in `failures.duplicates`

This is sufficient — no separate usedNames fix needed.

---

## Change 3: Proximity-Grouped Module Binding Dispatch

Replace the current sequential `MODULE_BATCH_SIZE=5` batching with proximity-based grouping. Module bindings within ±50 lines of each other form a group (up to 10 bindings). Each group takes one concurrency slot, same as a function.

**Rationale:**
- Functions process all their local vars in one LLM call. Module bindings should work similarly — a group of nearby declarations gets one LLM call with shared context.
- Proximity grouping gives the LLM neighboring declarations as context, improving name quality.
- Grouping happens once when ready module bindings are collected, not in a sequential batch loop.

**File changes in `src/rename/processor.ts`:**

### 3a. Replace `MODULE_BATCH_SIZE` with proximity grouping

Remove the `MODULE_BATCH_SIZE` constant. Add a `groupByProximity()` helper:

```typescript
function groupByProximity(bindings: ModuleBindingNode[], radius = 50, maxSize = 10): ModuleBindingNode[][] {
  // Sort by declarationLine
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
```

### 3b. Update dispatch loop

In `processUnified()`, replace the batching for-loop (lines 909-912):
```typescript
// OLD: sequential batching
for (let i = 0; i < readyModuleBindings.length; i += MODULE_BATCH_SIZE) { ... }

// NEW: proximity grouping
const groups = groupByProximity(readyModuleBindings);
for (const group of groups) {
  dispatchModuleBindingBatch(group);
}
```

---

## Critical Files

| File | Changes |
|------|---------|
| `src/analysis/types.ts` | Add `scopeParentEdges` to `UnifiedGraph` |
| `src/analysis/function-graph.ts` | Populate `scopeParentEdges` in `buildUnifiedGraph()` |
| `src/rename/processor.ts` | Two-tier deadlock breaker; unify module binding processing with function processing (retry, validation, reporting); remove batch dispatch in favor of individual/grouped dispatch |
| `src/rename/processor.test.ts` | Tests for two-tier deadlock breaking; tests for unified module binding processing |
| `src/analysis/function-graph.test.ts` | Test that `scopeParentEdges` is populated correctly |

---

## Implementation Order

### Step 1: scopeParentEdges type + graph population
- Add `scopeParentEdges: Set<string>` field to `UnifiedGraph` in types.ts
- Initialize and populate in `buildUnifiedGraph()` in function-graph.ts
- Add test in function-graph.test.ts verifying edges are recorded
- Run `npm run test:unit`

### Step 2: Two-tier deadlock breaker in processUnified
- Add `isNodeReadyIgnoringScopeParent` helper
- Replace initial deadlock check (lines 797-805) with two-tier
- Replace mid-loop deadlock breaker (lines 920-932) with two-tier
- Add tests: scopeParent-only blocked nodes unblock at Tier 1; callee-blocked nodes require Tier 2
- Run `npm run test:unit`

### Step 3: Unify module binding processing
- Refactor `processModuleBindingBatch()` to use same retry loop, `validateBatchRenames()`, conflict tracking, `resolveConflict()` fallback, and reporting as `processFunctionBatched()`
- Remove `MODULE_BATCH_SIZE` batching in dispatch loop — dispatch module bindings individually or as proximity groups
- Add tests: retry on collision, resolveConflict fallback, per-identifier outcome reporting
- Run `npm run test:unit && npm run test:fingerprint`

---

## Verification

```bash
# After each step:
npm run test:unit

# After all steps:
npm run test:unit && npm run test:fingerprint

# Full E2E run:
npx tsx src/index.ts /tmp/claude-humanify/index.js -o /tmp/claude-humanify/output2 \
  --endpoint http://192.168.1.234:8000/v1 --api-key dummy --retries 10 \
  --timeout 300000 -m openai/gpt-oss-20b -vv 2>&1 | tee /tmp/claude-humanify/run2.log

# Check deadlock behavior (should see Tier 1 scopeParent relaxation, not giant nuclear breaks):
grep "unified-processor" /tmp/claude-humanify/run2.log

# Check module binding coverage improvement:
grep -c "RENAME.*module-binding" /tmp/claude-humanify/run2.log

# Compare: should be significantly more than the 15,280 from the first run
```
