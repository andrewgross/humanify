//! The rate limiter (from llm/rate-limiter.ts): a concurrency cap, an
//! optional requests-per-minute window, and retries with exponential
//! backoff:
//!
//! 1. queue for a concurrency slot (FIFO — tokio's semaphore is fair);
//! 2. take a place in the requests-per-minute window: check AND record the
//!    timestamp under one lock, sleeping and re-checking while it is full;
//! 3. run the call with retries INSIDE the slot (backoff sleeps hold it).
//!
//! Finding #9 (fixed): the TS checked the window BEFORE queueing and
//! recorded the stamp only once the slot was taken, so a burst of
//! concurrent calls all saw the same window and all passed. Step 2 is now
//! one critical section, run inside the slot, so every recorded stamp is a
//! real start time. Production never sets requestsPerMinute, so pipeline
//! output cannot depend on this.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, LlmErrorKind, RateLimitConfig};
use tokio::sync::Semaphore;
use tokio::time::Instant;

use crate::metrics::MetricsTracker;
use crate::provider::AsyncProvider;

const WINDOW: Duration = Duration::from_millis(60_000);

pub struct RateLimited<P> {
    inner: P,
    config: RateLimitConfig,
    slots: Semaphore,
    timestamps: Mutex<VecDeque<Instant>>,
    metrics: Option<Arc<MetricsTracker>>,
}

/// Whether a failed call is worth another attempt, decided by the most
/// specific evidence the error carries:
///
/// - a transport failure or per-attempt timeout (the client's final
///   "Connection error." / "Request timed out.") — yes;
/// - a replay-only cache miss — never (the answer cannot appear);
/// - an HTTP status — 429 or 5xx only, whatever the body text says;
/// - anything else — a message naming a transient failure.
///
/// Finding #10 (fixed): the TS `isRetryableError` matched message
/// substrings only, so it never matched the client's two final transport
/// messages, and it DID match a 400 whose text held "500" (a
/// context-length error quoting "135000 tokens").
pub fn is_retryable(error: &LlmError) -> bool {
    match (error.kind, error.status) {
        (LlmErrorKind::Connection | LlmErrorKind::Timeout, _) => true,
        (LlmErrorKind::CacheMiss, _) => false,
        (_, Some(status)) => status == 429 || (500..600).contains(&status),
        (_, None) => names_a_transient_failure(&error.message),
    }
}

/// The TS message patterns, for errors that carry no kind or status.
fn names_a_transient_failure(message: &str) -> bool {
    const PATTERNS: [&str; 10] = [
        "network",
        "timeout",
        "econnreset",
        "econnrefused",
        "rate limit",
        "429",
        "500",
        "502",
        "503",
        "504",
    ];
    let message = message.to_lowercase();
    PATTERNS.iter().any(|p| message.contains(p))
}

impl<P> RateLimited<P> {
    pub fn new(inner: P, config: RateLimitConfig, metrics: Option<Arc<MetricsTracker>>) -> Self {
        RateLimited {
            inner,
            slots: Semaphore::new(config.max_concurrent),
            config,
            timestamps: Mutex::new(VecDeque::new()),
            metrics,
        }
    }

    /// Step 2: when the last minute holds fewer than `requestsPerMinute`
    /// started requests, record this one and return; else sleep until the
    /// oldest leaves the window and try again. Check and record share one
    /// lock, so concurrent callers can never both take the last place.
    async fn take_window_place(&self) {
        let limit = self.config.requests_per_minute;
        if limit == 0 {
            return;
        }
        loop {
            let wait = {
                let now = Instant::now();
                let mut stamps = self.timestamps.lock().expect("timestamps lock");
                // Keep stamps strictly newer than one minute ago.
                while stamps
                    .front()
                    .is_some_and(|ts| now.duration_since(*ts) >= WINDOW)
                {
                    stamps.pop_front();
                }
                if stamps.len() < limit {
                    stamps.push_back(now);
                    return;
                }
                stamps.front().map_or(WINDOW, |oldest| {
                    WINDOW.saturating_sub(now.duration_since(*oldest))
                })
            };
            tokio::time::sleep(wait).await;
        }
    }
}

impl<P: AsyncProvider> RateLimited<P> {
    /// Step 3 (`withRetry`): initial try + `retryAttempts` retries, sleeping
    /// `retryDelayMs * 2^attempt` between them; a non-retryable error stops
    /// at once. Metrics: one start, then exactly one of done / failed.
    async fn with_retry(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let start = self.metrics.as_ref().map(|m| m.llm_call_start());
        let mut last_error = None;
        for attempt in 0..=self.config.retry_attempts {
            match self.inner.suggest_all_names(call).await {
                Ok(response) => {
                    if let (Some(m), Some(start)) = (&self.metrics, start) {
                        m.llm_call_done(start);
                    }
                    return Ok(response);
                }
                Err(error) => {
                    if attempt == self.config.retry_attempts {
                        last_error = Some(error);
                        break;
                    }
                    if !is_retryable(&error) {
                        if let Some(m) = &self.metrics {
                            m.llm_call_failed();
                        }
                        return Err(error);
                    }
                    if let Some(m) = &self.metrics {
                        m.llm_retry();
                    }
                    let delay = self
                        .config
                        .retry_delay_ms
                        .saturating_mul(1 << attempt.min(62));
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    last_error = Some(error);
                }
            }
        }
        if let Some(m) = &self.metrics {
            m.llm_call_failed();
        }
        Err(last_error.expect("the loop runs at least once"))
    }
}

impl<P: AsyncProvider> AsyncProvider for RateLimited<P> {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let _slot = self
            .slots
            .acquire()
            .await
            .expect("the semaphore is never closed");
        self.take_window_place().await;
        self.with_retry(call).await
    }

    fn cache_stats(&self) -> Option<crate::cache::CacheStats> {
        self.inner.cache_stats()
    }
}
