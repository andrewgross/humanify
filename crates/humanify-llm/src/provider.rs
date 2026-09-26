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

use crate::cache::{CacheStats, CachedProvider, DiskCache};
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

/// The cache is optional in the TS (`--llm-cache` unset → the limited
/// provider is returned bare).
pub enum MaybeCached<P> {
    Cached(Box<CachedProvider<P>>),
    Plain(P),
}

impl<P: AsyncProvider> AsyncProvider for MaybeCached<P> {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        match self {
            MaybeCached::Cached(p) => p.suggest_all_names(call).await,
            MaybeCached::Plain(p) => p.suggest_all_names(call).await,
        }
    }

    fn cache_stats(&self) -> Option<CacheStats> {
        match self {
            MaybeCached::Cached(p) => p.cache_stats(),
            MaybeCached::Plain(_) => None,
        }
    }
}

/// The live stack, in unified.ts `buildProvider` order.
pub type LiveStack = MaybeCached<RateLimited<DebugProvider<OpenAiClient>>>;

/// Everything `buildProvider` resolves, as values (no env reads here).
pub struct LiveOptions {
    pub config: LlmConfig,
    pub rate: RateLimitConfig,
    /// `--llm-cache <dir>`; the key params are the TS's: the model, a
    /// literal temperature 0, maxTokens and reasoningEffort as configured.
    pub cache: Option<(std::path::PathBuf, CacheKeyParams)>,
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
}

impl LlmClient<CachedProvider<ReplayMiss>> {
    /// A replay-only client over an existing cache directory: hits answer,
    /// misses fail with `CacheMiss`, and the cache is opened READ-ONLY (a
    /// standing cache is never written, even by accident).
    pub fn replay_only(dir: &Path, params: CacheKeyParams) -> Self {
        LlmClient::with_provider(CachedProvider::new(
            ReplayMiss,
            DiskCache::open_read_only(dir),
            params,
            None,
        ))
    }
}

impl LlmClient<LiveStack> {
    /// The live stack (`buildProvider`): cache → rate limit → debug → HTTP.
    pub fn live(options: LiveOptions) -> std::io::Result<Self> {
        let http = OpenAiClient::new(options.config.clone(), options.log.clone());
        let debug = DebugProvider::new(
            http,
            Some(options.config.model.clone()),
            options.log.clone(),
        );
        let limited = RateLimited::new(debug, options.rate, options.metrics);
        let provider = match options.cache {
            Some((dir, params)) => MaybeCached::Cached(Box::new(CachedProvider::new(
                limited,
                DiskCache::open(&dir)?,
                params,
                options.log,
            ))),
            None => MaybeCached::Plain(limited),
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

    /// Truly pipelined: a finished call's follow-ups start at once, while
    /// the rest are still in flight (the stack's rate limiter still bounds
    /// how many run). `on_done` runs on the runtime thread between polls.
    fn run_pipelined(
        &self,
        initial: Vec<(usize, LlmCall)>,
        on_done: &mut humanify_model::llm::OnCallDone<'_>,
    ) {
        use futures_util::stream::{FuturesUnordered, StreamExt};
        let provider = &self.provider;
        let start = |id: usize, call: LlmCall| async move {
            let result = provider.suggest_all_names(&call).await;
            (id, result)
        };
        self.runtime.block_on(async {
            let mut in_flight = FuturesUnordered::new();
            for (id, call) in initial {
                in_flight.push(start(id, call));
            }
            while let Some((id, result)) = in_flight.next().await {
                for (next_id, call) in on_done(id, result) {
                    in_flight.push(start(next_id, call));
                }
            }
        });
    }
}
