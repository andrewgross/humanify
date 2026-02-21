# Resumable Processing

## Problem

Large bundles can take hours to process — thousands of functions, each requiring one or more LLM API calls. A crash, network outage, or user interrupt loses all progress. Users must restart from scratch, re-spending both time and API credits.

This is the single biggest pain point for large-bundle workflows:

1. **Cost**: Re-processing already-completed functions wastes LLM API calls
2. **Time**: Multi-hour runs that fail near the end are devastating
3. **Reliability**: Network hiccups, OOM errors, and rate-limit exhaustion are common on large inputs

## Solution Overview

Hybrid file-level + function-level checkpointing backed by SQLite. Progress is persisted after every function completion. On resume, completed work is replayed from the checkpoint without LLM calls.

### Key Design Decisions

- **SQLite over flat files**: Atomic writes via WAL mode; no partial-write corruption. Single file, easy to manage.
- **Function-level granularity**: Each function's rename mapping is checkpointed individually. A crash mid-function loses only that one function's work.
- **Replay, not replay-AST**: We don't serialize the AST. On resume, we re-parse the webcrack output (which is deterministic) and replay rename mappings via `scope.rename()`. This avoids AST serialization complexity and ensures Babel scope state stays consistent.
- **Input hash validation**: The checkpoint stores a hash of the input file(s). If the input changes between runs, the checkpoint is invalidated — stale rename mappings would produce nonsense.

## SQLite Schema

```sql
-- Run-level metadata (one row per checkpoint file)
CREATE TABLE run_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
-- Keys: 'input_hash', 'created_at', 'humanify_version', 'model', 'bundle_type'

-- File-level checkpoint (for multi-file bundles from webcrack)
CREATE TABLE file_checkpoint (
  file_path TEXT PRIMARY KEY,
  status TEXT NOT NULL CHECK (status IN ('pending', 'in_progress', 'done', 'skipped')),
  -- 'skipped' = library file detected and excluded
  function_count INTEGER,
  done_count INTEGER DEFAULT 0,
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Function-level checkpoint (the core unit of resumability)
CREATE TABLE function_checkpoint (
  session_id TEXT PRIMARY KEY,      -- e.g., "module-42.js:15:2"
  file_path TEXT NOT NULL REFERENCES file_checkpoint(file_path),
  structural_hash TEXT NOT NULL,    -- exactHash from fingerprint
  status TEXT NOT NULL CHECK (status IN ('pending', 'done', 'failed')),
  rename_mapping TEXT,              -- JSON: {"a": "count", "b": "items"}
  error_message TEXT,               -- if status = 'failed'
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Module-level rename checkpoint (for the module rename pass in rename.ts)
CREATE TABLE module_rename_checkpoint (
  file_path TEXT PRIMARY KEY,
  status TEXT NOT NULL CHECK (status IN ('pending', 'done')),
  output_code TEXT,                 -- final generated code
  source_map TEXT,                  -- JSON source map
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_fn_file ON function_checkpoint(file_path);
CREATE INDEX idx_fn_status ON function_checkpoint(status);
```

## CheckpointStore API

```typescript
// src/checkpoint/store.ts

import type { RenameMapping, FunctionNode } from "../analysis/types.js";

interface CheckpointStore {
  // --- Lifecycle ---

  /** Open or create a checkpoint database at the given path */
  static open(dbPath: string): CheckpointStore;

  /** Close the database connection */
  close(): void;

  /** Delete the checkpoint file */
  destroy(): void;

  // --- Run metadata ---

  /** Get a metadata value */
  getMeta(key: string): string | undefined;

  /** Set a metadata value */
  setMeta(key: string, value: string): void;

  /** Check if a previous run's checkpoint is compatible with the current input */
  isCompatible(inputHash: string): boolean;

  // --- File-level ---

  /** Mark a file's processing status */
  setFileStatus(filePath: string, status: "pending" | "in_progress" | "done" | "skipped", functionCount?: number): void;

  /** Get all files and their status */
  getFileStatuses(): Map<string, { status: string; functionCount: number; doneCount: number }>;

  /** Check if a file is fully done (all functions completed) */
  isFileDone(filePath: string): boolean;

  // --- Function-level ---

  /** Save a completed function's rename mapping */
  saveFunctionResult(sessionId: string, filePath: string, structuralHash: string, mapping: RenameMapping): void;

  /** Save a failed function */
  saveFunctionFailure(sessionId: string, filePath: string, structuralHash: string, error: string): void;

  /** Get all completed function mappings for a file */
  getCompletedFunctions(filePath: string): Map<string, { structuralHash: string; mapping: RenameMapping }>;

  /** Get count of completed functions for a file */
  getCompletedCount(filePath: string): number;

  // --- Module output ---

  /** Save the final output for a file */
  saveModuleOutput(filePath: string, code: string, sourceMap?: string): void;

  /** Get saved module output */
  getModuleOutput(filePath: string): { code: string; sourceMap?: string } | undefined;

  // --- Bulk operations ---

  /** Get overall progress summary */
  getProgress(): { totalFiles: number; doneFiles: number; totalFunctions: number; doneFunctions: number; failedFunctions: number };
}
```

