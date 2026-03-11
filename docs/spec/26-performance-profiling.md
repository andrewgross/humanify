# 26 — Performance Profiling

## Problem

A full humanify run on a 728K-line Bun bundle takes hours, but we have limited visibility into where time is spent. The existing MetricsTracker (spec 22) captures LLM call counts, token usage, and overall progress, but gives no breakdown of non-LLM work: graph building, context generation, AST parsing, ready-queue overhead, and code generation are all opaque. Without per-stage and per-function timing, we can't identify optimization targets or measure the impact of changes.

Specific blind spots:

- **Graph building** is a single "building-graph" stage with no sub-stage timing. Three AST passes run sequentially (function collection, callee analysis, scope nesting), plus a fourth for module-level bindings.
- **Context generation** (building the LLM prompt for each function) involves code generation, call site collection, and scope extraction — all hidden inside the LLM request latency.
- **Ready-queue polling** calls `isReady()` on every dequeue without pre-computed topological sort. On large graphs this could be O(n²).
- **Concurrency utilization** — we set `--concurrency 50` but don't know what fraction of slots are actually occupied vs waiting for dependencies.
- **Per-function timing** — we know averages but not the distribution. A few slow functions could dominate wall time.

## Goals

1. Instrument the pipeline to produce a **per-stage timing breakdown** with no user-visible overhead when profiling is off
2. Add **per-function timing** to the rename processor to identify outliers and dependency bottlenecks
3. Provide a **`--profile <path>`** CLI flag that writes a structured JSON profile to disk
4. Make the profile format compatible with common visualization tools (Chrome DevTools timeline or speedscope)
5. Capture **concurrency utilization** over time so we can tune `--concurrency` and identify dependency stalls

## Non-Goals

- Real-time profiling dashboard (the progress UI from spec 22 already covers live monitoring)
- Memory profiling (Node.js `--heap-prof` and `--inspect` cover this adequately)
- CPU flame graphs (Node.js `--prof` and `--cpu-prof` cover this; we want wall-clock stage timing, not CPU sampling)
- Profiling the LLM provider itself (response time is already tracked; provider-side latency is out of our control)

## Architecture

### Profiling Model

The profiler captures **spans** — named intervals with start/end timestamps and optional metadata. Spans nest: a "renaming" span contains per-function spans, which contain "context-build" and "llm-request" sub-spans.

```typescript
interface ProfileSpan {
  name: string;
  category: "pipeline" | "graph" | "rename" | "llm" | "io" | "transform";
  startMs: number;       // performance.now() relative to profile start
  endMs: number;
  metadata?: Record<string, string | number | boolean>;
  children?: ProfileSpan[];
}

interface ProfileReport {
  version: 1;
  startTime: string;     // ISO 8601
  totalMs: number;
  inputFile: string;
  inputBytes: number;
  functionCount: number;
  moduleBindingCount: number;
  spans: ProfileSpan[];
  concurrency: ConcurrencySnapshot[];
  summary: StageSummary[];
}
```

### Concurrency Tracking

Sample concurrency utilization at fixed intervals (250ms) to produce a time-series:

```typescript
interface ConcurrencySnapshot {
  timeMs: number;        // relative to profile start
  inFlight: number;      // LLM requests currently active
  ready: number;         // functions ready but waiting for a slot
  blocked: number;       // functions waiting on dependencies
  completed: number;     // cumulative completions
}
```

This tells us: "At t=30s we had 50 in-flight but 200 ready" (concurrency-bound) vs "At t=90s we had 3 in-flight and 0 ready" (dependency-bound).

### Stage Summary

Aggregate timing per pipeline stage for quick triage:

```typescript
interface StageSummary {
  stage: string;
  wallMs: number;        // wall-clock time for this stage
  cpuMs?: number;        // optional: sum of synchronous work (excludes awaits)
  count?: number;        // number of items processed (functions, files, etc.)
  p50Ms?: number;        // per-item percentiles (for rename stage)
  p95Ms?: number;
  p99Ms?: number;
}
```

Expected stages:
- `detection` — bundler/minifier detection
- `unpack` — adapter.unpack() (webcrack or passthrough)
- `library-detection` — detectLibraries()
- `parse` — babel parse per file
- `graph-build:functions` — Pass 1+2 (function collection + callee analysis)
- `graph-build:scopes` — Pass 3 (scope nesting)
- `graph-build:modules` — Module-level binding collection
- `rename:functions` — Function rename processing (contains per-function sub-spans)
- `rename:modules` — Module binding batch processing
- `rename:library-params` — Library parameter renaming
- `generate` — AST → code generation
- `babel-transforms` — Babel beautification plugins
- `prettier` — Prettier formatting
- `file-io` — Read/write operations

### Per-Function Rename Span

Each function processed by the rename loop gets a sub-span:

