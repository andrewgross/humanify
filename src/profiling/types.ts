/**
 * Types for the humanify performance profiler.
 *
 * The profiler measures how long each stage of the humanify pipeline takes
 * (graph building, rename processing, code generation, etc.) to guide
 * optimization of the tool itself.
 */

/** A completed timing span. */
export interface ProfileSpan {
  /** Human-readable span name (e.g., "parse", "graph-build", "fn:input.js:5:0") */
  name: string;
  /** Category for grouping (e.g., "pipeline", "rename", "graph", "io") */
  category: string;
  /** Start time in milliseconds relative to profile start */
  startMs: number;
  /** End time in milliseconds relative to profile start */
  endMs: number;
  /** Thread ID for trace event grouping */
  tid: number;
  /** Optional metadata */
  metadata?: Record<string, unknown>;
}

/** A point-in-time snapshot of concurrency utilization. */
export interface ConcurrencySnapshot {
  /** Time in milliseconds relative to profile start */
  timeMs: number;
  /** Number of functions currently being processed by the LLM */
  inFlight: number;
  /** Number of functions ready to dispatch */
  ready: number;
  /** Number of functions blocked on dependencies */
  blocked: number;
}

/** Summary statistics for a pipeline stage. */
export interface StageSummary {
  /** Stage name */
  name: string;
  /** Total duration in milliseconds */
  durationMs: number;
  /** Number of spans in this stage */
  spanCount: number;
}

/** Per-function rename timing statistics. */
export interface RenameTiming {
  /** p50 duration in milliseconds */
  p50: number;
  /** p95 duration in milliseconds */
  p95: number;
  /** p99 duration in milliseconds */
  p99: number;
  /** Minimum duration */
  minMs: number;
  /** Maximum duration */
  maxMs: number;
  /** Total rename span count */
  count: number;
}

/** The complete profile report. */
export interface ProfileReport {
  /** All recorded spans */
  spans: ProfileSpan[];
  /** Concurrency snapshots over time */
  concurrencySnapshots: ConcurrencySnapshot[];
  /** Per-stage summaries */
  stageSummaries: StageSummary[];
  /** Rename timing percentiles (if rename spans were recorded) */
  renameTiming?: RenameTiming;
  /** Profile metadata */
  meta: {
    /** Total profile duration in milliseconds */
    totalDurationMs: number;
    /** ISO timestamp when profile started */
    startedAt: string;
    /** Input file path */
    inputFile?: string;
  };
}

/** Handle returned by startSpan() — call end() to close the span. */
export interface SpanHandle {
  /** End the span and record it. Optionally add metadata. */
  end(metadata?: Record<string, unknown>): void;
}

/** Thread IDs for Chrome Trace Event grouping. */
export const TRACE_TID = {
  /** Pipeline-level spans (parse, graph-build, generate, etc.) */
  PIPELINE: 1,
  /** Per-function rename spans */
  RENAME_FUNCTION: 2,
  /** Module binding rename spans */
  RENAME_MODULE_BINDING: 3
} as const;
