//! Profiling tests (WPB.5). Two layers:
//!
//! 1. `profiler.test.ts` + `trace-events.test.ts` ported case-for-case.
//! 2. PARITY against the frozen TS probe (`test/parity/wpb5-profile-probe.ts`
//!    → `test/parity/wpb5-profile-vectors.json`): for every frozen report,
//!    the `--profile` file body (pretty JSON) and the console summary are
//!    BYTE-equal to the TS's; percentiles and `formatDuration` likewise.
//!    Three recorded strings are deliberate departures (finding #11, fixed):
//!    `formatDuration` 59999 → "1m 0s" and 3599999 → "1h 0m", and the
//!    `rich` summary's "p95: 1m 0s" (the TS printed "60.0s" / "59m 60s").

use std::time::Duration;

use humanify_model::profiling::{
    ConcurrencyCounts, ConcurrencySnapshot, JsNumber, JsObject, ProfileMeta, ProfileReport,
    ProfileSpan,
};
use serde_json::{Value, json};

use crate::profiling::{
    Profiler, compute_percentile, format_profile_summary, iso_from_unix_ms, to_trace_events,
};
use humanify_model::js::format_duration;

// ---- layer 1a: profiler.test.ts --------------------------------------------

#[test]
fn records_spans_with_correct_timing() {
    let p = Profiler::new(true);
    p.start_span("test-span", "test", 1, None).end(None);
    let r = p.finalize(None);
    assert_eq!(r.spans.len(), 1);
    assert_eq!(r.spans[0].name, "test-span");
    assert_eq!(r.spans[0].category, "test");
    assert_eq!(r.spans[0].tid, 1);
    assert!(r.spans[0].start_ms.0 >= 0.0);
    assert!(r.spans[0].end_ms.0 >= r.spans[0].start_ms.0);
    assert_eq!(r.spans[0].metadata, None);
}

#[test]
fn supports_custom_tid_and_metadata() {
    let p = Profiler::new(true);
    p.start_span(
        "fn:test",
        "rename",
        2,
        Some(JsObject::new().with("waitMs", 42)),
    )
    .end(Some(JsObject::new().with("outcome", "ok")));
    let r = p.finalize(None);
    assert_eq!(r.spans[0].tid, 2);
    assert_eq!(
        serde_json::to_value(&r.spans[0].metadata).unwrap(),
        json!({"waitMs": 42, "outcome": "ok"})
    );
}

#[test]
fn records_multiple_spans() {
    let p = Profiler::new(true);
    p.start_span("a", "cat1", 1, None).end(None);
    p.start_span("b", "cat2", 1, None).end(None);
    p.start_span("c", "cat1", 1, None).end(None);
    assert_eq!(p.finalize(None).spans.len(), 3);
}

#[test]
fn computes_stage_summaries_from_tid_1_spans() {
    let p = Profiler::new(true);
    p.start_span("parse", "pipeline", 1, None).end(None);
    p.start_span("fn:test", "rename", 2, None).end(None);
    let r = p.finalize(None);
    assert_eq!(r.stage_summaries.len(), 1);
    assert_eq!(r.stage_summaries[0].name, "parse");
}

#[test]
fn computes_rename_timing_percentiles() {
    let p = Profiler::new(true);
    for i in 0..5 {
        p.start_span(&format!("fn:test:{i}"), "rename", 2, None)
            .end(None);
    }
    let t = p.finalize(None).rename_timing.expect("rename timing");
    assert_eq!(t.count, 5);
    assert!(t.p50.0 >= 0.0 && t.p95.0 >= 0.0 && t.p99.0 >= 0.0);
}

#[test]
fn records_concurrency_snapshots() {
    let p = Profiler::new(true);
    p.record_concurrency(ConcurrencyCounts {
        in_flight: 5,
        ready: 3,
        blocked: 10,
    });
    p.record_concurrency(ConcurrencyCounts {
        in_flight: 8,
        ready: 0,
        blocked: 7,
    });
    let r = p.finalize(None);
    assert_eq!(r.concurrency_snapshots.len(), 2);
    assert_eq!(r.concurrency_snapshots[0].in_flight, 5);
    assert_eq!(r.concurrency_snapshots[1].in_flight, 8);
    assert!(r.concurrency_snapshots[0].time_ms.0 >= 0.0);
}

