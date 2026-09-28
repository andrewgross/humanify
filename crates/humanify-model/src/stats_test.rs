//! The `--stats-json` shape gate (WPB.4 gate part 3):
//!
//! 1. the Rust record's shape equals the recorded
//!    writeEvalStats' `stats` literal (test/parity/wpb4-stats-schema.json)
//!    — the `--stats-json` layout the eval harness consumes;
//! 2. the writer's outputs on synthetic results (wpb4-vectors.json
//!    `writers.evalStats`) round-trip the same way, covering the omitted
//!    optionals real runs always set.
//!
//! (The six recorded TS-run files — test/parity/wpb4-stats-*.json, the
//! oracle-f7a707d and main-2026-09-18 runs' own bytes — were retired
//! 2026-09-28 with the other TS-capture replays; real-run strict parses
//! are exercised by every eval the harness scores.)

use crate::js::{JsValue, stringify_pretty};
use crate::jsshape::JsType;
use crate::stats::EvalStats;

fn repo(rel: &str) -> String {
    format!("{}/../../{rel}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn record_shape_equals_the_ts_type() {
    let ts: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(repo("test/parity/wpb4-stats-schema.json")).unwrap(),
    )
    .unwrap();
    // The probe keeps the type's declaration order; compare as a set.
    fn sort_props(v: &mut serde_json::Value) {
        if let Some(props) = v.get_mut("props").and_then(|p| p.as_array_mut()) {
            props.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
            for p in props.iter_mut() {
                sort_props(&mut p["type"]);
            }
        }
        for key in ["items", "values"] {
            if let Some(inner) = v.get_mut(key) {
                sort_props(inner);
            }
        }
    }
    let mut ts = ts;
    sort_props(&mut ts);
    let rust = EvalStats::schema().to_probe_json();
    assert_eq!(
        rust,
        ts,
        "\nrust {}\nts   {}",
        serde_json::to_string(&rust).unwrap(),
        serde_json::to_string(&ts).unwrap()
    );
}

fn vectors() -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(repo("test/parity/wpb4-vectors.json")).unwrap())
        .unwrap()
}

#[test]
fn ts_writer_outputs_round_trip() {
    let v = vectors();
    let cases = v["writers"]["evalStats"].as_array().unwrap();
    for case in cases {
        let text = case["text"].as_str().unwrap();
        let stats = EvalStats::parse(text).unwrap();
        assert_eq!(stats.to_file_text(), text);
    }
    assert_eq!(cases.len(), 4);
}

#[test]
fn locale_compare_matches_js_on_every_corpus_pair() {
    let v = vectors();
    let c = &v["collation"];
    let corpus: Vec<&str> = c["corpus"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    let mut pairs = 0;
    for (i, a) in corpus.iter().enumerate() {
        for (j, b) in corpus.iter().enumerate() {
            let want = c["sign"][i][j].as_i64().unwrap();
            let got = match crate::js::locale_compare(a, b) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            assert_eq!(got, want, "{a:?}.localeCompare({b:?})");
            pairs += 1;
        }
    }
    let mut sorted = corpus.clone();
    sorted.sort_by(|a, b| crate::js::locale_compare(a, b));
    let ts: Vec<&str> = c["sorted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    assert_eq!(sorted, ts);
    eprintln!("collation: {pairs} pairs identical");
}

#[test]
fn stringify_pretty_matches_json_stringify_indent_2() {
    let v = vectors();
    for case in v["pretty"].as_array().unwrap() {
        let text = case["text"].as_str().unwrap();
        // Parse the TS output (JS object rules) and re-emit.
        let value = JsValue::parse(text).unwrap();
        assert_eq!(stringify_pretty(&value, 2), text);
    }
}
