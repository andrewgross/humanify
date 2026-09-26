//! The LLM client (WP4.1; 02 §7): an OpenAI-compatible HTTP client, the
//! rate limiter, the metrics tracker, the debug wrapper and the disk
//! response cache — async inside (tokio + reqwest), SYNCHRONOUS outside
//! (`humanify_model::llm::NameProvider`, implemented by [`LlmClient`]).
//!
//! The provider stack mirrors src/commands/unified.ts `buildProvider`:
//! the answer memo OUTERMOST on every run (one key, one answer; the disk
//! cache, when set, is its backing store; hits bypass the limiter and the
//! debug wrapper), then the rate limiter (concurrency + requests/minute +
//! retries), the debug wrapper, and the HTTP client.
//!
//! | TS                         | here                 |
//! | -------------------------- | -------------------- |
//! | llm/types.ts               | humanify_model::llm  |
//! | llm/cached-provider.ts     | [`cache`] (the key: humanify_model::llm) |
//! | llm/openai-compatible.ts   | [`client`]           |
//! | llm/rate-limiter.ts        | [`rate`]             |
//! | llm/metrics.ts             | [`metrics`]          |
//! | llm/debug-wrapper.ts       | [`debug`]            |
//!
//! This crate never reads the process environment (the CLI resolves every
//! setting into `LlmConfig`/`RateLimitConfig`).

pub mod cache;
pub mod client;
pub mod debug;
pub mod metrics;
pub mod provider;
pub mod rate;
/// The cache-key + replay golden: frozen TS dispatches replayed through
/// the live key derivation and cache (test-only since the cutover).
#[cfg(test)]
mod replay_gate;

pub use provider::{AsyncProvider, LlmClient};

#[cfg(test)]
mod cache_test;
#[cfg(test)]
mod client_test;
#[cfg(test)]
mod metrics_test;
#[cfg(test)]
mod provider_test;
#[cfg(test)]
mod rate_test;
#[cfg(test)]
mod replay_gate_test;
#[cfg(test)]
mod stub_server;
