//! Port of src/llm/rate-limiter.test.ts, fixture for fixture, plus the
//! requests-per-minute window (untested in the TS) on tokio's paused clock.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use humanify_model::llm::{
    BatchRenameRequest, BatchRenameResponse, LlmCall, LlmError, LlmErrorKind, RateLimitConfig,
    Renames,
};

use crate::metrics::MetricsTracker;
use crate::provider::AsyncProvider;
use crate::rate::{RateLimited, is_retryable};

/// `makeRequest(identifiers)`.
fn make_request(identifiers: &[&str]) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest {
            code: "function test() {}".to_string(),
            identifiers: identifiers.iter().map(|s| s.to_string()).collect(),
            ..BatchRenameRequest::default()
        },
        system_prompt: String::new(),
        user_prompt: String::new(),
    }
}

/// `renameAll`: every identifier → `${id}Renamed`.
fn rename_all(call: &LlmCall) -> BatchRenameResponse {
    BatchRenameResponse {
        renames: Renames::from_entries(
            call.request
                .identifiers
                .iter()
                .map(|id| (id.clone(), Some(format!("{id}Renamed")))),
        ),
        ..BatchRenameResponse::default()
    }
}

fn config(max_concurrent: usize, retry_attempts: u32, retry_delay_ms: u64) -> RateLimitConfig {
    RateLimitConfig {
        max_concurrent,
        requests_per_minute: 0,
        retry_attempts,
        retry_delay_ms,
    }
}

/// A provider that sleeps `delay`, tracking concurrency.
#[derive(Default)]
struct Sleeper {
    delay_ms: u64,
    current: AtomicUsize,
    max: AtomicUsize,
}

impl AsyncProvider for Arc<Sleeper> {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        self.max.fetch_max(now, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
        self.current.fetch_sub(1, Ordering::SeqCst);
        Ok(rename_all(call))
    }
}

/// Fails with `message` until attempt `succeed_on` (1-based; 0 = never).
struct Flaky {
    attempts: AtomicUsize,
    message: String,
    succeed_on: usize,
}

impl Flaky {
    fn new(message: &str, succeed_on: usize) -> Self {
        Flaky {
            attempts: AtomicUsize::new(0),
            message: message.to_string(),
            succeed_on,
        }
    }
    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}

impl AsyncProvider for &Flaky {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let n = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        if self.succeed_on != 0 && n >= self.succeed_on {
            return Ok(rename_all(call));
        }
        Err(LlmError::new(LlmErrorKind::Other, self.message.clone()))
    }
}

fn wave<P: AsyncProvider>(
    limited: &RateLimited<P>,
    n: usize,
) -> Vec<Result<BatchRenameResponse, LlmError>> {
    let calls: Vec<LlmCall> = (0..n)
        .map(|i| make_request(&[&format!("var{i}")]))
        .collect();
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(futures_util::future::join_all(
            calls.iter().map(|c| limited.suggest_all_names(c)),
        ))
}

fn one<P: AsyncProvider>(
    limited: &RateLimited<P>,
    call: &LlmCall,
) -> Result<BatchRenameResponse, LlmError> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(limited.suggest_all_names(call))
}

/// "respects maxConcurrent limit" — 10 requests, maxConcurrent 3.
#[test]
fn respects_max_concurrent_limit() {
    let sleeper = Arc::new(Sleeper {
        delay_ms: 50,
        ..Sleeper::default()
    });
    let limited = RateLimited::new(sleeper.clone(), config(3, 3, 1000), None);
    wave(&limited, 10);
    let max = sleeper.max.load(Ordering::SeqCst);
    assert!(max <= 3, "Max concurrent was {max}, expected <= 3");
    assert!(max >= 1, "Should have had at least 1 concurrent");
}

/// "allows full concurrency when under limit" — 5 requests, cap 10.
#[test]
fn allows_full_concurrency_when_under_limit() {
    let sleeper = Arc::new(Sleeper {
        delay_ms: 20,
        ..Sleeper::default()
    });
    let limited = RateLimited::new(sleeper.clone(), config(10, 3, 1000), None);
    wave(&limited, 5);
    assert_eq!(
        sleeper.max.load(Ordering::SeqCst),
        5,
        "All 5 should have run concurrently"
    );
}

