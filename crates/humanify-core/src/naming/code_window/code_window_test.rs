//! Ported fixture-for-fixture from src/rename/code-window.test.ts (the TS
//! probe vectors that also ran here were retired 2026-09-28).

use super::*;

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

/// The prompt guard's audit (2026-10-04): when the loc mapping cannot be
/// trusted the selection USED to fall back to the flat first-500 cut, so a
/// requested identifier declared past line 500 was asked blind. With the
/// requested names known, every one is located in the generated code and
/// windowed — the flat cut is left only for a selection that names no
/// identifier.
#[test]
fn an_untrusted_mapping_still_windows_every_requested_identifier() {
    let code = make_lines(600).replace("  line(550);", "  var Qz = line(550);");
    let anchors = [Some(630)];
    let names = ["Qz".to_string()];
    let r = select_function_code(&FunctionCodeSelection {
        fn_start_line: Some(100),
        fn_end_line: Some(650),
        anchor_start_lines: Some(&anchors),
        identifier_names: Some(&names),
        ..sel(&code)
    });
    assert!(r.contains("var Qz = line(550);"), "{r}");
    assert!(line_count(&r) <= MAX_CODE_LINES);
    assert!(crate::naming::shown::unshown(&r, &names).is_empty());
}
