import { performance } from "node:perf_hooks";
import {
  type ConcurrencySnapshot,
  type ProfileReport,
  type ProfileSpan,
  type RenameTiming,
  type SpanHandle,
  type StageSummary,
  TRACE_TID
} from "./types.js";

/** No-op span handle — returned when profiling is disabled. */
const NOOP_HANDLE: SpanHandle = { end() {} };

/**
 * Span-based profiler for the humanify pipeline.
 *
 * When disabled, all methods are no-ops with zero overhead.
 * When enabled, records spans with `performance.now()` relative to profile start.
 *
 * Thread IDs (tid) are used for grouping in Chrome Trace Event format:
 * - tid 1: pipeline-level spans (parse, graph-build, generate, etc.)
 * - tid 2: per-function rename spans
 * - tid 3: module binding spans
 */
export class Profiler {
  /** Whether profiling is active. Check this to avoid hot-path overhead. */
  readonly isEnabled: boolean;
  private enabled: boolean;
  private startTime: number = 0;
  private startedAt: string = "";
  private spans: ProfileSpan[] = [];
  private concurrencySnapshots: ConcurrencySnapshot[] = [];
  private samplingTimer: ReturnType<typeof setInterval> | null = null;

  constructor(enabled: boolean) {
    this.isEnabled = enabled;
    this.enabled = enabled;
    if (enabled) {
      this.startTime = performance.now();
      this.startedAt = new Date().toISOString();
    }
  }

  /**
   * Start a timing span. Call `.end()` on the returned handle to close it.
   *
   * @param name Human-readable name (e.g., "parse", "fn:input.js:5:0")
   * @param category Category for grouping (e.g., "pipeline", "rename")
   * @param tid Thread ID for trace event grouping (default: 1)
   * @param metadata Optional metadata to attach to the span
   */
  startSpan(
    name: string,
    category: string,
    tid: number = 1,
    metadata?: Record<string, unknown>
  ): SpanHandle {
    if (!this.enabled) return NOOP_HANDLE;

    const startMs = performance.now() - this.startTime;

    return {
      end: (endMetadata?: Record<string, unknown>) => {
        const endMs = performance.now() - this.startTime;
        const combined =
          metadata || endMetadata ? { ...metadata, ...endMetadata } : undefined;
        this.spans.push({
          name,
          category,
          startMs,
          endMs,
          tid,
          metadata: combined
        });
      }
    };
  }

  /** Record a concurrency snapshot. */
  recordConcurrency(snapshot: Omit<ConcurrencySnapshot, "timeMs">): void {
    if (!this.enabled) return;
    this.concurrencySnapshots.push({
      timeMs: performance.now() - this.startTime,
      ...snapshot
    });
  }

  /**
   * Start periodic concurrency sampling.
   *
   * @param sampler Function that returns current concurrency state
   * @param intervalMs Sampling interval (default: 250ms)
   */
  startConcurrencySampling(
    sampler: () => Omit<ConcurrencySnapshot, "timeMs">,
    intervalMs: number = 250
  ): void {
    if (!this.enabled) return;
    this.stopConcurrencySampling();
    this.samplingTimer = setInterval(() => {
      this.recordConcurrency(sampler());
    }, intervalMs);
  }

  /** Stop periodic concurrency sampling. */
  stopConcurrencySampling(): void {
    if (this.samplingTimer !== null) {
      clearInterval(this.samplingTimer);
      this.samplingTimer = null;
    }
  }

  /**
   * Finalize the profile and produce a report.
   *
   * @param meta Additional metadata (e.g., input file path)
   */
  finalize(meta?: { inputFile?: string }): ProfileReport {
    this.stopConcurrencySampling();

    if (!this.enabled) {
      return {
        spans: [],
        concurrencySnapshots: [],
        stageSummaries: [],
        meta: { totalDurationMs: 0, startedAt: "" }
      };
    }

    const totalDurationMs = performance.now() - this.startTime;

    // Compute stage summaries from pipeline-level spans (tid=1)
    const stageMap = new Map<string, { durationMs: number; count: number }>();
    for (const span of this.spans) {
      if (span.tid === 1) {
        const existing = stageMap.get(span.name);
        const dur = span.endMs - span.startMs;
        if (existing) {
          existing.durationMs += dur;
          existing.count++;
        } else {
          stageMap.set(span.name, { durationMs: dur, count: 1 });
        }
      }
    }

    const stageSummaries: StageSummary[] = [];
    for (const [name, data] of stageMap) {
      stageSummaries.push({
        name,
        durationMs: data.durationMs,
        spanCount: data.count
      });
    }

    // Compute rename timing percentiles from rename spans (tid=2)
    const renameDurations: number[] = [];
    for (const span of this.spans) {
      if (
        span.category === "rename" &&
        span.tid === TRACE_TID.RENAME_FUNCTION
      ) {
        renameDurations.push(span.endMs - span.startMs);
      }
    }

    let renameTiming: RenameTiming | undefined;
    if (renameDurations.length > 0) {
      renameDurations.sort((a, b) => a - b);
      renameTiming = {
        p50: computePercentile(renameDurations, 50),
        p95: computePercentile(renameDurations, 95),
        p99: computePercentile(renameDurations, 99),
        minMs: renameDurations[0],
        maxMs: renameDurations[renameDurations.length - 1],
        count: renameDurations.length
      };
    }

    return {
      spans: this.spans,
      concurrencySnapshots: this.concurrencySnapshots,
      stageSummaries,
      renameTiming,
      meta: {
        totalDurationMs,
        startedAt: this.startedAt,
        inputFile: meta?.inputFile
      }
    };
  }
}

/** Compute a percentile from a sorted array of numbers. */
export function computePercentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.ceil((p / 100) * sorted.length) - 1;
  return sorted[Math.max(0, idx)];
}

/** Singleton no-op profiler for when profiling is disabled. */
export const NULL_PROFILER = new Profiler(false);
