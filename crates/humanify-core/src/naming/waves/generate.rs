//! `@babel/generator` output for a node of the BEAUTIFIED text — the
//! text every naming prompt shows (the function code, callee bodies and
//! params, parent declarations, module-level declarations, assignment and
//! usage snippets, the prior version's code) and the graph's compact
//! call-site code.
//!
//! The beautified text IS babel's own pretty output (the unminify stage
//! prints it with `@babel/generator`, compact off), so re-printing one of
//! its nodes is re-running the same printer on a subtree. What differs from
//! the node's source slice is exactly what the printer decides afresh:
//!
//! - **indentation** — the subtree starts at indent 0: every line after
//!   the first loses the indentation of the line the node starts on,
//!   EXCEPT a line that begins inside raw text (a template quasi, a string
//!   continuation) — babel prints raw text verbatim;
//! - **names** — the rename overlay's current names (a [`Replacement`] per
//!   renamed occurrence), and the two name-dependent ObjectProperty forms
//!   (`{ key: key = d }` prints as `{ key = d }`; a shorthand whose value
//!   no longer equals its key prints as `key: value`) — the caller builds
//!   those replacements ([`super::render`]);
//! - **comments** — `comments: false` drops them (callee bodies/params);
//! - **compact** — `compact: true` drops all layout whitespace and keeps
//!   only the spaces babel's `word()` / `token()` force (word after word,
//!   `+ +`, `- -`, `! --`, `.` after an integer, `/ /`).
//!
//! Every rule above is pinned against babel itself by
//! test/parity/wp43-gen-probe.ts over the four oracle texts (every
//! function's `generate()`, every callee body, every call site) — the
//! `humanify waves-gen` gate.

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::Span;

use crate::babel_view::BabelLines;

/// A raw-text region babel prints verbatim.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum RawKind {
    /// A template element's raw text.
    Quasi,
    /// A string literal, quotes included.
    Str,
    /// A regular expression literal (printed through `word()`).
    Regex,
}

/// One text edit applied while printing: the source bytes in `span` are
/// replaced by `text` (a renamed identifier, a re-formed property).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replacement {
    pub span: Span,
    pub text: String,
}

/// The printer's view of one beautified text: its raw regions, comments,
/// number literals and line starts.
pub struct TextView<'a> {
    pub text: &'a str,
    /// Sorted by start, non-overlapping.
    raws: Vec<(u32, u32, RawKind)>,
    /// (start, end, is_line) sorted by start.
    comments: Vec<(u32, u32, bool)>,
    /// start → (end, babel's "integer" marker: `number()` sets lastChar -2).
    numbers: HashMap<u32, (u32, bool)>,
    /// BigInt literal starts (printed through `word()`).
    bigints: HashMap<u32, u32>,
    /// Parenthesized OBJECT literals: start → end of the paren pair.
    /// Babel prints those parens only because the object is first in an
    /// arrow body / expression statement — a standalone print drops them.
    paren_objects: HashMap<u32, u32>,
    line_starts: Vec<u32>,
    /// Babel's `loc` lines (every JS line terminator).
    babel_lines: BabelLines<'a>,
}

