/**
 * Fixture for test/clone-census.test.ts (red case). `collectRetryDelays` is a
 * DELIBERATE byte-identical cross-file twin of the copy in alpha.ts — the
 * test asserts the strict census exits 1 and names this group. Never imported
 * by production code.
 */
export interface RetryOptions {
  attempts: number;
  baseDelayMs: number;
  maxJitterMs: number;
}

export function collectRetryDelays(options: RetryOptions): number[] {
  const delays: number[] = [];
  for (let attempt = 0; attempt < options.attempts; attempt++) {
    const backoff = options.baseDelayMs * 2 ** attempt;
    const jitter = Math.min(backoff * 0.1, options.maxJitterMs);
    delays.push(Math.round(backoff + jitter));
  }
  if (delays.length === 0) {
    delays.push(options.baseDelayMs);
  }
  return delays;
}

export function betaFormatDelay(delayMs: number): string {
  if (delayMs < 1000) {
    return `${delayMs}ms`;
  }
  return `${(delayMs / 1000).toFixed(2)}s`;
}