/// "retries on retryable errors" — "rate limit exceeded 429" twice, then ok.
#[test]
fn retries_on_retryable_errors() {
    let flaky = Flaky::new("rate limit exceeded 429", 3);
    let limited = RateLimited::new(&flaky, config(50, 3, 10), None);
    let result = one(&limited, &make_request(&["test"])).unwrap();
    assert_eq!(flaky.attempts(), 3, "Should have tried 3 times");
    assert_eq!(result.renames.get("test"), Some("testRenamed"));
}

/// "does not retry on non-retryable errors" — "Invalid API key".
#[test]
fn does_not_retry_on_non_retryable_errors() {
    let flaky = Flaky::new("Invalid API key", 0);
    let limited = RateLimited::new(&flaky, config(50, 3, 10), None);
    let error = one(&limited, &make_request(&["a"])).unwrap_err();
    assert!(error.message.contains("Invalid API key"));
    assert_eq!(flaky.attempts(), 1, "Should not retry non-retryable errors");
}

/// "fails after exhausting retries" — "network timeout", 2 retries.
#[test]
fn fails_after_exhausting_retries() {
    let flaky = Flaky::new("network timeout", 0);
    let limited = RateLimited::new(&flaky, config(50, 2, 10), None);
    let error = one(&limited, &make_request(&["a"])).unwrap_err();
    assert!(error.message.contains("network timeout"));
    assert_eq!(flaky.attempts(), 3, "Should try initial + 2 retries");
}

/// "recognizes various retryable error patterns"
#[test]
fn recognizes_various_retryable_error_patterns() {
    for message in [
        "network error",
        "timeout exceeded",
        "ECONNRESET",
        "ECONNREFUSED",
        "rate limit hit",
        "429 Too Many Requests",
        "500 Internal Server Error",
        "502 Bad Gateway",
        "503 Service Unavailable",
        "504 Gateway Timeout",
    ] {
        let flaky = Flaky::new(message, 2);
        let limited = RateLimited::new(&flaky, config(50, 1, 1), None);
        let result = one(&limited, &make_request(&["test"])).unwrap();
        assert_eq!(
            result.renames.get("test"),
            Some("testRenamed"),
            "Should retry on: {message}"
        );
    }
}

/// Not a TS case: what the classifier does NOT retry — the SDK's final
/// connection/timeout messages (already retried one layer down), and a 400.
#[test]
fn sdk_final_errors_and_client_errors_are_not_retryable() {
    let connection = LlmError::new(LlmErrorKind::Connection, "Connection error.");
    let timeout = LlmError::new(LlmErrorKind::Timeout, "Request timed out.");
    let bad_request = LlmError {
        kind: LlmErrorKind::Status,
        message: "400 bad request".to_string(),
        status: Some(400),
    };
    let overloaded = LlmError {
        kind: LlmErrorKind::Status,
        message: "529 overloaded".to_string(),
        status: Some(529),
    };
    assert!(!is_retryable(&connection));
    assert!(!is_retryable(&timeout));
    assert!(!is_retryable(&bad_request));
    assert!(is_retryable(&overloaded), "any 5xx status retries");
}

/// "tracks successful calls" (metrics integration)
#[test]
fn tracks_successful_calls() {
    let metrics = Arc::new(MetricsTracker::default());
    let flaky = Flaky::new("", 1);
    let limited = RateLimited::new(&flaky, RateLimitConfig::default(), Some(metrics.clone()));
    one(&limited, &make_request(&["test1"])).unwrap();
    one(&limited, &make_request(&["test2"])).unwrap();
    let stats = metrics.metrics();
    assert_eq!(stats.llm.completed_calls, 2);
    assert_eq!(stats.llm.failed_calls, 0);
    assert_eq!(stats.llm.in_flight_calls, 0);
}

