/**
 * Metrics tracking for LLM calls and function processing.
 * Provides real-time visibility into the processing pipeline.
 */

export interface LLMMetrics {
  /** Total LLM calls made */
  totalCalls: number;

  /** Currently in-flight LLM calls */
  inFlightCalls: number;

  /** Completed LLM calls */
  completedCalls: number;

  /** Failed LLM calls (after retries exhausted) */
  failedCalls: number;

  /** Total tokens used (if available from provider) */
  totalTokens?: number;

  /** Average response time in ms */
  avgResponseTimeMs: number;
}

export interface FunctionMetrics {
  /** Total functions to process */
  total: number;

  /** Functions completed */
  completed: number;

  /** Functions currently being processed */
  inProgress: number;

  /** Functions waiting (dependencies not met) */
  pending: number;

  /** Functions ready to process (dependencies met) */
  ready: number;
}

export interface ProcessingMetrics {
  llm: LLMMetrics;
  functions: FunctionMetrics;

  /** Processing start time */
  startTime: number;

  /** Elapsed time in ms */
  elapsedMs: number;

  /** Estimated time remaining in ms (based on current rate) */
  estimatedRemainingMs?: number;
}

export type MetricsCallback = (metrics: ProcessingMetrics) => void;

/**
 * Tracks metrics throughout the processing pipeline.
 */
export class MetricsTracker {
  private llmCalls = 0;
  private llmInFlight = 0;
  private llmCompleted = 0;
  private llmFailed = 0;
  private llmTotalTokens = 0;
  private llmResponseTimes: number[] = [];

  private fnTotal = 0;
  private fnCompleted = 0;
  private fnInProgress = 0;
  private fnPending = 0;
  private fnReady = 0;

  private startTime = Date.now();
  private callback?: MetricsCallback;
  private throttleMs: number;
  private lastCallbackTime = 0;

  constructor(options: { onMetrics?: MetricsCallback; throttleMs?: number } = {}) {
    this.callback = options.onMetrics;
    this.throttleMs = options.throttleMs ?? 100; // Default 100ms throttle
  }

  // ============ LLM Metrics ============

  /** Call when starting an LLM request */
  llmCallStart(): () => void {
    this.llmCalls++;
    this.llmInFlight++;
    const startTime = Date.now();
    this.emitThrottled();

    // Return a function to call when done
    return () => {
      this.llmInFlight--;
      this.llmCompleted++;
      this.llmResponseTimes.push(Date.now() - startTime);
      this.emitThrottled();
    };
  }

  /** Call when an LLM request fails (after retries) */
  llmCallFailed(): void {
    this.llmInFlight--;
    this.llmFailed++;
    this.emitThrottled();
  }

  /** Record token usage if available */
  recordTokens(tokens: number): void {
    this.llmTotalTokens += tokens;
  }

  // ============ Function Metrics ============

  /** Set total function count */
  setFunctionTotal(total: number): void {
    this.fnTotal = total;
    this.fnPending = total;
    this.emitThrottled();
  }

  /** Update function processing state */
  updateFunctionState(state: {
    completed?: number;
    inProgress?: number;
    pending?: number;
    ready?: number;
  }): void {
    if (state.completed !== undefined) this.fnCompleted = state.completed;
    if (state.inProgress !== undefined) this.fnInProgress = state.inProgress;
    if (state.pending !== undefined) this.fnPending = state.pending;
    if (state.ready !== undefined) this.fnReady = state.ready;
    this.emitThrottled();
  }

  /** Convenience: mark a function as started */
  functionStarted(): void {
    this.fnInProgress++;
    this.fnReady = Math.max(0, this.fnReady - 1);
    this.emitThrottled();
  }

  /** Convenience: mark a function as completed */
  functionCompleted(): void {
    this.fnInProgress = Math.max(0, this.fnInProgress - 1);
    this.fnCompleted++;
    this.emitThrottled();
  }

