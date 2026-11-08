//! The call ledger's categories (exp206): one fixed classification per
//! call, three views that never mix.

use crate::call_ledger::{
    Attempt, AttemptRecord, CallLedger, FailKind, SentOutcome, Served, classify_sent, judge_answer,
};
use crate::js::JsValue;
use crate::llm::{
    BatchRenameRequest, BatchRenameResponse, CallSite, LlmCall, LlmError, LlmErrorKind, Renames,
    StrMap, Usage,
};

fn legal(name: &str) -> bool {
    !name.is_empty() && name != "class" && name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn call(site: CallSite, ids: &[&str], used: &[&str]) -> LlmCall {
    LlmCall {
        request: BatchRenameRequest {
            identifiers: ids.iter().map(|s| s.to_string()).collect(),
            used_names: used.iter().map(|s| s.to_string()).collect(),
            ..BatchRenameRequest::default()
        },
        system_prompt: String::new(),
        user_prompt: String::new(),
        site,
    }
}

fn answer(pairs: &[(&str, Option<&str>)]) -> BatchRenameResponse {
    BatchRenameResponse {
        renames: Renames::from_entries(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.map(str::to_string))),
        ),
        finish_reason: None,
        usage: None,
    }
}

fn at<'a>(v: &'a JsValue, path: &str) -> &'a JsValue {
    path.split('.').fold(v, |v, k| {
        v.as_object()
            .and_then(|o| o.get(k))
            .unwrap_or_else(|| panic!("no {k} in {path}"))
    })
}

fn n(v: &JsValue, path: &str) -> f64 {
    match at(v, path) {
        JsValue::Number(x) => *x,
        other => panic!("{path} is {other:?}"),
    }
}

fn rec(result: Attempt, ms: u64) -> AttemptRecord {
    AttemptRecord { result, ms }
}

#[test]
fn every_asked_identifier_gets_exactly_one_verdict() {
    let mut c = call(
        CallSite::Naming,
        &["a", "b", "c", "d", "e", "f", "g"],
        &["taken"],
    );
    c.request.prior_name_hints = Some(StrMap(vec![
        ("a".into(), "count".into()),
        ("g".into(), "other".into()),
    ]));
    let r = answer(&[
        ("a", Some("count")),
        ("b", Some("b")),
        ("c", Some("class")),
        ("d", Some("same")),
        ("e", Some("same")),
        ("f", Some("taken")),
        ("g", None),
        ("zz", Some("stray")),
    ]);
    let v = judge_answer(&c, &r, legal);
    assert_eq!(v.asked, 7);
    assert_eq!(v.proposed, 1, "a");
    assert_eq!(v.unchanged, 1, "b");
    assert_eq!(v.invalid, 1, "c");
    assert_eq!(v.duplicate_in_answer, 2, "d and e");
    assert_eq!(v.taken_name, 1, "f");
    assert_eq!(v.missing, 1, "g");
    assert_eq!(
        v.proposed + v.unchanged + v.invalid + v.duplicate_in_answer + v.taken_name + v.missing,
        v.asked
    );
    assert_eq!((v.hinted, v.equals_prior_hint), (2, 1));
    assert_eq!(v.extra_keys, 1);
}

#[test]
fn a_sent_call_is_first_try_after_retry_or_failed_by_its_final_kind() {
    let ok = Ok(answer(&[]));
    assert_eq!(
        classify_sent(&[rec(Attempt::Ok, 5)], &ok),
        SentOutcome::FirstTry
    );
    assert_eq!(
        classify_sent(&[rec(Attempt::Timeout, 5), rec(Attempt::Ok, 5)], &ok),
        SentOutcome::AfterRetry
    );
    let timeout = Err(LlmError::new(LlmErrorKind::Timeout, "Request timed out."));
    assert_eq!(
        classify_sent(&[rec(Attempt::Timeout, 5)], &timeout),
        SentOutcome::Failed(FailKind::Timeout)
    );
    let malformed = Err(LlmError::new(
        LlmErrorKind::Other,
        "invalid completion body",
    ));
    assert_eq!(
        classify_sent(&[rec(Attempt::Malformed, 5)], &malformed),
        SentOutcome::Failed(FailKind::Malformed)
    );
}

