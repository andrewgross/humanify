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

/// The UTF-16 code-unit offset of byte offset `byte_at` (a char boundary
/// of `s`) — a JS string index, for writers that must reproduce the TS's
/// offsets from a Rust byte span.
pub fn utf16_offset(s: &str, byte_at: usize) -> usize {
    s[..byte_at].chars().map(char::len_utf16).sum()
}

/// An identifier character (`[A-Za-z0-9_$]`): what a minifier's names and
/// the shapes' captures are made of.
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// The identifier at `at` (`[A-Za-z_$][A-Za-z0-9_$]*`), returning the
/// offset past it.
pub fn eat_ident(s: &str, at: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let first = *bytes.get(at)?;
    if !is_ident_byte(first) || first.is_ascii_digit() {
        return None;
    }
    let len = bytes[at..]
        .iter()
        .take_while(|&&b| is_ident_byte(b))
        .count();
    Some(at + len)
}

/// A minifier-insensitive SHAPE at `at`: `pattern`'s space-separated
/// tokens in order, JS whitespace allowed between any two. A token `$a`
/// … `$z` matches an identifier — the SAME identifier wherever that
/// capture repeats (a minifier renames a binding, never one use of it); a
/// token `x|y` matches either literal; every other token is a literal.
/// Returns the offset just past the match.
pub fn eat_shape(s: &str, at: usize, pattern: &str) -> Option<usize> {
    let mut captures: [Option<&str>; 26] = [None; 26];
    let mut pos = at;
    for (k, token) in pattern.split(' ').filter(|t| !t.is_empty()).enumerate() {
        if k > 0 {
            pos = skip_js_space(s, pos);
        }
        let capture = token
            .strip_prefix('$')
            .filter(|c| c.len() == 1 && c.as_bytes()[0].is_ascii_lowercase());
        pos = match capture {
            Some(c) => {
                let end = eat_ident(s, pos)?;
                let slot = &mut captures[usize::from(c.as_bytes()[0] - b'a')];
                match slot {
                    Some(seen) if *seen != &s[pos..end] => return None,
                    _ => *slot = Some(&s[pos..end]),
                }
                end
            }
            None => token.split('|').find_map(|alt| eat(s, pos, alt))?,
        };
    }
    Some(pos)
}

/// How many times `pattern` ([`eat_shape`]) occurs in `s`, counting up to
/// `limit`. Its first token must be a plain literal: the search anchors on
/// it.
pub fn count_shapes(s: &str, pattern: &str, limit: usize) -> usize {
    let anchor = pattern.split(' ').find(|t| !t.is_empty()).unwrap_or("");
    debug_assert!(!anchor.is_empty() && !anchor.starts_with('$') && !anchor.contains('|'));
    s.match_indices(anchor)
        .filter(|&(i, _)| eat_shape(s, i, pattern).is_some())
        .take(limit)
        .count()
}

/// Past what may precede a program's first statement: whitespace, a
/// hashbang line at the very start, and comments (a license banner).
pub fn skip_preamble(s: &str) -> usize {
    let mut at = 0;
    if s.starts_with("#!") {
        at = s.find('\n').map_or(s.len(), |i| i + 1);
    }
    loop {
        at = skip_js_space(s, at);
        if let Some(open) = eat(s, at, "/*") {
            match s[open..].find("*/") {
                Some(i) => at = open + i + 2,
                None => return s.len(),
            }
        } else if let Some(open) = eat(s, at, "//") {
            at = s[open..].find('\n').map_or(s.len(), |i| open + i + 1);
        } else {
            return at;
        }
    }
}
