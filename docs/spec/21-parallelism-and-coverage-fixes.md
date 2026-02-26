# Fix Parallelism Bottleneck and Rename Coverage Gaps

## Context

A test run of the unified rename pipeline on a 728K-line Bun CJS bundle (68,601 functions + ~20K module bindings) revealed two problems:

1. **Serialization bottleneck**: Only 35% of nodes (24,294) were initially ready. After those drained, `processUnified()` stalled for ~7 hours until a nuclear deadlock break force-unlocked 28,136 nodes (41% of total) all at once with no ordering. Total run: ~9 hours.

2. **~4,000 un-renamed variables**: Module binding renames had a 74% success rate (15,280 / ~20,560). Failures were primarily name collisions — no retry logic, no conflict resolution fallback.

**Root causes:**
- `processUnified()` has only a nuclear deadlock breaker (force-unlock ALL), while `processAll()` has a two-tier system: first relax `scopeParent` (preserving callee ordering), then force-break only if still stuck.
- `processModuleBindingBatch()` makes a single LLM call with no retry. When names collide, they're silently skipped.
- Previously renamed names aren't visible to later batches due to proximity windowing.

---

## Change 1: Two-Tier Deadlock Breaking in `processUnified()`

**File:** `src/analysis/types.ts`

Add `scopeParentEdges` to `UnifiedGraph`:
```typescript
export interface UnifiedGraph {
  // ...existing fields...
  /** Edges that are scopeParent relationships (relaxable for deadlock breaking) */
  scopeParentEdges: Set<string>;  // Set of "childId->parentId" keys
}
```

**File:** `src/analysis/function-graph.ts`

In `buildUnifiedGraph()`, initialize and populate `scopeParentEdges`:
- Initialize: `const scopeParentEdges = new Set<string>();`
- At line 494 (where scopeParent deps are added): `scopeParentEdges.add(\`${fn.sessionId}->${fn.scopeParent.sessionId}\`);`
- Include in return value

**File:** `src/rename/processor.ts`

In `processUnified()`:

1. Add `isNodeReadyIgnoringScopeParent` helper (next to existing `isNodeReady` at line 758):
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

2. Replace initial deadlock check (lines 797-805) with two-tier:
   - Tier 1: try `isNodeReadyIgnoringScopeParent` for all nodes
   - Tier 2: only if Tier 1 yields zero, force-break all

3. Replace mid-loop deadlock breaker (lines 920-932) with two-tier:
   - Tier 1: scan remaining nodes with `isNodeReadyIgnoringScopeParent`
   - Tier 2: only if Tier 1 yields zero, force-break all remaining

**Expected impact:** Eliminates the 7-hour stall. Nodes blocked only by scopeParent will unblock progressively while still respecting callee ordering. The nuclear break becomes a last resort for true cycles only.
>> I think we want to be treating the module and function bindings the same, as that is eseentially how they are being treated post minification. View them in the same dep graph, queue and retry them the same, just have different prompt generation. Id love some examples of some of the code layouts that cause the deadlocks, so we can make some better decisions around what to block on.
---

## Change 2: Retry Logic for Module Binding Batches

**File:** `src/rename/processor.ts`

Restructure `processModuleBindingBatch()` (lines 955-1060) to add a retry loop:

1. Track `remaining` set of binding names not yet successfully renamed
2. Loop up to `MAX_MODULE_ROUNDS = 2` (original attempt + 1 retry)
3. Each round: build prompt from remaining identifiers only, call LLM, validate/apply
4. Track `previousAttempt` map of failed suggestions for conflict resolution
5. After all rounds: for any still-remaining names where the LLM suggested a valid-but-colliding name, apply `resolveConflict()` (from `src/llm/validation.ts:156`) as fallback

Also add `resolveConflict` to the existing imports from `../llm/validation.js` at line 19.

>> this is good, but as above, i think we just want to unify all of this logic for calling the llm, the only difference between the various approaches should be in the prompt, not retries, error handling etc.

---

## Change 3: Fix usedNames Visibility Across Batches

**File:** `src/rename/processor.ts`

In `processModuleBindingBatch()`, after computing `windowedNames` via `getProximateUsedNames()`, add all non-minified names from the shared `usedNames` set that were added during this run (i.e., names assigned by previous batches):

```typescript
// Ensure names assigned by previous batches are visible
for (const name of usedNames) {
  if (!looksMinified(name)) {
    windowedNames.add(name);
  }
}
```

This prevents the LLM from suggesting names that were already assigned to other bindings by earlier batches, regardless of line proximity.

>> shouldnt this already be the case since we update the bindings after the batch, so we see them when looking at the graph to generate the prompt? Why wouldnt they already be there.
---

## Change 4: Increase Module Batch Size

**File:** `src/rename/processor.ts`

Change `MODULE_BATCH_SIZE` from 5 to 10 (line 34). Benefits:
- 2x fewer LLM calls for module bindings
- More context per batch → better name suggestions
- Fewer cross-batch collision opportunities


>> this may increase the number of scope var names by a lot, we should be careful.  Also, shouldnt we treat modules as similar to functions? we do one function at a time, but looking at all vars inside of it, up to some limit. Shouldnt we do the same with modules? ask for var renames and a name for the module?
---

## Critical Files

| File | Changes |
|------|---------|
| `src/analysis/types.ts` | Add `scopeParentEdges` to `UnifiedGraph` |
| `src/analysis/function-graph.ts` | Populate `scopeParentEdges` in `buildUnifiedGraph()` |
| `src/rename/processor.ts` | Two-tier deadlock breaker, retry loop, usedNames fix, batch size |
| `src/rename/processor.test.ts` | Tests for two-tier deadlock breaking, retry logic |
| `src/analysis/function-graph.test.ts` | Test that `scopeParentEdges` is populated correctly |

---

## Implementation Order

### Step 1: scopeParentEdges type + graph population
- Add field to `UnifiedGraph` in types.ts
- Populate in `buildUnifiedGraph()` in function-graph.ts
- Add test in function-graph.test.ts
- Run `npm run test:unit`

### Step 2: Two-tier deadlock breaker in processUnified
- Add `isNodeReadyIgnoringScopeParent` helper
- Replace initial deadlock check (lines 797-805)
- Replace mid-loop deadlock breaker (lines 920-932)
- Add test: scopeParent-only blocked nodes unblock at Tier 1, callee-blocked nodes wait for Tier 2
- Run `npm run test:unit`

### Step 3: Module binding retry + conflict resolution
- Restructure `processModuleBindingBatch()` with retry loop
- Add `resolveConflict` import
- Add usedNames visibility fix (non-minified names from shared set)
- Add test: batch with collision retries and resolves
- Run `npm run test:unit`

### Step 4: Increase batch size
- Change `MODULE_BATCH_SIZE = 10`
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

# Check deadlock behavior (should see Tier 1 messages, not giant nuclear breaks):
grep "unified-processor" /tmp/claude-humanify/run2.log

# Check module binding coverage improvement:
grep -c "RENAME.*module-binding" /tmp/claude-humanify/run2.log
```
