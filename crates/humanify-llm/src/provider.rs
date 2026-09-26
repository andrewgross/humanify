//! The async provider trait every layer implements, and [`LlmClient`] — the
//! synchronous `NameProvider` that owns the tokio runtime and fans a wave
//! out over the stack (02 §7: "one trait at one seam").

use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use humanify_model::llm::{
    BatchRenameResponse, CacheKeyParams, LlmCall, LlmConfig, LlmError, LlmErrorKind, NameProvider,
    RateLimitConfig,
};

use crate::cache::{AnswerMemo, CacheStats, DiskCache, MemoStats};
use crate::client::OpenAiClient;
use crate::debug::{DebugProvider, LogSink};
use crate::metrics::MetricsTracker;
use crate::rate::RateLimited;

/// One layer of the provider stack (the TS `LLMProvider` interface).
pub trait AsyncProvider: Send + Sync {
    fn suggest_all_names<'a>(
        &'a self,
        call: &'a LlmCall,
    ) -> impl Future<Output = Result<BatchRenameResponse, LlmError>> + Send + 'a;

    /// The disk cache's counters when this stack has a cache.
    fn cache_stats(&self) -> Option<CacheStats> {
        None
    }

    /// The answer memo's counters when this stack has one.
    fn memo_stats(&self) -> Option<MemoStats> {
        None
    }
}

/// A provider that answers every call with a cache-miss error: the inner
/// provider of a REPLAY-ONLY stack (the parity loop's warm replay, R14 —
/// nothing can reach a model and nothing can be written).
pub struct ReplayMiss;

impl AsyncProvider for ReplayMiss {
    async fn suggest_all_names(&self, _call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        Err(LlmError::new(
            LlmErrorKind::CacheMiss,
            "replay-only: no cache entry for this request",
        ))
    }
}

/// The live stack, in unified.ts `buildProvider` order. The answer memo is
/// outermost on EVERY run (one key, one answer, finding #57); the disk
/// cache, when `--llm-cache` is set, is its backing store.
pub type LiveStack = AnswerMemo<RateLimited<DebugProvider<OpenAiClient>>>;

/// Everything `buildProvider` resolves, as values (no env reads here).
pub struct LiveOptions {
    pub config: LlmConfig,
    pub rate: RateLimitConfig,
    /// The request key's params (the TS's: the model, a literal
    /// temperature 0, maxTokens and reasoningEffort as configured) — the
    /// memo keys by them on every run, the disk cache too when set.
    pub key_params: CacheKeyParams,
    /// `--llm-cache <dir>`: the memo's optional disk backing store.
    pub cache_dir: Option<std::path::PathBuf>,
    pub metrics: Option<Arc<MetricsTracker>>,
    pub log: Option<LogSink>,
}

/// The synchronous client: a current-thread tokio runtime and a provider
/// stack. `run_wave` drives every call of the wave concurrently on the
/// runtime; the stack's rate limiter bounds how many are in flight.
pub struct LlmClient<P> {
    runtime: tokio::runtime::Runtime,
    provider: P,
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("the tokio runtime should build")
}

impl<P: AsyncProvider> LlmClient<P> {
    pub fn with_provider(provider: P) -> Self {
        LlmClient {
            runtime: runtime(),
            provider,
        }
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    pub fn cache_stats(&self) -> Option<CacheStats> {
        self.provider.cache_stats()
    }

    /// The answer memo's counters (zeros for a stack without one).
    pub fn memo_stats(&self) -> MemoStats {
        self.provider.memo_stats().unwrap_or_default()
    }
}

impl LlmClient<AnswerMemo<ReplayMiss>> {
    /// A replay-only client over an existing cache directory: hits answer,
    /// misses fail with `CacheMiss`, and the cache is opened READ-ONLY (a
    /// standing cache is never written, even by accident).
    pub fn replay_only(dir: &Path, params: CacheKeyParams) -> Self {
        LlmClient::with_provider(AnswerMemo::on_disk(
            ReplayMiss,
            DiskCache::open_read_only(dir),
            params,
            None,
        ))
    }
}

impl LlmClient<LiveStack> {
    /// The live stack (`buildProvider`): memo (+ disk cache) → rate limit →
    /// debug → HTTP.
    pub fn live(options: LiveOptions) -> std::io::Result<Self> {
        let http = OpenAiClient::new(options.config.clone(), options.log.clone());
        let debug = DebugProvider::new(
            http,
            Some(options.config.model.clone()),
            options.log.clone(),
        );
        let limited = RateLimited::new(debug, options.rate, options.metrics);
        let provider = match options.cache_dir {
            Some(dir) => AnswerMemo::on_disk(
                limited,
                DiskCache::open(&dir)?,
                options.key_params,
                options.log,
            ),
            None => AnswerMemo::in_memory(limited, options.key_params),
        };
        Ok(LlmClient::with_provider(provider))
    }
}

impl<P: AsyncProvider> NameProvider for LlmClient<P> {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        let provider = &self.provider;
        self.runtime.block_on(futures_util::future::join_all(
            calls.iter().map(|call| provider.suggest_all_names(call)),
        ))
    }
}
