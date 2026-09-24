//! The WP4.1 concurrency gate: the synchronous `LlmClient` (the live stack,
//! unified.ts `buildProvider` order) against the in-process stub server —
//! a wave of 40 calls with maxConcurrent 8 must hold EXACTLY 8 requests in
//! flight at the server at peak, never more; plus the cache-outermost
//! property (hits never reach the limiter or the server) and the
//! replay-only client.

use std::sync::Arc;
use std::time::Duration;

use humanify_model::llm::{
    BatchRenameRequest, CacheKeyParams, LlmCall, LlmConfig, LlmErrorKind, NameProvider,
    RateLimitConfig,
};

use crate::cache_test::tmp_dir;
use crate::metrics::MetricsTracker;
use crate::provider::{LiveOptions, LlmClient};
use crate::stub_server::{StubResponse, StubServer, completion};

fn call(i: usize) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest {
            code: format!("function f{i}() {{}}"),
            identifiers: vec![format!("f{i}")],
            ..BatchRenameRequest::default()
        },
        system_prompt: "sys".to_string(),
        user_prompt: format!("user {i}"),
    }
}

fn params() -> CacheKeyParams {
    CacheKeyParams {
        model: "stub-model".to_string(),
        temperature: Some(0.0),
        max_tokens: None,
        reasoning_effort: None,
    }
}

fn live(
    server: &StubServer,
    max_concurrent: usize,
    cache: Option<std::path::PathBuf>,
) -> LlmClient<crate::provider::LiveStack> {
    LlmClient::live(LiveOptions {
        config: LlmConfig::new(&server.base_url, "k", "stub-model"),
        rate: RateLimitConfig {
            max_concurrent,
            ..RateLimitConfig::default()
        },
        cache: cache.map(|dir| (dir, params())),
        metrics: Some(Arc::new(MetricsTracker::default())),
        log: None,
    })
    .unwrap()
}

/// The gate: 40 calls, cap 8, 100 ms per request at the server → peak
/// in-flight is exactly 8 (sustained: 40 / 8 = 5 full rounds), and every
/// answer comes back in call order.
#[test]
fn sustains_the_configured_concurrency_against_a_stub_server() {
    let server = StubServer::start(
        Duration::from_millis(100),
        Arc::new(|_, body| {
            // Echo the user prompt's index back as the rename.
            let v: serde_json::Value = serde_json::from_str(body).unwrap();
            let user = v["messages"][1]["content"].as_str().unwrap().to_string();
            let i = user.trim_start_matches("user ");
            StubResponse::ok(completion(Some(&format!(r#"{{"f{i}":"name{i}"}}"#))))
        }),
    );
    let client = live(&server, 8, None);
    let started = std::time::Instant::now();
    let results = client.run_wave((0..40).map(call).collect());
    let elapsed = started.elapsed();
    assert_eq!(server.requests(), 40);
    assert_eq!(
        server.max_in_flight(),
        8,
        "peak in-flight must equal maxConcurrent"
    );
    for (i, result) in results.iter().enumerate() {
        let response = result.as_ref().unwrap();
        assert_eq!(
            response.renames.get(&format!("f{i}")),
            Some(format!("name{i}").as_str())
        );
    }
    // 5 rounds of 100 ms: well under the 4 s a serial client would take.
    assert!(
        elapsed < Duration::from_millis(2000),
        "wave took {elapsed:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(500),
        "wave took {elapsed:?}"
    );
}

/// Under the cap every call runs at once (the TS "allows full concurrency
/// when under limit", end to end).
#[test]
fn a_wave_under_the_cap_runs_fully_concurrent() {
    let server = StubServer::start(
        Duration::from_millis(100),
        Arc::new(|_, _| StubResponse::ok(completion(Some(r#"{"x":"y"}"#)))),
    );
    let client = live(&server, 50, None);
    client.run_wave((0..12).map(call).collect());
    assert_eq!(server.max_in_flight(), 12);
}

/// Cache outermost: a second identical wave is served from disk — zero
/// requests reach the server, and the stats say so.
#[test]
fn cache_hits_never_reach_the_server() {
    let server = StubServer::start(
        Duration::ZERO,
        Arc::new(|_, _| StubResponse::ok(completion(Some(r#"{"x":"y"}"#)))),
    );
    let dir = tmp_dir("provider");
    let client = live(&server, 4, Some(dir));
    client.run_wave((0..6).map(call).collect());
    assert_eq!(server.requests(), 6);
    let again = client.run_wave((0..6).map(call).collect());
    assert_eq!(server.requests(), 6, "a warm wave asks the server nothing");
    assert!(
        again
            .iter()
            .all(|r| r.as_ref().unwrap().renames.get("x") == Some("y"))
    );
    let stats = client.cache_stats().unwrap();
    assert_eq!((stats.hits, stats.misses, stats.writes), (6, 6, 6));
}

/// Replay-only: hits answer, misses are `CacheMiss` errors, nothing written.
#[test]
fn replay_only_client_answers_hits_and_fails_misses() {
    let server = StubServer::start(
        Duration::ZERO,
        Arc::new(|_, _| StubResponse::ok(completion(Some(r#"{"x":"y"}"#)))),
    );
    let dir = tmp_dir("provider");
    live(&server, 4, Some(dir.clone())).run_wave(vec![call(0)]);
    let replay = LlmClient::replay_only(&dir, params());
    let results = replay.run_wave(vec![call(0), call(1)]);
    assert_eq!(results[0].as_ref().unwrap().renames.get("x"), Some("y"));
    assert_eq!(
        results[1].as_ref().unwrap_err().kind,
        LlmErrorKind::CacheMiss
    );
    let stats = replay.cache_stats().unwrap();
    assert_eq!((stats.hits, stats.misses, stats.writes), (1, 1, 0));
}
