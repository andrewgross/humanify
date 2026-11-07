//! The ground-truth predicate's canonical form (exp092; ported from
//! experiments/lib/match-truth/canonical.ts).
//!
//! A function whose canonical form is byte-identical across the two
//! versions MUST be matched by the matcher — that is the harness's
//! ground-truth rule. Canonicalization is deliberately SIMPLE and
//! conservative in one direction: it may MISS ground truth (a genuinely
//! unchanged function whose spelling changed in a way below does not enter
//! the must set), but it never MANUFACTURES it.
//!
//! What it erases:
//! - the wrapper spelling: `function (a, b) { … }` ≡ `(a, b) => { … }` (the
//!   leading function head, with its optional `var name =` prefix; `async`
//!   is preserved) — REFUSED when the flip would rebind something (exp094),
//!   by the pipeline's own rule ([`humanify_ag_core::hash::wrapper_spelling`]);
//! - identifier NAMES: every non-keyword identifier is rewritten to its
//!   first-occurrence index (`_0`, `_1`, …). String literal contents are
//!   opaque. A TEMPLATE literal is split the way the pipeline reads it: each
//!   quasi is opaque like a string, each `${…}` interpolation is CODE;
//! - nothing else: spaces/tabs are one space EACH (canonical.ts's doc said
//!   "runs collapse"; its code never did, and the port keeps the code), and
//!   a run of newlines is one newline.
//!
//! What it keeps: punctuation, operators, numbers, keywords, string and
//! quasi contents, REGEX literals (scanned as one token), and line
//! structure (the near/far classifier counts differing lines).
//!
//! FIDELITY. The scanner runs over UTF-16 code units, exactly as the
//! JavaScript did, so every index, every `j += 2` past an escape and every
//! blurred length (`#s<n>`, a UTF-16 count) is the TS's. The one deliberate
//! difference is in the head-erasure REFUSAL, which parses with oxc instead
//! of Babel and asks the pipeline's rule instead of the TS twin:
//! - `new.target` refuses only in the function's OWN scope (the TS refused
//!   on one anywhere in the subtree; a nested classic function binds its
//!   own, so the TS only missed ground truth there);
//! - the parse is oxc's unambiguous parse (script, then module) of a `.js`
//!   source — Babel parsed `sourceType: "unambiguous"` with no plugins.
//!   Both accept every slice the pipeline cut from code it parsed; they can
//!   disagree only on text neither pipeline could have produced.

use humanify_ag_core::hash::wrapper_spelling::function_expression_flip_is_semantics_preserving;
use humanify_ag_core::ingest::{Ingest, program_estree_json};
use oxc_allocator::Allocator;
use serde_json::Value;
use std::collections::HashMap;

const KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "async",
    "await",
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
    "from",
    "function",
    "get",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "interface",
    "let",
    "new",
    "null",
    "of",
    "return",
    "set",
    "static",
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
];

/// The keywords a regex literal may directly follow (`return /x/` is a
/// regex; `total / 2` is division).
const REGEX_AFTER_KEYWORDS: &[&str] = &[
    "await",
    "case",
    "delete",
    "do",
    "else",
    "in",
    "instanceof",
    "new",
    "of",
    "return",
    "throw",
    "typeof",
    "void",
    "yield",
];

type Units = [u16];

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn unit(c: char) -> u16 {
    c as u16
}

fn is_ascii_unit(u: u16, f: fn(u8) -> bool) -> bool {
    u < 0x80 && f(u as u8)
}

fn ident_start(u: u16) -> bool {
    is_ascii_unit(u, |b| b.is_ascii_alphabetic() || b == b'_' || b == b'$')
}

