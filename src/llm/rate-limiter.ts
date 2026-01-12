import type { LLMContext } from "../analysis/types.js";
import type { LLMProvider, NameSuggestion, RateLimitConfig } from "./types.js";
import type { MetricsTracker } from "./metrics.js";

/**
 * Wraps an LLM provider with rate limiting and retry logic.
 *
 * Features:
 * - Concurrency limiting
 * - Requests per minute limiting
 * - Automatic retries with exponential backoff
 * - Optional metrics tracking
 */
export class RateLimitedProvider implements LLMProvider {
  private inner: LLMProvider;
  private config: Required<RateLimitConfig>;
  private metrics?: MetricsTracker;

  // Concurrency tracking
  private running = 0;
  private queue: Array<{
    fn: () => Promise<unknown>;
    resolve: (value: unknown) => void;
    reject: (error: unknown) => void;
  }> = [];

  // Rate limiting tracking
  private requestTimestamps: number[] = [];

  constructor(
    inner: LLMProvider,
    config: Partial<RateLimitConfig> = {},
    metrics?: MetricsTracker
  ) {
    this.inner = inner;
    this.config = {
      maxConcurrent: config.maxConcurrent ?? 50,
      requestsPerMinute: config.requestsPerMinute ?? 0,
      retryAttempts: config.retryAttempts ?? 3,
      retryDelayMs: config.retryDelayMs ?? 1000
    };
    this.metrics = metrics;
  }

  async suggestName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    return this.withRateLimit(() =>
      this.withRetry(() => this.inner.suggestName(currentName, context))
    );
  }

  async suggestFunctionName(
    currentName: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    if (this.inner.suggestFunctionName) {
      return this.withRateLimit(() =>
        this.withRetry(() => this.inner.suggestFunctionName!(currentName, context))
      );
    }
    return this.suggestName(currentName, context);
  }

  async retrySuggestName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    if (this.inner.retrySuggestName) {
      return this.withRateLimit(() =>
        this.withRetry(() =>
          this.inner.retrySuggestName!(currentName, rejectedName, reason, context)
        )
      );
    }
    // Fallback: re-call suggestName with rejected name added to used set
    const updatedContext = {
      ...context,
      usedIdentifiers: new Set([...context.usedIdentifiers, rejectedName])
    };
    return this.suggestName(currentName, updatedContext);
  }

  async retryFunctionName(
    currentName: string,
    rejectedName: string,
    reason: string,
    context: LLMContext
  ): Promise<NameSuggestion> {
    if (this.inner.retryFunctionName) {
      return this.withRateLimit(() =>
        this.withRetry(() =>
          this.inner.retryFunctionName!(currentName, rejectedName, reason, context)
        )
      );
    }
    // Fallback: try retrySuggestName or suggestFunctionName
    if (this.inner.retrySuggestName) {
      return this.retrySuggestName(currentName, rejectedName, reason, context);
    }
    const updatedContext = {
      ...context,
      usedIdentifiers: new Set([...context.usedIdentifiers, rejectedName])
    };
    return this.suggestFunctionName(currentName, updatedContext);
  }

  async suggestNames(
    requests: Array<{ name: string; context: LLMContext }>
  ): Promise<NameSuggestion[]> {
    if (this.inner.suggestNames) {
      return this.withRateLimit(() =>
        this.withRetry(() => this.inner.suggestNames!(requests))
      );
    }
    // Fall back to individual requests
    const results: NameSuggestion[] = [];
    for (const req of requests) {
      const suggestion = await this.suggestName(req.name, req.context);
      results.push(suggestion);
    }
    return results;
  }

  /**
   * Executes a function with concurrency and rate limiting.
   */
  private async withRateLimit<T>(fn: () => Promise<T>): Promise<T> {
    // Wait for rate limit (requests per minute)
    await this.waitForRateLimit();

    // Queue for concurrency
    return new Promise<T>((resolve, reject) => {
      this.queue.push({
        fn: async () => {
          this.recordRequest();
          return fn();
        },
        resolve: resolve as (v: unknown) => void,
        reject
      });
      this.processQueue();
    });
  }

  /**
   * Waits if we've exceeded the requests per minute limit.
   */
  private async waitForRateLimit(): Promise<void> {
    if (this.config.requestsPerMinute <= 0) {
      return;
    }

    const now = Date.now();
    const oneMinuteAgo = now - 60000;

    // Remove timestamps older than one minute
    this.requestTimestamps = this.requestTimestamps.filter(
      (ts) => ts > oneMinuteAgo
    );

    // If we're at the limit, wait until the oldest request expires
    if (this.requestTimestamps.length >= this.config.requestsPerMinute) {
      const oldestTimestamp = this.requestTimestamps[0];
      const waitTime = oldestTimestamp - oneMinuteAgo;
      if (waitTime > 0) {
        await sleep(waitTime);
      }
    }
  }

  /**
   * Records a request timestamp for rate limiting.
   */
  private recordRequest(): void {
    if (this.config.requestsPerMinute > 0) {
      this.requestTimestamps.push(Date.now());
    }
  }

  /**
   * Processes the queue respecting concurrency limits.
   */
  private processQueue(): void {
    while (
      this.running < this.config.maxConcurrent &&
      this.queue.length > 0
    ) {
      const item = this.queue.shift()!;
      this.running++;

      item
        .fn()
        .then((result) => {
          this.running--;
          item.resolve(result);
          this.processQueue();
        })
        .catch((error) => {
          this.running--;
          item.reject(error);
          this.processQueue();
        });
    }
  }

  /**
   * Executes a function with retries and exponential backoff.
   */
  private async withRetry<T>(fn: () => Promise<T>): Promise<T> {
    let lastError: unknown;

    // Track LLM call start
    const done = this.metrics?.llmCallStart();

    for (let attempt = 0; attempt <= this.config.retryAttempts; attempt++) {
      try {
        const result = await fn();
        done?.(); // Mark as completed
        return result;
      } catch (error) {
        lastError = error;

        // Don't retry if we've exhausted attempts
        if (attempt === this.config.retryAttempts) {
          break;
        }

        // Check if error is retryable
        if (!this.isRetryableError(error)) {
          this.metrics?.llmCallFailed();
          throw error;
        }

        // Exponential backoff
        const delay = this.config.retryDelayMs * Math.pow(2, attempt);
        await sleep(delay);
      }
    }

    // All retries exhausted
    this.metrics?.llmCallFailed();
    throw lastError;
  }

  /**
   * Determines if an error is retryable.
   */
  private isRetryableError(error: unknown): boolean {
    // Retry on network errors
    if (error instanceof Error) {
      const message = error.message.toLowerCase();
      if (
        message.includes("network") ||
        message.includes("timeout") ||
        message.includes("econnreset") ||
        message.includes("econnrefused") ||
        message.includes("rate limit") ||
        message.includes("429") ||
        message.includes("500") ||
        message.includes("502") ||
        message.includes("503") ||
        message.includes("504")
      ) {
        return true;
      }
    }

    // Check for HTTP status code errors
    if (typeof error === "object" && error !== null && "status" in error) {
      const status = (error as { status: number }).status;
      // Retry on rate limits and server errors
      if (status === 429 || (status >= 500 && status < 600)) {
        return true;
      }
    }

    return false;
  }
}

/**
 * Sleep for a given number of milliseconds.
 */
function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * Creates a rate-limited wrapper around an LLM provider.
 */
export function withRateLimit(
  provider: LLMProvider,
  config?: Partial<RateLimitConfig>,
  metrics?: MetricsTracker
): RateLimitedProvider {
  return new RateLimitedProvider(provider, config, metrics);
}
