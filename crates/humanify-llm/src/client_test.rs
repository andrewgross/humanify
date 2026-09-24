//! Port of src/llm/openai-compatible.test.ts, fixture for fixture. The TS
//! stubs the SDK's `chat.completions.create`; here the client talks real
//! HTTP to the in-process stub server, so the request body is the WIRE
//! body and the SDK retry layer is exercised too.

use std::sync::Arc;
use std::time::Duration;

use humanify_model::llm::{BatchRenameRequest, LlmCall, LlmConfig, LlmErrorKind};

use crate::client::{OpenAiClient, parse_renames_from_content, renames_from_content, status_error};
use crate::provider::AsyncProvider;
use crate::stub_server::{StubResponse, StubServer, completion};

/// `makeRequest(identifiers)` — code "function a(b, c) { return b + c; }".
fn make_request(identifiers: &[&str]) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest {
            code: "function a(b, c) { return b + c; }".to_string(),
            identifiers: identifiers.iter().map(|s| s.to_string()).collect(),
            ..BatchRenameRequest::default()
        },
        system_prompt: "sys".to_string(),
        user_prompt: "user".to_string(),
    }
}

/// A stub answering every request with a completion whose content is
/// `content` (the TS `stubClient`).
fn stub(content: Option<&str>) -> StubServer {
    let body = completion(content);
    StubServer::start(
        Duration::ZERO,
        Arc::new(move |_, _| StubResponse::ok(body.clone())),
    )
}

/// `makeProvider(overrides)` against the stub.
fn provider(server: &StubServer, edit: impl FnOnce(&mut LlmConfig)) -> OpenAiClient {
    let mut config = LlmConfig::new(&server.base_url, "test-key", "test-model");
    edit(&mut config);
    OpenAiClient::new(config, None)
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(f)
}

fn sent_body(server: &StubServer) -> serde_json::Value {
    serde_json::from_str(&server.bodies()[0]).unwrap()
}

// ---- suggestAllNames ----

