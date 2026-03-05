# 22 — Rich Progress UI, Debug Log File, and Coverage Diagnostics

## Status: Draft

## Problem

After implementing parallelism fixes, a full run on a 728K-line Bun bundle still takes hours. The current progress output is a single `console.log` line that prints every 100ms — no in-place updates, no stage awareness, no module binding breakdown, no token stats. Debug logs (`-vv`) go to stdout via `console.log`, making it impossible to use a nice progress UI while also capturing debug output. Some identifiers remain unminified with no summary explaining why.

## Goals

1. **Rich TTY progress dashboard** — in-place updating display with per-stage progress bars, LLM stats, and ETA
2. **Debug log file** — `--log-file <path>` flag to redirect debug output to a file, keeping the TTY UI clean
3. **Post-run coverage summary** — breakdown of why identifiers were or weren't renamed

---

## Design

### What The User Sees

#### TTY mode (default when running in a terminal):

```
 humanify                                      elapsed 2h 14m  ETA 1h 30m
 ── Renaming functions & modules ──────────────────────────────────────────
 Functions  [==================>·············]   34,201 / 68,601   (49.9%)
 Modules    [==========================>····]   16,400 / 20,560   (79.8%)
 LLM        1,205 reqs · 12 in-flight · 3 failed · avg 320ms
 Tokens     2.4M total · 1,180 tok/s
```

Updates in-place at ~4 Hz. When a stage changes, a one-time message scrolls above:

```
 ✓ Graph built: 68,601 functions, 20,560 module bindings (3.2s)
 humanify                                      elapsed 0h 02m  ETA 6h 12m
 ── Renaming functions & modules ──────────────────────────────────────────
 ...
```

#### Non-TTY mode (piped/redirected):

```
[49.9%] 34,201/68,601 functions | 16,400/20,560 modules | LLM: 12 in-flight | ETA: 1h 30m
```

One line every ~5s (or on stage change).

#### `--log-file run.log`:

Debug output goes to `run.log` instead of stdout. TTY progress UI is shown even with `-vv`. Implies `-vv` automatically.

#### Post-run coverage summary:

```
 ── Coverage Summary ──────────────────────────────────────────────────────
 Functions:        34,201 renamed / 68,601 total  (49.9%)
 Module bindings:  16,400 renamed / 20,560 total  (79.8%)
 Identifiers:     142,000 renamed / 180,000 total (78.9%)
   Not minified:   28,000  (already human-readable)
   LLM missing:     6,200  (not returned after retries)
   LLM collision:   2,340  (name conflict unresolved)
   LLM invalid:       460  (invalid name returned)
   Deadlocked:      1,000  (processed without full context)
```

---

## Implementation

### Step 1: Extend MetricsTracker with stage + module binding counts + token rate

**File: `src/llm/metrics.ts`**

- Add `PipelineStage` type: `"parsing" | "building-graph" | "renaming" | "library-params" | "generating" | "done"`
- Add to `ProcessingMetrics`:
  - `stage: PipelineStage`
  - `moduleBindings: { total: number; completed: number; inProgress: number }`
  - `tokensPerSecond: number`
- Add methods:
  - `setStage(stage)` — sets stage, force-emits
  - `setModuleBindingTotal(n)` — sets mbTotal
  - `moduleBindingStarted()` / `moduleBindingCompleted()` — mirror functionStarted/Completed
  - Update `recordTokens()` to push to `tokenHistory` for rolling rate calculation
  - `getTokensPerSecond()` — 30s rolling window
- Update `getMetrics()` to include new fields
- Update `formatMetricsCompact()` to include module count and stage

### Step 2: Wire module binding metrics in processor

**File: `src/rename/processor.ts`**

- After counting module binding nodes, call `metrics.setModuleBindingTotal(count)`
- In `dispatchModuleBindingBatch()`: call `metrics.moduleBindingStarted()` per binding
- In the finally block: call `metrics.moduleBindingCompleted()` per binding
- Adjust `metrics.setFunctionTotal()` to only count function nodes (not module bindings)

### Step 3: Wire token usage from LLM responses to metrics

**File: `src/llm/types.ts`**

- Add optional `usage?: { totalTokens?: number }` to `BatchRenameResponse`

**File: `src/llm/openai-compatible.ts`**

- Include `usage` in the returned `BatchRenameResponse` from `suggestAllNames()`

**File: `src/rename/processor.ts`**

- After each `llm.suggestAllNames()` call, call `this.metrics?.recordTokens(response.usage?.totalTokens ?? 0)`

### Step 4: Add stage transitions in rename plugin

**File: `src/plugins/rename.ts`**

- Call `metrics.setStage("building-graph")` before `buildUnifiedGraph()`
- Call `metrics.setStage("renaming")` before `processUnified()`
- Call `metrics.setStage("library-params")` before library param processing
- Call `metrics.setStage("generating")` before `generate()`
- Call `metrics.setStage("done")` at the end
- Pass raw metrics object to progress callback instead of pre-formatting

### Step 5: Add configurable output target to debug module

**File: `src/debug.ts`**

- Add `private _output: (text: string) => void` field, defaulting to `(text) => console.log(text)`
- Add `setOutput(writer: (text: string) => void)` and `resetOutput()` methods
- Replace all `console.log(...)` calls with `this._output(...)` calls

### Step 6: Create progress renderer

**New file: `src/ui/progress.ts`**

```typescript
export interface ProgressRenderer {
  update(metrics: ProcessingMetrics): void;
  message(text: string): void;   // one-off messages that scroll above the dashboard
  finish(): void;                 // clean up, restore cursor
}

export function createProgressRenderer(opts: { tty: boolean }): ProgressRenderer;
```

