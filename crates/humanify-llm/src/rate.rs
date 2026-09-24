//! The rate limiter (llm/rate-limiter.ts): a concurrency cap, an optional
//! requests-per-minute window, and retries with exponential backoff —
//! same order of operations as the TS:
//!
//! 1. wait for the requests-per-minute window (checked BEFORE queueing);
//! 2. queue for a concurrency slot (FIFO — tokio's semaphore is fair, as
//!    the TS's array queue is);
//! 3. record the request timestamp when the slot is taken;
//! 4. run the call with retries INSIDE the slot (backoff sleeps hold it).
//!
//! TS behavior reproduced, not fixed: the per-minute check is not atomic
//! with the timestamp record (step 1 vs step 3), so a burst of concurrent
//! calls all see the same window and all pass — the limit only binds once
//! earlier calls have STARTED. Production never sets requestsPerMinute
//! (unified.ts passes only maxConcurrent + retryAttempts), so this is
//! recorded for the record, not load-bearing.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError, RateLimitConfig};
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

/// `isRetryableError`: a message naming a transient failure, or an HTTP
/// status of 429 / 5xx. Note what it does NOT match: the openai SDK's
/// "Connection error." and "Request timed out." (no "network", no
/// "timeout" substring, no status) — those were already retried by the
/// SDK's own layer (client.rs) and are final here.
pub fn is_retryable(error: &LlmError) -> bool {
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
    let message = error.message.to_lowercase();
    if PATTERNS.iter().any(|p| message.contains(p)) {
        return true;
    }
    matches!(error.status, Some(s) if s == 429 || (500..600).contains(&s))
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

    /// Step 1: if the last minute already holds `requestsPerMinute`
    /// started requests, sleep until the oldest leaves the window.
    async fn wait_for_rate_limit(&self) {
        let limit = self.config.requests_per_minute;
        if limit == 0 {
            return;
        }
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
            if stamps.len() >= limit {
                stamps
                    .front()
                    .map(|oldest| WINDOW.saturating_sub(now.duration_since(*oldest)))
            } else {
                None
            }
        };
        match wait.filter(|w| !w.is_zero()) {
            Some(wait) => tokio::time::sleep(wait).await,
            // The TS `await this.waitForRateLimit()` yields even when it
            // does not wait: every call of a burst runs its check before
            // any of them records (the burst hole above depends on it).
            None => tokio::task::yield_now().await,
        }
    }

    /// Step 3.
    fn record_request(&self) {
        if self.config.requests_per_minute > 0 {
            self.timestamps
                .lock()
                .expect("timestamps lock")
                .push_back(Instant::now());
        }
    }
}

impl<P: AsyncProvider> RateLimited<P> {
    /// Step 4 (`withRetry`): initial try + `retryAttempts` retries, sleeping
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
        self.wait_for_rate_limit().await;
        let _slot = self
            .slots
            .acquire()
            .await
            .expect("the semaphore is never closed");
        self.record_request();
        self.with_retry(call).await
    }

    fn cache_stats(&self) -> Option<crate::cache::CacheStats> {
        self.inner.cache_stats()
    }
}
