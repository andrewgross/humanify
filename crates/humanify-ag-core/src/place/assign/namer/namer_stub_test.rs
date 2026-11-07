//! Finding #39 end to end over HTTP: the REAL live client against an
//! in-process OpenAI-compatible stub that enforces a context window the
//! way vLLM does (`400 … maximum context length`). One unbounded prompt is
//! refused and every entry falls back; the context-derived batches all fit,
//! and every answer is applied to its own request.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use humanify_ag_llm::LlmClient;
use humanify_ag_llm::provider::LiveOptions;
use humanify_ag_model::llm::{LlmConfig, RateLimitConfig};
use humanify_ag_testkit::answers::{completion, user_prompt};
use humanify_ag_testkit::stub_server::{StubResponse, StubServer};
use serde_json::Value;

use super::{NameKind, ProviderSplitNamer, SplitNameRequest, SplitNamer, SplitNamerBudget};

const CONTEXT_TOKENS: u64 = 4_096;
const COMPLETION_TOKENS: u64 = 1_000;

/// Every user prompt the stub received, and whether it was refused.
type Seen = Arc<Mutex<Vec<(usize, bool)>>>;

/// vLLM-like token estimate: #39's refused prompts ran ~3.9 bytes/token.
fn stub_tokens(body: &Value) -> u64 {
    let bytes: usize = body["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["content"].as_str().unwrap_or("").len())
        .sum();
    (bytes as f64 / 3.9).ceil() as u64
}

/// Answer `{"<key>": "renamed-<key>", …}` for every `### <key> (` brief.
fn answer(user: &str) -> String {
    let entries: serde_json::Map<String, Value> = user
        .lines()
        .filter_map(|l| l.strip_prefix("### "))
        .filter_map(|l| l.split_once(" ("))
        .map(|(k, _)| (k.to_string(), Value::String(format!("renamed-{k}"))))
        .collect();
    Value::Object(entries).to_string()
}

fn respond(body: &str, seen: &Seen) -> StubResponse {
    let v: Value = serde_json::from_str(body).unwrap();
    let user = user_prompt(body);
    let total = stub_tokens(&v) + v["max_tokens"].as_u64().unwrap();
    let refused = total > CONTEXT_TOKENS;
    seen.lock().unwrap().push((user.len(), refused));
    if refused {
        let msg = format!(
            "This model's maximum context length is {CONTEXT_TOKENS} tokens. \
             However, you requested {total} tokens."
        );
        return StubResponse::status(
            400,
            serde_json::json!({"error": {"message": msg}}).to_string(),
        );
    }
    StubResponse::ok(completion(Some(&answer(&user))))
}

/// Start the stub; returns its base URL. It lives until the test process
/// ends.
fn start(seen: Seen) -> String {
    StubServer::start(
        Duration::ZERO,
        Arc::new(move |_, body| respond(body, &seen)),
    )
    .base_url
}

fn client(url: &str) -> LlmClient<humanify_ag_llm::provider::LiveStack> {
    let mut config = LlmConfig::new(url, "k", "stub-model");
    config.max_tokens = COMPLETION_TOKENS;
    config.sdk_max_retries = 0;
    LlmClient::live(LiveOptions {
        config,
        rate: RateLimitConfig {
            max_concurrent: 4,
            retry_attempts: 0,
            ..RateLimitConfig::default()
        },
        key_params: humanify_ag_model::llm::CacheKeyParams {
            model: "stub-model".to_string(),
            temperature: Some(0.0),
            max_tokens: None,
            reasoning_effort: None,
        },
        cache_dir: None,
        metrics: None,
        log: None,
    })
    .unwrap()
}

fn mints(n: usize) -> Vec<SplitNameRequest> {
    (0..n)
        .map(|i| SplitNameRequest {
            kind: NameKind::File,
            mechanical_stem: format!("module{i}"),
            siblings: (0..20).map(|s| format!("sibling-{s}")).collect(),
            bindings: (0..12).map(|b| format!("declared{i}x{b}")).collect(),
            members: None,
            level: None,
            evidence: None,
        })
        .collect()
}

/// The retaining test log (finding #65): the namer tests read the calls
/// back out of it.
fn split_log() -> crate::artifact_dump::DispatchLog {
    crate::artifact_dump::DispatchLog::retain_for_tests(
        humanify_ag_model::llm::CacheKeyParams::default(),
    )
}

#[test]
fn one_unbounded_prompt_is_refused_and_every_mint_falls_back() {
    let seen: Seen = Arc::default();
    let url = start(seen.clone());
    let provider = client(&url);
    let unbounded = SplitNamerBudget {
        max_prompt_chars: usize::MAX,
        max_entries: usize::MAX,
    };
    let mut log = split_log();
    let mut namer = ProviderSplitNamer::with_budget(&provider, &mut log, unbounded);
    let names = namer.name(&mints(200));
    assert_eq!(namer.dispatched.len(), 1);
    assert_eq!(namer.failed_batches, 1);
    assert!(names.iter().all(Option::is_none));
    assert_eq!(seen.lock().unwrap().iter().filter(|s| s.1).count(), 1);
}

#[test]
fn context_sized_batches_all_fit_and_every_answer_is_applied() {
    let seen: Seen = Arc::default();
    let url = start(seen.clone());
    let provider = client(&url);
    let budget = SplitNamerBudget::for_model(CONTEXT_TOKENS, COMPLETION_TOKENS);
    let mut log = split_log();
    let mut namer = ProviderSplitNamer::with_budget(&provider, &mut log, budget);
    let requests = mints(200);
    let names = namer.name(&requests);
    let seen = seen.lock().unwrap();
    assert!(seen.len() > 1, "{} requests", seen.len());
    assert_eq!(seen.len(), namer.dispatched.len());
    assert!(seen.iter().all(|s| !s.1), "a batch was refused: {seen:?}");
    assert_eq!(namer.failed_batches, 0);
    assert_eq!(namer.proposals, requests.len());
    for (request, name) in requests.iter().zip(&names) {
        assert_eq!(
            name.as_deref(),
            Some(format!("renamed-{}", request.mechanical_stem).as_str())
        );
    }
}
