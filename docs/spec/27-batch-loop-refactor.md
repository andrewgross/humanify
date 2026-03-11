# 27 — Batch Loop Refactor: Separate Batching from Retries

## Problem

Diagnostics from a claude-code (2.1.71) run show **6,544 identifiers (6.8%)** failing to get renamed:

- **LLM Missing: 4,390 (4.6%)** — Large functions exhaust the fixed 5-round limit before all identifiers are even sent once
- **LLM Collisions: 2,154 (2.2%)** — No fallback resolution for function bindings

Root cause: `runBatchRenameLoop` conflated **batching** (sliding a window over identifiers) with **retrying** (re-sending failures). With `MAX_ROUNDS=5` x `MAX_BATCH_SIZE=10` = 50 max identifiers, a function with 2,032 bindings renamed only 44.

Additionally, function bindings lacked the `resolveRemaining` fallback that module bindings had, leaving collisions permanently unresolved.

## Solution

### Batch-Until-Done Loop Model

The rewritten `runBatchRenameLoop` processes identifiers in batch windows with per-identifier retry tracking:

```
queue = [...allIdentifiers]
retryExhausted = []

while queue is not empty:
  batch = queue.splice(0, batchSize)

  while batch has retryable identifiers:
    usedNamesSnapshot = copy(usedNames)
    response = llm.suggestAllNames(batch)
    apply valid renames, remove successes from batch

    for each failure:
      classify as free retry (cross-lane collision) or real failure
      if exhausted: move to retryExhausted
      if retryable: keep in batch

    if no progress: break

retryExhausted → straggler pass → resolveRemaining fallback
```

**No `MAX_ROUNDS`**: Loop terminates naturally when queue empties or all identifiers exhaust retries.

### Per-Identifier Tracking

Each identifier maintains its own state:

```typescript
interface IdentifierAttemptState {
  attempts: number;          // real failures (counts against maxRetriesPerIdentifier)
  freeRetries: number;       // cross-lane collisions (counts against maxFreeRetries)
  lastSuggestion?: string;
  lastFailureReason?: "duplicate" | "invalid" | "missing" | "unchanged";
}
```

### Free Retry Mechanism

When two parallel lanes both send LLM requests without knowing about a name claimed between them:

```typescript
const usedNamesSnapshot = new Set(usedNames);
// ... LLM call ...
if (usedNames.has(suggestedName) && !usedNamesSnapshot.has(suggestedName)) {
  // Cross-lane collision → free retry (don't count against attempts)
  state.freeRetries++;
}
```

### Parallel Lanes for Large Functions

Functions with >25 bindings (configurable via `--lane-threshold`) split identifiers into 4 lanes processed in parallel:

```typescript
if (bindings.length > laneThreshold) {
  const lanes = splitByPosition(bindings, 4);
  await Promise.all(lanes.map(lane => runBatchRenameLoop(lane)));
}
```

All lanes share:
- **`usedIdentifiers` Set** — collision detection (safe: JS is single-threaded, validate+apply runs synchronously between `await` points)
- **AST** — renames mutate in-place, visible to all lanes when they next call `generate()`

### Universal `resolveRemaining` Fallback

Both function and module bindings now have a `resolveRemaining` callback that fires after the loop. For each remaining identifier with a last LLM suggestion:
- If the suggestion collides, `resolveConflict()` appends a numeric suffix (e.g., `config` -> `config2`)
- If the suggestion is valid and non-colliding, apply it directly

### Proximity-Based Ban List

Function bindings now use `getProximateUsedNames()` (previously only used for module bindings) to window the ban list by identifier positions. This avoids overwhelming the LLM prompt with irrelevant distant names.

## CLI Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--batch-size <n>` | 10 | Identifiers per LLM batch |
| `--max-retries <n>` | 3 | Per-identifier retry limit |
| `--max-free-retries <n>` | 100 | Cross-lane collision retry limit |
| `--lane-threshold <n>` | 25 | Min bindings to enable parallel lanes |

## Type Changes

### `IdentifierOutcome`
- Failure variants: `rounds` -> `attempts` (reflects per-identifier tracking, not global round count)

### `FunctionRenameReport`
- `rounds` -> `totalLLMCalls` (more accurate name for the count)

### `ProcessorOptions`
- Added: `batchSize`, `maxRetriesPerIdentifier`, `maxFreeRetries`, `laneThreshold`

### `DiagnosticsReport`
- `patterns.failuresByRound` -> `patterns.failuresByAttempts`
- `UnrenamedEntry.rounds` -> `UnrenamedEntry.attempts`

## Files Modified

| File | Changes |
|------|---------|
| `src/rename/processor.ts` | Rewrote `runBatchRenameLoop`, added parallel lanes, added `resolveRemaining` to function bindings, added proximity windowing, added `splitByPosition` helper |
| `src/analysis/types.ts` | `IdentifierOutcome` rounds->attempts, `FunctionRenameReport` rounds->totalLLMCalls, `ProcessorOptions` batch tuning fields |
| `src/rename/diagnostics.ts` | Updated to use `attempts` field, renamed `failuresByRound` -> `failuresByAttempts` |
| `src/plugins/rename.ts` | `RenamePluginOptions` batch tuning fields, threading to processor |
| `src/commands/unified.ts` | CLI flags for batch tuning |
| `src/rename/processor.test.ts` | Updated test expectations for new loop behavior |
| `src/rename/diagnostics.test.ts` | Updated to new field names |
| `src/rename/coverage.test.ts` | Updated to new field names |
| `test/e2e/harness/humanify.ts` | Updated `.rounds` -> `.attempts` references |

## Supersedes

- Parts of `docs/spec/11-batched-renaming.md` — its retry model (MAX_ATTEMPTS=3 per function) is replaced by per-identifier retry tracking
- Parts of `docs/spec/21-parallelism-and-coverage-fixes.md` — its "Change 2: Unify Module Binding Processing" is completed by this work (both function and module bindings now share the same batch-until-done loop with `resolveRemaining`)