impl<'a> TextView<'a> {
    /// Build the view over a parsed text.
    pub fn build(semantic: &Semantic<'a>) -> TextView<'a> {
        let text = semantic.source_text();
        let mut raws = Vec::new();
        let mut numbers = HashMap::new();
        let mut bigints = HashMap::new();
        let mut paren_objects = HashMap::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ParenthesizedExpression(p) => {
                    if matches!(
                        crate::babel_view::unparen(&p.expression),
                        oxc_ast::ast::Expression::ObjectExpression(_)
                    ) {
                        paren_objects.insert(p.span.start, p.span.end);
                    }
                }
                AstKind::TemplateElement(e) => {
                    if e.span.end > e.span.start {
                        raws.push((e.span.start, e.span.end, RawKind::Quasi));
                    }
                }
                AstKind::StringLiteral(s) => raws.push((s.span.start, s.span.end, RawKind::Str)),
                AstKind::RegExpLiteral(r) => raws.push((r.span.start, r.span.end, RawKind::Regex)),
                AstKind::NumericLiteral(n) => {
                    let raw = &text[n.span.start as usize..n.span.end as usize];
                    numbers.insert(n.span.start, (n.span.end, integer_marker(raw, n.value)));
                }
                AstKind::BigIntLiteral(b) => {
                    bigints.insert(b.span.start, b.span.end);
                }
                _ => {}
            }
        }
        raws.sort_unstable();
        raws.dedup();
        let comments = semantic
            .nodes()
            .program()
            .comments
            .iter()
            .map(|c| (c.span.start, c.span.end, c.is_line()))
            .collect();
        let mut line_starts = vec![0u32];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i as u32 + 1);
            }
        }
        TextView {
            text,
            raws,
            comments,
            numbers,
            bigints,
            paren_objects,
            line_starts,
            babel_lines: BabelLines::new(text),
        }
    }

    /// The edits that drop the context parens of an object literal that
    /// STARTS `span` (an arrow's expression body printed alone).
    pub fn leading_object_paren_drop(&self, span: Span) -> Vec<Replacement> {
        match self.paren_objects.get(&span.start) {
            Some(&end) if end <= span.end => vec![
                Replacement {
                    span: Span::new(span.start, span.start + 1),
                    text: String::new(),
                },
                Replacement {
                    span: Span::new(end - 1, end),
                    text: String::new(),
                },
            ],
            _ => Vec::new(),
        }
    }

    /// The raw region strictly containing byte `pos` (a newline there is
    /// raw text, not layout).
    fn raw_containing(&self, pos: u32) -> Option<(u32, u32, RawKind)> {
        let i = self.raws.partition_point(|r| r.0 <= pos);
        // Candidates: the last region starting at or before pos.
        i.checked_sub(1)
            .map(|j| self.raws[j])
            .filter(|&(s, e, _)| s <= pos && pos < e)
    }

    /// The start of the line holding byte `pos`.
    pub fn line_start_of(&self, pos: u32) -> u32 {
        let i = self.line_starts.partition_point(|s| *s <= pos);
        self.line_starts[i.saturating_sub(1)]
    }

    /// Babel's 1-based `loc.line` of byte `pos` ([`BabelLines`], the
    /// owner — the layout's `line_start_of` counts `\n` only).
    pub fn line_of(&self, pos: u32) -> u32 {
        self.babel_lines.line(pos) as u32
    }

    /// Babel `loc` (1-based line, 0-based UTF-16 column) of byte `pos`.
    pub fn loc_of(&self, pos: u32) -> (u32, u32) {
        let (line, col) = self.babel_lines.loc(pos);
        (line as u32, col as u32)
    }

    /// The printer's indent at a node that starts at `pos`: the leading
    /// spaces of its line.
    fn base_indent(&self, pos: u32) -> usize {
        // A line that begins INSIDE raw text (a template quasi's
        // continuation) carries no layout indentation: the printer's level
        // is that of the nearest line above that begins in layout.
        let mut ls = self.line_start_of(pos);
        while ls > 0 && self.raw_containing(ls - 1).is_some() {
            ls = self.line_start_of(ls - 1);
        }
        let end = pos.max(ls);
        self.text.as_bytes()[ls as usize..end as usize]
            .iter()
            .take_while(|b| **b == b' ')
            .count()
    }

    /// `generate(node).code` (pretty): the node's source slice with the
    /// edits applied, re-indented to column 0; `comments: false` drops
    /// every comment inside the node.
    pub fn pretty(&self, span: Span, edits: &[Replacement], comments: bool) -> String {
        self.pretty_lines(span, edits, comments, usize::MAX).0
    }

    /// [`TextView::pretty`] stopped at the `max_newlines + 1`-th newline
    /// (callers that keep only a statement's first lines); the flag says
    /// whether the text went on.
    pub fn pretty_lines(
        &self,
        span: Span,
        edits: &[Replacement],
        comments: bool,
        max_newlines: usize,
    ) -> (String, bool) {
        let mut all: Vec<Replacement> = edits
            .iter()
            .filter(|r| r.span.start >= span.start && r.span.end <= span.end)
            .cloned()
            .collect();
        if !comments {
            all.extend(self.comment_removals(span));
        }
        all.sort_by_key(|r| (r.span.start, r.span.end));
        let base = self.base_indent(span.start);
        let bytes = self.text.as_bytes();
        let mut out = String::with_capacity(((span.end - span.start) as usize).min(1 << 20));
        let mut pos = span.start;
        let mut edits = all.iter().peekable();
        let mut dedent_left = 0usize;
        let mut newlines = 0usize;
        while pos < span.end {
            if let Some(r) = edits.peek()
                && r.span.start <= pos
            {
                out.push_str(&r.text);
                pos = pos.max(r.span.end);
                edits.next();
                dedent_left = 0;
                continue;
            }
            let b = bytes[pos as usize];
            if dedent_left > 0 {
                if b == b' ' {
                    dedent_left -= 1;
                    pos += 1;
                    continue;
                }
                dedent_left = 0;
            }
            if b == b'\n' {
                if newlines == max_newlines {
                    return (out, true);
                }
                newlines += 1;
                out.push('\n');
                // A newline inside raw text (a template quasi) is content:
                // the next line prints verbatim.
                if self.raw_containing(pos).is_none() {
                    dedent_left = base;
                }
                pos += 1;
                continue;
            }
            let len = utf8_len(b);
            out.push_str(&self.text[pos as usize..pos as usize + len]);
            pos += len as u32;
        }
        (out, false)
    }

    /// The comment removals `comments: false` makes inside `span`: a
    /// comment alone on its line takes the whole line; a trailing comment
    /// takes the whitespace before it; an inline block comment takes the
    /// whitespace after it.
    fn comment_removals(&self, span: Span) -> Vec<Replacement> {
        let bytes = self.text.as_bytes();
        let mut out = Vec::new();
        let first = self.comments.partition_point(|c| c.0 < span.start);
        for &(start, end, _) in self.comments[first..].iter() {
            if start >= span.end {
                break;
            }
            if end > span.end {
                continue;
            }
            let ls = self.line_start_of(start);
            let before_ws = bytes[ls as usize..start as usize]
                .iter()
                .all(|b| *b == b' ' || *b == b'\t');
            let mut le = end;
            while (le as usize) < bytes.len() && bytes[le as usize] != b'\n' {
                le += 1;
            }
            let after_ws = bytes[end as usize..le as usize]
                .iter()
                .all(|b| *b == b' ' || *b == b'\t');
            let (s, e) = if before_ws && after_ws && ls > span.start {
                // The whole line, including its newline (from the previous
                // line's newline so the dedent bookkeeping stays aligned).
                (ls - 1, le)
            } else if after_ws {
                let mut s = start;
                while s > span.start && matches!(bytes[s as usize - 1], b' ' | b'\t') {
                    s -= 1;
                }
                (s, end)
            } else {
                let mut e = end;
                while e < span.end && matches!(bytes[e as usize], b' ' | b'\t') {
                    e += 1;
                }
                (start, e)
            };
            out.push(Replacement {
                span: Span::new(s.max(span.start), e.min(span.end)),
                text: String::new(),
            });
        }
        out
    }

    /// `generate(node, { compact: true }).code`: the node's tokens with
    /// only babel's forced spaces between them.
    pub fn compact(&self, span: Span) -> String {
        let toks = self.tokens(span);
        let mut out = String::with_capacity((span.end - span.start) as usize);
        let mut prev: Option<&Tok<'_>> = None;
        for t in &toks {
            if let Some(p) = prev
                && needs_space(p, t)
            {
                out.push(' ');
            }
            out.push_str(t.text);
            prev = Some(t);
        }
        out
    }

    fn tokens(&self, span: Span) -> Vec<Tok<'a>> {
        let bytes = self.text.as_bytes();
        let mut toks = Vec::new();
        let mut p = span.start;
        let raw_first = self.raws.partition_point(|r| r.0 < span.start);
        let mut raw_idx = raw_first;
        let com_first = self.comments.partition_point(|c| c.0 < span.start);
        let mut com_idx = com_first;
        while p < span.end {
            while raw_idx < self.raws.len() && self.raws[raw_idx].0 < p {
                raw_idx += 1;
            }
            while com_idx < self.comments.len() && self.comments[com_idx].0 < p {
                com_idx += 1;
            }
            // Raw text first: a template quasi may START with whitespace.
            if raw_idx < self.raws.len() && self.raws[raw_idx].0 == p {
                let (s, e, kind) = self.raws[raw_idx];
                toks.push(Tok {
                    text: &self.text[s as usize..e as usize],
                    word: kind == RawKind::Regex,
                    int: false,
                });
                p = e;
                continue;
            }
            let b = bytes[p as usize];
            if matches!(b, b' ' | b'\n' | b'\t' | b'\r') {
                p += 1;
                continue;
            }
            if com_idx < self.comments.len() && self.comments[com_idx].0 == p {
                p = self.comments[com_idx].1;
                continue;
            }
            if let Some(&(e, int)) = self.numbers.get(&p) {
                toks.push(Tok {
                    text: &self.text[p as usize..e as usize],
                    word: true,
                    int,
                });
                p = e;
                continue;
            }
            if let Some(&e) = self.bigints.get(&p) {
                toks.push(Tok {
                    text: &self.text[p as usize..e as usize],
                    word: true,
                    int: false,
                });
                p = e;
                continue;
            }
            if is_ident_start(b) {
                let s = p;
                while p < span.end && is_ident_part(bytes[p as usize]) {
                    p += utf8_len(bytes[p as usize]) as u32;
                }
                toks.push(Tok {
                    text: &self.text[s as usize..p as usize],
                    word: true,
                    int: false,
                });
                continue;
            }
            let len = punct_len(&bytes[p as usize..span.end as usize]);
            toks.push(Tok {
                text: &self.text[p as usize..(p as usize + len)],
                word: false,
                int: false,
            });
            p += len as u32;
        }
        toks
    }
}

