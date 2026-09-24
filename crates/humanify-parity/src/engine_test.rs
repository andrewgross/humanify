//! `parse_sections`: an unknown name is an ERROR, never a silent drop — a
//! typo (`--sections twin-gates`) used to compare nothing and still print
//! IDENTICAL.

use crate::engine::{ALL_SECTIONS, parse_sections};

#[test]
fn empty_spec_selects_every_section() {
    let all = parse_sections("").expect("empty spec is valid");
    assert_eq!(all.len(), ALL_SECTIONS.len());
}

#[test]
fn known_names_come_back_in_canonical_order() {
    let got = parse_sections("matches, functions").expect("known names");
    assert_eq!(got, vec!["functions".to_string(), "matches".to_string()]);
}

#[test]
fn an_unknown_name_is_an_error_naming_it() {
    let err = parse_sections("matches,twin-gates").expect_err("unknown name must fail");
    assert!(
        err.contains("twin-gates"),
        "error must name the bad section: {err}"
    );
}
