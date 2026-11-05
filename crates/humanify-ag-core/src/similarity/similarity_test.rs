use std::collections::BTreeSet;

use super::{
    containment_from_counts, jaccard, jaccard_from_counts, shingle_windows, weighted_jaccard,
};

fn windows(tokens: &[&str], k: usize) -> Vec<String> {
    shingle_windows(tokens, k).map(|w| w.join(" ")).collect()
}

#[test]
fn a_stream_no_longer_than_k_is_one_shingle_of_the_whole() {
    assert_eq!(windows(&[], 4), vec![""]);
    assert_eq!(windows(&["a"], 4), vec!["a"]);
    assert_eq!(windows(&["a", "b", "c", "d"], 4), vec!["a b c d"]);
}

#[test]
fn a_longer_stream_yields_every_k_window_in_order() {
    assert_eq!(
        windows(&["a", "b", "c", "d", "e", "f"], 4),
        vec!["a b c d", "b c d e", "c d e f"]
    );
    assert_eq!(windows(&["a", "b", "a", "b"], 1), vec!["a", "b", "a", "b"]);
}

#[test]
fn jaccard_of_two_empty_sets_is_one_and_of_one_empty_set_is_zero() {
    let empty: BTreeSet<String> = BTreeSet::new();
    let a: BTreeSet<String> = ["x", "y", "z"].iter().map(|s| s.to_string()).collect();
    let b: BTreeSet<String> = ["y", "z", "w"].iter().map(|s| s.to_string()).collect();
    assert_eq!(jaccard(&empty, &empty), 1.0);
    assert_eq!(jaccard(&a, &empty), 0.0);
    assert_eq!(jaccard(&a, &b), 2.0 / 4.0);
    assert_eq!(jaccard(&a, &a), 1.0);
}

#[test]
fn jaccard_from_counts_is_zero_on_an_empty_union() {
    assert_eq!(jaccard_from_counts(0, 0, 0), 0.0);
    assert_eq!(jaccard_from_counts(2, 3, 3), 2.0 / 4.0);
    assert_eq!(jaccard_from_counts(3, 3, 3), 1.0);
}

#[test]
fn weighted_jaccard_is_zero_on_a_non_positive_union() {
    assert_eq!(weighted_jaccard(0.0, 0.0, 0.0), 0.0);
    assert_eq!(weighted_jaccard(2.0, 3.0, 3.0), 2.0 / 4.0);
    // The union is (a + b) - shared, in that order — the float order the
    // vendor pairing's literal score has always used.
    let (s, a, b) = (0.1, 0.7, 0.2);
    assert_eq!(weighted_jaccard(s, a, b), s / (a + b - s));
}

#[test]
fn containment_divides_by_the_smaller_set() {
    assert_eq!(containment_from_counts(2, 2, 10), 1.0);
    assert_eq!(containment_from_counts(1, 4, 2), 0.5);
    assert_eq!(containment_from_counts(0, 0, 5), 0.0);
}
