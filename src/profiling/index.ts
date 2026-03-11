/**
 * Performance profiling for the humanify pipeline.
 *
 * This module instruments humanify's internal stages (graph building, rename
 * processing, code generation, etc.) to measure where time is spent during
 * a humanify run. This is a developer/operator tool for optimizing humanify,
 * not related to the JavaScript code being unminified.
 *
 * Usage:
 *   humanify input.js --profile /tmp/profile.json
 *
 * The output is Chrome Trace Event format, viewable at:
 *   - chrome://tracing
 *   - https://ui.perfetto.dev/
 *   - https://www.speedscope.app/
 */

export { Profiler, NULL_PROFILER, computePercentile } from "./profiler.js";
export { toTraceEvents } from "./trace-events.js";
export { formatProfileSummary } from "./summary.js";
export type {
  ProfileSpan,
  ConcurrencySnapshot,
  StageSummary,
  RenameTiming,
  ProfileReport,
  SpanHandle
} from "./types.js";
