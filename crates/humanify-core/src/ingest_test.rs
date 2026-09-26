//! Ingest unit tests: parse + semantic over small programs; the semantic
//! model's relative behavior (a symbol exists per binding; references grow
//! with uses) and loud parse errors.

use oxc_allocator::Allocator;

use crate::ingest::Ingest;

/// (symbols, scopes, references, top-level statements, errors) of one text.
fn counts(text: &str) -> (usize, usize, usize, usize, usize) {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "t.js");
    let scoping = ingest.semantic().scoping();
    (
        scoping.symbol_ids().len(),
        scoping.scope_descendants_from_root().len(),
        scoping.references_len(),
        ingest.program.body.len(),
        ingest.errors.len(),
    )
}

#[test]
fn ingest_models_a_simple_program() {
    let (symbols, scopes, references, statements, errors) =
        counts("var a = 1;\nfunction f() { return a + 1; }\nconsole.log(f());\n");
    assert_eq!(errors, 0, "a simple program must parse clean");
    assert!(symbols >= 2, "a and f are symbols (globals are unresolved)");
    assert!(scopes >= 2, "program + function scopes");
    assert!(references >= 2, "a's use inside f + f's call");
    assert_eq!(statements, 3);
}

#[test]
fn ingest_reports_parse_errors_loud() {
    let (.., errors) = counts("function {{{{{");
    assert!(errors > 0, "a broken program must report errors");
}

#[test]
fn ingest_references_grow_with_uses() {
    let one = counts("var a = 1; a;");
    let two = counts("var a = 1; a; a; a; a; a;");
    assert_eq!(one.0, two.0, "same bindings");
    assert!(two.2 > one.2, "more uses = more references");
}
