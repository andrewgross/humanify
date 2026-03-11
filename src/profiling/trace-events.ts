import type { ProfileReport } from "./types.js";

/** Chrome Trace Event format entry. */
interface TraceEvent {
  name: string;
  cat: string;
  ph: "X" | "C" | "M";
  ts: number;
  dur?: number;
  pid: number;
  tid: number;
  args?: Record<string, unknown>;
}

/** Thread ID to human-readable name mapping. */
const THREAD_NAMES: Record<number, string> = {
  1: "Pipeline",
  2: "Rename (functions)",
  3: "Rename (module bindings)"
};

/**
 * Convert a ProfileReport to Chrome Trace Event format.
 *
 * The output can be loaded into chrome://tracing or ui.perfetto.dev.
 */
export function toTraceEvents(report: ProfileReport): { traceEvents: TraceEvent[] } {
  const events: TraceEvent[] = [];
  const pid = 1;

  // Metadata events for process and thread names
  events.push({
    name: "process_name", cat: "__metadata", ph: "M",
    ts: 0, pid, tid: 0,
    args: { name: "humanify" }
  });

  const usedTids = new Set<number>();
  for (const span of report.spans) {
    usedTids.add(span.tid);
  }

  for (const tid of usedTids) {
    const threadName = THREAD_NAMES[tid] ?? `Thread ${tid}`;
    events.push({
      name: "thread_name", cat: "__metadata", ph: "M",
      ts: 0, pid, tid,
      args: { name: threadName }
    });
  }

  // Spans → "X" (complete duration) events
  for (const span of report.spans) {
    const event: TraceEvent = {
      name: span.name,
      cat: span.category,
      ph: "X",
      ts: span.startMs * 1000, // ms → μs
      dur: (span.endMs - span.startMs) * 1000,
      pid,
      tid: span.tid
    };
    if (span.metadata) {
      event.args = span.metadata;
    }
    events.push(event);
  }

  // Concurrency snapshots → "C" (counter) events
  for (const snapshot of report.concurrencySnapshots) {
    events.push({
      name: "Concurrency",
      cat: "concurrency",
      ph: "C",
      ts: snapshot.timeMs * 1000,
      pid,
      tid: 0,
      args: {
        inFlight: snapshot.inFlight,
        ready: snapshot.ready,
        blocked: snapshot.blocked
      }
    });
  }

  return { traceEvents: events };
}
