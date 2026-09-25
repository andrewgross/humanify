//! The reconcile pass's line lexer — diff-reconcile.ts `tokenizeLine`,
//! `compareLinePair` and `lineSkeleton`.
//!
//! The TS walks a JS string, so every index and column here is a UTF-16
//! code unit and every character class is the TS one: the ident/number
//! classes are ASCII (`[A-Za-z_$]`, `[A-Za-z0-9_$]`, `[0-9A-Za-z_$.]`), and
//! `\s` / `trim()` are ECMAScript WhiteSpace ∪ LineTerminator
//! (`humanify_model::js::is_js_whitespace`). Token texts are kept as UTF-16
//! slices: a lone surrogate is a one-unit punctuation token in the TS, and
//! two different lone surrogates must not compare equal (a lossy UTF-8
//! conversion would merge them).

use humanify_model::js::is_js_whitespace;

/// A token's kind (`LineToken.kind`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Ident,
    Text,
}

/// One token: kind + the UTF-16 range `[start, end)` of the line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineToken {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

impl LineToken {
    /// The token's text in `line`.
    pub fn text<'l>(&self, line: &'l [u16]) -> &'l [u16] {
        &line[self.start..self.end]
    }

    /// `col` — the 0-based UTF-16 column of the token's first unit.
    pub fn col(&self) -> usize {
        self.start
    }
}

/// Words kept verbatim (keyword changes read as genuine).
const RESERVED_TOKEN_WORDS: [&str; 42] = [
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "static",
    "async",
    "await",
    "of",
];

/// `get`/`set` complete the TS set (kept apart only for the array length).
const RESERVED_ACCESSORS: [&str; 2] = ["get", "set"];

/// Tokens after which `/` is division.
const DIVISION_PRECEDERS: [&str; 7] = [")", "]", "this", "true", "false", "null", "super"];

fn is_ident_start(u: u16) -> bool {
    matches!(u, 0x41..=0x5a | 0x61..=0x7a | 0x5f | 0x24)
}

fn is_ident_cont(u: u16) -> bool {
    is_ident_start(u) || (0x30..=0x39).contains(&u)
}

fn is_number_cont(u: u16) -> bool {
    is_ident_cont(u) || u == 0x2e
}

fn is_digit(u: u16) -> bool {
    (0x30..=0x39).contains(&u)
}

/// ECMAScript `\s` on one UTF-16 unit (a surrogate half is never space).
pub fn is_space_unit(u: u16) -> bool {
    char::from_u32(u32::from(u)).is_some_and(is_js_whitespace)
}

/// `/^\s+$/` — non-empty and all JS whitespace.
pub fn is_ws_only(text: &[u16]) -> bool {
    !text.is_empty() && text.iter().all(|&u| is_space_unit(u))
}

/// `text.trim().length > 0`.
fn has_non_space(text: &[u16]) -> bool {
    text.iter().any(|&u| !is_space_unit(u))
}

fn eq_ascii(text: &[u16], word: &str) -> bool {
    text.len() == word.len()
        && text
            .iter()
            .zip(word.bytes())
            .all(|(&u, b)| u == u16::from(b))
}

fn is_reserved(text: &[u16]) -> bool {
    RESERVED_TOKEN_WORDS
        .iter()
        .chain(RESERVED_ACCESSORS.iter())
        .any(|w| eq_ascii(text, w))
}

/// A template/brace frame (`"quasi"` or a `${` brace depth).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Frame {
    Quasi,
    Depth(u32),
}

struct State<'l> {
    line: &'l [u16],
    i: usize,
    tokens: Vec<LineToken>,
    frames: Vec<Frame>,
    prev: Option<LineToken>,
    failed: bool,
}

