import assert from "node:assert";
import { describe, it } from "node:test";
import { createConcurrencyLimiter, createSemaphore } from "./concurrency.js";

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

describe("createSemaphore", () => {
  it("limits concurrent operations to permit count", async () => {
    const semaphore = createSemaphore(2);
    let running = 0;
    let maxRunning = 0;

    const task = async () => {
      await semaphore.acquire();
      running++;
      maxRunning = Math.max(maxRunning, running);
      await new Promise((r) => setTimeout(r, 10));
      running--;
      semaphore.release();
    };

    await Promise.all([task(), task(), task(), task(), task()]);

    assert.ok(
      maxRunning <= 2,
      `Max concurrent should be <= 2, got ${maxRunning}`
    );
    assert.strictEqual(running, 0, "All tasks should complete");
  });

  it("allows up to permit count immediately", async () => {
    const semaphore = createSemaphore(3);
    let acquired = 0;

    // These should all resolve immediately
    semaphore.acquire().then(() => acquired++);
    semaphore.acquire().then(() => acquired++);
    semaphore.acquire().then(() => acquired++);

    // Flush microtasks
    await Promise.resolve();
    await Promise.resolve();

    assert.strictEqual(acquired, 3, "Should acquire 3 permits immediately");

    semaphore.release();
    semaphore.release();
    semaphore.release();
  });

  it("queues when all permits are taken", async () => {
    const semaphore = createSemaphore(1);
    const order: string[] = [];

    await semaphore.acquire();
    order.push("first-acquired");

    let secondResolved = false;
    const p2 = semaphore.acquire().then(() => {
      secondResolved = true;
      order.push("second-acquired");
    });

    // Second acquire should be queued
    await Promise.resolve();
    assert.strictEqual(
      secondResolved,
      false,
      "Second acquire should be queued"
    );

    semaphore.release();
    await p2;
    assert.strictEqual(
      secondResolved,
      true,
      "Second acquire should resolve after release"
    );

    semaphore.release();
  });
});
