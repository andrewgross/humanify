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

// ── the ESTree JSON parse: every value JavaScript can hold ───────────────

/// The program's ESTree JSON for `text`, parsed by the one owner.
fn estree_of(text: &str) -> serde_json::Value {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "t.js");
    assert!(ingest.errors.is_empty(), "the program must parse clean");
    crate::ingest::program_estree_json(ingest.program)
}

/// The `init` of the `i`th declarator of the program's first statement.
fn init_of(program: &serde_json::Value, i: usize) -> &serde_json::Value {
    &program["body"][0]["declarations"][i]["init"]
}

/// A string holding a LONE surrogate (legal JavaScript, not legal
/// Unicode): oxc writes it as a `\uXXXX` escape that a strict JSON parser
/// refuses, and the whole program used to parse as `Null` — every reader
/// then lost every statement. Each lone surrogate keeps its own code unit,
/// so two different ones stay two different values.
#[test]
fn a_lone_surrogate_string_parses_and_keeps_its_code_unit() {
    let program = estree_of(r#"var a = "\ud800", b = "\udfff", c = "x\udc00y";"#);
    assert!(
        program.is_object(),
        "the program must parse, not read as Null"
    );
    let a = init_of(&program, 0)["value"].as_str().expect("a string");
    let b = init_of(&program, 1)["value"].as_str().expect("a string");
    let c = init_of(&program, 2)["value"].as_str().expect("a string");
    assert_ne!(a, b, "two different lone surrogates stay distinct");
    assert!(c.starts_with('x') && c.ends_with('y'), "{c:?}");
}

/// A tagged template's cooked text holds lone surrogates the same way
/// (`String.raw` over a surrogate range): the shape that stopped a real
/// split.
#[test]
fn a_lone_surrogate_in_a_cooked_template_parses() {
    let program = estree_of(r"var r = String.raw`\ud800-\udfff`;");
    assert!(
        program.is_object(),
        "the program must parse, not read as Null"
    );
    let quasi = &init_of(&program, 0)["quasi"]["quasis"][0]["value"];
    assert_eq!(quasi["raw"], r"\ud800-\udfff");
    assert!(quasi["cooked"].is_string());
}

/// What is NOT a lone surrogate is left exactly as it was: a valid pair
/// (one astral character) and an escaped backslash followed by the text
/// `ud800` — the latter must not read as the lone surrogate.
#[test]
fn pairs_and_escaped_backslashes_are_untouched() {
    let program = estree_of(r#"var p = "😀", t = "\\ud800", l = "\ud800";"#);
    assert_eq!(init_of(&program, 0)["value"], "\u{1F600}");
    assert_eq!(init_of(&program, 1)["value"], "\\ud800");
    assert_ne!(
        init_of(&program, 1)["value"],
        init_of(&program, 2)["value"],
        "the text backslash-u-d800 and a lone surrogate are different strings"
    );
}

/// A number literal past the double range is `Infinity`; oxc writes it as
/// `1e+400`, which a strict JSON parser refuses as out of range. It parses
/// as a NUMBER (readers classify literals by their JSON type), and a string
/// holding the same text is left a string.
#[test]
fn an_infinite_number_literal_parses_as_a_number() {
    let program = estree_of(r#"var i = 1e400, s = "1e+400", n = 5;"#);
    assert!(
        program.is_object(),
        "the program must parse, not read as Null"
    );
    assert!(init_of(&program, 0)["value"].is_number());
    assert_eq!(init_of(&program, 1)["value"], "1e+400");
    assert_eq!(init_of(&program, 2)["value"], 5);
}
