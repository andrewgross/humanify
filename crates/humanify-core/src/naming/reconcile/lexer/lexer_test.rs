//! The lexer's JS semantics the TS fixtures cannot reach (their lines are
//! ASCII): `\s` is ECMAScript WhiteSpace ∪ LineTerminator, columns are
//! UTF-16 units, and two different lone surrogates are different tokens.

use super::*;

fn toks(line: &str) -> Option<Vec<(TokenKind, String, usize)>> {
    let u = units(line);
    tokenize_line(&u).map(|t| {
        t.iter()
            .map(|t| (t.kind, String::from_utf16_lossy(t.text(&u)), t.col()))
            .collect()
    })
}

#[test]
fn feff_is_whitespace_and_nel_is_not() {
    // U+FEFF is JS `\s` (Rust's is_whitespace says no); U+0085 is not JS
    // `\s` (Rust's says yes) — it is a one-unit punctuation token.
    let t = toks("a\u{FEFF}b").unwrap();
    assert_eq!(t[1].0, TokenKind::Text);
    assert!(is_ws_only(&units("\u{FEFF}")));
    assert!(!is_ws_only(&units("\u{0085}")));
    let t = toks("a\u{0085}b").unwrap();
    assert_eq!(t.len(), 3);
}

#[test]
fn columns_count_utf16_units() {
    // `é` is one unit, an astral char two: the identifier after them
    // starts at the UTF-16 column the TS reports.
    let t = toks("\"é😀\" + x").unwrap();
    let x = t.iter().find(|t| t.1 == "x").unwrap();
    assert_eq!(x.2, 8); // TS: col 8 (probed on f7a707d)
}

#[test]
fn lone_surrogates_do_not_compare_equal() {
    let a: Vec<u16> = vec![u16::from(b'x'), 0xD800];
    let b: Vec<u16> = vec![u16::from(b'x'), 0xD801];
    assert_eq!(compare_line_pair(&a, &b), None);
    assert!(compare_line_pair(&a, &a).is_some());
}

#[test]
fn open_constructs_are_not_self_contained() {
    assert_eq!(toks("var a = `x${"), None);
    assert_eq!(toks("/* open"), None);
    assert_eq!(toks("'open"), None);
    assert!(toks("a / b").is_some());
    assert_eq!(toks("x = /re[/]g"), None, "unterminated regex");
    assert_eq!(toks("x = /re[/]/g").map(|t| t.len()), Some(5));
}
