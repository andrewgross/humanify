//! TS originals: `src/rename/prior-match-map.test.ts` (its five cases) and
//! the carry's slice contract.

use super::{build_prior_match_map, empty_matcher_carry, matcher_carry};

fn map(refs: &[(&str, &str)]) -> Vec<(String, String)> {
    build_prior_match_map(refs.iter().copied())
}

#[test]
fn keeps_a_flipped_binding() {
    assert_eq!(
        map(&[("noop4", "serializeTask")]),
        [("noop4".to_string(), "serializeTask".to_string())]
    );
}

#[test]
fn drops_a_pinned_binding() {
    assert_eq!(
        map(&[
            ("serializeTask", "serializeTask"),
            ("emptyFn", "processDataVal")
        ]),
        [("emptyFn".to_string(), "processDataVal".to_string())]
    );
}

#[test]
fn drops_a_final_name_with_conflicting_priors() {
    assert!(map(&[("noop4", "serializeTask"), ("noop4", "authManager")]).is_empty());
}

#[test]
fn keeps_a_final_name_seen_twice_with_the_same_prior() {
    assert_eq!(
        map(&[("noop4", "serializeTask"), ("noop4", "serializeTask")]),
        [("noop4".to_string(), "serializeTask".to_string())]
    );
}

#[test]
fn an_empty_ref_list_is_an_empty_map() {
    assert!(map(&[]).is_empty());
}

#[test]
fn the_carry_slices_each_statement() {
    let text = "var a = 1;\nfunction f() {}\n";
    let spans = [oxc_span::Span::new(0, 10), oxc_span::Span::new(11, 26)];
    assert_eq!(
        matcher_carry(text, &spans).statement_texts,
        ["var a = 1;", "function f() {}"]
    );
    assert!(empty_matcher_carry().statement_texts.is_empty());
}
