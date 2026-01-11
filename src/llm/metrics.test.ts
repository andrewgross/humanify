import { describe, it } from "node:test";
import assert from "node:assert";
import {
  MetricsTracker,
  formatMetrics,
  formatMetricsCompact
} from "./metrics.js";

describe("MetricsTracker", () => {
  describe("LLM metrics", () => {
    it("tracks call start and completion", async () => {
      const tracker = new MetricsTracker();

      const done1 = tracker.llmCallStart();
      const done2 = tracker.llmCallStart();

      let metrics = tracker.getMetrics();
      assert.strictEqual(metrics.llm.inFlightCalls, 2);
      assert.strictEqual(metrics.llm.totalCalls, 2);
      assert.strictEqual(metrics.llm.completedCalls, 0);

      done1();
      metrics = tracker.getMetrics();
      assert.strictEqual(metrics.llm.inFlightCalls, 1);
      assert.strictEqual(metrics.llm.completedCalls, 1);

      done2();
      metrics = tracker.getMetrics();
      assert.strictEqual(metrics.llm.inFlightCalls, 0);
      assert.strictEqual(metrics.llm.completedCalls, 2);
    });

    it("tracks failed calls", () => {
      const tracker = new MetricsTracker();

      tracker.llmCallStart();
      tracker.llmCallFailed();

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.llm.failedCalls, 1);
      assert.strictEqual(metrics.llm.inFlightCalls, 0);
    });

    it("calculates average response time", async () => {
      const tracker = new MetricsTracker();

      // Simulate two calls with known durations
      const done1 = tracker.llmCallStart();
      await new Promise((r) => setTimeout(r, 50));
      done1();

      const done2 = tracker.llmCallStart();
      await new Promise((r) => setTimeout(r, 50));
      done2();

      const metrics = tracker.getMetrics();
      assert.ok(metrics.llm.avgResponseTimeMs >= 40, "Average should be around 50ms");
      assert.ok(metrics.llm.avgResponseTimeMs <= 100, "Average should be around 50ms");
    });

    it("records tokens when provided", () => {
      const tracker = new MetricsTracker();

      tracker.recordTokens(100);
      tracker.recordTokens(200);

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.llm.totalTokens, 300);
    });
  });

  describe("function metrics", () => {
    it("sets total function count", () => {
      const tracker = new MetricsTracker();

      tracker.setFunctionTotal(50);

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.total, 50);
      assert.strictEqual(metrics.functions.pending, 50);
    });

    it("tracks function started", () => {
      const tracker = new MetricsTracker();
      tracker.setFunctionTotal(10);
      tracker.functionsReady(5);

      tracker.functionStarted();

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.inProgress, 1);
      assert.strictEqual(metrics.functions.ready, 4);
    });

    it("tracks function completed", () => {
      const tracker = new MetricsTracker();
      tracker.setFunctionTotal(10);
      tracker.functionsReady(5);
      tracker.functionStarted();

      tracker.functionCompleted();

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.completed, 1);
      assert.strictEqual(metrics.functions.inProgress, 0);
    });

    it("tracks functions becoming ready", () => {
      const tracker = new MetricsTracker();
      tracker.setFunctionTotal(10);

      tracker.functionsReady(3);

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.ready, 3);
      assert.strictEqual(metrics.functions.pending, 7);
    });

    it("updates state correctly through full lifecycle", () => {
      const tracker = new MetricsTracker();
      tracker.setFunctionTotal(5);

      // Initially all pending
      let metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.pending, 5);
      assert.strictEqual(metrics.functions.ready, 0);

      // 3 become ready
      tracker.functionsReady(3);
      metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.pending, 2);
      assert.strictEqual(metrics.functions.ready, 3);

      // Start processing 2
      tracker.functionStarted();
      tracker.functionStarted();
      metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.ready, 1);
      assert.strictEqual(metrics.functions.inProgress, 2);

      // Complete 1
      tracker.functionCompleted();
      metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.completed, 1);
      assert.strictEqual(metrics.functions.inProgress, 1);
    });
  });

  describe("timing metrics", () => {
    it("tracks elapsed time", async () => {
      const tracker = new MetricsTracker();

      await new Promise((r) => setTimeout(r, 50));

      const metrics = tracker.getMetrics();
      assert.ok(metrics.elapsedMs >= 40, "Should track elapsed time");
    });

    it("estimates remaining time based on completion rate", async () => {
      const tracker = new MetricsTracker();
      tracker.setFunctionTotal(10);
      tracker.functionsReady(10);

      // Complete 5 functions
      for (let i = 0; i < 5; i++) {
        tracker.functionStarted();
        tracker.functionCompleted();
      }

      await new Promise((r) => setTimeout(r, 50));

      const metrics = tracker.getMetrics();
      assert.ok(
        metrics.estimatedRemainingMs !== undefined,
        "Should have ETA after some completions"
      );
    });

    it("does not estimate when no completions yet", () => {
      const tracker = new MetricsTracker();
      tracker.setFunctionTotal(10);

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.estimatedRemainingMs, undefined);
    });
  });

  describe("callbacks", () => {
    it("calls onMetrics callback", async () => {
      let callCount = 0;
      const tracker = new MetricsTracker({
        onMetrics: () => {
          callCount++;
        },
        throttleMs: 0 // Disable throttling for test
      });

      tracker.llmCallStart();
      tracker.functionStarted();

      // Give callbacks time to fire
      await new Promise((r) => setTimeout(r, 10));

      assert.ok(callCount >= 2, "Should have called callback");
    });

    it("throttles callbacks", async () => {
      let callCount = 0;
      const tracker = new MetricsTracker({
        onMetrics: () => {
          callCount++;
        },
        throttleMs: 100
      });

      // Fire many events rapidly
      for (let i = 0; i < 10; i++) {
        tracker.llmCallStart();
      }

      // Should be throttled to fewer calls
      assert.ok(callCount < 10, "Should throttle callbacks");
    });

    it("emit() bypasses throttle", () => {
      let callCount = 0;
      const tracker = new MetricsTracker({
        onMetrics: () => {
          callCount++;
        },
        throttleMs: 10000 // Very long throttle
      });

      tracker.emit();
      tracker.emit();
      tracker.emit();

      assert.strictEqual(callCount, 3, "emit() should bypass throttle");
    });
  });

  describe("reset", () => {
    it("resets all metrics", () => {
      const tracker = new MetricsTracker();

      tracker.setFunctionTotal(10);
      tracker.functionsReady(5);
      tracker.functionStarted();
      tracker.functionCompleted();
      const done = tracker.llmCallStart();
      done();
      tracker.recordTokens(100);

      tracker.reset();

      const metrics = tracker.getMetrics();
      assert.strictEqual(metrics.functions.total, 0);
      assert.strictEqual(metrics.functions.completed, 0);
      assert.strictEqual(metrics.llm.totalCalls, 0);
      assert.strictEqual(metrics.llm.totalTokens, undefined);
    });
  });
});

