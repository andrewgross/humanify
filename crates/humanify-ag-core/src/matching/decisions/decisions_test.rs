use serde_json::json;

use super::{Decided, Functions, record};

const FRESH: &str = "function a() {}\nvar b = function () {}, c = () => 1;\nfunction d() {}\n";
const PRIOR: &str = "function one() {}\nfunction two() {}\n\nfunction three() {}\n";

fn at(text: &str, needle: &str) -> u32 {
    text.find(needle).unwrap() as u32
}

#[test]
fn decisions_name_functions_by_their_place_in_each_text() {
    let fresh = Functions {
        text: FRESH,
        starts: vec![
            ("f:d", at(FRESH, "function d")),
            ("f:c", at(FRESH, "() => 1")),
            ("f:a", 0),
            ("f:b", at(FRESH, "function ()")),
        ],
    };
    let prior = Functions {
        text: PRIOR,
        starts: vec![
            ("p:1", 0),
            ("p:2", at(PRIOR, "function two")),
            ("p:3", at(PRIOR, "function three")),
        ],
    };
    let decided = Decided {
        exact: vec![("p:1", "f:a"), ("p:9", "f:b")],
        close: vec![("p:3", "f:c")],
        ambiguous: vec!["p:2"],
        unmatched: vec!["p:3", "p:8"],
    };
    let got = serde_json::to_value(record(&fresh, &prior, &decided)).unwrap();
    assert_eq!(
        got,
        json!({
            "version": 1,
            "fresh": [1, 2, 2, 3],
            "prior": [1, 2, 4],
            "exact": [[0, 0]],
            "close": [[2, 2]],
            "ambiguous": [1],
            "unmatched": [2],
        })
    );
}
