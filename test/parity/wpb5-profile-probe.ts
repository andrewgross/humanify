// probe: WPB.5 ground truth — the profiling converters' exact outputs,
// frozen as the Rust port's expectations (crates/humanify-core/src/
// profiling_test.rs reads test/parity/wpb5-profile-vectors.json).
//
// Runs the TS code ITSELF: toTraceEvents (the `--profile` file body, as
// unified.ts writes it: JSON.stringify(trace, null, 2)), formatProfileSummary
// (the console summary), computePercentile, formatDuration. Timings are
// data here — every report is fixed, so the outputs are byte-exact
// targets. One report comes from the REAL Profiler (live performance.now
// fractions, metadata spread, end-order recording) and is frozen as data.
//
// The edges it pins: integer-valued numbers print without a fraction; an
// unknown tid gets "Thread <n>"; array-index metadata keys enumerate
// first; spread keeps a repeated key's first position; toFixed rounds an
// exact tie AWAY from zero (0.25 -> "0.3") where Rust's {:.1} would not;
// the stable descending stage sort keeps tied stages in order; the
// formatDuration branches (ms / s / m s / h m).
//
// Usage: npx tsx test/parity/wpb5-profile-probe.ts

import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { formatDuration } from "../../src/llm/metrics.js";
import { computePercentile, Profiler } from "../../src/profiling/profiler.js";
import { formatProfileSummary } from "../../src/profiling/summary.js";
import { toTraceEvents } from "../../src/profiling/trace-events.js";
import type { ProfileReport } from "../../src/profiling/types.js";

const OUT = join(import.meta.dirname, "wpb5-profile-vectors.json");

function base(overrides: Partial<ProfileReport> = {}): ProfileReport {
  return {
    spans: [],
    concurrencySnapshots: [],
    stageSummaries: [],
    meta: { totalDurationMs: 1000, startedAt: "2024-01-01T00:00:00Z" },
    ...overrides
  };
}

function liveReport(): ProfileReport {
  const p = new Profiler(true);
  const outer = p.startSpan("parse", "pipeline", 1, { b: 1, "2": "two" });
  for (let i = 0; i < 3; i++) {
    p.startSpan(`fn:input.js:${i}:0`, "rename", 2, { waitMs: i }).end({
      outcome: "ok"
    });
  }
  p.startSpan("module-binding", "rename", 3).end();
  p.recordConcurrency({ inFlight: 3, ready: 1, blocked: 0 });
  outer.end({ b: 9, a: true, "0": null });
  p.startSpan("parse", "pipeline").end();
  const report = p.finalize({ inputFile: "input.js" });
  return { ...report, meta: { ...report.meta, startedAt: "frozen" } };
}

const REPORTS: Record<string, ProfileReport> = {
  empty: base(),
  "ts-test-parse": base({
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
  }),
  rich: base({
    spans: [
      { name: "b", category: "rename", startMs: 0.5, endMs: 1.75, tid: 2 },
      {
        name: "graph-build",
        category: "pipeline",
        startMs: 1.123456789,
        endMs: 12.000000001,
        tid: 1,
        metadata: { z: [1, 2.5, "x"], "10": { nested: true }, "1": 0, a: null }
      },
      { name: "mb", category: "rename", startMs: 3, endMs: 4, tid: 3 },
      {
        name: "odd",
        category: "custom",
        startMs: 5,
        endMs: 5,
        tid: 7,
        metadata: {}
      }
    ],
    concurrencySnapshots: [
      { timeMs: 250, inFlight: 0, ready: 5, blocked: 20 },
      { timeMs: 500.25, inFlight: 0, ready: 0, blocked: 1 },
      { timeMs: 750, inFlight: 0, ready: 0, blocked: 0 },
      { timeMs: 1000, inFlight: 1, ready: 0, blocked: 0 }
    ],
    stageSummaries: [
      { name: "tie-first", durationMs: 1, spanCount: 1 },
      {
        name: "a-long-stage-name-over-25-chars",
        durationMs: 2.5,
        spanCount: 3
      },
      { name: "tie-second", durationMs: 1, spanCount: 2 },
      { name: "slow", durationMs: 3_700_000, spanCount: 1 },
      { name: "mins", durationMs: 61_000, spanCount: 1 },
      { name: "secs", durationMs: 1_500, spanCount: 1 },
      { name: "sub-ms", durationMs: 0.35, spanCount: 1 }
    ],
    renameTiming: {
      p50: 999.5,
      p95: 59_999,
      p99: 60_000,
      minMs: 0.000001,
      maxMs: 7_199_999,
      count: 4
    },
    meta: {
      totalDurationMs: 400,
      startedAt: "2026-09-24T00:00:00.000Z",
      inputFile: "in.js"
    }
  }),
  "zero-total": base({
    stageSummaries: [{ name: "x", durationMs: 0, spanCount: 1 }],
    meta: { totalDurationMs: 0, startedAt: "" }
  }),
  live: liveReport()
};

// serde_json's float PARSER (without float_roundtrip) can land 1 ulp off
// a full-precision literal (porting lesson 8), and the live report's
// timings are exactly such literals — so every report float also ships as
// its IEEE bit pattern, which the Rust test patches in after parsing.
function bits(x: number): string {
  const view = new DataView(new ArrayBuffer(8));
  view.setFloat64(0, x);
  return view.getBigUint64(0).toString(16).padStart(16, "0");
}

function reportBits(r: ProfileReport) {
  const t = r.renameTiming;
  return {
    spans: r.spans.map((s) => [bits(s.startMs), bits(s.endMs)]),
    snapshots: r.concurrencySnapshots.map((s) => bits(s.timeMs)),
    stages: r.stageSummaries.map((s) => bits(s.durationMs)),
    renameTiming: t ? [t.p50, t.p95, t.p99, t.minMs, t.maxMs].map(bits) : null,
    total: bits(r.meta.totalDurationMs)
  };
}

const vectors = {
  generatedBy: "test/parity/wpb5-profile-probe.ts",
  percentile: [
    [[], 50],
    [[42], 50],
    [[42], 99],
    [[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 50],
    [[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 90],
    [[1, 2, 3, 4, 5, 6, 7, 8, 9, 10], 100],
    [[1, 2, 3], 0],
    [[0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5], 95]
  ].map(([sorted, p]) => ({
    sorted,
    p,
    out: computePercentile(sorted as number[], p as number)
  })),
  durations: [
    0, 0.5, 1e-7, 1.5e-7, 999.999, 1000, 1049, 1050, 59_999, 60_000, 61_499,
    3_599_999, 3_600_000, 7_199_999, 123_456_789
  ].map((ms) => ({ ms, out: formatDuration(ms) })),
  reports: Object.entries(REPORTS).map(([name, report]) => ({
    name,
    report,
    bits: reportBits(report),
    trace: JSON.stringify(toTraceEvents(report), null, 2),
    summary: formatProfileSummary(report)
  }))
};

writeFileSync(OUT, `${JSON.stringify(vectors, null, 2)}\n`);
console.log(
  `wrote ${OUT}: ${vectors.reports.length} reports, ${vectors.percentile.length} percentile + ${vectors.durations.length} duration vectors`
);
