use std::collections::BTreeMap;

use serde_json::json;

use super::{Decisions, score_decisions};
use crate::difflabel::{InputFile, Tree, Truth};

const FROM: &str = "function a(x){return x+1}function b(y){return y*2}function c(z){return z-3}";
const TO: &str =
    "function r(y){return y*2}function n(){}function q(x){return x+1}function s(z){return z-3}";

fn tree(text: &str, functions: &[&str]) -> Tree {
    let rows: Vec<String> = functions
        .iter()
        .enumerate()
        .map(|(i, f)| format!("[{},0,{}]", i + 1, text.find(f).unwrap()))
        .collect();
    let provenance = format!(
        r#"{{"version":1,"rows":"","inputFiles":["index.js"],"files":{{".humanify/humanified.js":[{}]}}}}"#,
        rows.join(",")
    );
    Tree::from_parts(BTreeMap::new(), Some(&provenance)).unwrap()
}

#[test]
fn the_card_counts_each_unchanged_function_once() {
    let truth = Truth::compare(
        vec![InputFile::parse("index.js", FROM)],
        vec![InputFile::parse("index.js", TO)],
    );
    let from = tree(FROM, &["function a", "function b", "function c"]);
    let to = tree(
        TO,
        &["function r", "function n", "function q", "function s"],
    );
    let decisions: Decisions = serde_json::from_value(json!({
        "version": 1, "fresh": [1, 2, 3, 4], "prior": [1, 2, 3],
        "exact": [[1, 0], [2, 2]], "close": [], "ambiguous": [0], "unmatched": [],
    }))
    .unwrap();
    let scored = score_decisions(&truth, &from, &to, &decisions);
    assert_eq!(
        scored.card,
        json!({
            "functions": {"fresh": 4, "prior": 3, "exact": 2, "close": 0, "ambiguous": 1, "unmatched": 0},
            "freshStandings": {"changed": 1, "unchanged": 3},
            "freshByOwnByte": 4,
            "unchanged": 3, "correct": 1, "wrong": 1,
            "missedClose": 0, "missedCloseOther": 0, "missedAmbiguous": 0, "missedUnmatched": 1,
            "notJudged": 0, "otherMatched": 0, "otherUnmatched": 1,
            "precision": 0.5, "recall": 1.0 / 3.0,
            "initWrappers": {
                "unchanged": 0, "correct": 0, "wrong": 0,
                "missedClose": 0, "missedCloseOther": 0, "missedAmbiguous": 0, "missedUnmatched": 0,
                "notJudged": 0, "otherMatched": 2, "otherUnmatched": 2,
                "precision": 0.0, "recall": 0.0, "inputFileUnpaired": 0,
            },
        })
    );
    assert!(
        scored
            .report
            .contains("precision 50.0% (1 of 2); recall 33.3% (1 of 3)"),
        "{}",
        scored.report
    );
}