#[test]
fn finalize_includes_metadata() {
    let p = Profiler::new(true);
    let r = p.finalize(Some("test.js"));
    assert_eq!(r.meta.input_file.as_deref(), Some("test.js"));
    assert!(!r.meta.started_at.is_empty());
    assert!(r.meta.total_duration_ms.0 >= 0.0);
}

#[test]
fn disabled_profiler_is_callable_and_records_nothing() {
    let p = Profiler::disabled();
    assert!(!p.is_enabled());
    p.start_span("test", "cat", 1, None).end(None);
    p.record_concurrency(ConcurrencyCounts::default());
    p.start_concurrency_sampling(ConcurrencyCounts::default, Duration::from_millis(1));
    p.stop_concurrency_sampling();
    let r = p.finalize(None);
    assert!(r.spans.is_empty());
    assert!(r.concurrency_snapshots.is_empty());
    assert!(r.stage_summaries.is_empty());
    assert_eq!(r.meta.total_duration_ms.0, 0.0);
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"spans":[],"concurrencySnapshots":[],"stageSummaries":[],"meta":{"totalDurationMs":0,"startedAt":""}}"#
    );
}

#[test]
fn concurrency_sampling_records_until_stopped() {
    let p = Profiler::new(true);
    p.start_concurrency_sampling(
        || ConcurrencyCounts {
            in_flight: 1,
            ready: 2,
            blocked: 3,
        },
        Duration::from_millis(2),
    );
    std::thread::sleep(Duration::from_millis(30));
    p.stop_concurrency_sampling();
    let n = p.finalize(None).concurrency_snapshots.len();
    assert!(n >= 1, "sampled {n}");
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(p.finalize(None).concurrency_snapshots.len(), n);
}

#[test]
fn spans_end_from_other_threads() {
    let p = Profiler::new(true);
    std::thread::scope(|s| {
        for i in 0..4 {
            let span = p.start_span(&format!("fn:{i}"), "rename", 2, None);
            s.spawn(move || span.end(None));
        }
    });
    assert_eq!(p.finalize(None).spans.len(), 4);
}

#[test]
fn compute_percentile_matches_ts_cases() {
    assert_eq!(compute_percentile(&[], 50.0), 0.0);
    assert_eq!(compute_percentile(&[42.0], 50.0), 42.0);
    assert_eq!(compute_percentile(&[42.0], 99.0), 42.0);
    let sorted: Vec<f64> = (1..=10).map(f64::from).collect();
    assert_eq!(compute_percentile(&sorted, 50.0), 5.0);
    assert_eq!(compute_percentile(&sorted, 90.0), 9.0);
    assert_eq!(compute_percentile(&sorted, 100.0), 10.0);
}