```typescript
// Inside the "rename:functions" span
{
  name: "fn:calculateTotal",  // or "fn:a" for minified
  category: "rename",
  startMs: 45200,
  endMs: 45850,
  metadata: {
    sessionId: "fn-42",
    bindingCount: 12,
    contextBytes: 2400,
    llmResponseMs: 580,
    contextBuildMs: 45,
    waitMs: 120,           // time spent waiting for dependency or concurrency slot
    retries: 0,
    outcome: "renamed"     // or "failed", "skipped", "deadlocked"
  }
}
```

The `waitMs` field is key — it separates "this function was slow because the LLM was slow" from "this function was slow because it was blocked on dependencies".

## Implementation

### Step 1: Profiler Core (`src/profiling/profiler.ts`)

A lightweight span-based profiler that does zero work when disabled:

```typescript
export class Profiler {
  private enabled: boolean;
  private spans: ProfileSpan[];
  private spanStack: ProfileSpan[];
  private startTime: number;
  private concurrencySnapshots: ConcurrencySnapshot[];
  private snapshotInterval?: ReturnType<typeof setInterval>;

  constructor(enabled: boolean);

  /** Start a named span. Returns a handle to end it. */
  startSpan(name: string, category: ProfileSpan["category"], metadata?: Record<string, string | number | boolean>): SpanHandle;

  /** Record a concurrency snapshot (called by the rename processor). */
  recordConcurrency(snapshot: Omit<ConcurrencySnapshot, "timeMs">): void;

  /** Start periodic concurrency sampling. */
  startConcurrencySampling(sampler: () => Omit<ConcurrencySnapshot, "timeMs">, intervalMs?: number): void;

  /** Stop sampling and finalize. */
  finalize(): ProfileReport;
}

export interface SpanHandle {
  addMetadata(key: string, value: string | number | boolean): void;
  end(): void;
}

/** No-op profiler for when profiling is disabled. */
export const NULL_PROFILER: Profiler;
```

When `enabled: false`, all methods are no-ops (no allocations, no timestamps). The `NULL_PROFILER` singleton avoids `if (profiler)` checks throughout the codebase.

### Step 2: Instrument Graph Building (`src/analysis/function-graph.ts`)

Add span points around each pass:

```
graph-build:functions  — traverse + buildFunctionGraph()
graph-build:scopes     — scope nesting pass
graph-build:modules    — buildUnifiedGraph() module binding collection
```

The profiler instance is passed via an options parameter to `buildFunctionGraph()` and `buildUnifiedGraph()`. When null/disabled, zero overhead.

### Step 3: Instrument Rename Processor (`src/rename/processor.ts`)

This is the highest-value instrumentation target:

1. **Per-function span**: Wrap each function's processing in `profiler.startSpan()`. Record `waitMs` (time from ready to processing start), `contextBuildMs`, `llmResponseMs`, binding count, and outcome.
2. **Concurrency sampling**: Call `profiler.startConcurrencySampling()` at rename start with a sampler that reads `inFlight`, `ready`, `blocked` counts from the processor state.
3. **Dependency stall detection**: When a function becomes ready but all concurrency slots are full, record a `concurrency-stall` event. When a function's dependencies all complete but it doesn't start within 100ms, record a `scheduling-delay` event.

### Step 4: Instrument Plugin Stages (`src/plugins/rename.ts`, `src/plugins/babel/babel.ts`)

Wrap each plugin in a stage span:
- `babel-transforms` around the babel plugin chain
- `prettier` around the prettier plugin
- `generate` around the code generation step

These are lightweight — one span per file, not per function.

### Step 5: Instrument Pipeline (`src/unminify.ts`)

Wrap top-level stages:
- `detection` around `detectBundle()`
- `unpack` around `adapter.unpack()`
- `library-detection` around `detectLibraries()`
- `file-io:read` / `file-io:write` around file operations

### Step 6: CLI Flag (`src/commands/unified.ts`)

Add `--profile <path>` option:
- Creates a `Profiler(true)` instance
- Passes it through to `unminify()` and downstream
- On completion, calls `profiler.finalize()` and writes JSON to the specified path
- Log the output path

### Step 7: Profile Output Format

Write the `ProfileReport` as JSON. Additionally, provide a converter to **Chrome Trace Event Format** (the `chrome://tracing` / Perfetto format) for visualization:

```typescript
export function toTraceEvents(report: ProfileReport): TraceEvent[];
```

Each span becomes a `"X"` (complete) event:

```json
{"name": "rename:functions", "cat": "rename", "ph": "X", "ts": 45200000, "dur": 650000, "pid": 1, "tid": 1}
```

Concurrency snapshots become `"C"` (counter) events:

```json
{"name": "concurrency", "ph": "C", "ts": 30000000, "pid": 1, "args": {"inFlight": 50, "ready": 200, "blocked": 1400}}
```

