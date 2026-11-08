//! The label's LLM call diagnostics (exp206): one digest per scored run,
//! folded without inventing numbers for runs that have none.

use humanify_ag_model::js::JsValue;

use super::{digest, fold, tables};

fn stats(calls: f64, p90: f64, taken: f64) -> JsValue {
    JsValue::parse(&format!(
        r#"{{"llmCalls":{{"outcomes":{{"total":{{"calls":{calls},"failed":0,"truncated":0,
        "answers":{{"asked":10,"proposed":7,"takenName":{taken},"duplicateInAnswer":0,"invalid":0,
        "unchanged":0,"missing":0,"hinted":0,"equalsPriorHint":0}}}}}},
        "transport":{{"total":{{"sent":{calls},"servedMemo":0,"servedDisk":0,"okAfterRetry":1,
        "retried":{{"timeout":2,"connection":0,"status5xx":0,"status429":0}},
        "failed":{{"timeout":0,"status":0}}}}}},
        "timing":{{"total":{{"attemptMs":{{"p50":10,"p90":{p90},"p99":50,"max":60}}}}}}}},
        "reask":{{"lateRejections":1,"invalidSuggestionFinishes":0,"sweepReasked":0}}}}"#
    ))
    .unwrap()
}

fn with_pair(pair: &str, d: JsValue) -> JsValue {
    let mut o = humanify_ag_model::js::JsObject::new();
    o.insert("pair", JsValue::str(pair));
    for (k, v) in d.as_object().unwrap().entries() {
        o.insert(k.clone(), v.clone());
    }
    JsValue::Object(o)
}

fn n(v: &JsValue, k: &str) -> Option<f64> {
    super::number_at(v, k)
}

#[test]
fn a_run_without_a_call_ledger_has_no_digest() {
    assert!(digest(&JsValue::parse(r#"{"reask":{"lateRejections":3}}"#).unwrap()).is_none());
    let d = digest(&stats(5.0, 20.0, 1.0)).unwrap();
    assert_eq!(n(&d, "calls"), Some(5.0));
    assert_eq!(n(&d, "rTimeout"), Some(2.0));
    assert_eq!(n(&d, "taken"), Some(1.0));
    assert_eq!(n(&d, "lateRej"), Some(1.0));
    // Backup counters a pre-exp206 reask block lacks are absent, not zero.
    assert_eq!(n(&d, "handoff"), None);
}

#[test]
fn counts_sum_and_percentiles_take_the_worst_pair() {
    let digests = vec![
        with_pair("a->b", digest(&stats(5.0, 20.0, 1.0)).unwrap()),
        with_pair("b->c", digest(&stats(7.0, 90.0, 2.0)).unwrap()),
    ];
    let pairs = vec!["a->b".to_string(), "b->c".to_string()];
    let f = fold(&digests, &pairs).unwrap();
    assert_eq!(n(&f, "calls"), Some(12.0));
    assert_eq!(n(&f, "taken"), Some(3.0));
    assert_eq!(n(&f, "p90ms"), Some(90.0));
    // A pair without a digest makes the whole fold unknown.
    let more = vec!["a->b".to_string(), "c->d".to_string()];
    assert!(fold(&digests, &more).is_none());
}

#[test]
fn the_tables_print_deltas_against_the_first_row_and_dash_for_legacy() {
    let a = digest(&stats(5.0, 20.0, 1.0));
    let b = digest(&stats(5.0, 80.0, 4.0));
    let text = tables(
        "t",
        &[
            ("base".into(), a),
            ("cand".into(), b),
            ("legacy".into(), None),
        ],
        true,
    )
    .join("\n");
    assert!(text.contains("DIAGNOSTICS, not KPIs"), "{text}");
    let cand = text
        .lines()
        .find(|l| l.starts_with("cand") && l.contains("(+60)"))
        .unwrap();
    assert!(cand.contains("80 (+60)"), "{cand}");
    let legacy: Vec<&str> = text.lines().filter(|l| l.starts_with("legacy")).collect();
    assert!(
        legacy
            .iter()
            .all(|l| l.split_whitespace().skip(1).all(|c| c == "-")),
        "{legacy:?}"
    );
}
