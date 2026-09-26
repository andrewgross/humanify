//! Port of src/llm/metrics.test.ts, fixture for fixture, plus the
//! formatter vectors recorded from the TS (test/parity/wp41-js-vectors.json).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::metrics::{MetricsTracker, PipelineStage, format_duration, format_tokens};

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

fn counting(throttle_ms: u64) -> (MetricsTracker, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let tracker = MetricsTracker::new(
        Some(Box::new(move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        })),
        Some(throttle_ms),
    );
    (tracker, count)
}

// ---- LLM metrics ----

/// "tracks call start and completion"
#[test]
fn tracks_call_start_and_completion() {
    let tracker = MetricsTracker::default();
    let done1 = tracker.llm_call_start();
    let done2 = tracker.llm_call_start();
    let m = tracker.metrics();
    assert_eq!(
        (
            m.llm.in_flight_calls,
            m.llm.total_calls,
            m.llm.completed_calls
        ),
        (2, 2, 0)
    );
    tracker.llm_call_done(done1);
    let m = tracker.metrics();
    assert_eq!((m.llm.in_flight_calls, m.llm.completed_calls), (1, 1));
    tracker.llm_call_done(done2);
    let m = tracker.metrics();
    assert_eq!((m.llm.in_flight_calls, m.llm.completed_calls), (0, 2));
}

/// "tracks failed calls"
#[test]
fn tracks_failed_calls() {
    let tracker = MetricsTracker::default();
    tracker.llm_call_start();
    tracker.llm_call_failed();
    let m = tracker.metrics();
    assert_eq!((m.llm.failed_calls, m.llm.in_flight_calls), (1, 0));
}

/// "calculates average response time" — two ~50 ms calls.
#[test]
fn calculates_average_response_time() {
    let tracker = MetricsTracker::default();
    let done1 = tracker.llm_call_start();
    sleep_ms(50);
    tracker.llm_call_done(done1);
    let done2 = tracker.llm_call_start();
    sleep_ms(50);
    tracker.llm_call_done(done2);
    let avg = tracker.metrics().llm.avg_response_time_ms;
    assert!(avg >= 40.0, "Average should be around 50ms, got {avg}");
    assert!(avg <= 100.0, "Average should be around 50ms, got {avg}");
}

/// "records tokens when provided"
#[test]
fn records_tokens_when_provided() {
    let tracker = MetricsTracker::default();
    tracker.record_tokens(100, None, None);
    tracker.record_tokens(200, None, None);
    assert_eq!(tracker.metrics().llm.total_tokens, Some(300));
}

/// "records input and output tokens separately"
#[test]
fn records_input_and_output_tokens_separately() {
    let tracker = MetricsTracker::default();
    tracker.record_tokens(300, Some(200), Some(100));
    tracker.record_tokens(600, Some(400), Some(200));
    let m = tracker.metrics();
    assert_eq!(
        (m.llm.total_tokens, m.llm.input_tokens, m.llm.output_tokens),
        (Some(900), Some(600), Some(300))
    );
}

/// "tracks HTTP retries"
#[test]
fn tracks_http_retries() {
    let tracker = MetricsTracker::default();
    tracker.llm_retry();
    tracker.llm_retry();
    tracker.llm_retry();
    assert_eq!(tracker.metrics().llm.retries, 3);
}

/// "resets retries and input/output tokens"
#[test]
fn resets_retries_and_input_output_tokens() {
    let tracker = MetricsTracker::default();
    tracker.record_tokens(100, Some(80), Some(20));
    tracker.llm_retry();
    tracker.reset();
    let m = tracker.metrics();
    assert_eq!(
        (m.llm.retries, m.llm.input_tokens, m.llm.output_tokens),
        (0, None, None)
    );
}

// ---- function metrics ----

/// "sets total function count"
#[test]
fn sets_total_function_count() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(50);
    let m = tracker.metrics();
    assert_eq!((m.functions.total, m.functions.pending), (50, 50));
}

/// "tracks function started"
#[test]
fn tracks_function_started() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(10);
    tracker.functions_ready(5);
    tracker.function_started();
    let m = tracker.metrics();
    assert_eq!((m.functions.in_progress, m.functions.ready), (1, 4));
}

/// "tracks function completed"
#[test]
fn tracks_function_completed() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(10);
    tracker.functions_ready(5);
    tracker.function_started();
    tracker.function_completed();
    let m = tracker.metrics();
    assert_eq!((m.functions.completed, m.functions.in_progress), (1, 0));
}