  /** Mark functions as ready */
  functionsReady(count: number): void {
    this.fnReady += count;
    this.fnPending = Math.max(0, this.fnPending - count);
    this.emitThrottled();
  }

  // ============ Metrics Retrieval ============

  /** Get current metrics snapshot */
  getMetrics(): ProcessingMetrics {
    const elapsedMs = Date.now() - this.startTime;
    const avgResponseTimeMs = this.llmResponseTimes.length > 0
      ? this.llmResponseTimes.reduce((a, b) => a + b, 0) / this.llmResponseTimes.length
      : 0;

    // Estimate remaining time based on completion rate
    let estimatedRemainingMs: number | undefined;
    if (this.fnCompleted > 0) {
      const msPerFunction = elapsedMs / this.fnCompleted;
      const remaining = this.fnTotal - this.fnCompleted;
      estimatedRemainingMs = Math.round(msPerFunction * remaining);
    }

    return {
      llm: {
        totalCalls: this.llmCalls,
        inFlightCalls: this.llmInFlight,
        completedCalls: this.llmCompleted,
        failedCalls: this.llmFailed,
        totalTokens: this.llmTotalTokens > 0 ? this.llmTotalTokens : undefined,
        avgResponseTimeMs: Math.round(avgResponseTimeMs)
      },
      functions: {
        total: this.fnTotal,
        completed: this.fnCompleted,
        inProgress: this.fnInProgress,
        pending: this.fnPending,
        ready: this.fnReady
      },
      startTime: this.startTime,
      elapsedMs,
      estimatedRemainingMs
    };
  }

  /** Force emit current metrics (bypasses throttle) */
  emit(): void {
    if (this.callback) {
      this.callback(this.getMetrics());
    }
  }

  private emitThrottled(): void {
    const now = Date.now();
    if (now - this.lastCallbackTime >= this.throttleMs) {
      this.lastCallbackTime = now;
      this.emit();
    }
  }

  /** Reset all metrics */
  reset(): void {
    this.llmCalls = 0;
    this.llmInFlight = 0;
    this.llmCompleted = 0;
    this.llmFailed = 0;
    this.llmTotalTokens = 0;
    this.llmResponseTimes = [];
    this.fnTotal = 0;
    this.fnCompleted = 0;
    this.fnInProgress = 0;
    this.fnPending = 0;
    this.fnReady = 0;
    this.startTime = Date.now();
  }
}

/**
 * Formats metrics for console output.
 */
export function formatMetrics(metrics: ProcessingMetrics): string {
  const { llm, functions, elapsedMs, estimatedRemainingMs } = metrics;

  const elapsed = formatDuration(elapsedMs);
  const eta = estimatedRemainingMs ? formatDuration(estimatedRemainingMs) : "calculating...";

  const lines = [
    `Functions: ${functions.completed}/${functions.total} done | ${functions.inProgress} processing | ${functions.ready} ready | ${functions.pending} pending`,
    `LLM Calls: ${llm.completedCalls} done | ${llm.inFlightCalls} in-flight | ${llm.failedCalls} failed | avg ${llm.avgResponseTimeMs}ms`,
    `Time: ${elapsed} elapsed | ETA: ${eta}`
  ];

  if (llm.totalTokens) {
    lines.push(`Tokens: ${llm.totalTokens.toLocaleString()}`);
  }

  return lines.join("\n");
}

/**
 * Formats metrics as a single-line status.
 */
export function formatMetricsCompact(metrics: ProcessingMetrics): string {
  const { llm, functions, elapsedMs, estimatedRemainingMs } = metrics;
  const pct = functions.total > 0
    ? Math.round((functions.completed / functions.total) * 100)
    : 0;
  const eta = estimatedRemainingMs ? formatDuration(estimatedRemainingMs) : "...";

  return `[${pct}%] ${functions.completed}/${functions.total} functions | LLM: ${llm.inFlightCalls} in-flight | ETA: ${eta}`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.round((ms % 60000) / 1000);
  return `${mins}m ${secs}s`;
}
