import assert from "node:assert";
import { describe, it } from "node:test";
import { computePercentile, NULL_PROFILER, Profiler } from "./profiler.js";

describe("Profiler", () => {
  describe("enabled", () => {
    it("records spans with correct timing", () => {
      const profiler = new Profiler(true);
      const span = profiler.startSpan("test-span", "test");
      span.end();

      const report = profiler.finalize();
      assert.strictEqual(report.spans.length, 1);
      assert.strictEqual(report.spans[0].name, "test-span");
      assert.strictEqual(report.spans[0].category, "test");
      assert.strictEqual(report.spans[0].tid, 1);
      assert.ok(report.spans[0].startMs >= 0);
      assert.ok(report.spans[0].endMs >= report.spans[0].startMs);
    });

    it("supports custom tid and metadata", () => {
      const profiler = new Profiler(true);
      const span = profiler.startSpan("fn:test", "rename", 2, { waitMs: 42 });
      span.end({ outcome: "ok" });

      const report = profiler.finalize();
      assert.strictEqual(report.spans[0].tid, 2);
      assert.deepStrictEqual(report.spans[0].metadata, {
        waitMs: 42,
        outcome: "ok"
      });
    });

    it("records multiple spans", () => {
      const profiler = new Profiler(true);
      profiler.startSpan("a", "cat1").end();
      profiler.startSpan("b", "cat2").end();
      profiler.startSpan("c", "cat1").end();

      const report = profiler.finalize();
      assert.strictEqual(report.spans.length, 3);
    });

    it("computes stage summaries from tid=1 spans", () => {
      const profiler = new Profiler(true);
      profiler.startSpan("parse", "pipeline", 1).end();
      profiler.startSpan("fn:test", "rename", 2).end();

      const report = profiler.finalize();
      assert.strictEqual(report.stageSummaries.length, 1);
      assert.strictEqual(report.stageSummaries[0].name, "parse");
    });

    it("computes rename timing percentiles", () => {
      const profiler = new Profiler(true);
      for (let i = 0; i < 5; i++) {
        profiler.startSpan(`fn:test:${i}`, "rename", 2).end();
      }

      const report = profiler.finalize();
      assert.ok(report.renameTiming);
      assert.strictEqual(report.renameTiming?.count, 5);
      assert.ok(report.renameTiming?.p50 >= 0);
      assert.ok(report.renameTiming?.p95 >= 0);
      assert.ok(report.renameTiming?.p99 >= 0);
    });

    it("records concurrency snapshots", () => {
      const profiler = new Profiler(true);
      profiler.recordConcurrency({ inFlight: 5, ready: 3, blocked: 10 });
      profiler.recordConcurrency({ inFlight: 8, ready: 0, blocked: 7 });

      const report = profiler.finalize();
      assert.strictEqual(report.concurrencySnapshots.length, 2);
      assert.strictEqual(report.concurrencySnapshots[0].inFlight, 5);
      assert.strictEqual(report.concurrencySnapshots[1].inFlight, 8);
      assert.ok(report.concurrencySnapshots[0].timeMs >= 0);
    });

    it("finalize includes metadata", () => {
      const profiler = new Profiler(true);
      const report = profiler.finalize({ inputFile: "test.js" });
      assert.strictEqual(report.meta.inputFile, "test.js");
      assert.ok(report.meta.startedAt);
      assert.ok(report.meta.totalDurationMs >= 0);
    });
  });

  describe("NULL_PROFILER (disabled)", () => {
    it("all methods are callable without errors", () => {
      const handle = NULL_PROFILER.startSpan("test", "cat");
      handle.end();
      NULL_PROFILER.recordConcurrency({ inFlight: 0, ready: 0, blocked: 0 });
      NULL_PROFILER.startConcurrencySampling(() => ({
        inFlight: 0,
        ready: 0,
        blocked: 0
      }));
      NULL_PROFILER.stopConcurrencySampling();
    });

    it("finalize returns empty report", () => {
      const report = NULL_PROFILER.finalize();
      assert.strictEqual(report.spans.length, 0);
      assert.strictEqual(report.concurrencySnapshots.length, 0);
      assert.strictEqual(report.stageSummaries.length, 0);
      assert.strictEqual(report.meta.totalDurationMs, 0);
    });
  });
});

describe("computePercentile", () => {
  it("handles empty array", () => {
    assert.strictEqual(computePercentile([], 50), 0);
  });

  it("handles single element", () => {
    assert.strictEqual(computePercentile([42], 50), 42);
    assert.strictEqual(computePercentile([42], 99), 42);
  });

  it("computes correct percentiles", () => {
    const sorted = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    assert.strictEqual(computePercentile(sorted, 50), 5);
    assert.strictEqual(computePercentile(sorted, 90), 9);
    assert.strictEqual(computePercentile(sorted, 100), 10);
  });
});
