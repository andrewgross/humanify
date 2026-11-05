//! The JS TEXT lexer — the one owner of reading a module's tokens (and so
//! its string literals) from TEXT rather than from a parse (`lex.py`,
//! finding #90's pairing investigation): strings, templates, regex
//! literals, identifiers, numbers, punctuation; comments and whitespace
//! dropped. Good enough for similarity and for the declared-identity
//! readers, never for parsing.
//!
//! Callers: the vendor content pairing (`super::vendor_pairing` — masked
//! stream, its 5-token shingles and its literal feature), the declared-
//! identity readers (`super::identity`, `super::identity::provenance`) and
//! the vendor namer's evidence (`super::vendor_names`) — raw stream.
//!
//! Not every literal question reads this lexer, deliberately (see
//! docs/responsibility.md, "Which string literals does this code hold?"):
//! the function fingerprint's `stringLiterals` come from the AST
//! (`matching::features`, directives excluded), and the content anchor's
//! rare literals (`place::anchor::literals_of`) are the TS regex ported
//! verbatim — it matches quotes the lexer would read as part of a template
//! or a comment and stops at a backslash, so the two answer differently on
//! the same text and the anchor's answer is frozen.

const KEYWORDS: [&str; 40] = [
    "var",
    "let",
    "const",
    "function",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "break",
    "continue",
    "new",
    "this",
    "typeof",
    "instanceof",
    "in",
    "of",
    "void",
    "delete",
    "throw",
    "try",
    "catch",
    "finally",
    "class",
    "extends",
    "super",
    "null",
    "true",
    "false",
    "undefined",
    "async",
    "await",
    "yield",
    "get",
    "set",
    "static",
    "default",
];

/// The previous tokens after which a `/` starts a regex literal.
const REGEX_AFTER_WORDS: [&str; 14] = [
    "return",
    "typeof",
    "case",
    "do",
    "else",
    "in",
    "of",
    "new",
    "delete",
    "void",
    "throw",
    "instanceof",
    "yield",
    "await",
];
const REGEX_AFTER_CHARS: &str = "(,=:[!&|?{};+-*%<>~^";
const REGEX_AFTER_OPS: [&str; 10] = ["=>", "&&", "||", "??", "==", "===", "!=", "!==", "+=", "-="];

/// Multi-character punctuators, in the order the alternation tries them.
const PUNCTUATORS: [&str; 29] = [
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "=>", "==", "!=", "<=", ">=", "&&",
    "||", "??", "?.", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<", ">>",
];
const PUNCTUATOR_CHARS: &str = "{}()[];,<>+-*/%&|^!~?:=.@#";

fn is_id_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || ('\u{80}'..='\u{ffff}').contains(&c)
}

fn is_id_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$' || ('\u{80}'..='\u{ffff}').contains(&c)
}

/// Minified-looking: `lib_<8 hex>` (optionally `_<n>`), or 1-4 identifier
/// characters starting with an ASCII letter, `_` or `$`.
fn is_minified(t: &str) -> bool {
    if let Some(rest) = t.strip_prefix("lib_")
        && rest.len() >= 8
        && rest.is_char_boundary(8)
    {
        let (hex, tail) = rest.split_at(8);
        let hex_ok = hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let tail_ok = tail.is_empty()
            || tail
                .strip_prefix('_')
                .is_some_and(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()));
        if hex_ok && tail_ok {
            return true;
        }
    }
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let head_ok = first.is_ascii_alphabetic() || first == '_' || first == '$';
    let rest: Vec<char> = chars.collect();
    head_ok
        && rest.len() <= 3
        && rest
            .iter()
            .all(|&c| c.is_alphanumeric() || c == '_' || c == '$')
}

fn regex_may_follow(prev: Option<&str>) -> bool {
    let Some(p) = prev else {
        return true;
    };
    REGEX_AFTER_WORDS.contains(&p)
        || (p.chars().count() == 1 && REGEX_AFTER_CHARS.contains(p))
        || REGEX_AFTER_OPS.contains(&p)
}

fn starts_with_at(s: &[char], i: usize, pat: &str) -> bool {
    pat.chars()
        .enumerate()
        .all(|(k, c)| s.get(i + k) == Some(&c))
}

fn text_of(s: &[char], from: usize, to: usize) -> String {
    s[from.min(s.len())..to.min(s.len())].iter().collect()
}

/// The end of a quoted string starting at `i` (one past the closing quote,
/// clamped to the text).
fn string_end(s: &[char], i: usize) -> usize {
    let quote = s[i];
    let mut j = i + 1;
    while j < s.len() && s[j] != quote {
        j += if s[j] == '\\' { 2 } else { 1 };
    }
    j + 1
}

fn template_end(s: &[char], i: usize) -> usize {
    let mut j = i + 1;
    let mut depth = 0usize;
    while j < s.len() {
        if s[j] == '\\' {
            j += 2;
            continue;
        }
        if depth == 0 && s[j] == '`' {
            break;
        }
        if starts_with_at(s, j, "${") {
            depth += 1;
            j += 2;
            continue;
        }
        if depth > 0 && s[j] == '}' {
            depth -= 1;
        }
        j += 1;
    }
    j + 1
}

