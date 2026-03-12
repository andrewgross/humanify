import assert from "node:assert";
import { describe, it } from "node:test";
import { toTraceEvents } from "./trace-events.js";
import type { ProfileReport } from "./types.js";

function makeReport(overrides: Partial<ProfileReport> = {}): ProfileReport {
  return {
    spans: [],
    concurrencySnapshots: [],
    stageSummaries: [],
    meta: { totalDurationMs: 1000, startedAt: "2024-01-01T00:00:00Z" },
    ...overrides
  };
}

describe("toTraceEvents", () => {
  it("includes process metadata event", () => {
    const result = toTraceEvents(makeReport());
    const meta = result.traceEvents.find((e) => e.name === "process_name");
    assert.ok(meta);
    assert.strictEqual(meta?.ph, "M");
    assert.deepStrictEqual(meta?.args, { name: "humanify" });
  });

  it("converts spans to X events with μs timestamps", () => {
    const report = makeReport({
      spans: [
        {
          name: "parse",
          category: "pipeline",
          startMs: 100,
          endMs: 250,
          tid: 1,
          metadata: { codeLength: 5000 }
        }
      ]
    });

    const result = toTraceEvents(report);
    const xEvents = result.traceEvents.filter((e) => e.ph === "X");
    assert.strictEqual(xEvents.length, 1);
    assert.strictEqual(xEvents[0].name, "parse");
    assert.strictEqual(xEvents[0].cat, "pipeline");
    assert.strictEqual(xEvents[0].ts, 100_000); // ms → μs
    assert.strictEqual(xEvents[0].dur, 150_000);
    assert.strictEqual(xEvents[0].tid, 1);
    assert.deepStrictEqual(xEvents[0].args, { codeLength: 5000 });
  });

  it("includes thread name metadata for used tids", () => {
    const report = makeReport({
      spans: [
        { name: "a", category: "pipeline", startMs: 0, endMs: 1, tid: 1 },
        { name: "b", category: "rename", startMs: 0, endMs: 1, tid: 2 }
      ]
    });

    const result = toTraceEvents(report);
    const threadNames = result.traceEvents.filter(
      (e) => e.name === "thread_name"
    );
    assert.strictEqual(threadNames.length, 2);
    const tids = threadNames.map((e) => e.tid);
    assert.ok(tids.includes(1));
    assert.ok(tids.includes(2));
  });

  it("converts concurrency snapshots to C events", () => {
    const report = makeReport({
      concurrencySnapshots: [
        {
          timeMs: 500,
          inFlight: 10,
          ready: 5,
          blocked: 20
        }
      ]
    });

    const result = toTraceEvents(report);
    const cEvents = result.traceEvents.filter((e) => e.ph === "C");
    assert.strictEqual(cEvents.length, 1);
    assert.strictEqual(cEvents[0].ts, 500_000);
    assert.deepStrictEqual(cEvents[0].args, {
      inFlight: 10,
      ready: 5,
      blocked: 20
    });
  });

  it("handles empty report", () => {
    const result = toTraceEvents(makeReport());
    const meta = result.traceEvents.filter((e) => e.ph === "M");
    assert.strictEqual(meta.length, 1); // just process_name
  });

  it("omits args when span has no metadata", () => {
    const report = makeReport({
      spans: [{ name: "x", category: "c", startMs: 0, endMs: 1, tid: 1 }]
    });
    const result = toTraceEvents(report);
    const xEvent = result.traceEvents.find((e) => e.ph === "X");
    assert.strictEqual(xEvent?.args, undefined);
  });
});
