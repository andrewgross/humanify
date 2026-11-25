use super::{Decided, Functions, MatchDecisions, record};

const FRESH: &str = "function a() {}\nvar b = function () {}, c = () => 1;\nfunction d() {}\n";
const PRIOR: &str = "function one() {}\nfunction two() {}\n\nfunction three() {}\n";

fn at(text: &str, needle: &str) -> u32 {
    text.find(needle).unwrap() as u32
}

#[test]
fn decisions_name_functions_by_their_place_in_each_text() {
    let fresh = Functions {
        starts: vec![
            ("f:d", at(FRESH, "function d")),
            ("f:c", at(FRESH, "() => 1")),
            ("f:a", 0),
            ("f:b", at(FRESH, "function ()")),
        ],
    };
    let prior = Functions {
        starts: vec![
            ("p:1", 0),
            ("p:2", at(PRIOR, "function two")),
            ("p:3", at(PRIOR, "function three")),
        ],
    };
    let decided = Decided {
        exact: vec![("p:1", "f:a"), ("p:9", "f:b")],
        close: vec![("p:3", "f:c")],
        ambiguous: vec!["p:2", "p:8"],
    };
    assert_eq!(
        record(&fresh, &prior, &decided),
        MatchDecisions {
            fresh: vec![0, 24, 44, 53],
            prior: 3,
            exact: vec![(0, 0)],
            close: vec![(2, 2)],
            ambiguous: vec![1],
        }
    );
}
