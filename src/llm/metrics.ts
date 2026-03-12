/**
 * Metrics tracking for LLM calls and function processing.
 * Provides real-time visibility into the processing pipeline.
 */

export type PipelineStage =
  | "parsing"
  | "building-graph"
  | "renaming"
  | "library-params"
  | "generating"
  | "done";

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

  /** Input (prompt) tokens used */
  inputTokens?: number;

  /** Output (completion) tokens used */
  outputTokens?: number;

  /** Number of HTTP-level retries (rate limits, server errors) */
  retries: number;

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

export interface ModuleBindingMetrics {
  /** Total module bindings to process */
  total: number;

  /** Module bindings completed */
  completed: number;

  /** Module bindings currently being processed */
  inProgress: number;
}

export interface ProcessingMetrics {
  llm: LLMMetrics;
  functions: FunctionMetrics;
  moduleBindings: ModuleBindingMetrics;

  /** Current pipeline stage */
  stage: PipelineStage;

  /** Processing start time */
  startTime: number;

  /** Elapsed time in ms */
  elapsedMs: number;

  /** Estimated time remaining in ms (based on current rate) */
  estimatedRemainingMs?: number;

  /** Rolling tokens per second rate */
  tokensPerSecond: number;
}

export type MetricsCallback = (metrics: ProcessingMetrics) => void;

/** Rolling window size for tokens-per-second calculation */
const TOKEN_RATE_WINDOW_MS = 30_000;

/**
 * Tracks metrics throughout the processing pipeline.
 */
export class MetricsTracker {
  private llmCalls = 0;
  private llmInFlight = 0;
  private llmCompleted = 0;
  private llmFailed = 0;
  private llmTotalTokens = 0;
  private llmInputTokens = 0;
  private llmOutputTokens = 0;
  private llmRetries = 0;
  private llmResponseTimes: number[] = [];

  private fnTotal = 0;
  private fnCompleted = 0;
  private fnInProgress = 0;
  private fnPending = 0;
  private fnReady = 0;

  private mbTotal = 0;
  private mbCompleted = 0;
  private mbInProgress = 0;

  private _stage: PipelineStage = "parsing";
  private tokenHistory: Array<{ time: number; tokens: number }> = [];

  private startTime = Date.now();
  private callback?: MetricsCallback;
  private throttleMs: number;
  private lastCallbackTime = 0;

  constructor(
    options: { onMetrics?: MetricsCallback; throttleMs?: number } = {}
  ) {
    this.callback = options.onMetrics;
    this.throttleMs = options.throttleMs ?? 100; // Default 100ms throttle
  }

  // ============ Stage ============