/// "parses valid JSON response"
#[test]
fn parses_valid_json_response() {
    let server = stub(Some(r#"{"a":"addNumbers","b":"firstValue"}"#));
    let client = provider(&server, |_| {});
    let result = run(client.suggest_all_names(&make_request(&["a", "b"]))).unwrap();
    assert_eq!(result.renames.get("a"), Some("addNumbers"));
    assert_eq!(result.renames.get("b"), Some("firstValue"));
    assert_eq!(result.finish_reason.as_deref(), Some("stop"));
    let usage = result.usage.unwrap();
    assert_eq!(
        (usage.total_tokens, usage.input_tokens, usage.output_tokens),
        (Some(15), Some(10), Some(5))
    );
}

/// "returns empty renames when no response"
#[test]
fn returns_empty_renames_when_no_response() {
    let server = stub(None);
    let client = provider(&server, |_| {});
    let result = run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    assert!(result.renames.is_empty());
}

/// "extracts renames from malformed JSON"
#[test]
fn extracts_renames_from_malformed_json() {
    let server = stub(Some(
        r#"Here are the names: "a": "calculateSum", "b": "inputValue" — done!"#,
    ));
    let client = provider(&server, |_| {});
    let result = run(client.suggest_all_names(&make_request(&["a", "b"]))).unwrap();
    assert_eq!(result.renames.get("a"), Some("calculateSum"));
    assert_eq!(result.renames.get("b"), Some("inputValue"));
}

/// "passes a raw invalid identifier through unchanged (no longer sanitized)"
#[test]
fn passes_a_raw_invalid_identifier_through() {
    let server = stub(Some(r#"{"a":"123invalid"}"#));
    let client = provider(&server, |_| {});
    let result = run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    assert_eq!(result.renames.get("a"), Some("123invalid"));
}

/// "passes a raw reserved word through unchanged (JSON path)"
#[test]
fn passes_a_raw_reserved_word_through_json_path() {
    let server = stub(Some(r#"{"a":"delete"}"#));
    let client = provider(&server, |_| {});
    let result = run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    assert_eq!(result.renames.get("a"), Some("delete"));
}

/// "passes a raw reserved/builtin word through unchanged (regex-fallback path)"
#[test]
fn passes_a_raw_reserved_word_through_regex_path() {
    let server = stub(Some(r#"names: "a": "Map", "b": "delete" done"#));
    let client = provider(&server, |_| {});
    let result = run(client.suggest_all_names(&make_request(&["a", "b"]))).unwrap();
    assert_eq!(result.renames.get("a"), Some("Map"));
    assert_eq!(result.renames.get("b"), Some("delete"));
}

// ---- configuration (the TS reads private fields; here: the wire body) ----

/// "uses default maxTokens" + "sends max_tokens directly (no multiplier) —
/// default 6000" + "defaults to temperature 0" + "sends temperature 0 in the
/// request body by default" + "omits reasoning_effort by default".
#[test]
fn default_body_carries_6000_tokens_temperature_0_and_no_effort() {
    let server = stub(Some(r#"{"a":"value"}"#));
    let client = provider(&server, |_| {});
    run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    let body = sent_body(&server);
    assert_eq!(body["max_tokens"], 6000);
    assert_eq!(body["temperature"], 0);
    assert!(
        body.get("reasoning_effort").is_none(),
        "reasoning_effort must not be sent unless configured"
    );
    // Byte-level: JSON.stringify's key order and `0` (not `0.0`).
    assert_eq!(
        server.bodies()[0],
        r#"{"model":"test-model","messages":[{"role":"system","content":"sys"},{"role":"user","content":"user"}],"response_format":{"type":"json_object"},"temperature":0,"max_tokens":6000}"#
    );
}

/// "uses custom maxTokens" / "sends the configured maxTokens as max_tokens"
#[test]
fn sends_the_configured_max_tokens() {
    let server = stub(Some(r#"{"a":"value"}"#));
    let client = provider(&server, |c| c.max_tokens = 2000);
    run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    assert_eq!(sent_body(&server)["max_tokens"], 2000);
}

/// "uses custom temperature" (0.7 serializes as JS writes it)
#[test]
fn sends_a_custom_temperature() {
    let server = stub(Some(r#"{"a":"value"}"#));
    let client = provider(&server, |c| c.temperature = 0.7);
    run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    assert!(server.bodies()[0].contains(r#""temperature":0.7,"#));
}

/// "sends reasoning_effort when configured"
#[test]
fn sends_reasoning_effort_when_configured() {
    let server = stub(Some(r#"{"a":"value"}"#));
    let client = provider(&server, |c| c.reasoning_effort = Some("low".to_string()));
    run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    assert_eq!(sent_body(&server)["reasoning_effort"], "low");
}

/// The auth header and path the SDK sends.
#[test]
fn posts_to_chat_completions_with_bearer_auth() {
    let server = stub(Some(r#"{"a":"value"}"#));
    let client = provider(&server, |_| {});
    run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    let headers = server.stats.headers.lock().unwrap()[0].clone();
    assert!(headers.contains(&("authorization".to_string(), "Bearer test-key".to_string())));
}

// ---- the SDK retry envelope (openai 4.90.0 core.js) ----

/// A 503 then a 200: the SDK layer retries by itself (maxRetries 2).
#[test]
fn sdk_layer_retries_a_5xx() {
    let ok = completion(Some(r#"{"a":"x"}"#));
    let server = StubServer::start(
        Duration::ZERO,
        Arc::new(move |i, _| {
            if i == 0 {
                StubResponse {
                    status: 503,
                    headers: vec![("retry-after-ms".to_string(), "1".to_string())],
                    body: r#"{"error":{"message":"busy"}}"#.to_string(),
                }
            } else {
                StubResponse::ok(ok.clone())
            }
        }),
    );
    let client = provider(&server, |_| {});
    let result = run(client.suggest_all_names(&make_request(&["a"]))).unwrap();
    assert_eq!(result.renames.get("a"), Some("x"));
    assert_eq!(server.requests(), 2);
}

/// A 400 is final at once, with the SDK's message shape "{status} {msg}".
#[test]
fn sdk_layer_does_not_retry_a_400() {
    let server = StubServer::start(
        Duration::ZERO,
        Arc::new(|_, _| StubResponse::status(400, r#"{"error":{"message":"bad model"}}"#)),
    );
    let client = provider(&server, |_| {});
    let error = run(client.suggest_all_names(&make_request(&["a"]))).unwrap_err();
    assert_eq!(error.message, "400 bad model");
    assert_eq!(error.status, Some(400));
    assert_eq!(server.requests(), 1);
}

/// `x-should-retry: false` overrides a retryable status.
#[test]
fn sdk_layer_obeys_x_should_retry() {
    let server = StubServer::start(
        Duration::ZERO,
        Arc::new(|_, _| StubResponse {
            status: 500,
            headers: vec![("x-should-retry".to_string(), "false".to_string())],
            body: String::new(),
        }),
    );
    let client = provider(&server, |_| {});
    let error = run(client.suggest_all_names(&make_request(&["a"]))).unwrap_err();
    assert_eq!(error.message, "500 status code (no body)");
    assert_eq!(server.requests(), 1);
}

/// A dead endpoint: 1 + maxRetries attempts, then "Connection error." —
/// retries disabled here to keep the test fast.
#[test]
fn connection_failure_is_the_sdk_connection_error() {
    let mut config = LlmConfig::new("http://127.0.0.1:9/v1", "k", "m");
    config.sdk_max_retries = 0;
    let client = OpenAiClient::new(config, None);
    let error = run(client.suggest_all_names(&make_request(&["a"]))).unwrap_err();
    assert_eq!(error.kind, LlmErrorKind::Connection);
    assert_eq!(error.message, "Connection error.");
    assert_eq!(error.status, None);
}

/// The per-attempt timeout: "Request timed out.".
#[test]
fn slow_server_times_out() {
    let body = completion(Some(r#"{"a":"x"}"#));
    let server = StubServer::start(
        Duration::from_millis(500),
        Arc::new(move |_, _| StubResponse::ok(body.clone())),
    );
    let client = provider(&server, |c| {
        c.timeout_ms = 50;
        c.sdk_max_retries = 0;
    });
    let error = run(client.suggest_all_names(&make_request(&["a"]))).unwrap_err();
    assert_eq!(error.kind, LlmErrorKind::Timeout);
    assert_eq!(error.message, "Request timed out.");
}

// ---- response parsing (probed: test/parity/wp41-js-vectors.json) ----

/// `Object.entries(JSON.parse(content))` on the probe's contents: non-string
/// values dropped, a bare string yields one entry PER CHARACTER, `null`
/// throws into the regex fallback, a repeated key keeps its first slot.
#[test]
fn content_parsing_matches_the_ts_probe() {
    let path = format!(
        "{}/../../test/parity/wp41-js-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    for case in v["entries"].as_array().unwrap() {
        let content = case["content"].as_str().unwrap();
        let renames = renames_from_content(content);
        match case.get("ok") {
            Some(expected) => {
                let got = humanify_model::js::stringify(&renames.to_js());
                assert_eq!(got, expected.as_str().unwrap(), "content {content}");
            }
            // Object.entries(null) threw: the fallback regex finds nothing.
            None => assert!(renames.is_empty(), "content {content}"),
        }
    }
}

/// The regex fallback's scan: a failed attempt resumes one character on,
/// `\s` is JS's set (U+FEFF counts), empty keys/values never match.
#[test]
fn regex_fallback_scans_like_the_global_regex() {
    let r = parse_renames_from_content(
        "x \"\": \"no\" \"a\"\u{feff}:\u{a0}\"b\" \"c\" : \"\" \"d\":\"e\"",
    );
    assert_eq!(
        humanify_model::js::stringify(&r.to_js()),
        r#"{"a":"b","d":"e"}"#
    );
}

/// The SDK's error-message shapes.
#[test]
fn status_error_messages_match_the_sdk() {
    assert_eq!(
        status_error(429, r#"{"error":{"message":"slow down"}}"#).message,
        "429 slow down"
    );
    assert_eq!(
        status_error(500, r#"{"error":{"code":1}}"#).message,
        r#"500 {"code":1}"#
    );
    assert_eq!(status_error(502, "Bad Gateway").message, "502 Bad Gateway");
    assert_eq!(status_error(503, "").message, "503 status code (no body)");
    assert_eq!(
        status_error(504, r#"{"detail":"x"}"#).message,
        "504 status code (no body)"
    );
}