#[test]
fn the_three_views_are_kept_apart_and_sum_into_total() {
    let ledger = CallLedger::new(legal);
    let c = call(CallSite::Vendor, &["a"], &[]);
    let mut ok = answer(&[("a", Some("name"))]);
    ok.usage = Some(Usage {
        total_tokens: Some(30),
        input_tokens: Some(20),
        output_tokens: Some(10),
    });
    let ok = Ok(ok);
    // One sent call answered after a 503 and a timeout.
    ledger.sent(
        CallSite::Vendor,
        &[
            rec(Attempt::Status(503), 100),
            rec(Attempt::Timeout, 300),
            rec(Attempt::Ok, 200),
        ],
        &ok,
        900,
    );
    ledger.outcome(&c, &ok);
    // A copy served by the memo.
    ledger.served(CallSite::Vendor, Served::Memo);
    ledger.outcome(&c, &ok);
    // A naming call that failed.
    let n_call = call(CallSite::Naming, &["x"], &[]);
    let failed = Err(LlmError::new(LlmErrorKind::Connection, "Connection error."));
    ledger.sent(
        CallSite::Naming,
        &[rec(Attempt::Connection, 50)],
        &failed,
        50,
    );
    ledger.outcome(&n_call, &failed);

    let s = ledger.stats();
    for (path, want) in [
        ("outcomes.vendor.calls", 2.0),
        ("outcomes.vendor.answered", 2.0),
        ("outcomes.vendor.answers.proposed", 2.0),
        ("outcomes.naming.failed", 1.0),
        ("outcomes.total.calls", 3.0),
        ("transport.vendor.sent", 1.0),
        ("transport.vendor.servedMemo", 1.0),
        ("transport.vendor.okAfterRetry", 1.0),
        ("transport.vendor.httpAttempts", 3.0),
        ("transport.vendor.retried.status5xx", 1.0),
        ("transport.vendor.retried.timeout", 1.0),
        ("transport.vendor.tokens.input", 20.0),
        ("transport.naming.failed.connection", 1.0),
        ("transport.total.sent", 2.0),
        ("timing.vendor.callMs.max", 900.0),
        ("timing.vendor.attemptMs.n", 3.0),
        ("timing.vendor.attemptMs.p50", 200.0),
        ("timing.total.callMs.n", 2.0),
    ] {
        assert_eq!(n(&s, path), want, "{path}");
    }
    // Every namer is present, zeros included.
    assert_eq!(n(&s, "outcomes.treeReviser.calls"), 0.0);
    assert_eq!(ledger.failed(CallSite::Naming), 1);
    let cov = ledger.coverage_llm();
    assert_eq!(cov.retries, 2);
    assert_eq!(cov.avg_response_time_ms, 475.0);
    assert_eq!(cov.input_tokens, Some(20));
}

#[test]
fn the_outcomes_view_does_not_depend_on_completion_order_or_transport() {
    // A cold leg (sent) and a warm leg (served from disk) that got the same
    // answers must write the same outcomes view, byte for byte.
    let calls = [
        call(CallSite::Naming, &["a"], &[]),
        call(CallSite::Naming, &["b"], &["x"]),
    ];
    let answers = [
        Ok(answer(&[("a", Some("alpha"))])),
        Ok(answer(&[("b", Some("x"))])),
    ];
    let cold = CallLedger::new(legal);
    for (c, r) in calls.iter().zip(&answers) {
        cold.sent(c.site, &[rec(Attempt::Ok, 7)], r, 7);
        cold.outcome(c, r);
    }
    let warm = CallLedger::new(legal);
    for (c, r) in calls.iter().zip(&answers).rev() {
        warm.served(c.site, Served::Disk);
        warm.outcome(c, r);
    }
    let text = |l: &CallLedger| crate::js::stringify(at(&l.stats(), "outcomes"));
    assert_eq!(text(&cold), text(&warm));
}
