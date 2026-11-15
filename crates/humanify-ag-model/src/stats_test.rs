//! The `--stats-json` record: the absent-is-not-zero rules the harness
//! reads, and counts written as integers.

use crate::stats::{CoverageSummary, EvalStats, LlmCoverage, RenameCounts};

fn repo(rel: &str) -> String {
    format!("{}/../../{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn vectors() -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(repo("test/parity/wpb4-vectors.json")).unwrap())
        .unwrap()
}

#[test]
fn absent_blocks_are_omitted_and_the_always_written_ones_are_present() {
    let text = EvalStats::default().to_file_text();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(!text.ends_with('\n'), "no trailing newline");
    assert!(v["bindingResolutionStats"].is_null());
    assert!(
        v.as_object()
            .unwrap()
            .contains_key("bindingResolutionStats")
    );
    assert!(v["renameClaims"].is_object());
    for absent in ["vendorNaming", "coverage", "reask", "llmCalls", "inputLink"] {
        assert!(v.get(absent).is_none(), "{absent} is omitted when absent");
    }
}

#[test]
fn counts_are_integers_and_a_mean_keeps_its_value() {
    let stats = EvalStats {
        coverage: Some(CoverageSummary {
            functions: RenameCounts {
                total: 3,
                ..RenameCounts::default()
            },
            llm: Some(LlmCoverage {
                avg_response_time_ms: 412.5,
                ..LlmCoverage::default()
            }),
            ..CoverageSummary::default()
        }),
        ..EvalStats::default()
    };
    let text = stats.to_file_text();
    assert!(text.contains("\"total\": 3,"), "{text}");
    assert!(text.contains("\"avgResponseTimeMs\": 412.5"), "{text}");
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
