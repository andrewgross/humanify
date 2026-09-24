//! `ProfileReport` → Chrome trace-event JSON (TS: src/profiling/
//! trace-events.ts). Event order is the TS's: the process-name metadata
//! event, one thread-name event per tid in first-USE order over the spans,
//! one `X` event per span, one `C` counter event per concurrency sample.

use humanify_model::profiling::{
    JsNumber, JsObject, ProfileReport, TraceEvent, TraceFile, trace_tid,
};

/// The trace's single process id.
const PID: u32 = 1;

/// TS `THREAD_NAMES`, falling back to `Thread <tid>`.
fn thread_name(tid: u32) -> String {
    match tid {
        trace_tid::PIPELINE => "Pipeline".to_string(),
        trace_tid::RENAME_FUNCTION => "Rename (functions)".to_string(),
        trace_tid::RENAME_MODULE_BINDING => "Rename (module bindings)".to_string(),
        _ => format!("Thread {tid}"),
    }
}

fn metadata_event(name: &str, tid: u32, value: String) -> TraceEvent {
    TraceEvent {
        name: name.to_string(),
        cat: "__metadata".to_string(),
        ph: "M".to_string(),
        ts: JsNumber(0.0),
        dur: None,
        pid: PID,
        tid,
        args: Some(JsObject::new().with("name", value)),
    }
}

/// TS `toTraceEvents`.
pub fn to_trace_events(report: &ProfileReport) -> TraceFile {
    let mut events = vec![metadata_event("process_name", 0, "humanify".to_string())];

    let mut used_tids: Vec<u32> = Vec::new();
    for span in &report.spans {
        if !used_tids.contains(&span.tid) {
            used_tids.push(span.tid);
        }
    }
    events.extend(
        used_tids
            .into_iter()
            .map(|tid| metadata_event("thread_name", tid, thread_name(tid))),
    );

    events.extend(report.spans.iter().map(|span| TraceEvent {
        name: span.name.clone(),
        cat: span.category.clone(),
        ph: "X".to_string(),
        ts: JsNumber(span.start_ms.0 * 1000.0),
        dur: Some(JsNumber((span.end_ms.0 - span.start_ms.0) * 1000.0)),
        pid: PID,
        tid: span.tid,
        args: span.metadata.clone(),
    }));

    events.extend(report.concurrency_snapshots.iter().map(|s| {
        TraceEvent {
            name: "Concurrency".to_string(),
            cat: "concurrency".to_string(),
            ph: "C".to_string(),
            ts: JsNumber(s.time_ms.0 * 1000.0),
            dur: None,
            pid: PID,
            tid: 0,
            args: Some(
                JsObject::new()
                    .with("inFlight", s.in_flight)
                    .with("ready", s.ready)
                    .with("blocked", s.blocked),
            ),
        }
    }));

    TraceFile {
        trace_events: events,
    }
}
