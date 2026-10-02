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

/// The 2026-09-29 `reask` bump is ADDITIVE: a pre-bump text (no `reask`
/// key) strict-parses under the new record and re-emits WITHOUT the block —
/// absent stays absent. (The committed results/ scorecards are all TS-era
/// files that predate even `bindingResolutionStats` and never
/// strict-parsed; the writer-format proof is the four TS vectors in the
/// round-trip test, the reader-format proof is 034's analyze.test.ts over a
/// recorded card.)
#[test]
fn an_old_format_stats_text_stays_reask_absent() {
    let v = vectors();
    let text = v["writers"]["evalStats"][0]["text"].as_str().unwrap();
    let stats = EvalStats::parse(text).unwrap();
    assert!(stats.reask.is_none(), "the TS-era text predates the bump");
    assert!(!stats.to_file_text().contains("\"reask\""));
}

/// The 2026-10-02 `waveGauges` bump is additive the same way `reask` was:
/// a pre-bump text strict-parses under the new record and re-emits
/// WITHOUT the block — absent stays absent (finding #66's gauges; every
/// recorded scorecard predates the block).
#[test]
fn an_old_format_stats_text_stays_wave_gauges_absent() {
    let v = vectors();
    let text = v["writers"]["evalStats"][0]["text"].as_str().unwrap();
    let stats = EvalStats::parse(text).unwrap();
    assert!(stats.wave_gauges.is_none(), "the text predates the bump");
    assert!(!stats.to_file_text().contains("\"waveGauges\""));
}

/// The 2026-10-02 strategy-split bump INSIDE `waveGauges` is additive the
/// same way again: a pre-split `waveGauges` block (the sixth vector)
/// strict-parses with every sub-key absent and re-emits byte-identically
/// — absent stays absent (finding #66's taken-set sub-gauge).
#[test]
fn a_pre_split_wave_gauges_block_stays_split_absent() {
    let v = vectors();
    let text = v["writers"]["evalStats"][5]["text"].as_str().unwrap();
    let stats = EvalStats::parse(text).unwrap();
    let g = stats.wave_gauges.as_ref().expect("the block is present");
    assert!(g.strategy_bindings_bytes.is_none());
    assert!(g.strategy_taken_bytes.is_none());
    assert!(g.strategy_callee_bytes.is_none());
    assert!(g.strategy_callsite_bytes.is_none());
    assert!(g.strategy_context_var_bytes.is_none());
    assert!(g.strategy_module_bytes.is_none());
    assert!(g.taken_set_names.is_none());
    assert_eq!(stats.to_file_text(), text);
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
    // 4 = the TS's recorded bytes (must stay byte-for-byte: old-format
    // scorecards still strict-parse and re-emit unchanged); +1 = the
    // Rust-added `reask` vector (2026-09-29, the deliberate schema bump);
    // +1 = the Rust-added `waveGauges` vector (2026-10-02, finding #66's
    // additive bump — the same precedent); +1 = the Rust-added strategy-
    // SPLIT vector (2026-10-02, finding #66's taken-set sub-gauge).
    assert_eq!(cases.len(), 7);
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