impl State<'_> {
    fn at(&self, i: usize) -> Option<u16> {
        self.line.get(i).copied()
    }

    fn emit(&mut self, kind: TokenKind, start: usize) {
        let token = LineToken {
            kind,
            start,
            end: self.i,
        };
        self.tokens.push(token);
        if has_non_space(token.text(self.line)) {
            self.prev = Some(token);
        }
    }

    fn scan_simple_string(&mut self, quote: u16) {
        let start = self.i;
        self.i += 1;
        while self.i < self.line.len() {
            let ch = self.line[self.i];
            if ch == u16::from(b'\\') {
                self.i += 2;
                continue;
            }
            if ch == quote {
                self.i += 1;
                self.emit(TokenKind::Text, start);
                return;
            }
            self.i += 1;
        }
        self.failed = true;
    }

    fn scan_quasi(&mut self) {
        let start = self.i;
        while self.i < self.line.len() {
            let ch = self.line[self.i];
            if ch == u16::from(b'\\') {
                self.i += 2;
                continue;
            }
            if ch == u16::from(b'`') {
                self.i += 1;
                self.emit(TokenKind::Text, start);
                self.frames.pop();
                return;
            }
            if ch == u16::from(b'$') && self.at(self.i + 1) == Some(u16::from(b'{')) {
                self.i += 2;
                self.emit(TokenKind::Text, start);
                self.frames.push(Frame::Depth(0));
                return;
            }
            self.i += 1;
        }
        self.failed = true;
    }

    fn scan_slash(&mut self) {
        let next = self.at(self.i + 1);
        if next == Some(u16::from(b'/')) {
            let start = self.i;
            self.i = self.line.len();
            self.emit(TokenKind::Text, start);
            return;
        }
        if next == Some(u16::from(b'*')) {
            let Some(end) = index_of_close_comment(self.line, self.i + 2) else {
                self.failed = true;
                return;
            };
            let start = self.i;
            self.i = end + 2;
            self.emit(TokenKind::Text, start);
            return;
        }
        if self.is_division_context() {
            let start = self.i;
            self.i += 1;
            self.emit(TokenKind::Text, start);
            return;
        }
        self.scan_regex();
    }

    fn is_division_context(&self) -> bool {
        let Some(prev) = self.prev else {
            return false;
        };
        if prev.kind == TokenKind::Ident {
            return true;
        }
        let text = prev.text(self.line);
        if DIVISION_PRECEDERS.iter().any(|w| eq_ascii(text, w)) {
            return true;
        }
        text.first().is_some_and(|&u| is_digit(u))
    }

    fn scan_regex(&mut self) {
        let start = self.i;
        self.i += 1;
        let mut in_class = false;
        while self.i < self.line.len() {
            let ch = self.line[self.i];
            if ch == u16::from(b'\\') {
                self.i += 2;
                continue;
            }
            if ch == u16::from(b'[') {
                in_class = true;
            } else if ch == u16::from(b']') {
                in_class = false;
            } else if ch == u16::from(b'/') && !in_class {
                self.i += 1;
                while self.i < self.line.len() && is_ident_cont(self.line[self.i]) {
                    self.i += 1;
                }
                self.emit(TokenKind::Text, start);
                return;
            }
            self.i += 1;
        }
        self.failed = true;
    }

    fn scan_word(&mut self) {
        let start = self.i;
        self.i += 1;
        while self.i < self.line.len() && is_ident_cont(self.line[self.i]) {
            self.i += 1;
        }
        let kind = if is_reserved(&self.line[start..self.i]) {
            TokenKind::Text
        } else {
            TokenKind::Ident
        };
        self.emit(kind, start);
    }

    fn scan_number(&mut self) {
        let start = self.i;
        self.i += 1;
        while self.i < self.line.len() && is_number_cont(self.line[self.i]) {
            self.i += 1;
        }
        self.emit(TokenKind::Text, start);
    }

    fn scan_whitespace(&mut self) {
        let start = self.i;
        while self.i < self.line.len() && is_space_unit(self.line[self.i]) {
            self.i += 1;
        }
        self.emit(TokenKind::Text, start);
    }

    fn scan_one(&mut self) {
        let start = self.i;
        self.i += 1;
        self.emit(TokenKind::Text, start);
    }

    fn step_brace(&mut self, open: bool) {
        if let Some(Frame::Depth(top)) = self.frames.last().copied() {
            let n = self.frames.len() - 1;
            if open {
                self.frames[n] = Frame::Depth(top + 1);
            } else if top == 0 {
                self.frames.pop();
            } else {
                self.frames[n] = Frame::Depth(top - 1);
            }
        }
        self.scan_one();
    }

    fn step_code(&mut self) {
        let ch = self.line[self.i];
        match ch {
            0x22 | 0x27 => self.scan_simple_string(ch),
            0x60 => {
                self.scan_one();
                self.frames.push(Frame::Quasi);
            }
            0x2f => self.scan_slash(),
            _ if is_ident_start(ch) => self.scan_word(),
            _ if is_digit(ch) => self.scan_number(),
            _ if is_space_unit(ch) => self.scan_whitespace(),
            0x7b => self.step_brace(true),
            0x7d => self.step_brace(false),
            _ => self.scan_one(),
        }
    }
}

