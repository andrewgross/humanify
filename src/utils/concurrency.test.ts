import assert from "node:assert";
import { describe, it } from "node:test";
import { createConcurrencyLimiter } from "./concurrency.js";

describe("createConcurrencyLimiter", () => {
  it("limits concurrent executions", async () => {
    const limit = createConcurrencyLimiter(2);
    let running = 0;
    let maxRunning = 0;

    const task = () =>
      limit(async () => {
        running++;
        maxRunning = Math.max(maxRunning, running);
        await new Promise((r) => setTimeout(r, 10));
        running--;
      });

    await Promise.all([task(), task(), task(), task(), task()]);

    assert.ok(
      maxRunning <= 2,
      `Max concurrent should be <= 2, got ${maxRunning}`
    );
    assert.strictEqual(running, 0, "All tasks should complete");
  });

  it("executes all tasks", async () => {
    const limit = createConcurrencyLimiter(3);
    const results: number[] = [];

    const tasks = [1, 2, 3, 4, 5].map((n) =>
      limit(async () => {
        results.push(n);
        return n;
      })
    );

    const returned = await Promise.all(tasks);
    assert.deepStrictEqual(returned, [1, 2, 3, 4, 5]);
    assert.strictEqual(results.length, 5);
  });

  it("propagates errors", async () => {
    const limit = createConcurrencyLimiter(2);

    await assert.rejects(
      () =>
        limit(async () => {
          throw new Error("test error");
        }),
      { message: "test error" }
    );
  });
});