/// "tracks functions becoming ready"
#[test]
fn tracks_functions_becoming_ready() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(10);
    tracker.functions_ready(3);
    let m = tracker.metrics();
    assert_eq!((m.functions.ready, m.functions.pending), (3, 7));
}

/// "updates state correctly through full lifecycle"
#[test]
fn updates_state_through_full_lifecycle() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(5);
    let m = tracker.metrics();
    assert_eq!((m.functions.pending, m.functions.ready), (5, 0));
    tracker.functions_ready(3);
    let m = tracker.metrics();
    assert_eq!((m.functions.pending, m.functions.ready), (2, 3));
    tracker.function_started();
    tracker.function_started();
    let m = tracker.metrics();
    assert_eq!((m.functions.ready, m.functions.in_progress), (1, 2));
    tracker.function_completed();
    let m = tracker.metrics();
    assert_eq!((m.functions.completed, m.functions.in_progress), (1, 1));
}

// ---- module binding metrics ----

/// "sets total module binding count"
#[test]
fn sets_total_module_binding_count() {
    let tracker = MetricsTracker::default();
    tracker.set_module_binding_total(20);
    let m = tracker.metrics();
    assert_eq!(
        (
            m.module_bindings.total,
            m.module_bindings.completed,
            m.module_bindings.in_progress
        ),
        (20, 0, 0)
    );
}

/// "tracks module binding started and completed"
#[test]
fn tracks_module_binding_started_and_completed() {
    let tracker = MetricsTracker::default();
    tracker.set_module_binding_total(10);
    tracker.module_binding_started();
    tracker.module_binding_started();
    assert_eq!(tracker.metrics().module_bindings.in_progress, 2);
    tracker.module_binding_completed();
    let m = tracker.metrics();
    assert_eq!(
        (m.module_bindings.in_progress, m.module_bindings.completed),
        (1, 1)
    );
}

/// "includes module bindings in ETA calculation"
#[test]
fn includes_module_bindings_in_eta() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(5);
    tracker.set_module_binding_total(5);
    tracker.functions_ready(5);
    for _ in 0..3 {
        tracker.function_started();
        tracker.function_completed();
    }
    for _ in 0..2 {
        tracker.module_binding_started();
        tracker.module_binding_completed();
    }
    assert!(tracker.metrics().estimated_remaining_ms.is_some());
}

// ---- stage tracking ----

/// "starts at parsing stage"
#[test]
fn starts_at_parsing_stage() {
    assert_eq!(
        MetricsTracker::default().metrics().stage,
        PipelineStage::Parsing
    );
}

/// "setStage updates stage"
#[test]
fn set_stage_updates_stage() {
    let tracker = MetricsTracker::default();
    for stage in [
        PipelineStage::BuildingGraph,
        PipelineStage::Renaming,
        PipelineStage::Done,
    ] {
        tracker.set_stage(stage);
        assert_eq!(tracker.metrics().stage, stage);
    }
    assert_eq!(PipelineStage::BuildingGraph.as_str(), "building-graph");
}

/// "setStage force-emits callback"
#[test]
fn set_stage_force_emits() {
    let (tracker, count) = counting(10_000);
    tracker.set_stage(PipelineStage::Renaming);
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "setStage should force-emit"
    );
    tracker.set_stage(PipelineStage::Done);
    assert_eq!(
        count.load(Ordering::SeqCst),
        2,
        "setStage should force-emit again"
    );
}

// ---- token rate ----

/// "returns 0 with no token history"
#[test]
fn tokens_per_second_is_zero_without_history() {
    assert_eq!(MetricsTracker::default().tokens_per_second(), 0.0);
}

/// "calculates rate from token history"
#[test]
fn calculates_rate_from_token_history() {
    let tracker = MetricsTracker::default();
    tracker.record_tokens(1000, None, None);
    sleep_ms(200);
    tracker.record_tokens(1000, None, None);
    assert!(tracker.tokens_per_second() > 0.0, "Rate should be positive");
}

/// "includes tokensPerSecond in getMetrics"
#[test]
fn includes_tokens_per_second_in_metrics() {
    assert_eq!(MetricsTracker::default().metrics().tokens_per_second, 0.0);
}

// ---- timing ----