**TtyRenderer** (when `tty: true`):
- Writes to `process.stderr` (keeps stdout clean)
- Uses `setInterval` at 250ms to redraw from latest cached metrics
- Tracks line count from last render; on redraw: `\x1b[{N}A` (move up N), `\x1b[2K` (clear each), write new
- `message()`: writes text ABOVE the dashboard area
- `finish()`: clears interval, writes final summary, restores cursor visibility
- Signal handlers: `process.once('exit')` and `process.once('SIGINT')` call `finish()`
- Progress bar: `[====>····]` style, width = `Math.max(10, (process.stderr.columns ?? 80) - 45)`
- No color dependencies — optional ANSI via raw escape codes (dim for labels, bold for numbers)

**LineRenderer** (when `tty: false`):
- `update()`: throttled to emit every ~5s or on stage changes
- Outputs enhanced `formatMetricsCompact()` via `process.stderr.write()`
- `message()`: `process.stderr.write(text + '\n')`
- `finish()`: writes final summary line

### Step 7: Wire everything in unified command

**File: `src/commands/unified.ts`**

- Add `--log-file <path>` CLI option
- Decision matrix for renderer:
  - `isTTY && (verbose.level < 2 || opts.logFile)` → TtyRenderer
  - Otherwise → LineRenderer
- When `--log-file` specified:
  - Open `fs.createWriteStream(path, { flags: 'a' })`
  - Call `debug.setOutput(text => stream.write(text + '\n'))`
  - Set `verbose.level = Math.max(verbose.level, 2)`
- Create renderer via `createProgressRenderer({ tty: useRichUI })`
- Change `onProgress` from `console.log` to `(m) => renderer.update(m)`
- Call `renderer.finish()` in finally block

### Step 8: Route informational messages through renderer

**File: `src/unminify.ts`**

- Add `log?: (message: string) => void` to `UnminifyOptions`
- Replace `console.log(...)` calls with `(options?.log ?? console.log)(...)`

**File: `src/commands/unified.ts`**

- Pass `log: (msg) => renderer.message(msg)` into unminify options

### Step 9: Post-run coverage summary

**New file: `src/rename/coverage.ts`**

```typescript
export interface CoverageSummary {
  functions: { total: number; renamed: number; skipped: number };
  moduleBindings: { total: number; renamed: number; skipped: number };
  identifiers: {
    total: number;
    renamed: number;
    notMinified: number;   // looksMinified() returned false
    llmMissing: number;    // LLM didn't return a name after all rounds
    llmCollision: number;  // name conflicted and resolveConflict also failed
    llmInvalid: number;    // LLM returned an invalid name
    deadlocked: number;    // function was force-broken and processed without full context
  };
}

export function buildCoverageSummary(graph: UnifiedGraph): CoverageSummary;
export function formatCoverageSummary(summary: CoverageSummary): string;
```

- `buildCoverageSummary()`: Iterates all nodes in the graph, reads `renameReport` from each function/module-binding node, aggregates `IdentifierOutcome` statuses into summary buckets
- `formatCoverageSummary()`: Produces the human-readable block shown in the design section above
- Track which functions were force-broken (deadlocked) during processing by collecting their sessionIds into a set during deadlock breaking in `processor.ts`

**File: `src/plugins/rename.ts`**

- After processing completes, call `buildCoverageSummary(graph)` and `formatCoverageSummary(summary)`
- Pass formatted summary to `renderer.message()`

### Step 10: Tests

- `src/llm/metrics.test.ts`: Test stage tracking, module binding metrics, token rate calculation
- `src/ui/progress.test.ts`: Test TtyRenderer output (mock stderr.write, verify ANSI sequences), LineRenderer throttling, message() behavior
- `src/debug.test.ts` (or extend existing): Test setOutput redirects all console.log calls
- `src/rename/coverage.test.ts`: Test buildCoverageSummary aggregation with mock graph, formatCoverageSummary output

---

## Critical Files

| File | Change |
|------|--------|
| `src/llm/metrics.ts` | Stage, module binding metrics, token rate |
| `src/llm/types.ts` | Add `usage` to BatchRenameResponse |
| `src/llm/openai-compatible.ts` | Return usage in suggestAllNames response |
| `src/rename/processor.ts` | Split module binding metrics, pipe token usage |
| `src/plugins/rename.ts` | Stage transitions, raw metrics onProgress |
| `src/debug.ts` | Configurable output target |
| `src/ui/progress.ts` | **NEW** — ProgressRenderer, TtyRenderer, LineRenderer |
| `src/rename/coverage.ts` | **NEW** — CoverageSummary, build + format |
| `src/commands/unified.ts` | --log-file flag, renderer wiring, TTY detection |
| `src/unminify.ts` | Route console.log through log callback |

## Verification

```bash
# After each step:
npm run test:unit

# Manual TTY test:
npx tsx src/index.ts /tmp/test.js -o /tmp/out -c 10 --endpoint ... --api-key ...
# Should see in-place updating dashboard

# Log file test:
npx tsx src/index.ts /tmp/test.js -o /tmp/out --log-file /tmp/debug.log --endpoint ...
# Should see dashboard + debug in file

# Non-TTY test:
npx tsx src/index.ts /tmp/test.js -o /tmp/out --endpoint ... | cat
# Should see periodic line-based output

# Verify debug redirect:
wc -l /tmp/debug.log  # should have content
```

---

## Notes

_Space for back-and-forth discussion notes._