/// One compact-mode token.
struct Tok<'a> {
    text: &'a str,
    /// Printed through `word()` (identifier, keyword, number, regex,
    /// bigint): babel's lastChar -3 / -2 marker.
    word: bool,
    /// An integer number literal (lastChar -2: a following `.` spaces).
    int: bool,
}

/// Babel's compact spacing (printer.js `word` / `token` / `tokenChar`).
fn needs_space(prev: &Tok<'_>, next: &Tok<'_>) -> bool {
    let next_first = next.text.as_bytes()[0];
    if next.word {
        if prev.word {
            return true;
        }
        // `/` then a regex word.
        return prev.text.ends_with('/') && next_first == b'/';
    }
    if prev.int && next_first == b'.' {
        return true;
    }
    if prev.word {
        return false;
    }
    let last = *prev.text.as_bytes().last().unwrap_or(&0);
    (next_first == b'+' && last == b'+')
        || (next_first == b'-' && last == b'-')
        || ((next.text == "--" || next_first == b'=') && last == b'!')
}

/// Babel `number()`'s -2 marker: an integer literal written in decimal,
/// without an exponent or a trailing `.` / `.0`.
fn integer_marker(raw: &str, value: f64) -> bool {
    let non_decimal = raw.len() > 2
        && raw.as_bytes()[0] == b'0'
        && matches!(raw.as_bytes()[1], b'b' | b'o' | b'x' | b'B' | b'O' | b'X');
    let scientific = raw.contains(['e', 'E']);
    let zero_decimal = {
        // /\.0*$/ — a trailing decimal point with only zeros after it.
        match raw.rfind('.') {
            Some(i) => raw[i + 1..].bytes().all(|b| b == b'0'),
            None => false,
        }
    };
    value.fract() == 0.0
        && value.is_finite()
        && !non_decimal
        && !scientific
        && !zero_decimal
        && !raw.ends_with('.')
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b == b'\\' || b >= 0x80
}

fn is_ident_part(b: u8) -> bool {
    is_ident_start(b) || b.is_ascii_digit()
}

/// The longest punctuator at the start of `rest`.
fn punct_len(rest: &[u8]) -> usize {
    const PUNCTS: [&str; 52] = [
        ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "?.", "=>",
        "==", "!=", "<=", ">=", "&&", "||", "??", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=",
        "|=", "^=", "**", "<<", ">>", "${", "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+",
        "-", "*", "/", "%", "&", "|", "^",
    ];
    for p in PUNCTS {
        if rest.starts_with(p.as_bytes()) {
            // `?.` followed by a digit is `?` + a number (spec lookahead).
            if p == "?." && rest.get(2).is_some_and(u8::is_ascii_digit) {
                return 1;
            }
            return p.len();
        }
    }
    utf8_len(rest[0])
}

#[cfg(test)]
mod generate_test;