fn ident_part(u: u16) -> bool {
    is_ascii_unit(u, |b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
}

/// JS RegExp `\s` (WhiteSpace + LineTerminator; every member is in the BMP).
fn js_space(u: u16) -> bool {
    char::from_u32(u32::from(u)).is_some_and(humanify_ag_model::js::is_js_whitespace)
}

/// `code.slice(a, b)` with JavaScript's clamping.
fn slice(code: &Units, a: usize, b: usize) -> &Units {
    let b = b.min(code.len());
    let a = a.min(b);
    &code[a..b]
}

// --- the function head ------------------------------------------------------

/// Where `FUNCTION_HEAD` matched: group 1 (`\s*` + optional `var x =`),
/// group 2 (`async\s+`, possibly empty), group 3 (the parameter list), and
/// the index just past the `{`.
struct Head {
    g1: (usize, usize),
    g2: (usize, usize),
    g3: (usize, usize),
    end: usize,
}

fn skip_spaces(code: &Units, mut i: usize) -> usize {
    while i < code.len() && js_space(code[i]) {
        i += 1;
    }
    i
}

fn starts_with(code: &Units, i: usize, word: &str) -> bool {
    let w = units(word);
    code.len() >= i + w.len() && code[i..i + w.len()] == w[..]
}

/// `(?:var|let|const)\s+[A-Za-z_$][A-Za-z0-9_$]*\s*=\s*` at `i`.
fn declaration_prefix(code: &Units, i: usize) -> Option<usize> {
    let kw = ["var", "let", "const"]
        .into_iter()
        .find(|k| starts_with(code, i, k))?;
    let mut j = i + kw.len();
    let after_kw = skip_spaces(code, j);
    if after_kw == j {
        return None;
    }
    j = after_kw;
    if j >= code.len() || !ident_start(code[j]) {
        return None;
    }
    while j < code.len() && ident_part(code[j]) {
        j += 1;
    }
    j = skip_spaces(code, j);
    if j >= code.len() || code[j] != unit('=') {
        return None;
    }
    Some(skip_spaces(code, j + 1))
}

/// A half-open range of code units.
type Range = (usize, usize);

/// `(async\s+)?function\s*(\([^)]*\))\s*\{` at `i`: group 2, group 3 and
/// the index past the `{`.
fn function_head_at(code: &Units, i: usize) -> Option<(Range, Range, usize)> {
    let mut j = i;
    if starts_with(code, j, "async") {
        let k = skip_spaces(code, j + 5);
        if k > j + 5 {
            j = k;
        }
    }
    let g2 = (i, j);
    if !starts_with(code, j, "function") {
        return None;
    }
    j = skip_spaces(code, j + 8);
    if j >= code.len() || code[j] != unit('(') {
        return None;
    }
    let open = j;
    while j < code.len() && code[j] != unit(')') {
        j += 1;
    }
    if j >= code.len() {
        return None;
    }
    let g3 = (open, j + 1);
    j = skip_spaces(code, j + 1);
    if j >= code.len() || code[j] != unit('{') {
        return None;
    }
    Some((g2, g3, j + 1))
}

/// canonical.ts's `FUNCTION_HEAD`, anchored at the start:
/// `^(\s*(?:(?:var|let|const)\s+IDENT\s*=\s*)?)(async\s+)?function\s*(\([^)]*\))\s*\{`.
fn function_head(code: &Units) -> Option<Head> {
    let lead = skip_spaces(code, 0);
    // The optional declaration first (the regex's greedy `?`), then none.
    let starts = declaration_prefix(code, lead)
        .into_iter()
        .chain(std::iter::once(lead));
    for g1_end in starts {
        if let Some((g2, g3, end)) = function_head_at(code, g1_end) {
            return Some(Head {
                g1: (0, g1_end),
                g2,
                g3,
                end,
            });
        }
    }
    None
}

/// Erase the leading function head's spelling (`$1$2$3 => {`) unless the
/// erasure is refused.
fn normalize_wrapper(slice_text: &str) -> Vec<u16> {
    let code = units(slice_text);
    let Some(head) = function_head(&code) else {
        return code;
    };
    if head_erasure_refused(slice_text) {
        return code;
    }
    let mut out = Vec::with_capacity(code.len());
    out.extend_from_slice(&code[head.g1.0..head.g1.1]);
    out.extend_from_slice(&code[head.g2.0..head.g2.1]);
    out.extend_from_slice(&code[head.g3.0..head.g3.1]);
    out.extend(units(" => {"));
    out.extend_from_slice(&code[head.end..]);
    out
}

// --- the head-erasure soundness check (refusal side) -------------------------

/// The slice without one trailing `;` (and the whitespace after it):
/// canonical.ts's `slice.replace(/;\s*$/, "")`.
fn strip_trailing_semicolon(s: &str) -> &str {
    let trimmed = s.trim_end_matches(humanify_ag_model::js::is_js_whitespace);
    trimmed.strip_suffix(';').unwrap_or(s)
}

/// The slice's ESTree program when one of the three spellings parses
/// cleanly to exactly one statement (canonical.ts's `parseSlice`).
fn parse_slice(slice_text: &str) -> Option<Value> {
    let candidates = [
        slice_text.to_string(),
        format!("({slice_text})"),
        format!("({})", strip_trailing_semicolon(slice_text)),
    ];
    candidates.iter().find_map(|candidate| {
        // A fresh arena per parse (each slice is small).
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, candidate);
        if !ingest.errors.is_empty() {
            return None;
        }
        let program = program_estree_json(ingest.program);
        let statements = program.get("body")?.as_array()?.len();
        (statements == 1).then_some(program)
    })
}