## Resume Flow

### Auto-Detection

When `humanify` starts, it checks for an existing checkpoint file at the default location (alongside the output directory):

```
<output-dir>/.humanify-checkpoint.db
```

If found and `--no-resume` is not set:

1. **Validate input hash**: Compute SHA-256 of the input file(s) and compare against `run_meta.input_hash`. If different, warn and start fresh.
2. **Log resume info**: Print what's being resumed (e.g., "Resuming: 142/188 functions already done, 46 remaining").
3. **Skip webcrack**: If all files are present in `file_checkpoint`, skip the webcrack extraction step. The unpacked files should still exist in the output directory.
4. **File-level skip**: For files where `file_checkpoint.status = 'done'`, skip entirely.
5. **Function-level replay**: For partially-done files, replay completed function mappings and only send remaining functions to the LLM.

### The Webcrack Problem

Webcrack calls `clearDirectory()` on the output directory before extracting. On resume, this would destroy the unpacked files we need. The solution:

1. On first run, after webcrack completes, record all extracted file paths in `file_checkpoint`.
2. On resume, **skip the webcrack step entirely** — read the already-extracted files from the output directory.
3. If any expected file is missing (user deleted it?), fall back to re-running webcrack for that file's parent bundle.

### Replay Logic

Replaying a cached rename mapping on a fresh AST:

```typescript
function replayRenames(
  fnPath: NodePath<t.Function>,
  mapping: RenameMapping
): void {
  // Get bindings owned by this function
  const bindings = getOwnBindings(fnPath);

  // Apply each cached rename
  for (const binding of bindings) {
    const newName = mapping.names[binding.name];
    if (newName && newName !== binding.name) {
      binding.scope.rename(binding.name, newName);
    }
  }
}
```

This works because:
- Webcrack output is deterministic (same input → same unpacked files)
- `getOwnBindings()` returns bindings in a deterministic order
- `scope.rename()` updates all references throughout the AST
- The binding names in the fresh AST match the keys in the cached mapping

### Structural Hash Validation

As an extra safety check, we compare the `structuralHash` of the function in the fresh AST against the stored hash. If they differ, the cached mapping is discarded and the function is re-processed. This catches edge cases where the input hash is the same but webcrack's output changed (e.g., webcrack version upgrade).

## Integration Points

### 1. `unminify.ts` — File Loop

```typescript
// In the file processing loop:
for (const file of filesToProcess) {
  // Check checkpoint
  if (checkpoint && checkpoint.isFileDone(file.path)) {
    const saved = checkpoint.getModuleOutput(file.path);
    if (saved) {
      // Write saved output directly, skip processing
      await writeFile(outputPath, saved.code);
      continue;
    }
  }

  // Mark file as in-progress
  checkpoint?.setFileStatus(file.path, "in_progress", /* functionCount */);

  // Process file (rename plugin handles function-level checkpointing)
  const result = await processFile(file, { checkpoint });

  // Mark file as done
  checkpoint?.setFileStatus(file.path, "done");
  checkpoint?.saveModuleOutput(file.path, result.code, result.sourceMap);
}
```

### 2. `rename.ts` — Module Rename Plugin

The rename plugin receives a `checkpoint` option and uses it to:

1. Pre-populate `preDone` with functions that have cached mappings
2. Replay cached renames before starting LLM processing
3. Pass an `onFunctionDone` callback to the processor

```typescript
// In createRenamePlugin():
const completedFunctions = checkpoint?.getCompletedFunctions(filePath);

if (completedFunctions) {
  // Replay cached renames
  for (const fn of functions) {
    const cached = completedFunctions.get(fn.sessionId);
    if (cached && cached.structuralHash === fn.fingerprint.exactHash) {
      replayRenames(fn.path, cached.mapping);
      fn.status = "done";
      fn.renameMapping = cached.mapping;
      resumedFunctions.push(fn);
    }
  }
}

// Only send un-cached functions to the processor
const processor = new RenameProcessor(ast);
await processor.processAll(novelFunctions, provider, {
  concurrency,
  metrics,
  preDone: [...libraryFunctions, ...resumedFunctions],
  onFunctionDone: (fn) => {
    checkpoint?.saveFunctionResult(
      fn.sessionId,
      filePath,
      fn.fingerprint.exactHash,
      fn.renameMapping!
    );
  }
});
```

### 3. `processor.ts` — `onFunctionDone` Callback

A new `onFunctionDone` option in `ProcessorOptions`:

```typescript
interface ProcessorOptions {
  // ... existing fields ...

  /**
   * Called after each function completes (successfully or failed).
   * Used by checkpoint system to persist progress incrementally.
   */
  onFunctionDone?: (fn: FunctionNode) => void;
}
```

The callback fires in the `finally` block of the processing loop, after `fn.status = "done"` and `fn.renameMapping` are set. This ensures the checkpoint captures the final state.

## CLI

### `--no-resume` Flag

```bash
# Start fresh, ignoring any existing checkpoint
humanify bundle.min.js -o output/ --no-resume
```

When `--no-resume` is set:
1. Delete any existing checkpoint file for this output directory
2. Process everything from scratch

### Auto-Detection (Default)

When no flag is specified:
1. Check for `<output-dir>/.humanify-checkpoint.db`
2. If found and compatible, resume
3. If found but incompatible (input hash mismatch), warn and start fresh
4. If not found, start fresh (create checkpoint as we go)

### Progress Display on Resume

```
Resuming from checkpoint: 142/188 functions done (75%)
  Files: 28/33 complete, 5 remaining
  Skipped: 1,659 library functions

Processing [████████████████████████████████░░░░░░░░] 80%
  Functions: 150/188 done | 5 processing | 3 ready | 30 pending
```

## Crash Safety

### WAL Mode

SQLite is configured with `PRAGMA journal_mode=WAL` for:
- Concurrent reads during writes (progress queries don't block checkpointing)
- Crash recovery (WAL survives process crashes)
- Better performance (no full-file locks)

### Transaction Granularity

Each `saveFunctionResult` call is a single transaction. This means:
- A crash after function N completes but before function N+1 starts loses zero work
- A crash mid-function loses only that one function (it will be re-processed on resume)

### Cleanup on Success

When processing completes successfully:
1. Write all output files
2. Delete the checkpoint file (it's no longer needed)
3. Log: "Processing complete. Checkpoint cleaned up."

If `--keep-checkpoint` is desired in the future, the checkpoint file can be preserved for debugging.

## Implementation Phases

### Phase 1: Core Checkpoint Infrastructure
- [ ] `src/checkpoint/store.ts` — SQLite-backed `CheckpointStore` implementation
- [ ] `src/checkpoint/index.ts` — Public API
- [ ] Input hash computation (SHA-256 of input file contents)
- [ ] WAL mode configuration
- [ ] Unit tests for store CRUD operations

### Phase 2: Function-Level Integration
- [ ] `onFunctionDone` callback in `ProcessorOptions`
- [ ] Wire callback in `processor.ts` processing loop
- [ ] `replayRenames()` utility function
- [ ] Structural hash validation on replay
- [ ] Integration test: process half, "crash", resume, verify output matches full run

### Phase 3: File-Level Integration
- [ ] Checkpoint hooks in `unminify.ts` file loop
- [ ] Skip webcrack on resume (detect existing unpacked files)
- [ ] Module output caching (`saveModuleOutput` / `getModuleOutput`)
- [ ] Handle missing unpacked files (re-run webcrack fallback)

### Phase 4: CLI + Polish
- [ ] `--no-resume` flag in `src/commands/unified.ts`
- [ ] Auto-detection of existing checkpoint
- [ ] Resume progress display
- [ ] Checkpoint cleanup on success
- [ ] Incompatible checkpoint warning
- [ ] E2E test: full resume flow with a real bundle

## Testing Strategy

### Unit Tests

- `CheckpointStore` CRUD operations (open, save, get, destroy)
- Input hash validation (compatible vs incompatible)
- `replayRenames()` correctness (mapping applied to fresh AST matches original)
- Structural hash mismatch detection

### Integration Tests

- **Simulated crash**: Process N functions, close checkpoint, create new processor, resume, verify all N+remaining functions produce correct output
- **Input change detection**: Modify input between runs, verify checkpoint is invalidated
- **Mixed resume**: Some files done, some partially done, some not started — verify correct behavior for each
- **Library functions + resume**: Ensure library functions in `preDone` don't get re-processed and don't corrupt checkpoint

### E2E Tests

- Full run with checkpoint, compare output to non-checkpoint run (should be identical)
- Interrupt mid-run (SIGINT), resume, verify output is correct
- `--no-resume` flag clears existing checkpoint

## SQLite Dependency

This feature introduces a new dependency: `better-sqlite3` (synchronous SQLite bindings for Node.js). This is chosen over async alternatives because:

1. Checkpoint writes are small and fast (microseconds) — async overhead is unnecessary
2. Synchronous API simplifies the integration (no need to await checkpoint saves)
3. `better-sqlite3` is the most mature and widely-used SQLite binding for Node.js
4. Native addon is pre-built for all major platforms via prebuild

Alternative: `sql.js` (pure WASM, no native addon) could be used if native addon distribution is problematic. Performance is adequate for our write volume (~1 write per function completion).