fn regex_end(s: &[char], i: usize) -> usize {
    let mut j = i + 1;
    let mut class = false;
    while j < s.len() && s[j] != '\n' {
        match s[j] {
            '\\' => {
                j += 2;
                continue;
            }
            '[' => class = true,
            ']' => class = false,
            '/' if !class => break,
            _ => {}
        }
        j += 1;
    }
    j += 1;
    while j < s.len() && s[j].is_alphabetic() {
        j += 1;
    }
    j
}

/// lex.py's NUM: hex/binary/octal, decimal with fraction and exponent, or
/// a leading-dot fraction; an optional BigInt `n`. None when nothing
/// matches (the char then lexes as punctuation).
fn number_end(s: &[char], i: usize) -> Option<usize> {
    let digits = |mut j: usize, allow_underscore: bool| {
        while j < s.len() && (s[j].is_ascii_digit() || (allow_underscore && s[j] == '_')) {
            j += 1;
        }
        j
    };
    let exponent = |j: usize| -> usize {
        if j < s.len() && (s[j] == 'e' || s[j] == 'E') {
            let mut k = j + 1;
            if k < s.len() && (s[k] == '+' || s[k] == '-') {
                k += 1;
            }
            let end = digits(k, false);
            if end > k {
                return end;
            }
        }
        j
    };
    let mut j;
    if s[i] == '0' && matches!(s.get(i + 1), Some('x' | 'X' | 'b' | 'B' | 'o' | 'O')) && {
        let k = i + 2;
        k < s.len() && (s[k].is_ascii_hexdigit() || s[k] == '_')
    } {
        j = i + 2;
        while j < s.len() && (s[j].is_ascii_hexdigit() || s[j] == '_') {
            j += 1;
        }
    } else if s[i].is_ascii_digit() {
        j = digits(i + 1, true);
        if j < s.len() && s[j] == '.' {
            j = digits(j + 1, false);
        }
        j = exponent(j);
    } else if s[i] == '.' && s.get(i + 1).is_some_and(|c| c.is_ascii_digit()) {
        j = digits(i + 1, false);
        j = exponent(j);
    } else {
        return None;
    }
    if j < s.len() && s[j] == 'n' {
        j += 1;
    }
    Some(j)
}

fn punctuator_end(s: &[char], i: usize) -> Option<usize> {
    if let Some(p) = PUNCTUATORS.iter().find(|p| starts_with_at(s, i, p)) {
        return Some(i + p.chars().count());
    }
    PUNCTUATOR_CHARS.contains(s[i]).then_some(i + 1)
}

/// The token stream of `text`, minified-looking identifiers masked to `I`.
pub fn lex(text: &str) -> Vec<String> {
    lex_with(text, true)
}

/// The token stream of `text` with every identifier as written — the
/// declared-identity readers' view ([`super::identity`]): a string token
/// keeps its quotes, so a reader sees where a string starts and ends.
pub fn lex_raw(text: &str) -> Vec<String> {
    lex_with(text, false)
}

fn lex_with(text: &str, mask: bool) -> Vec<String> {
    let s: Vec<char> = text.chars().collect();
    let n = s.len();
    let mut out: Vec<String> = Vec::new();
    // The previous significant token, RAW (unmasked).
    let mut prev: Option<String> = None;
    let mut i = 0usize;
    while i < n {
        let c = s[i];
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            i += 1;
            continue;
        }
        if starts_with_at(&s, i, "//") {
            i = (i..n).find(|&j| s[j] == '\n').unwrap_or(n);
            continue;
        }
        if starts_with_at(&s, i, "/*") {
            i = (i + 2..n.saturating_sub(1))
                .find(|&j| s[j] == '*' && s[j + 1] == '/')
                .map_or(n, |j| j + 2);
            continue;
        }
        let end = match c {
            '"' | '\'' => Some(string_end(&s, i)),
            '`' => Some(template_end(&s, i)),
            '/' if regex_may_follow(prev.as_deref()) => Some(regex_end(&s, i)),
            _ => None,
        };
        if let Some(end) = end {
            let tok = text_of(&s, i, end);
            prev = Some(tok.clone());
            out.push(tok);
            i = end;
            continue;
        }
        if is_id_start(c) {
            let mut j = i + 1;
            while j < n && is_id_part(s[j]) {
                j += 1;
            }
            let tok = text_of(&s, i, j);
            out.push(
                if mask && !KEYWORDS.contains(&tok.as_str()) && is_minified(&tok) {
                    "I".to_string()
                } else {
                    tok.clone()
                },
            );
            prev = Some(tok);
            i = j;
            continue;
        }
        let end = if c.is_ascii_digit() || c == '.' {
            number_end(&s, i)
        } else {
            None
        }
        .or_else(|| punctuator_end(&s, i))
        .unwrap_or(i + 1);
        let tok = text_of(&s, i, end);
        prev = Some(tok.clone());
        out.push(tok);
        i = end;
    }
    out
}

/// Whether a token is a string or template literal (it starts with its
/// quote).
pub fn is_string_token(t: &str) -> bool {
    matches!(t.chars().next(), Some('"' | '\'' | '`'))
}

/// The inside of a quoted token (its first and last character dropped).
pub fn unquoted(t: &str) -> &str {
    let mut chars = t.char_indices();
    let start = chars.next().map_or(0, |(_, c)| c.len_utf8());
    let end = t
        .char_indices()
        .last()
        .map_or(t.len(), |(i, _)| i)
        .max(start);
    &t[start..end]
}

#[cfg(test)]
#[path = "lex/lex_test.rs"]
mod lex_test;