fn node_start(v: &Value) -> u64 {
    v.get("start").and_then(Value::as_u64).unwrap_or(0)
}

fn is_node(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str).is_some()
}

fn is_function(v: &Value) -> bool {
    matches!(
        v.get("type").and_then(Value::as_str),
        Some("FunctionExpression" | "FunctionDeclaration" | "ArrowFunctionExpression")
    )
}

/// The first function node in Babel's visitor order: canonical.ts's
/// `headFunction` pops an explicit stack and pushes each node's children
/// in VISITOR_KEYS order, which is source order; here the children are
/// pushed sorted by their start offset, the same order.
fn head_function(program: &Value) -> Option<&Value> {
    let mut stack: Vec<&Value> = vec![program];
    while let Some(node) = stack.pop() {
        if is_function(node) {
            return Some(node);
        }
        let Some(map) = node.as_object() else {
            continue;
        };
        let mut children: Vec<&Value> = Vec::new();
        for value in map.values() {
            match value {
                Value::Array(items) => children.extend(items.iter().filter(|c| is_node(c))),
                v if is_node(v) => children.push(v),
                _ => {}
            }
        }
        children.sort_by_key(|c| node_start(c));
        stack.extend(children);
    }
    None
}

/// True => keep the `function` head UN-erased (the pair then compares
/// unequal, which may only miss ground truth, never invent it).
/// Unparseable slices are refused for the same reason.
fn head_erasure_refused(slice_text: &str) -> bool {
    let Some(program) = parse_slice(slice_text) else {
        return true;
    };
    match head_function(&program).and_then(Value::as_object) {
        Some(map) => !function_expression_flip_is_semantics_preserving(map),
        None => true,
    }
}

// --- the scanner ---------------------------------------------------------------

/// The index just past the string literal starting at `i` (may exceed the
/// length after a trailing escape, as in the JS).
fn scan_literal_end(code: &Units, i: usize) -> usize {
    let quote = code[i];
    let mut j = i + 1;
    while j < code.len() {
        let d = code[j];
        if d == unit('\\') {
            j += 2;
            continue;
        }
        if d == quote {
            return j + 1;
        }
        // A non-template literal cannot span lines in parsed code.
        if d == unit('\n') {
            return j;
        }
        j += 1;
    }
    j
}

/// The index of the regex body's closing `/`, or None when a newline or the
/// end comes first.
fn regex_body_end(code: &Units, mut j: usize) -> Option<usize> {
    let mut in_class = false;
    while j < code.len() {
        let r = code[j];
        if r == unit('\\') {
            j += 2;
            continue;
        }
        if r == unit('\n') {
            return None;
        }
        if in_class {
            in_class = r != unit(']');
        } else if r == unit('[') {
            in_class = true;
        } else if r == unit('/') {
            return Some(j);
        }
        j += 1;
    }
    None
}

/// Past the regex literal at `i` and its flags, or None (the `/` is then
/// division).
fn scan_regex_end(code: &Units, i: usize) -> Option<usize> {
    let close = regex_body_end(code, i + 1)?;
    let mut k = close + 1;
    while k < code.len() && is_ascii_unit(code[k], |b| b.is_ascii_alphabetic()) {
        k += 1;
    }
    Some(k)
}