describe("formatMetrics", () => {
  it("formats complete metrics", () => {
    const tracker = new MetricsTracker();
    tracker.setFunctionTotal(100);
    tracker.functionsReady(50);

    for (let i = 0; i < 25; i++) {
      tracker.functionStarted();
      tracker.functionCompleted();
    }

    for (let i = 0; i < 5; i++) {
      tracker.functionStarted();
    }

    const done = tracker.llmCallStart();
    done();

    const output = formatMetrics(tracker.getMetrics());

    assert.ok(output.includes("Functions:"), "Should include functions");
    assert.ok(output.includes("LLM Calls:"), "Should include LLM calls");
    assert.ok(output.includes("Time:"), "Should include time");
    assert.ok(output.includes("25"), "Should include completed count");
  });
});

describe("formatMetricsCompact", () => {
  it("formats compact single-line output", () => {
    const tracker = new MetricsTracker();
    tracker.setFunctionTotal(100);
    tracker.functionsReady(100);

    for (let i = 0; i < 50; i++) {
      tracker.functionStarted();
      tracker.functionCompleted();
    }

    const output = formatMetricsCompact(tracker.getMetrics());

    assert.ok(output.includes("[50%]"), "Should include percentage");
    assert.ok(output.includes("50/100"), "Should include counts");
    assert.ok(output.includes("functions"), "Should mention functions");
    assert.ok(output.includes("LLM:"), "Should include LLM status");
  });

  it("calculates percentage correctly", () => {
    const tracker = new MetricsTracker();
    tracker.setFunctionTotal(200);
    tracker.functionsReady(200);

    for (let i = 0; i < 50; i++) {
      tracker.functionStarted();
      tracker.functionCompleted();
    }

    const output = formatMetricsCompact(tracker.getMetrics());
    assert.ok(output.includes("[25%]"), "Should show 25% for 50/200");
  });
});
