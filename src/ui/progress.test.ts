import assert from "node:assert";
import { afterEach, beforeEach, describe, it } from "node:test";
import type { ProcessingMetrics } from "../llm/metrics.js";
import { createProgressRenderer } from "./progress.js";

function makeMetrics(
  overrides: Partial<ProcessingMetrics> = {}
): ProcessingMetrics {
  return {
    llm: {
      totalCalls: 10,
      inFlightCalls: 2,
      completedCalls: 8,
      failedCalls: 0,
      totalTokens: 5000,
      retries: 0,
      avgResponseTimeMs: 200
    },
    functions: {
      total: 100,
      completed: 50,
      inProgress: 5,
      pending: 20,
      ready: 25
    },
    moduleBindings: {
      total: 20,
      completed: 10,
      inProgress: 2
    },
    stage: "renaming",
    startTime: Date.now() - 60000,
    elapsedMs: 60000,
    estimatedRemainingMs: 60000,
    tokensPerSecond: 500,
    ...overrides
  };
}

describe("LineRenderer", () => {
  let stderrWrites: string[];
  let originalWrite: typeof process.stderr.write;

  beforeEach(() => {
    stderrWrites = [];
    originalWrite = process.stderr.write;
    process.stderr.write = ((chunk: string) => {
      stderrWrites.push(chunk);
      return true;
    }) as any;
  });

  afterEach(() => {
    process.stderr.write = originalWrite;
  });

  it("creates a line renderer for non-TTY", () => {
    const renderer = createProgressRenderer({ tty: false });
    assert.ok(renderer);
    assert.ok(typeof renderer.update === "function");
    assert.ok(typeof renderer.message === "function");
    assert.ok(typeof renderer.finish === "function");
  });

  it("message() writes to stderr", () => {
    const renderer = createProgressRenderer({ tty: false });
    renderer.message("test message");
    assert.ok(stderrWrites.some((w) => w.includes("test message")));
  });

  it("update() throttles output", () => {
    const renderer = createProgressRenderer({ tty: false });
    const metrics = makeMetrics();

    // First update should emit
    renderer.update(metrics);
    const firstCount = stderrWrites.length;
    assert.ok(firstCount > 0, "First update should emit");

    // Rapid subsequent updates should be throttled
    renderer.update(metrics);
    renderer.update(metrics);
    assert.strictEqual(
      stderrWrites.length,
      firstCount,
      "Should throttle rapid updates"
    );
  });

  it("update() emits on stage change", () => {
    const renderer = createProgressRenderer({ tty: false });

    renderer.update(makeMetrics({ stage: "renaming" }));
    const firstCount = stderrWrites.length;

    // Stage change should force output even if within throttle window
    renderer.update(makeMetrics({ stage: "generating" }));
    assert.ok(
      stderrWrites.length > firstCount,
      "Stage change should force output"
    );
  });

  it("update() includes function and module counts", () => {
    const renderer = createProgressRenderer({ tty: false });
    renderer.update(makeMetrics());

    const output = stderrWrites.join("");
    assert.ok(output.includes("functions"), "Should include functions");
    assert.ok(output.includes("modules"), "Should include modules");
  });

  it("finish() is safe to call multiple times", () => {
    const renderer = createProgressRenderer({ tty: false });
    renderer.finish();
    renderer.finish(); // Should not throw
  });
});

describe("TtyRenderer", () => {
  let stderrWrites: string[];
  let originalWrite: typeof process.stderr.write;

  beforeEach(() => {
    stderrWrites = [];
    originalWrite = process.stderr.write;
    process.stderr.write = ((chunk: string) => {
      stderrWrites.push(chunk);
      return true;
    }) as any;
  });

  afterEach(() => {
    process.stderr.write = originalWrite;
  });

  it("creates a TTY renderer", () => {
    const renderer = createProgressRenderer({ tty: true });
    assert.ok(renderer);
    renderer.finish(); // Clean up interval
  });

  it("message() queues messages for display", () => {
    const renderer = createProgressRenderer({ tty: true });
    renderer.message("Graph built: 100 functions");
    // Message will be printed on next redraw
    renderer.finish();
    assert.ok(stderrWrites.some((w) => w.includes("Graph built")));
  });

  it("finish() prints final summary", () => {
    const renderer = createProgressRenderer({ tty: true });
    renderer.update(makeMetrics({ stage: "done" }));
    renderer.finish();

    const output = stderrWrites.join("");
    assert.ok(output.includes("Done"), "Should print done message");
  });

  it("finish() is idempotent", () => {
    const renderer = createProgressRenderer({ tty: true });
    renderer.finish();
    const count = stderrWrites.length;
    renderer.finish();
    assert.strictEqual(
      stderrWrites.length,
      count,
      "Second finish should be no-op"
    );
  });

  it("computes fresh elapsed from startTime", async () => {
    const renderer = createProgressRenderer({ tty: true });
    // Give metrics with startTime 2 seconds ago but stale elapsedMs of 0
    const staleMetrics = makeMetrics({
      startTime: Date.now() - 2000,
      elapsedMs: 0 // stale
    });
    renderer.update(staleMetrics);

    // Wait for a redraw
    await new Promise((r) => setTimeout(r, 300));
    renderer.finish();

    // The output should show elapsed time > 0 (computed fresh from startTime, not from stale elapsedMs)
    const output = stderrWrites.join("");
    // Should NOT show "0ms" or "0.0s" for elapsed — it should show ~2s
    assert.ok(
      !output.includes("elapsed 0ms"),
      "Should compute fresh elapsed, not use stale elapsedMs"
    );
  });

  it("shows retry count when retries > 0", () => {
    const renderer = createProgressRenderer({ tty: true });
    renderer.update(
      makeMetrics({
        llm: {
          totalCalls: 10,
          inFlightCalls: 2,
          completedCalls: 8,
          failedCalls: 0,
          totalTokens: 5000,
          retries: 3,
          avgResponseTimeMs: 200
        }
      })
    );

    // Wait for redraw
    renderer.finish();

    const output = stderrWrites.join("");
    assert.ok(output.includes("3 retries"), "Should show retry count");
  });

  it("shows input/output token breakdown", () => {
    const renderer = createProgressRenderer({ tty: true });
    renderer.update(
      makeMetrics({
        llm: {
          totalCalls: 10,
          inFlightCalls: 2,
          completedCalls: 8,
          failedCalls: 0,
          totalTokens: 5000,
          inputTokens: 4000,
          outputTokens: 1000,
          retries: 0,
          avgResponseTimeMs: 200
        }
      })
    );

    // Force a redraw by finishing
    renderer.finish();

    const output = stderrWrites.join("");
    assert.ok(
      output.includes("in /") && output.includes("out"),
      "Should show input/output token breakdown"
    );
  });
});