This format loads directly into `chrome://tracing` or [Perfetto UI](https://ui.perfetto.dev/) with no extra tooling.

### Step 8: Summary Report

When `--profile` is used, print a human-readable summary to the console after processing:

```
── Profile Summary ──────────────────────────────────────────────────
Total:           4m 32s
  detection:       12ms
  unpack:        1.2s    (webcrack, 342 modules)
  library-det:   890ms
  graph-build:   3.4s    (functions: 2.1s, scopes: 0.4s, modules: 0.9s)
  rename:        4m 18s  (68,601 functions, 20,560 modules)
    p50:  38ms   p95: 220ms   p99: 1.4s   max: 8.2s
    concurrency:  avg 42/50 slots used, 12 stalls
    dep-bound:    34% of wall time (ready queue empty)
  babel:         4.2s
  prettier:      2.8s
  file-io:       1.1s

Profile written to: ./profile.json
View at: chrome://tracing (load profile.json)
```

## File Structure

```
src/profiling/
  profiler.ts          # Profiler class, SpanHandle, NULL_PROFILER
  profiler.test.ts     # Unit tests
  types.ts             # ProfileSpan, ProfileReport, ConcurrencySnapshot, StageSummary
  trace-events.ts      # Chrome Trace Event format converter
  trace-events.test.ts # Converter tests
  summary.ts           # Human-readable summary formatter
  index.ts             # Public API re-exports
```

## Files to Modify

| File | Change |
|------|--------|
| `src/unminify.ts` | Accept `Profiler` option, wrap top-level stages |
| `src/commands/unified.ts` | Add `--profile <path>` flag, create profiler, write output |
| `src/plugins/rename.ts` | Pass profiler to processor, wrap generate stage |
| `src/plugins/babel/babel.ts` | Accept profiler, wrap transforms |
| `src/rename/processor.ts` | Per-function spans, concurrency sampling, wait tracking |
| `src/analysis/function-graph.ts` | Per-pass span instrumentation |

## Testing

### Unit Tests

1. **Profiler core**: Start/end spans, nesting, metadata, finalize produces valid report
2. **Disabled profiler**: NULL_PROFILER methods are callable and produce no output
3. **Concurrency tracking**: Snapshots recorded at correct intervals, stopped on finalize
4. **Trace event converter**: Spans → Chrome Trace Events with correct timestamps and categories
5. **Summary formatter**: Produces expected output for known profile data

### Integration Tests

1. **E2E with `--profile`**: Run on a small fixture, verify JSON output is valid and contains expected stages
2. **Zero overhead when disabled**: Profile off → no measurable regression (benchmark parse+rename on a fixture with and without profiler)
3. **Large graph**: Verify profiler handles 50K+ spans without excessive memory

### Manual Verification

```bash
# Generate a profile
npx tsx src/index.ts test-fixture.js -o /tmp/out --profile /tmp/profile.json --endpoint ... --api-key ...

# View in browser
# Open chrome://tracing, click "Load", select /tmp/profile.json

# Quick summary only (profile still written)
npx tsx src/index.ts test-fixture.js -o /tmp/out --profile /tmp/profile.json --endpoint ...
# Summary prints to console automatically
```

## Implementation Phases

### Phase 1: Core + Top-Level Stages

- [ ] `Profiler` class with span tracking and `NULL_PROFILER`
- [ ] `ProfileReport` and `StageSummary` types
- [ ] `--profile <path>` CLI flag
- [ ] Instrument `unminify.ts` top-level stages (detection, unpack, library-detection)
- [ ] Human-readable summary formatter
- [ ] Unit tests for profiler core

### Phase 2: Rename Instrumentation

- [ ] Per-function spans in `processor.ts` with wait/context/LLM breakdown
- [ ] Concurrency sampling (250ms snapshots)
- [ ] Dependency stall detection
- [ ] Percentile calculation for per-function timing
- [ ] Update summary to include rename breakdown

### Phase 3: Chrome Trace Export

- [ ] `toTraceEvents()` converter
- [ ] Concurrency counter events
- [ ] Verify loads in chrome://tracing and Perfetto UI

### Phase 4: Graph + Transform Instrumentation

- [ ] Per-pass spans in `function-graph.ts`
- [ ] Babel transform spans
- [ ] Prettier spans
- [ ] File I/O spans

## Open Questions

1. **Sampling vs full instrumentation**: For 70K+ functions, per-function spans generate a lot of data. Should we support a sampling mode (e.g., 1-in-10 functions) or is the full trace manageable? Chrome Trace format handles millions of events, so likely fine.

2. **Profile size**: A 70K-function run with per-function spans, concurrency snapshots every 250ms over 4 hours, and metadata could produce a 50MB+ JSON file. Consider: (a) streaming JSON write, (b) optional gzip, (c) span filtering by minimum duration.

3. **Profiler propagation**: The profiler needs to reach `processor.ts`, `function-graph.ts`, and plugins. Options: (a) pass through options objects (explicit, more churn), (b) singleton/context pattern (less churn, harder to test). Recommend option (a) — explicit is better for this codebase's style.

4. **Interaction with `--log-file`**: If both `--profile` and `--log-file` are used, they should be independent. Profile captures structured timing; log file captures debug text output.

5. **CPU time vs wall time**: `performance.now()` gives wall time. Node.js `process.cpuUsage()` gives CPU time but only for the whole process. Per-span CPU time isn't feasible without native profiling. Wall time is sufficient for identifying bottlenecks since the pipeline is I/O-bound (LLM requests dominate).
