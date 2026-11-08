//! The call ledger fed by the live stack (exp206): the memo files every
//! call's outcome and what it served, the limiter every sent call, the
//! client every HTTP attempt — one record per call, whatever the layer.

use std::sync::Arc;
use std::time::Duration;

use humanify_ag_model::call_ledger::CallLedger;
use humanify_ag_model::js::JsValue;
use humanify_ag_model::llm::{
    BatchRenameRequest, CacheKeyParams, CallSite, LlmCall, LlmConfig, NameProvider, RateLimitConfig,
};

use crate::provider::{LiveOptions, LlmClient};
use humanify_ag_testkit::answers::completion;
use humanify_ag_testkit::stub_server::{StubResponse, StubServer};

fn call(site: CallSite, id: &str) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest {
            code: format!("var {id};"),
            identifiers: vec![id.to_string()],
            ..BatchRenameRequest::default()
        },
        system_prompt: "sys".to_string(),
        user_prompt: format!("name {id}"),
        site,
    }
}

fn legal(name: &str) -> bool {
    !name.is_empty()
}

fn live(server: &StubServer, ledger: &Arc<CallLedger>) -> LlmClient<crate::provider::LiveStack> {
    LlmClient::live(LiveOptions {
        config: LlmConfig::new(&server.base_url, "k", "stub-model"),
        rate: RateLimitConfig {
            max_concurrent: 1,
            retry_attempts: 0,
            ..RateLimitConfig::default()
        },
        key_params: CacheKeyParams {
            model: "stub-model".to_string(),
            temperature: Some(0.0),
            max_tokens: None,
            reasoning_effort: None,
        },
        cache_dir: None,
        ledger: Some(ledger.clone()),
        log: None,
    })
    .unwrap()
}

fn n(v: &JsValue, path: &str) -> f64 {
    let leaf = path.split('.').fold(v, |v, k| {
        v.as_object()
            .and_then(|o| o.get(k))
            .unwrap_or_else(|| panic!("no {k} in {path}"))
    });
    match leaf {
        JsValue::Number(x) => *x,
        other => panic!("{path} is {other:?}"),
    }
}

/// The first request gets a 503 (retried by the client), every later one
/// answers; an identical copy of a call is served by the memo.
#[test]
fn the_live_stack_files_every_call_under_its_namer() {
    let ok = completion(Some(r#"{"a":"alpha","b":"beta"}"#));
    let server = StubServer::start(
        Duration::ZERO,
        Arc::new(move |i, _| {
            if i == 0 {
                StubResponse::status(503, r#"{"error":{"message":"busy"}}"#)
                    .with_header("retry-after-ms", "1")
            } else {
                StubResponse::ok(ok.clone())
            }
        }),
    );
    let ledger = Arc::new(CallLedger::new(legal));
    let client = live(&server, &ledger);
    let results = client.run_wave(vec![
        call(CallSite::Naming, "a"),
        call(CallSite::Naming, "a"),
        call(CallSite::Vendor, "b"),
    ]);
    assert!(results.iter().all(Result::is_ok));
    let s = ledger.stats();
    assert_eq!(n(&s, "outcomes.naming.calls"), 2.0);
    assert_eq!(n(&s, "outcomes.naming.answers.proposed"), 2.0);
    assert_eq!(n(&s, "outcomes.vendor.calls"), 1.0);
    assert_eq!(n(&s, "transport.naming.sent"), 1.0);
    assert_eq!(n(&s, "transport.naming.servedMemo"), 1.0);
    assert_eq!(n(&s, "transport.naming.okAfterRetry"), 1.0);
    assert_eq!(n(&s, "transport.naming.retried.status5xx"), 1.0);
    assert_eq!(n(&s, "transport.naming.httpAttempts"), 2.0);
    assert_eq!(n(&s, "transport.vendor.okFirstTry"), 1.0);
    assert_eq!(n(&s, "transport.vendor.tokens.total"), 2.0);
    assert_eq!(n(&s, "timing.total.attemptMs.n"), 3.0);
    assert_eq!(server.requests(), 3);
}

/// A call the server refuses for good is FAILED in both views, by kind.
#[test]
fn a_call_that_fails_after_every_retry_is_failed_by_kind() {
    let server = StubServer::start(
        Duration::ZERO,
        Arc::new(|_, _| {
            StubResponse::status(503, r#"{"error":{"message":"down"}}"#)
                .with_header("x-should-retry", "false")
        }),
    );
    let ledger = Arc::new(CallLedger::new(legal));
    let client = live(&server, &ledger);
    let results = client.run_wave(vec![call(CallSite::Split, "a")]);
    assert!(results[0].is_err());
    let s = ledger.stats();
    assert_eq!(n(&s, "outcomes.split.failed"), 1.0);
    assert_eq!(n(&s, "transport.split.failed.status"), 1.0);
    assert_eq!(n(&s, "transport.split.httpAttempts"), 1.0);
    assert_eq!(ledger.failed(CallSite::Split), 1);
}