/// `line.indexOf("*/", from)`.
fn index_of_close_comment(line: &[u16], from: usize) -> Option<usize> {
    (from..line.len().saturating_sub(1))
        .find(|&i| line[i] == u16::from(b'*') && line[i + 1] == u16::from(b'/'))
}

/// `tokenizeLine`: None when the line is not self-contained (an open
/// string, template, comment or regex).
pub fn tokenize_line(line: &[u16]) -> Option<Vec<LineToken>> {
    let mut st = State {
        line,
        i: 0,
        tokens: Vec::new(),
        frames: Vec::new(),
        prev: None,
        failed: false,
    };
    while st.i < line.len() && !st.failed {
        if st.frames.last() == Some(&Frame::Quasi) {
            st.scan_quasi();
        } else {
            st.step_code();
        }
    }
    if st.failed || !st.frames.is_empty() {
        return None;
    }
    Some(st.tokens)
}

/// One differing identifier position of a clean pair (`PairDiff`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PairDiff {
    /// 0-based UTF-16 column in the NEW line.
    pub col: usize,
    /// The new-leg token.
    pub from_name: String,
    /// The prior-leg token.
    pub to_name: String,
}

/// `compareLinePair`: `Some(diffs)` when the pair is rename noise (clean),
/// None when dirty.
pub fn compare_line_pair(prior_line: &[u16], new_line: &[u16]) -> Option<Vec<PairDiff>> {
    let prior_tokens = tokenize_line(prior_line)?;
    let new_tokens = tokenize_line(new_line)?;
    if prior_tokens.len() != new_tokens.len() {
        return None;
    }
    let mut diffs = Vec::new();
    for (prior, next) in prior_tokens.iter().zip(&new_tokens) {
        if prior.kind != next.kind {
            return None;
        }
        let pt = prior.text(prior_line);
        let nt = next.text(new_line);
        if pt == nt {
            continue;
        }
        if prior.kind == TokenKind::Text {
            if is_ws_only(pt) && is_ws_only(nt) {
                continue;
            }
            return None;
        }
        diffs.push(PairDiff {
            col: next.col(),
            from_name: String::from_utf16_lossy(nt),
            to_name: String::from_utf16_lossy(pt),
        });
    }
    Some(diffs)
}

/// `lineSkeleton`: the identifier-blanked token skeleton (property-position
/// identifiers keep their text), as UTF-16 units — the map key.
pub fn line_skeleton(line: &[u16]) -> Option<Vec<u16>> {
    let tokens = tokenize_line(line)?;
    let mut parts: Vec<u16> = Vec::new();
    for (k, tok) in tokens.iter().enumerate() {
        let text = tok.text(line);
        if tok.kind != TokenKind::Ident {
            if is_ws_only(text) {
                parts.push(u16::from(b' '));
            } else {
                parts.extend_from_slice(text);
            }
            continue;
        }
        let prev = previous_meaningful(&tokens, k, line);
        let next = next_meaningful(&tokens, k, line);
        let is_property = match prev {
            Some(p) if p.last() == Some(&u16::from(b'.')) => true,
            _ => next.is_some_and(|n| n.first() == Some(&u16::from(b':'))),
        };
        if is_property {
            parts.extend_from_slice(text);
        } else {
            parts.push(u16::from(b' '));
        }
    }
    Some(parts)
}

fn previous_meaningful<'l>(tokens: &[LineToken], k: usize, line: &'l [u16]) -> Option<&'l [u16]> {
    tokens[..k]
        .iter()
        .rev()
        .map(|t| t.text(line))
        .find(|t| !is_ws_only(t))
}

fn next_meaningful<'l>(tokens: &[LineToken], k: usize, line: &'l [u16]) -> Option<&'l [u16]> {
    tokens[k + 1..]
        .iter()
        .map(|t| t.text(line))
        .find(|t| !is_ws_only(t))
}

/// UTF-16 units of a line.
pub fn units(line: &str) -> Vec<u16> {
    line.encode_utf16().collect()
}

#[cfg(test)]
mod lexer_test;
