//! The matching counts (exp208): read from each scored run's stats,
//! checked for exact equality where the pipeline is deterministic and
//! banded where they move with the LLM's draws.

use crate::json::js::JsValue;

use super::*;
use crate::eval::card::number_at;
use crate::eval::llm_diag::digest;

fn stats(fn_llm: f64, fn_failed: f64, mb_llm: f64) -> JsValue {
    JsValue::parse(&format!(
        r#"{{"coverage":{{"functions":{{"total":100,"llm":{fn_llm},"cached":80,"closeMatch":5,"failed":{fn_failed}}},
        "moduleBindings":{{"total":50,"llm":{mb_llm},"cached":40}},"identifiers":{{"total":30,"llm":25}}}},
        "resolutionStats":{{"unmatched":7,"stillAmbiguous":3}}}}"#
    ))
    .unwrap()
}

#[test]
fn functions_sent_to_the_llm_count_the_ones_it_failed_to_name() {
    let d = digest(&MATCHING, &stats(10.0, 2.0, 4.0)).unwrap();
    let expected = [
        ("fnTotal", 100.0),
        ("fnCarried", 80.0),
        ("fnClose", 5.0),
        ("fnAsked", 12.0),
        ("fnLLMRenamed", 10.0),
        ("fnUnmatched", 7.0),
        ("fnAmbiguous", 3.0),
        ("mbCarried", 40.0),
        ("mbLLMRenamed", 4.0),
        ("idAsked", 30.0),
        ("idLLMRenamed", 25.0),
    ];
    for (k, v) in expected {
        assert_eq!(number_at(&d, k), Some(v), "{k}");
    }
}

#[test]
fn a_count_missing_from_the_stats_is_absent_never_zero() {
    assert!(digest(&MATCHING, &JsValue::parse(r#"{"reask":{}}"#).unwrap()).is_none());
    let partial = JsValue::parse(r#"{"coverage":{"functions":{"total":9,"llm":1}}}"#).unwrap();
    let d = digest(&MATCHING, &partial).unwrap();
    assert_eq!(number_at(&d, "fnTotal"), Some(9.0));
    assert_eq!(number_at(&d, "fnAsked"), None);
    assert_eq!(number_at(&d, "fnUnmatched"), None);
}
