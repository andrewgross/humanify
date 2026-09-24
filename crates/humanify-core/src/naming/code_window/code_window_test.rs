//! Ported fixture-for-fixture from src/rename/code-window.test.ts, plus the
//! probe vectors (line-math edges recorded from the real TS).

use serde_json::Value;

use super::*;
use crate::naming::test_vectors;

fn make_lines(n: usize) -> String {
    (1..=n)
        .map(|i| format!("  line({i});"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sel(code: &str) -> FunctionCodeSelection<'_> {
    FunctionCodeSelection {
        code,
        session_id: "t",
        fn_start_line: None,
        fn_end_line: None,
        anchor_start_lines: None,
        identifier_names: None,
    }
}

fn line_count(s: &str) -> usize {
    s.split('\n').count()
}

#[test]
fn returns_code_unchanged_at_or_under_the_cap() {
    let code = make_lines(MAX_CODE_LINES);
    assert_eq!(select_function_code(&sel(&code)), code);
}

#[test]
fn falls_back_to_flat_truncation_when_locs_are_missing() {
    let code = make_lines(600);
    let r = select_function_code(&sel(&code));
    assert_eq!(line_count(&r), MAX_CODE_LINES + 2);
    assert!(r.contains("[truncated]"));
    assert!(r.contains("line(500);"));
    assert!(!r.contains("line(501);"));
}

#[test]
fn falls_back_when_loc_span_disagrees_with_line_count() {
    let code = make_lines(600);
    let anchors = [Some(630)];
    let r = select_function_code(&FunctionCodeSelection {
        fn_start_line: Some(100),
        fn_end_line: Some(650),
        anchor_start_lines: Some(&anchors),
        ..sel(&code)
    });
    assert!(r.contains("[truncated]"));
    assert!(!r.contains("line(531);"));
}

#[test]
fn windows_around_a_past_cap_anchor() {
    let code = make_lines(1000);
    let anchors = [Some(2800)];
    let r = select_function_code(&FunctionCodeSelection {
        fn_start_line: Some(2000),
        fn_end_line: Some(2999),
        anchor_start_lines: Some(&anchors),
        ..sel(&code)
    });
    assert!(r.contains("line(801);"));
    assert!(r.contains("line(1);"));
    assert!(r.contains("line(1000);"));
    assert!(r.contains("omitted"));
    assert!(line_count(&r) <= MAX_CODE_LINES + 4);
    assert!(!r.contains("line(400);"));
}

#[test]
fn merges_overlapping_anchor_windows() {
    let code = make_lines(1000);
    let anchors = [Some(700), Some(710), Some(715)];
    let r = select_function_code(&FunctionCodeSelection {
        fn_start_line: Some(1),
        fn_end_line: Some(1000),
        anchor_start_lines: Some(&anchors),
        ..sel(&code)
    });
    assert_eq!(r.split('\n').filter(|l| l.contains("omitted")).count(), 2);
    assert!(r.contains("line(700);"));
    assert!(r.contains("line(715);"));
}

#[test]
fn anchors_outside_the_range_add_no_window() {
    let code = make_lines(800);
    let anchors = [Some(50), None];
    let r = select_function_code(&FunctionCodeSelection {
        fn_start_line: Some(100),
        fn_end_line: Some(899),
        anchor_start_lines: Some(&anchors),
        ..sel(&code)
    });
    assert!(r.contains("line(1);"));
    assert!(r.contains("line(800);"));
    assert!(line_count(&r) < 100);
}

#[test]
fn caps_prior_version_context() {
    let code = make_lines(3000);
    let capped = cap_context_code(&code, "t");
    assert!(line_count(&capped) <= MAX_CODE_LINES + 2);
    assert!(capped.contains("line(500);"));
    assert!(!capped.contains("line(501);"));
    let small = make_lines(100);
    assert_eq!(cap_context_code(&small, "t"), small);
}

#[test]
fn shrinks_padding_to_fit_many_spread_anchors() {
    let code = make_lines(3000);
    let anchors: Vec<Option<i64>> = (0..10).map(|i| Some(200 + i * 280)).collect();
    let r = select_function_code(&FunctionCodeSelection {
        fn_start_line: Some(1),
        fn_end_line: Some(3000),
        anchor_start_lines: Some(&anchors),
        ..sel(&code)
    });
    for a in anchors.iter().flatten() {
        assert!(r.contains(&format!("line({a});")), "anchor {a}");
    }
    assert!(line_count(&r) <= MAX_CODE_LINES + 12);
}

fn oversized(decl_line: usize, name: &str) -> String {
    let lines: Vec<String> = (1..=900)
        .map(|i| {
            if i == decl_line {
                format!("  let {name} = compute({});", i - 1)
            } else {
                format!("  line({i});")
            }
        })
        .collect();
    format!("function big() {{\n{}\n}}", lines.join("\n"))
}

#[test]
fn shows_an_identifier_whose_declaration_loc_is_unknown() {
    let code = oversized(700, "cr_2");
    let anchors = [None];
    let names = ["cr_2".to_string()];
    let r = select_function_code(&FunctionCodeSelection {
        session_id: "t.js:1:0",
        fn_start_line: Some(1),
        fn_end_line: Some(902),
        anchor_start_lines: Some(&anchors),
        identifier_names: Some(&names),
        ..sel(&code)
    });
    assert!(r.contains("cr_2"));
}

#[test]
fn shows_an_identifier_whose_declaration_loc_is_outside_the_range() {
    let code = oversized(800, "qt");
    let anchors = [Some(5000)];
    let names = ["qt".to_string()];
    let r = select_function_code(&FunctionCodeSelection {
        session_id: "t.js:1:0",
        fn_start_line: Some(1),
        fn_end_line: Some(902),
        anchor_start_lines: Some(&anchors),
        identifier_names: Some(&names),
        ..sel(&code)
    });
    assert!(r.contains("qt"));
}

// ---- probe vectors ----

fn opt_i64(v: &Value) -> Option<i64> {
    v.as_i64()
}

/// Run one recorded `selectFunctionCode` input (vector or capture row
/// shape: nulls for undefined).
pub(crate) fn run_recorded_selection(s: &Value) -> String {
    let anchors: Option<Vec<Option<i64>>> = s["anchorStartLines"]
        .as_array()
        .map(|a| a.iter().map(opt_i64).collect());
    let names: Option<Vec<String>> = s["identifierNames"]
        .as_array()
        .map(|a| a.iter().map(|n| n.as_str().unwrap().to_string()).collect());
    select_function_code(&FunctionCodeSelection {
        code: s["code"].as_str().unwrap(),
        session_id: s["sessionId"].as_str().unwrap(),
        fn_start_line: opt_i64(&s["fnStartLine"]),
        fn_end_line: opt_i64(&s["fnEndLine"]),
        anchor_start_lines: anchors.as_deref(),
        identifier_names: names.as_deref(),
    })
}

#[test]
fn probe_vectors_match_the_ts_byte_for_byte() {
    let v = test_vectors();
    let cases = v["codeWindow"].as_array().unwrap();
    assert!(cases.len() >= 16);
    for c in cases {
        let got = match c["fn"].as_str().unwrap() {
            "selectFunctionCode" => run_recorded_selection(&c["sel"]),
            "capContextCode" => cap_context_code(c["code"].as_str().unwrap(), "s"),
            other => panic!("{other}"),
        };
        assert_eq!(got, c["out"].as_str().unwrap(), "case {}", c["name"]);
    }
}