#[test]
fn started_at_is_js_iso_format() {
    assert_eq!(iso_from_unix_ms(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(
        iso_from_unix_ms(1_704_067_200_000),
        "2024-01-01T00:00:00.000Z"
    );
    assert_eq!(
        iso_from_unix_ms(1_709_210_096_789),
        "2024-02-29T12:34:56.789Z"
    );
    assert_eq!(
        iso_from_unix_ms(4_102_444_799_999),
        "2099-12-31T23:59:59.999Z"
    );
}

// ---- layer 1b: trace-events.test.ts ----------------------------------------

fn make_report(spans: Vec<ProfileSpan>, snapshots: Vec<ConcurrencySnapshot>) -> ProfileReport {
    ProfileReport {
        spans,
        concurrency_snapshots: snapshots,
        stage_summaries: Vec::new(),
        rename_timing: None,
        meta: ProfileMeta {
            total_duration_ms: JsNumber(1000.0),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            input_file: None,
        },
    }
}

fn span(name: &str, cat: &str, start: f64, end: f64, tid: u32) -> ProfileSpan {
    ProfileSpan {
        name: name.to_string(),
        category: cat.to_string(),
        start_ms: JsNumber(start),
        end_ms: JsNumber(end),
        tid,
        metadata: None,
    }
}

#[test]
fn includes_process_metadata_event() {
    let t = to_trace_events(&make_report(vec![], vec![]));
    let meta = t
        .trace_events
        .iter()
        .find(|e| e.name == "process_name")
        .expect("process_name");
    assert_eq!(meta.ph, "M");
    assert_eq!(
        serde_json::to_value(&meta.args).unwrap(),
        json!({"name": "humanify"})
    );
}

#[test]
fn converts_spans_to_x_events_with_microsecond_timestamps() {
    let mut s = span("parse", "pipeline", 100.0, 250.0, 1);
    s.metadata = Some(JsObject::new().with("codeLength", 5000));
    let t = to_trace_events(&make_report(vec![s], vec![]));
    let x: Vec<_> = t.trace_events.iter().filter(|e| e.ph == "X").collect();
    assert_eq!(x.len(), 1);
    assert_eq!(x[0].name, "parse");
    assert_eq!(x[0].cat, "pipeline");
    assert_eq!(x[0].ts.0, 100_000.0);
    assert_eq!(x[0].dur.map(|d| d.0), Some(150_000.0));
    assert_eq!(x[0].tid, 1);
    assert_eq!(
        serde_json::to_value(&x[0].args).unwrap(),
        json!({"codeLength": 5000})
    );
}

#[test]
fn includes_thread_name_metadata_for_used_tids() {
    let t = to_trace_events(&make_report(
        vec![
            span("a", "pipeline", 0.0, 1.0, 1),
            span("b", "rename", 0.0, 1.0, 2),
        ],
        vec![],
    ));
    let tids: Vec<u32> = t
        .trace_events
        .iter()
        .filter(|e| e.name == "thread_name")
        .map(|e| e.tid)
        .collect();
    assert_eq!(tids, vec![1, 2]);
}

#[test]
fn converts_concurrency_snapshots_to_c_events() {
    let t = to_trace_events(&make_report(
        vec![],
        vec![ConcurrencySnapshot {
            time_ms: JsNumber(500.0),
            in_flight: 10,
            ready: 5,
            blocked: 20,
        }],
    ));
    let c: Vec<_> = t.trace_events.iter().filter(|e| e.ph == "C").collect();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].ts.0, 500_000.0);
    assert_eq!(
        serde_json::to_value(&c[0].args).unwrap(),
        json!({"inFlight": 10, "ready": 5, "blocked": 20})
    );
}

#[test]
fn handles_empty_report() {
    let t = to_trace_events(&make_report(vec![], vec![]));
    assert_eq!(t.trace_events.iter().filter(|e| e.ph == "M").count(), 1);
}

#[test]
fn omits_args_when_span_has_no_metadata() {
    let t = to_trace_events(&make_report(vec![span("x", "c", 0.0, 1.0, 1)], vec![]));
    let x = t.trace_events.iter().find(|e| e.ph == "X").unwrap();
    assert!(x.args.is_none());
    assert!(!serde_json::to_string(x).unwrap().contains("args"));
}

// ---- JS object semantics ----------------------------------------------------

#[test]
fn js_object_enumerates_index_keys_first_and_spread_keeps_position() {
    let mut o = JsObject::new().with("b", 1).with("2", "two").with("10", 0);
    o.spread(
        &JsObject::new()
            .with("b", 9)
            .with("a", true)
            .with("0", Value::Null),
    );
    assert_eq!(
        serde_json::to_string(&o).unwrap(),
        r#"{"0":null,"2":"two","10":0,"b":9,"a":true}"#
    );
    // Non-canonical numerals are plain string keys.
    let o = JsObject::new()
        .with("z", 1)
        .with("01", 2)
        .with("4294967295", 3);
    assert_eq!(
        serde_json::to_string(&o).unwrap(),
        r#"{"z":1,"01":2,"4294967295":3}"#
    );
}