/// `/[0-9][\w.]*/` from `i` (a digit is there).
fn scan_number_end(code: &Units, i: usize) -> usize {
    let mut j = i + 1;
    while j < code.len()
        && is_ascii_unit(code[j], |b| {
            b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
        })
    {
        j += 1;
    }
    j
}

/// One entered template literal (`tpl`, accumulating quasi text) or one of
/// its `${…}` interpolations (counting braces).
struct Frame {
    tpl: bool,
    quasi: Vec<u16>,
    depth: usize,
}

impl Frame {
    fn new(tpl: bool) -> Frame {
        Frame {
            tpl,
            quasi: Vec::new(),
            depth: 0,
        }
    }
}

struct Canonicalizer {
    out: Vec<u16>,
    names: HashMap<Vec<u16>, usize>,
    last_significant: Option<u16>,
    last_word: Vec<u16>,
    frames: Vec<Frame>,
    loose: bool,
}

impl Canonicalizer {
    fn new(loose: bool) -> Canonicalizer {
        Canonicalizer {
            out: Vec::new(),
            names: HashMap::new(),
            last_significant: None,
            last_word: Vec::new(),
            frames: vec![Frame::new(false)],
            loose,
        }
    }

    fn run(mut self, code: &Units) -> String {
        let mut i = 0;
        while i < code.len() {
            i = self.step(code, i);
        }
        String::from_utf16_lossy(&self.out)
    }

    fn push_str(&mut self, s: &str) {
        self.out.extend(s.encode_utf16());
    }

    fn significant(&mut self, last: Option<u16>) {
        self.last_significant = last;
        self.last_word.clear();
    }

    fn step(&mut self, code: &Units, i: usize) -> usize {
        if self.frames.last().is_some_and(|f| f.tpl) {
            return self.template_char(code, i);
        }
        let c = code[i];
        if c == unit('"') || c == unit('\'') {
            return self.string_literal(code, i);
        }
        if c == unit('`') {
            self.frames.push(Frame::new(true));
            return i + 1;
        }
        if c == unit('/') {
            return self.slash(code, i);
        }
        if c == unit('{') || c == unit('}') {
            return self.brace(i, c);
        }
        if c == unit(' ') || c == unit('\t') {
            self.out.push(unit(' '));
            return i + 1;
        }
        if c == unit('\n') || c == unit('\r') {
            return self.newline_run(code, i);
        }
        self.word_or_number(code, i)
            .unwrap_or_else(|| self.punct(i, c))
    }

    fn word_or_number(&mut self, code: &Units, i: usize) -> Option<usize> {
        if ident_start(code[i]) {
            let mut j = i + 1;
            while j < code.len() && ident_part(code[j]) {
                j += 1;
            }
            let word = code[i..j].to_vec();
            let rewritten = self.rewritten_identifier(&word);
            self.out.extend(rewritten);
            self.last_significant = Some(code[j - 1]);
            self.last_word = word;
            return Some(j);
        }
        if is_ascii_unit(code[i], |b| b.is_ascii_digit()) {
            let end = scan_number_end(code, i);
            if self.loose {
                self.out.push(unit('#'));
            } else {
                self.out.extend_from_slice(&code[i..end]);
            }
            self.significant(Some(code[end - 1]));
            return Some(end);
        }
        None
    }

    fn punct(&mut self, i: usize, c: u16) -> usize {
        self.out.push(c);
        self.significant(Some(c));
        i + 1
    }

    fn template_char(&mut self, code: &Units, i: usize) -> usize {
        let c = code[i];
        if c == unit('\\') {
            let escape = slice(code, i, i + 2).to_vec();
            if let Some(f) = self.frames.last_mut() {
                f.quasi.extend(escape);
            }
            return i + 2;
        }
        if c == unit('`') {
            self.emit_quasi();
            self.frames.pop();
            self.significant(Some(unit('`')));
            return i + 1;
        }
        if c == unit('$') && code.get(i + 1) == Some(&unit('{')) {
            self.emit_quasi();
            self.frames.push(Frame::new(false));
            self.push_str("${");
            self.significant(Some(unit('{')));
            return i + 2;
        }
        if let Some(f) = self.frames.last_mut() {
            f.quasi.push(c);
        }
        i + 1
    }

