//! The system `diff` as the reconcile reads it: normal format, CRLF
//! normalized, exit 1 ("files differ") is a result, not an error.

use super::{DIFF_DEADLINE, compute_normal_diff};

#[test]
fn differing_texts_give_normal_format_hunks() {
    let out = compute_normal_diff("a\nb\nc\n", "a\nx\nc\n").expect("diff ran");
    assert_eq!(out, "2c2\n< b\n---\n> x\n");
}

#[test]
fn identical_texts_give_no_hunks() {
    assert_eq!(compute_normal_diff("a\n", "a\n").expect("diff ran"), "");
}

#[test]
fn crlf_line_ends_are_not_a_difference() {
    assert_eq!(
        compute_normal_diff("a\r\nb\r\n", "a\nb\n").expect("diff ran"),
        ""
    );
}

#[test]
fn the_production_deadline_is_ten_minutes() {
    assert_eq!(DIFF_DEADLINE, std::time::Duration::from_secs(10 * 60));
}
