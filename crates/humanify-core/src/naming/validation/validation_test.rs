//! Ported fixture-for-fixture from src/llm/validation.test.ts, plus the
//! probe vectors (test/parity/wp42-vectors.json, recorded from the real TS).

use std::collections::HashSet;

use super::*;
use crate::naming::test_vectors;

fn used(names: &[&str]) -> HashSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

fn conflict(name: &str, set: &HashSet<String>) -> String {
    resolve_conflict(name, |c| set.contains(c))
}

#[test]
fn accepts_valid_identifiers() {
    for name in [
        "foo",
        "_foo",
        "$foo",
        "foo123",
        "camelCase",
        "PascalCase",
        "_privateVar",
        "$jquery",
    ] {
        assert!(is_valid_identifier(name), "{name}");
    }
}

#[test]
fn rejects_invalid_identifiers() {
    for name in ["", "123foo", "foo-bar", "foo bar", "foo.bar"] {
        assert!(!is_valid_identifier(name), "{name}");
    }
}

#[test]
fn sanitize_removes_invalid_characters() {
    assert_eq!(sanitize_identifier("foo-bar"), "foobar");
    assert_eq!(sanitize_identifier("foo.bar"), "foobar");
    assert_eq!(sanitize_identifier("foo bar"), "foobar");
    assert_eq!(sanitize_identifier("foo@bar#baz"), "foobarbaz");
}

#[test]
fn sanitize_prefixes_identifiers_starting_with_numbers() {
    assert_eq!(sanitize_identifier("123foo"), "_123foo");
    assert_eq!(sanitize_identifier("1"), "_1");
}

#[test]
fn sanitize_handles_empty_input() {
    assert_eq!(sanitize_identifier(""), "_unnamed");
    // "@#$" strips to "$", which is a global builtin (jQuery) → suffixed
    assert_eq!(sanitize_identifier("@#$"), "$_");
    assert_eq!(sanitize_identifier("@#%"), "_unnamed");
}

#[test]
fn sanitize_appends_underscore_to_reserved_words() {
    assert_eq!(sanitize_identifier("if"), "if_");
    assert_eq!(sanitize_identifier("class"), "class_");
    assert_eq!(sanitize_identifier("function"), "function_");
    assert_eq!(sanitize_identifier("return"), "return_");
}

#[test]
fn sanitize_appends_underscore_to_global_builtins() {
    for name in ["Date", "Math", "JSON", "console", "Promise"] {
        assert_eq!(sanitize_identifier(name), format!("{name}_"));
    }
}

#[test]
fn sanitize_preserves_valid_identifiers() {
    for name in ["validName", "_private", "$dollar"] {
        assert_eq!(sanitize_identifier(name), name);
    }
}

#[test]
fn resolve_conflict_tries_semantic_suffixes_first() {
    assert_eq!(conflict("name", &used(&["name"])), "nameVal");
}

#[test]
fn resolve_conflict_uses_numeric_suffix_when_semantic_exhausted() {
    let set = used(&[
        "name",
        "nameVal",
        "nameVar",
        "nameRef",
        "nameItem",
        "nameData",
        "nameResult",
        "nameValue",
    ]);
    assert_eq!(conflict("name", &set), "name2");
}

#[test]
fn resolve_conflict_increments_numeric_suffix() {
    let set = used(&[
        "name",
        "nameVal",
        "nameVar",
        "nameRef",
        "nameItem",
        "nameData",
        "nameResult",
        "nameValue",
        "name2",
        "name3",
    ]);
    assert_eq!(conflict("name", &set), "name4");
}

#[test]
fn resolve_conflict_stays_numeric_past_100() {
    let mut set = used(&["name"]);
    for suffix in DECORATION_WORDS {
        set.insert(format!("name{suffix}"));
    }
    for i in 2..=100 {
        set.insert(format!("name{i}"));
    }
    assert_eq!(conflict("name", &set), "name101");
}

/// The TS test walks 1,200 rungs asserting stem recovery through
/// prior-name-snap's `nameStem` (WP4.5, not yet ported). The parity form is
/// stronger: the whole ladder, rung for rung, equals the TS's — through the
/// 999 numeric rungs into the `nameVal2…` decorated-numeric tail.
#[test]
fn conflict_ladders_match_the_ts_rung_for_rung() {
    let v = test_vectors();
    for (name, ladder) in v["conflictLadders"].as_object().unwrap() {
        let mut set = used(&[name.as_str()]);
        for (i, expected) in ladder.as_array().unwrap().iter().enumerate() {
            let got = conflict(name, &set);
            assert_eq!(got, expected.as_str().unwrap(), "{name} rung {i}");
            assert!(set.insert(got));
        }
    }
}

#[test]
fn reserved_words_contain_keywords_literals_and_arguments() {
    for w in [
        "if",
        "else",
        "for",
        "while",
        "function",
        "class",
        "const",
        "let",
        "var",
        "return",
        "await",
        "null",
        "true",
        "false",
        "undefined",
        "arguments",
    ] {
        assert!(is_reserved_word(w), "{w}");
    }
    // "async" is not a reserved word, only "await" is
    assert!(!is_reserved_word("async"));
}

#[test]
fn global_builtins_contain_core_platform_and_host_globals() {
    for name in [
        "Date",
        "Math",
        "JSON",
        "Array",
        "Object",
        "Map",
        "Set",
        "Promise",
        "Error",
        "RegExp",
        "Symbol",
        "Proxy",
        "Reflect",
        "console",
        "globalThis",
        "File",
        "Blob",
        "FormData",
        "URL",
        "URLSearchParams",
        "Buffer",
        "process",
        "setTimeout",
        "setInterval",
        "clearTimeout",
        "clearInterval",
        "AbortController",
        "window",
        "document",
        "self",
        "location",
        "navigator",
        "$",
        "jQuery",
        "define",
        "Bun",
        "importScripts",
        "postMessage",
    ] {
        assert!(is_global_builtin(name), "{name}");
    }
}

#[test]
fn global_builtins_do_not_blanket_forbid_browser_names() {
    for name in ["event", "status", "name", "history", "screen"] {
        assert!(!is_global_builtin(name), "{name}");
    }
}

/// The three tables equal the TS's, in the TS's order (GLOBAL_BUILTINS is
/// generated from the pinned `globals` package — a bump shows up here).
#[test]
fn tables_equal_the_ts_tables() {
    let v = test_vectors();
    let list = |key: &str| -> Vec<String> {
        v["tables"][key]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(GLOBAL_BUILTINS.to_vec(), list("globalBuiltins"));
    assert_eq!(RESERVED_WORDS.to_vec(), list("reservedWords"));
    assert_eq!(DECORATION_WORDS.to_vec(), list("decorationWords"));
}

#[test]
fn validation_vectors_match_the_ts() {
    let v = test_vectors();
    for row in v["validation"].as_array().unwrap() {
        let input = row["input"].as_str().unwrap();
        assert_eq!(
            is_valid_identifier(input),
            row["isValid"].as_bool().unwrap(),
            "isValid {input:?}"
        );
        assert_eq!(
            sanitize_identifier(input),
            row["sanitized"].as_str().unwrap(),
            "sanitize {input:?}"
        );
    }
}