// ---- layer 2: frozen TS vectors ---------------------------------------------

const VECTORS: &str = include_str!("../../../test/parity/wpb5-profile-vectors.json");

fn vectors() -> Value {
    serde_json::from_str(VECTORS).expect("vectors parse")
}

/// One frozen report, deserialized straight from the text: going through
/// `serde_json::Value` first would alphabetize the metadata keys (its map
/// is a BTreeMap) and hide the order the trace must reproduce.
#[derive(serde::Deserialize)]
struct FrozenReport {
    name: String,
    report: ProfileReport,
    bits: FrozenBits,
    trace: String,
    summary: String,
}

/// Every report float's IEEE bits (serde_json's parser can land 1 ulp off
/// a 17-digit literal — porting lesson 8 — so the floats are patched in
/// from their bit patterns after parsing).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrozenBits {
    spans: Vec<[String; 2]>,
    snapshots: Vec<String>,
    stages: Vec<String>,
    rename_timing: Option<[String; 5]>,
    total: String,
}

fn from_bits(hex: &str) -> JsNumber {
    JsNumber(f64::from_bits(
        u64::from_str_radix(hex, 16).expect("hex bits"),
    ))
}

impl FrozenReport {
    fn patch_exact_floats(&mut self) {
        let (r, b) = (&mut self.report, &self.bits);
        for (s, [start, end]) in r.spans.iter_mut().zip(&b.spans) {
            s.start_ms = from_bits(start);
            s.end_ms = from_bits(end);
        }
        for (s, t) in r.concurrency_snapshots.iter_mut().zip(&b.snapshots) {
            s.time_ms = from_bits(t);
        }
        for (s, d) in r.stage_summaries.iter_mut().zip(&b.stages) {
            s.duration_ms = from_bits(d);
        }
        if let (Some(t), Some([p50, p95, p99, min, max])) = (&mut r.rename_timing, &b.rename_timing)
        {
            t.p50 = from_bits(p50);
            t.p95 = from_bits(p95);
            t.p99 = from_bits(p99);
            t.min_ms = from_bits(min);
            t.max_ms = from_bits(max);
        }
        r.meta.total_duration_ms = from_bits(&b.total);
    }
}

#[derive(serde::Deserialize)]
struct Frozen {
    reports: Vec<FrozenReport>,
}

fn frozen_reports() -> Vec<FrozenReport> {
    let f: Frozen = serde_json::from_str(VECTORS).expect("vectors parse");
    assert!(f.reports.len() >= 5);
    f.reports
        .into_iter()
        .map(|mut r| {
            r.patch_exact_floats();
            r
        })
        .collect()
}

#[test]
fn trace_file_bytes_equal_ts_for_every_frozen_report() {
    for r in frozen_reports() {
        let rust = serde_json::to_string_pretty(&to_trace_events(&r.report)).unwrap();
        assert_eq!(rust, r.trace, "trace for {}", r.name);
    }
}

#[test]
fn report_json_round_trips_byte_equal() {
    for r in vectors()["reports"].as_array().unwrap() {
        let report: ProfileReport = serde_json::from_value(r["report"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&report).unwrap(),
            r["report"],
            "report {}",
            r["name"]
        );
    }
}

#[test]
fn summary_text_equals_ts_for_every_frozen_report() {
    for r in frozen_reports() {
        assert_eq!(
            format_profile_summary(&r.report),
            r.summary,
            "summary for {}",
            r.name
        );
    }
}

#[test]
fn percentiles_and_durations_equal_ts() {
    let v = vectors();
    for c in v["percentile"].as_array().unwrap() {
        let sorted: Vec<f64> = c["sorted"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect();
        let p = c["p"].as_f64().unwrap();
        assert_eq!(
            compute_percentile(&sorted, p),
            c["out"].as_f64().unwrap(),
            "{c}"
        );
    }
    for c in v["durations"].as_array().unwrap() {
        assert_eq!(
            format_duration(c["ms"].as_f64().unwrap()),
            c["out"].as_str().unwrap(),
            "{c}"
        );
    }
}
