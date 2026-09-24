//! The JavaScript-regex semantics the TS detectors inherit from V8, as
//! explicit primitives (WPB.1). The TS patterns are non-unicode-mode
//! `RegExp`s, so:
//!
//! - `\s` is ECMAScript WhiteSpace + LineTerminator — it includes U+FEFF
//!   (not Unicode White_Space) and excludes U+0085 (which IS Unicode
//!   White_Space). Rust `char::is_whitespace` gets both wrong.
//! - `\b` is an ASCII word boundary: `[A-Za-z0-9_]` only, so `é` is a
//!   non-word character. A Unicode `\b` would disagree.
//! - `.` matches anything except the four LineTerminators
//!   (`\n`, `\r`, U+2028, U+2029).
//! - `String.prototype.slice` counts UTF-16 code units.
//!
//! Every detector is written against these, never against Rust's own
//! notions (no `regex` crate: the patterns are few and literal-anchored,
//! and hand matching keeps each semantic above visible at the call site).

/// ECMAScript `\s` (WhiteSpace ∪ LineTerminator) — the one owner is
/// `humanify_model::js::is_js_whitespace` (the same set `trim` strips).
pub use humanify_model::js::is_js_whitespace as is_js_space;

/// ECMAScript LineTerminator (what `.` refuses).
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// A non-unicode `\w` character.
fn is_word(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Non-unicode `\b` at byte offset `at` (a char boundary of `s`).
pub fn is_word_boundary(s: &str, at: usize) -> bool {
    is_word(s[..at].chars().next_back()) != is_word(s[at..].chars().next())
}

/// `s` after a greedy `\s*` (the byte offset where the run ends).
pub fn skip_js_space(s: &str, from: usize) -> usize {
    s[from..]
        .char_indices()
        .find(|&(_, c)| !is_js_space(c))
        .map_or(s.len(), |(i, _)| from + i)
}

/// `lit` at `at`, returning the offset just past it.
pub fn eat(s: &str, at: usize, lit: &str) -> Option<usize> {
    s[at..].starts_with(lit).then(|| at + lit.len())
}

/// `["']` at `at`.
pub fn eat_quote(s: &str, at: usize) -> Option<usize> {
    eat(s, at, "\"").or_else(|| eat(s, at, "'"))
}

/// `\bLIT\b` anywhere in `s`, for a `lit` that starts and ends with word
/// characters (so both boundaries are the "non-word neighbour" test).
pub fn contains_word(s: &str, lit: &str) -> bool {
    s.match_indices(lit)
        .any(|(i, _)| is_word_boundary(s, i) && is_word_boundary(s, i + lit.len()))
}

/// JS `code.slice(0, units)`: the longest prefix of at most `units` UTF-16
/// code units. Where the cut splits a surrogate pair, JS keeps a lone high
/// surrogate; it is dropped here, which no detector can observe — a lone
/// surrogate at the end of the window is a non-word, non-space,
/// non-terminator, non-ASCII character, exactly like end-of-input for
/// every pattern in `signals` (none can consume a character after it).
pub fn js_prefix(s: &str, units: usize) -> &str {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        used += c.len_utf16();
        if used > units {
            return &s[..i];
        }
    }
    s
}
