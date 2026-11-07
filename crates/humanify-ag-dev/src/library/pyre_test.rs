//! Vectors recorded from Python 3.11.2's `re` (the oracle), including the
//! code points where Rust's `char::is_alphanumeric` and Python's
//! `isalnum` disagree (a Devanagari vowel sign, a circled letter).

use super::*;

#[test]
fn tokens_match_python_findall() {
    let cases: &[(&str, &[&str])] = &[
        (
            "foo bar baz1 _abc $x12 12 123 4567a ab1c",
            &["baz1", "_abc", "$x12", "123", "4567", "ab1c"],
        ),
        (
            "caf\u{e9}X\u{301}yz \u{966}\u{967}\u{968} x\u{b2}\u{b3}a \u{915}\u{93f}\u{915}\u{93f}\u{915} a\u{2160}bc",
            &[
                "caf\u{e9}X",
                "\u{966}\u{967}\u{968}",
                "x\u{b2}\u{b3}a",
                "a\u{2160}bc",
            ],
        ),
        ("X\u{903}abc A\u{24b6}bcd", &[]),
    ];
    for (text, want) in cases {
        assert_eq!(tokens(text), *want, "{text:?}");
    }
}

#[test]
fn commonjs_virtual_matches_python_search() {
    let cases = [
        ("a.cjs?commonjs-entry", true),
        ("a?commonjs-proxy\n", true),
        ("a?commonjs-Proxy", false),
        ("x?commonjs-a?commonjs-b", true),
        ("a?commonjs-", false),
        ("a?commonjs-x\ny", false),
    ];
    for (name, want) in cases {
        assert_eq!(is_commonjs_virtual(name), want, "{name:?}");
    }
}

#[test]
fn classes_differ_from_rust_where_python_says_so() {
    // U+093F DEVANAGARI VOWEL SIGN I: Rust alphanumeric, Python not.
    assert!('\u{93f}'.is_alphanumeric());
    assert!(!is_word('\u{93f}'));
    assert!(is_word('\u{b2}'));
    assert!(is_digit('\u{966}'));
    assert!(!is_digit('\u{b2}'));
    assert!(is_space('\u{a0}') && is_space('\x1c') && !is_space('\u{200b}'));
}
