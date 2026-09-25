//! The babel-printer emulation's rules, each pinned by a real divergence
//! the naming-graph probe found on the oracle texts (wp43-gen-probe.ts vs
//! `humanify waves --probe-graph`, all four oracle-f7a707d pairs).

use oxc_allocator::Allocator;
use oxc_span::Span;

use super::{Replacement, TextView};
use crate::ingest::Ingest;

fn with_view<R>(code: &str, f: impl FnOnce(&TextView<'_>, &str) -> R) -> R {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let view = TextView::build(ingest.semantic());
    f(&view, code)
}

fn span_of(code: &str, needle: &str, end_needle: &str) -> Span {
    let start = code.find(needle).expect("needle") as u32;
    let end = code.rfind(end_needle).expect("end needle") as u32 + end_needle.len() as u32;
    Span::new(start, end)
}

#[test]
fn pretty_dedents_to_the_nodes_line_indentation() {
    let code = "{\n  var f = function () {\n    return 1;\n  };\n}\n";
    with_view(code, |v, c| {
        let start = c.find("function").unwrap() as u32;
        let end = c.find("  };").unwrap() as u32 + 3;
        assert_eq!(
            v.pretty(Span::new(start, end), &[], true),
            "function () {\n  return 1;\n}"
        );
    });
}

#[test]
fn a_template_quasi_starting_with_a_newline_is_raw_text() {
    // The newline right after `${H}` belongs to the quasi — verbatim.
    let code = "{\n  var s = `a ${H}\n      b`;\n}\n";
    with_view(code, |v, c| {
        let span = span_of(c, "`a", "b`");
        assert_eq!(v.pretty(span, &[], true), "`a ${H}\n      b`");
        assert_eq!(v.compact(span), "`a ${H}\n      b`");
    });
}

#[test]
fn a_line_beginning_inside_a_quasi_takes_the_level_of_the_line_above() {
    // `$ => {` starts on a line that begins inside the template: the
    // printer's indent level is the statement's (4), not that line's (0).
    let code = "{\n  {\n    x(`a\n`).map($ => {\n      return $;\n    });\n  }\n}\n";
    with_view(code, |v, c| {
        let start = c.find("$ =>").unwrap() as u32;
        let end = c.find("    });").unwrap() as u32 + 5;
        assert_eq!(
            v.pretty(Span::new(start, end), &[], true),
            "$ => {\n  return $;\n}"
        );
    });
}

#[test]
fn an_object_that_starts_an_arrow_body_loses_its_context_parens() {
    let code = "var f = _ => ({ a: 1 })[_];\n";
    with_view(code, |v, c| {
        let body = span_of(c, "({", "[_]");
        let edits = v.leading_object_paren_drop(body);
        assert_eq!(v.pretty(body, &edits, false), "{ a: 1 }[_]");
    });
}

#[test]
fn edits_replace_their_span() {
    let code = "function f(a) {\n  return a;\n}\n";
    with_view(code, |v, c| {
        let span = span_of(c, "function", "}");
        let a = c.find("return a").unwrap() as u32 + 7;
        let edits = vec![Replacement {
            span: Span::new(a, a + 1),
            text: "value".to_string(),
        }];
        assert_eq!(
            v.pretty(span, &edits, true),
            "function f(a) {\n  return value;\n}"
        );
    });
}
