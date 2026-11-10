//! The matching counts (exp208): read from each scored run's stats,
//! checked for exact equality where the pipeline is deterministic and
//! banded where they move with the LLM's draws.

use humanify_ag_model::js::{JsObject, JsValue};

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

fn pair_digest(pair: &str, s: &JsValue) -> JsValue {
    let mut o = JsObject::new();
    o.insert("pair", JsValue::str(pair));
    for (k, v) in digest(&MATCHING, s).unwrap().as_object().unwrap().entries() {
        o.insert(k.clone(), v.clone());
    }
    JsValue::Object(o)
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

#[test]
fn the_deterministic_counts_are_the_exact_ones() {
    for k in [
        "fnTotal",
        "fnClose",
        "fnAsked",
        "fnUnmatched",
        "fnAmbiguous",
    ] {
        assert!(is_exact(k), "{k}");
    }
    for k in [
        "fnCarried",
        "fnLLMRenamed",
        "mbCarried",
        "mbLLMRenamed",
        "idAsked",
        "idLLMRenamed",
    ] {
        assert!(!is_exact(k), "{k}");
    }
}

fn label(name: &str, pairs: &[(&str, JsValue)]) -> (String, Vec<JsValue>) {
    (
        name.to_string(),
        pairs.iter().map(|(p, s)| pair_digest(p, s)).collect(),
    )
}

#[test]
fn repeats_band_the_draws_and_name_every_exact_count_that_differs() {
    let pairs = vec!["a->b".to_string(), "b->c".to_string()];
    let ok = repeats(
        &[
            label(
                "r1",
                &[
                    ("a->b", stats(10.0, 2.0, 4.0)),
                    ("b->c", stats(5.0, 0.0, 1.0)),
                ],
            ),
            label(
                "r2",
                &[
                    ("a->b", stats(11.0, 1.0, 6.0)),
                    ("b->c", stats(5.0, 0.0, 2.0)),
                ],
            ),
        ],
        &pairs,
    )
    .unwrap();
    assert!(ok.not_equal.is_empty(), "{:?}", ok.not_equal);
    let spread = |k: &str| ok.spreads.iter().find(|(x, _)| x == k).map(|(_, v)| *v);
    // Totals over the pairs: mbLLMRenamed 5 vs 8.
    assert_eq!(spread("mbLLMRenamed"), Some(3.0));
    assert_eq!(spread("fnLLMRenamed"), Some(1.0));
    assert_eq!(spread("fnAsked"), Some(0.0));
    // An exact count that differs on one pair is named, pair and values,
    // even when another pair's difference cancels it in the total.
    let bad = repeats(
        &[
            label(
                "r1",
                &[
                    ("a->b", stats(10.0, 2.0, 4.0)),
                    ("b->c", stats(5.0, 0.0, 1.0)),
                ],
            ),
            label(
                "r2",
                &[
                    ("a->b", stats(10.0, 3.0, 4.0)),
                    ("b->c", stats(4.0, 0.0, 1.0)),
                ],
            ),
        ],
        &pairs,
    )
    .unwrap();
    assert_eq!(bad.not_equal.len(), 2, "{:?}", bad.not_equal);
    assert!(bad.not_equal[0].contains("fnAsked"), "{:?}", bad.not_equal);
    assert!(bad.not_equal[0].contains("a->b"), "{:?}", bad.not_equal);
    assert!(bad.not_equal[0].contains("r1=12") && bad.not_equal[0].contains("r2=13"));
    // A repeat without matching counts: nothing measured.
    assert!(repeats(&[label("r1", &[]), label("r2", &[])], &pairs).is_none());
}

#[test]
fn a_same_commit_comparison_flags_exact_moves_and_banded_moves_outside_the_band() {
    let pairs = vec!["a->b".to_string()];
    let fold =
        |s: JsValue| crate::eval::llm_diag::fold(&MATCHING, &[pair_digest("a->b", &s)], &pairs);
    let rows = vec![
        ("base".to_string(), fold(stats(10.0, 2.0, 4.0))),
        ("cand".to_string(), fold(stats(10.0, 3.0, 9.0))),
        ("legacy".to_string(), None),
    ];
    let band = |k: &str| (k == "mbLLMRenamed").then_some(3.0);
    let text = flags(&rows, &band, true).join("\n");
    assert!(text.contains("cand") && text.contains("fnAsked"), "{text}");
    assert!(text.contains("must be EQUAL"), "{text}");
    assert!(
        text.contains("mbLLMRenamed") && text.contains("outside"),
        "{text}"
    );
    // fnLLMRenamed moved 0, no line; the legacy row cannot be judged.
    assert!(!text.contains("fnLLMRenamed"), "{text}");
    // Different commits: the counts move with the code, nothing is flagged.
    let other = flags(&rows, &band, false).join("\n");
    assert!(!other.contains("!!"), "{other}");
    // Inside the band: no line.
    let rows = vec![
        ("base".to_string(), fold(stats(10.0, 2.0, 4.0))),
        ("cand".to_string(), fold(stats(10.0, 2.0, 6.0))),
    ];
    assert!(!flags(&rows, &band, true).join("\n").contains("!!"));
}