/// "tracks elapsed time"
#[test]
fn tracks_elapsed_time() {
    let tracker = MetricsTracker::default();
    sleep_ms(50);
    assert!(
        tracker.metrics().elapsed_ms >= 40,
        "Should track elapsed time"
    );
}

/// "estimates remaining time based on completion rate"
#[test]
fn estimates_remaining_time() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(10);
    tracker.functions_ready(10);
    for _ in 0..5 {
        tracker.function_started();
        tracker.function_completed();
    }
    sleep_ms(50);
    assert!(
        tracker.metrics().estimated_remaining_ms.is_some(),
        "Should have ETA after some completions"
    );
}

/// "does not estimate when no completions yet"
#[test]
fn does_not_estimate_without_completions() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(10);
    assert_eq!(tracker.metrics().estimated_remaining_ms, None);
}

// ---- callbacks ----

/// "calls onMetrics callback" (throttle 0)
#[test]
fn calls_on_metrics_callback() {
    let (tracker, count) = counting(0);
    tracker.llm_call_start();
    tracker.function_started();
    assert!(
        count.load(Ordering::SeqCst) >= 2,
        "Should have called callback"
    );
}

/// "throttles callbacks"
#[test]
fn throttles_callbacks() {
    let (tracker, count) = counting(100);
    for _ in 0..10 {
        tracker.llm_call_start();
    }
    assert!(
        count.load(Ordering::SeqCst) < 10,
        "Should throttle callbacks"
    );
}

/// "emit() bypasses throttle"
#[test]
fn emit_bypasses_throttle() {
    let (tracker, count) = counting(10_000);
    tracker.emit();
    tracker.emit();
    tracker.emit();
    assert_eq!(
        count.load(Ordering::SeqCst),
        3,
        "emit() should bypass throttle"
    );
}

// ---- reset ----

/// "resets all metrics"
#[test]
fn resets_all_metrics() {
    let tracker = MetricsTracker::default();
    tracker.set_function_total(10);
    tracker.set_module_binding_total(5);
    tracker.set_stage(PipelineStage::Renaming);
    tracker.functions_ready(5);
    tracker.function_started();
    tracker.function_completed();
    tracker.module_binding_started();
    tracker.module_binding_completed();
    let done = tracker.llm_call_start();
    tracker.llm_call_done(done);
    tracker.record_tokens(100, None, None);
    tracker.reset();
    let m = tracker.metrics();
    assert_eq!((m.functions.total, m.functions.completed), (0, 0));
    assert_eq!(
        (m.module_bindings.total, m.module_bindings.completed),
        (0, 0)
    );
    assert_eq!(m.llm.total_calls, 0);
    assert_eq!(m.llm.total_tokens, None);
    assert_eq!(m.stage, PipelineStage::Parsing);
    assert_eq!(m.tokens_per_second, 0.0);
}

// ---- formatters ----

/// describe("formatDuration"): 500ms / 5.0s / 2m 5s / 2h 5m.
#[test]
fn format_duration_ts_cases() {
    assert_eq!(format_duration(500.0), "500ms");
    assert_eq!(format_duration(5000.0), "5.0s");
    assert_eq!(format_duration(125_000.0), "2m 5s");
    assert_eq!(format_duration(7_500_000.0), "2h 5m");
}

fn js_vectors() -> serde_json::Value {
    let path = format!(
        "{}/../../test/parity/wp41-js-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The formatter vectors from the real TS functions, including the
/// rounding quirks: 1150 ms → "1.1s" (1.15 is 1.1499… in binary), 1250 ms →
/// "1.3s" (an exact tie rounds UP; Rust's `{:.1}` would say "1.2s"). Two
/// entries are deliberate departures from the TS (finding #11 FIXED):
/// 59999 ms → "1m 0s" (TS "60.0s"), 3599999 ms → "1h 0m" (TS "59m 60s").
#[test]
fn formatters_match_the_ts_vectors() {
    let v = js_vectors();
    for case in v["formatDuration"].as_array().unwrap() {
        let ms = case["ms"].as_f64().unwrap();
        assert_eq!(
            format_duration(ms),
            case["out"].as_str().unwrap(),
            "formatDuration({ms})"
        );
    }
    for case in v["formatTokens"].as_array().unwrap() {
        let n = case["n"].as_f64().unwrap();
        assert_eq!(
            format_tokens(n),
            case["out"].as_str().unwrap(),
            "formatTokens({n})"
        );
    }
}