/// "tracks failed calls" (metrics integration, retryAttempts 0)
#[test]
fn tracks_failed_calls() {
    let metrics = Arc::new(MetricsTracker::default());
    let flaky = Flaky::new("API error", 0);
    let limited = RateLimited::new(&flaky, config(50, 0, 1000), Some(metrics.clone()));
    assert!(one(&limited, &make_request(&["a"])).is_err());
    let stats = metrics.metrics();
    assert_eq!(stats.llm.failed_calls, 1);
    assert_eq!(stats.llm.completed_calls, 0);
}

/// Captures the tracker's in-flight count from inside the call.
struct InFlightProbe {
    metrics: Arc<MetricsTracker>,
    captured: AtomicUsize,
}

impl AsyncProvider for &InFlightProbe {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let in_flight = self.metrics.metrics().llm.in_flight_calls;
        self.captured
            .store(in_flight.max(0) as usize, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(rename_all(call))
    }
}

/// "tracks in-flight calls during execution" — 3 concurrent, cap 5.
#[test]
fn tracks_in_flight_calls_during_execution() {
    let metrics = Arc::new(MetricsTracker::default());
    let probe = InFlightProbe {
        metrics: metrics.clone(),
        captured: AtomicUsize::new(0),
    };
    let limited = RateLimited::new(&probe, config(5, 3, 1000), Some(metrics));
    wave(&limited, 3);
    assert!(
        probe.captured.load(Ordering::SeqCst) >= 1,
        "Should have captured in-flight calls"
    );
}

/// requestsPerMinute (no TS test): with a limit of 2, the third request
/// started AFTER the first two waits until the oldest leaves the 60 s window.
#[tokio::test(start_paused = true)]
async fn requests_per_minute_waits_for_the_window() {
    let flaky = Flaky::new("", 1);
    let limited = RateLimited::new(
        &flaky,
        RateLimitConfig {
            requests_per_minute: 2,
            ..RateLimitConfig::default()
        },
        None,
    );
    let start = tokio::time::Instant::now();
    limited
        .suggest_all_names(&make_request(&["a"]))
        .await
        .unwrap();
    tokio::time::advance(Duration::from_secs(10)).await;
    limited
        .suggest_all_names(&make_request(&["b"]))
        .await
        .unwrap();
    assert!(start.elapsed() < Duration::from_secs(11));
    limited
        .suggest_all_names(&make_request(&["c"]))
        .await
        .unwrap();
    assert!(
        start.elapsed() >= Duration::from_secs(60),
        "the third call waited for the first to age out, elapsed {:?}",
        start.elapsed()
    );
    assert_eq!(flaky.attempts(), 3);
}

/// The TS window check is not atomic with the record: a BURST of
/// concurrent calls all see an empty window and all start at once — the
/// TS behavior, reproduced (see rate.rs's module doc).
#[tokio::test(start_paused = true)]
async fn requests_per_minute_does_not_bind_a_concurrent_burst() {
    let flaky = Flaky::new("", 1);
    let limited = RateLimited::new(
        &flaky,
        RateLimitConfig {
            requests_per_minute: 2,
            ..RateLimitConfig::default()
        },
        None,
    );
    let calls: Vec<LlmCall> = (0..4).map(|i| make_request(&[&format!("v{i}")])).collect();
    let start = tokio::time::Instant::now();
    futures_util::future::join_all(calls.iter().map(|c| limited.suggest_all_names(c))).await;
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "all four started at once"
    );
    assert_eq!(flaky.attempts(), 4);
}

/// Exponential backoff: retryDelayMs * 2^attempt (100, 200, 400 ms).
#[tokio::test(start_paused = true)]
async fn backoff_doubles_per_attempt() {
    let flaky = Flaky::new("503 Service Unavailable", 4);
    let limited = RateLimited::new(&flaky, config(50, 3, 100), None);
    let start = tokio::time::Instant::now();
    limited
        .suggest_all_names(&make_request(&["a"]))
        .await
        .unwrap();
    assert_eq!(start.elapsed(), Duration::from_millis(700));
    assert_eq!(flaky.attempts(), 4);
}