    fn emit_quasi(&mut self) {
        let quasi = self
            .frames
            .last_mut()
            .map(|f| std::mem::take(&mut f.quasi))
            .unwrap_or_default();
        if self.loose {
            self.push_str(&format!("#s{}", quasi.len()));
        } else {
            self.push_str("«");
            self.out.extend(quasi);
            self.push_str("»");
        }
        self.significant(Some(unit('»')));
    }

    fn brace(&mut self, i: usize, c: u16) -> usize {
        let depth_of_top = self.frames.last().map_or(0, |f| f.depth);
        if c == unit('{') {
            if let Some(f) = self.frames.last_mut() {
                f.depth += 1;
            }
            return self.punct(i, c);
        }
        if depth_of_top == 0 && self.frames.len() > 1 {
            self.frames.pop();
        } else if depth_of_top > 0
            && let Some(f) = self.frames.last_mut()
        {
            f.depth -= 1;
        }
        self.punct(i, unit('}'))
    }

    fn after_value(&self) -> bool {
        let value_char = self.last_significant.is_some_and(|u| {
            is_ascii_unit(u, |b| {
                b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$' | b')' | b'"' | b'\'' | b'`')
            })
        });
        let word = String::from_utf16_lossy(&self.last_word);
        value_char && !REGEX_AFTER_KEYWORDS.contains(&word.as_str())
    }

    fn slash(&mut self, code: &Units, i: usize) -> usize {
        let end = if self.after_value() {
            None
        } else {
            scan_regex_end(code, i)
        };
        let Some(end) = end else {
            return self.punct(i, unit('/'));
        };
        let raw = &code[i..end];
        self.out.extend_from_slice(raw);
        self.significant(raw.last().copied());
        end
    }

    fn string_literal(&mut self, code: &Units, i: usize) -> usize {
        let end = scan_literal_end(code, i);
        if self.loose {
            // `end - i - 2` on the unclamped end, as the JS computed it.
            let len = end as i64 - i as i64 - 2;
            self.push_str(&format!("#s{len}"));
        } else {
            self.out.extend_from_slice(slice(code, i, end));
        }
        self.significant(Some(unit('"')));
        end
    }

    fn newline_run(&mut self, code: &Units, i: usize) -> usize {
        self.out.push(unit('\n'));
        let mut j = i + 1;
        while j < code.len() && (code[j] == unit('\n') || code[j] == unit('\r')) {
            j += 1;
        }
        j
    }

    fn rewritten_identifier(&mut self, word: &[u16]) -> Vec<u16> {
        let text = String::from_utf16_lossy(word);
        if KEYWORDS.contains(&text.as_str()) {
            return word.to_vec();
        }
        let next = self.names.len();
        let idx = *self.names.entry(word.to_vec()).or_insert(next);
        units(&format!("_{idx}"))
    }
}

/// The slice's canonical form — the ground-truth comparison key.
pub fn canonical(slice_text: &str) -> String {
    Canonicalizer::new(false).run(&normalize_wrapper(slice_text))
}

/// The LOOSE canonical form — the tier-2 "should match" key: the same
/// normalization, plus literals blurred (numbers to `#`, string and quasi
/// contents to their UTF-16 LENGTH). Advisory: two genuinely different
/// functions that differ only in literals also read loose-equal.
pub fn canonical_loose(slice_text: &str) -> String {
    Canonicalizer::new(true).run(&normalize_wrapper(slice_text))
}

/// The symmetric line-multiset distance between two canonical forms: how
/// many LINES (with multiplicity) exist on one side but not the other.
pub fn line_diff_mass(a: &str, b: &str) -> usize {
    let ma = line_counts(a);
    let mb = line_counts(b);
    let one_way = |x: &HashMap<&str, usize>, y: &HashMap<&str, usize>| -> usize {
        x.iter()
            .map(|(k, v)| v.saturating_sub(y.get(k).copied().unwrap_or(0)))
            .sum()
    };
    one_way(&ma, &mb) + one_way(&mb, &ma)
}

fn line_counts(text: &str) -> HashMap<&str, usize> {
    let mut m = HashMap::new();
    for line in text.split('\n') {
        *m.entry(line).or_insert(0) += 1;
    }
    m
}

#[cfg(test)]
#[path = "canonical_test.rs"]
mod canonical_test;
