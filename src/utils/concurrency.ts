/**
 * Creates a simple concurrency limiter.
 * Limits the number of concurrent async operations.
 */
export function createConcurrencyLimiter(
  concurrency: number
): <T>(fn: () => Promise<T>) => Promise<T> {
  let running = 0;
  const queue: Array<{
    fn: () => Promise<unknown>;
    resolve: (value: unknown) => void;
    reject: (error: unknown) => void;
  }> = [];

  async function run<T>(fn: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      queue.push({ fn, resolve: resolve as (v: unknown) => void, reject });
      processQueue();
    });
  }

  function processQueue(): void {
    while (running < concurrency && queue.length > 0) {
      const item = queue.shift()!;
      running++;

      item
        .fn()
        .then((result) => {
          running--;
          item.resolve(result);
          processQueue();
        })
        .catch((error) => {
          running--;
          item.reject(error);
          processQueue();
        });
    }
  }

  return run;
}