  /** Set the current pipeline stage (force-emits) */
  setStage(stage: PipelineStage): void {
    this._stage = stage;
    this.emit();
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
  recordTokens(tokens: number, input?: number, output?: number): void {
    this.llmTotalTokens += tokens;
    if (input) this.llmInputTokens += input;
    if (output) this.llmOutputTokens += output;
    if (tokens > 0) {
      this.tokenHistory.push({ time: Date.now(), tokens });
    }
  }

  /** Record an HTTP-level retry (rate limit, server error) */
  llmRetry(): void {
    this.llmRetries++;
    this.emitThrottled();
  }

  /** Calculate rolling tokens-per-second over a 30s window */
  getTokensPerSecond(): number {
    const now = Date.now();
    const cutoff = now - TOKEN_RATE_WINDOW_MS;

    // Remove entries outside the window
    while (this.tokenHistory.length > 0 && this.tokenHistory[0].time < cutoff) {
      this.tokenHistory.shift();
    }

    if (this.tokenHistory.length === 0) return 0;

    const totalTokens = this.tokenHistory.reduce((sum, e) => sum + e.tokens, 0);
    const windowMs = now - this.tokenHistory[0].time;
    if (windowMs < 100) return 0; // Avoid division by tiny intervals

    return Math.round(totalTokens / (windowMs / 1000));
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

  // ============ Module Binding Metrics ============

  /** Set total module binding count */
  setModuleBindingTotal(total: number): void {
    this.mbTotal = total;
    this.emitThrottled();
  }

  /** Mark a module binding as started */
  moduleBindingStarted(): void {
    this.mbInProgress++;
    this.emitThrottled();
  }

  /** Mark a module binding as completed */
  moduleBindingCompleted(): void {
    this.mbInProgress = Math.max(0, this.mbInProgress - 1);
    this.mbCompleted++;
    this.emitThrottled();
  }

  // ============ Metrics Retrieval ============

  /** Get current metrics snapshot */
  getMetrics(): ProcessingMetrics {
    const elapsedMs = Date.now() - this.startTime;
    const avgResponseTimeMs =
      this.llmResponseTimes.length > 0
        ? this.llmResponseTimes.reduce((a, b) => a + b, 0) /
          this.llmResponseTimes.length
        : 0;

    // Estimate remaining time based on combined completion rate
    let estimatedRemainingMs: number | undefined;
    const totalCompleted = this.fnCompleted + this.mbCompleted;
    const totalItems = this.fnTotal + this.mbTotal;
    if (totalCompleted > 0) {
      const msPerItem = elapsedMs / totalCompleted;
      const remaining = totalItems - totalCompleted;
      estimatedRemainingMs = Math.round(msPerItem * remaining);
    }

    return {
      llm: {
        totalCalls: this.llmCalls,
        inFlightCalls: this.llmInFlight,
        completedCalls: this.llmCompleted,
        failedCalls: this.llmFailed,
        totalTokens: this.llmTotalTokens > 0 ? this.llmTotalTokens : undefined,
        inputTokens: this.llmInputTokens > 0 ? this.llmInputTokens : undefined,
        outputTokens:
          this.llmOutputTokens > 0 ? this.llmOutputTokens : undefined,
        retries: this.llmRetries,
        avgResponseTimeMs: Math.round(avgResponseTimeMs)
      },
      functions: {
        total: this.fnTotal,
        completed: this.fnCompleted,
        inProgress: this.fnInProgress,
        pending: this.fnPending,
        ready: this.fnReady
      },
      moduleBindings: {
        total: this.mbTotal,
        completed: this.mbCompleted,
        inProgress: this.mbInProgress
      },
      stage: this._stage,
      startTime: this.startTime,
      elapsedMs,
      estimatedRemainingMs,
      tokensPerSecond: this.getTokensPerSecond()
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
    this.llmInputTokens = 0;
    this.llmOutputTokens = 0;
    this.llmRetries = 0;
    this.llmResponseTimes = [];
    this.fnTotal = 0;
    this.fnCompleted = 0;
    this.fnInProgress = 0;
    this.fnPending = 0;
    this.fnReady = 0;
    this.mbTotal = 0;
    this.mbCompleted = 0;
    this.mbInProgress = 0;
    this._stage = "parsing";
    this.tokenHistory = [];
    this.startTime = Date.now();
  }
}

/**
 * Formats metrics for console output.
 */
export function formatMetrics(metrics: ProcessingMetrics): string {
  const { llm, functions, elapsedMs, estimatedRemainingMs } = metrics;

  const elapsed = formatDuration(elapsedMs);
  const eta = estimatedRemainingMs
    ? formatDuration(estimatedRemainingMs)
    : "calculating...";

  const lines = [
    `Functions: ${functions.completed}/${functions.total} done | ${functions.inProgress} processing | ${functions.ready} ready | ${functions.pending} pending`,
    `LLM Calls: ${llm.completedCalls} done | ${llm.inFlightCalls} in-flight | ${llm.failedCalls} failed | avg ${llm.avgResponseTimeMs}ms`,
    `Time: ${elapsed} elapsed | ETA: ${eta}`
  ];

  if (metrics.moduleBindings.total > 0) {
    lines.splice(
      1,
      0,
      `Modules: ${metrics.moduleBindings.completed}/${metrics.moduleBindings.total} done | ${metrics.moduleBindings.inProgress} processing`
    );
  }

  if (llm.totalTokens) {
    lines.push(`Tokens: ${llm.totalTokens.toLocaleString()}`);
  }

  return lines.join("\n");
}

/**
 * Formats metrics as a single-line status.
 */
export function formatMetricsCompact(metrics: ProcessingMetrics): string {
  const { llm, functions, moduleBindings, estimatedRemainingMs } = metrics;
  const totalCompleted = functions.completed + moduleBindings.completed;
  const totalItems = functions.total + moduleBindings.total;
  const pct =
    totalItems > 0 ? Math.round((totalCompleted / totalItems) * 100) : 0;
  const eta = estimatedRemainingMs
    ? formatDuration(estimatedRemainingMs)
    : "...";

  let line = `[${pct}%] ${functions.completed}/${functions.total} functions`;
  if (moduleBindings.total > 0) {
    line += ` | ${moduleBindings.completed}/${moduleBindings.total} modules`;
  }
  line += ` | LLM: ${llm.inFlightCalls} in-flight | ETA: ${eta}`;

  return line;
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60000);
  const secs = Math.round((ms % 60000) / 1000);
  if (mins < 60) return `${mins}m ${secs}s`;
  const hours = Math.floor(mins / 60);
  const remainMins = mins % 60;
  return `${hours}h ${remainMins}m`;
}
