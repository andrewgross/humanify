use humanify_ag_core::layout_record::{FunctionId, Match};
use serde_json::json;

use super::score_layouts;
use crate::difflabel::{InputFile, Truth};
use crate::matchcard::join::join_test::{at, layout};

const FROM: &str = "function a(x){return x+1}function b(y){return y*2}function c(z){return z-3}";
const TO: &str =
    "function r(y){return y*2}function n(){}function q(x){return x+1}function s(z){return z-3}";

fn truth() -> Truth {
    Truth::compare(
        vec![InputFile::parse("index.js", FROM)],
        vec![InputFile::parse("index.js", TO)],
    )
}

fn new(id: FunctionId) -> (u32, FunctionId, Match) {
    (0, id, Match::New)
}

fn exact(id: FunctionId, prior: FunctionId) -> (u32, FunctionId, Match) {
    (0, id, Match::Exact(Some((0, prior))))
}

#[test]
fn the_card_counts_each_unchanged_function_once() {
    let from = layout(
        &["index.js"],
        &[
            new(at(FROM, "function a")),
            new(at(FROM, "function b")),
            new(at(FROM, "function c")),
        ],
    );
    let mut to = layout(
        &["index.js"],
        &[
            exact(at(TO, "function r"), at(FROM, "function b")),
            new(at(TO, "function n")),
            exact(at(TO, "function q"), at(FROM, "function c")),
            new(at(TO, "function s")),
        ],
    );
    to.prior_ambiguous = vec![(0, at(FROM, "function a"))];
    let scored = score_layouts(&truth(), &from, &to).unwrap();
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

#[test]
fn a_partner_the_previous_layout_does_not_list_is_an_error() {
    let from = layout(&["index.js"], &[new(at(FROM, "function a"))]);
    let to = layout(
        &["index.js"],
        &[exact(at(TO, "function r"), at(FROM, "function b"))],
    );
    let got = score_layouts(&truth(), &from, &to).map(|s| s.report);
    assert_eq!(
        got,
        Err("the previous layout lists no function Byte(25) of input 0".into())
    );
}
