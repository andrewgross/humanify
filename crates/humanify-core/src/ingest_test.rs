//! Ingest unit tests (WP1.2): parse + semantic over small programs; the
//! counts' relative behavior (a symbol exists per binding; references grow
//! with uses).

use oxc_allocator::Allocator;

use crate::ingest::{Ingest, ingest_counts_of_file, wrapper_statement_count};

#[test]
fn ingest_counts_a_simple_program() {
    let text = "var a = 1;\nfunction f() { return a + 1; }\nconsole.log(f());\n";
    let (counts, errors) = ingest_counts_of_file(text, "t.js");
    assert!(
        errors.is_empty(),
        "a simple program must parse clean: {errors:?}"
    );
    assert!(
        counts.symbols >= 2,
        "a and f are symbols (globals are unresolved): {counts:?}"
    );
    assert!(counts.scopes >= 2, "program + function scopes: {counts:?}");
    assert!(
        counts.references >= 2,
        "a's use inside f + f's call: {counts:?}"
    );
    assert_eq!(counts.top_level_statements, 3);
}

#[test]
fn ingest_counts_the_wrapper_statements() {
    let allocator = Allocator::default();
    let text = "(function(){var a=1;var b=2;return a+b})();\n";
    let ingest = Ingest::parse(&allocator, text, "w.js");
    assert!(ingest.errors.is_empty());
    assert_eq!(wrapper_statement_count(ingest.program), 3);
}

#[test]
fn ingest_reports_parse_errors_loud() {
    let text = "function {{{{{";
    let (_counts, errors) = ingest_counts_of_file(text, "bad.js");
    assert!(!errors.is_empty(), "a broken program must report errors");
}

#[test]
fn ingest_counts_grow_with_uses() {
    let one = "var a = 1; a;";
    let two = "var a = 1; a; a; a; a; a;";
    let (c1, _) = ingest_counts_of_file(one, "1.js");
    let (c2, _) = ingest_counts_of_file(two, "2.js");
    assert_eq!(c1.symbols, c2.symbols, "same bindings");
    assert!(
        c2.references > c1.references,
        "more uses = more references ({c2:?} vs {c1:?})"
    );
}
