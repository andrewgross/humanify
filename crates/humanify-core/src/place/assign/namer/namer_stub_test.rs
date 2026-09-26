//! Finding #39 end to end over HTTP: the REAL live client against an
//! in-process OpenAI-compatible stub that enforces a context window the
//! way vLLM does (`400 … maximum context length`). One unbounded prompt is
//! refused and every entry falls back; the context-derived batches all fit,
//! and every answer is applied to its own request.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use humanify_llm::LlmClient;
use humanify_llm::provider::LiveOptions;
use humanify_model::llm::{LlmConfig, RateLimitConfig};
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

fn respond(body: &str, seen: &Seen) -> (u16, String) {
    let v: Value = serde_json::from_str(body).unwrap();
    let user = v["messages"][1]["content"].as_str().unwrap();
    let total = stub_tokens(&v) + v["max_tokens"].as_u64().unwrap();
    let refused = total > CONTEXT_TOKENS;
    seen.lock().unwrap().push((user.len(), refused));
    if refused {
        let msg = format!(
            "This model's maximum context length is {CONTEXT_TOKENS} tokens. \
             However, you requested {total} tokens."
        );
        return (
            400,
            serde_json::json!({"error": {"message": msg}}).to_string(),
        );
    }
    let completion = serde_json::json!({
        "choices": [{"message": {"role": "assistant", "content": answer(user)},
                     "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
    });
    (200, completion.to_string())
}

fn serve(stream: std::net::TcpStream, seen: &Seen) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut len = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.eq_ignore_ascii_case("content-length")
        {
            len = v.trim().parse().unwrap();
        }
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).unwrap();
    let (status, out) = respond(&String::from_utf8(body).unwrap(), seen);
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 {status} STUB\r\ncontent-type: application/json\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n{out}",
        out.len()
    );
}

/// Start the stub; returns its base URL.
fn start(seen: Seen) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let seen = seen.clone();
            std::thread::spawn(move || serve(stream, &seen));
        }
    });
    url
}

fn client(url: &str) -> LlmClient<humanify_llm::provider::LiveStack> {
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
        key_params: humanify_model::llm::CacheKeyParams {
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

#[test]
fn one_unbounded_prompt_is_refused_and_every_mint_falls_back() {
    let seen: Seen = Arc::default();
    let url = start(seen.clone());
    let provider = client(&url);
    let unbounded = SplitNamerBudget {
        max_prompt_chars: usize::MAX,
        max_entries: usize::MAX,
    };
    let mut namer = ProviderSplitNamer::with_budget(&provider, unbounded);
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
    let mut namer = ProviderSplitNamer::with_budget(&provider, budget);
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
