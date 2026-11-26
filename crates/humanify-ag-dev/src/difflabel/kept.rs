//! Lines of changed input that the other side also has: the same code
//! under other names, at another indentation, or in another place.

use std::collections::BTreeMap;

use super::Label;
use super::label::{Found, match_lines};

/// One changed line of the diff and its label so far.
pub struct ChangedLine<'a> {
    /// The diff hunk the line is in, numbered over the whole diff.
    pub hunk: u32,
    pub to_side: bool,
    pub text: &'a str,
    pub label: Label,
}

const KEYWORDS: [&str; 44] = [
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "get",
    "if",
    "import",
    "in",
    "instanceof",
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
    "undefined",
    "using",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

const NAME_MARK: char = '\u{1}';
/// A line shorter than this, or with fewer names, is too common to pair.
const MIN_LENGTH: usize = 16;
const MIN_NAMES: usize = 3;

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// Where the string that opens at `start` ends (past its closing quote).
fn string_end(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut at = start + 1;
    while at < chars.len() && chars[at] != quote {
        at += if chars[at] == '\\' { 2 } else { 1 };
    }
    (at + 1).min(chars.len())
}

fn word_end(chars: &[char], start: usize) -> usize {
    let rest = chars[start..].iter().position(|c| !word_char(*c));
    rest.map_or(chars.len(), |n| start + n)
}

/// Whether the word at `start..end` is a binding's name: not a keyword,
/// a number, a property after a dot, or an object key before a colon.
fn is_name(chars: &[char], start: usize, end: usize) -> bool {
    let word: String = chars[start..end].iter().collect();
    let before = chars[..start].iter().rev().find(|c| !c.is_whitespace());
    let after = chars[end..].iter().find(|c| !c.is_whitespace());
    !chars[start].is_ascii_digit()
        && !KEYWORDS.contains(&word.as_str())
        && before != Some(&'.')
        && after != Some(&':')
}

/// The line with each binding name replaced by one mark and spaces
/// dropped; strings, keywords, property names and object keys stay.
fn without_names(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        let end = if matches!(c, '"' | '\'' | '`') {
            string_end(&chars, at)
        } else if word_char(c) {
            word_end(&chars, at)
        } else {
            at + 1
        };
        if word_char(c) && is_name(&chars, at, end) {
            out.push(NAME_MARK);
        } else if word_char(c) {
            out.extend(&chars[at..end]);
            out.push(' ');
        } else if !c.is_whitespace() {
            out.extend(&chars[at..end]);
        }
        at = end;
    }
    out
}

/// Whether a line says enough to be told apart from other lines.
fn distinct(text: &str, key: &str) -> bool {
    text.trim().len() >= MIN_LENGTH && key.matches(NAME_MARK).count() >= MIN_NAMES
}

fn open(lines: &[ChangedLine<'_>], hunk: &[usize], to_side: bool) -> Vec<usize> {
    let wanted =
        |i: &&usize| lines[**i].to_side == to_side && lines[**i].label == Label::InputChanged;
    hunk.iter().filter(wanted).copied().collect()
}

/// Pair the open lines of one hunk under `text` and `key`, and label a
/// line found by text `by_text` and one found by key `by_key`.
fn pair_in_hunk(
    lines: &mut [ChangedLine<'_>],
    hunk: &[usize],
    parts: fn(&str) -> (&str, String),
    by_text: fn(&str, &str) -> Option<Label>,
    by_key: fn(&str, &str) -> Option<Label>,
) {
    let sides = [open(lines, hunk, false), open(lines, hunk, true)];
    let keyed = |side: &[usize]| -> Vec<(&str, String)> {
        side.iter().map(|i| parts(lines[*i].text)).collect()
    };
    let (from, to) = (keyed(&sides[0]), keyed(&sides[1]));
    let found = [match_lines(&from, &to), match_lines(&to, &from)];
    let mut labels = Vec::new();
    for (side, (rows, found)) in [&from, &to].into_iter().zip(&found).enumerate() {
        for (n, (row, how)) in rows.iter().zip(found).enumerate() {
            let label = match how {
                Found::SameText => by_text(row.0, &row.1),
                Found::SameKey => by_key(row.0, &row.1),
                Found::Missing => None,
            };
            labels.extend(label.map(|l| (sides[side][n], l)));
        }
    }
    for (index, label) in labels {
        lines[index].label = label;
    }
}

fn moved_if_distinct(text: &str, _key: &str) -> Option<Label> {
    distinct(text, &without_names(text)).then_some(Label::KeptMoved)
}

fn renamed_if_distinct(text: &str, key: &str) -> Option<Label> {
    distinct(text, key).then_some(Label::KeptRenamed)
}

fn trimmed_and_key(text: &str) -> (&str, String) {
    (text.trim(), without_names(text))
}

/// Above this many cell comparisons a hunk is paired without order.
const ALIGN_LIMIT: usize = 16_000_000;

/// The longest run of equal keys the two lists share in order, as index
/// pairs.
fn aligned(from: &[String], to: &[String]) -> Vec<(usize, usize)> {
    let width = to.len() + 1;
    let mut best = vec![0u32; (from.len() + 1) * width];
    for i in (0..from.len()).rev() {
        for j in (0..to.len()).rev() {
            best[i * width + j] = if from[i] == to[j] {
                best[(i + 1) * width + j + 1] + 1
            } else {
                best[(i + 1) * width + j].max(best[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < from.len() && j < to.len() {
        if from[i] == to[j] {
            out.push((i, j));
            (i, j) = (i + 1, j + 1);
        } else if best[(i + 1) * width + j] >= best[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Pair the open lines of one hunk that read the same in order once
/// names are set aside. A hunk too large to align pairs its distinct
/// lines without order.
fn pair_renamed(lines: &mut [ChangedLine<'_>], hunk: &[usize]) {
    let (from, to) = (open(lines, hunk, false), open(lines, hunk, true));
    if from.len().saturating_mul(to.len()) > ALIGN_LIMIT {
        return pair_in_hunk(
            lines,
            hunk,
            trimmed_and_key,
            |_, _| None,
            renamed_if_distinct,
        );
    }
    let keys = |side: &[usize]| -> Vec<String> {
        side.iter().map(|i| without_names(lines[*i].text)).collect()
    };
    let (from_keys, to_keys) = (keys(&from), keys(&to));
    for (i, j) in aligned(&from_keys, &to_keys) {
        if from_keys[i].contains(NAME_MARK) {
            lines[from[i]].label = Label::KeptRenamed;
            lines[to[j]].label = Label::KeptRenamed;
        }
    }
}

/// A line removed in one hunk and added in another, names aside.
fn pair_across_hunks(lines: &mut [ChangedLine<'_>]) {
    let mut by_key: BTreeMap<String, [Vec<usize>; 2]> = BTreeMap::new();
    for (index, line) in lines.iter().enumerate() {
        let key = without_names(line.text);
        if line.label == Label::InputChanged && distinct(line.text, &key) {
            by_key.entry(key).or_default()[usize::from(line.to_side)].push(index);
        }
    }
    for [from, to] in by_key.values() {
        let pairs = from.len().min(to.len());
        for index in from[..pairs].iter().chain(&to[..pairs]) {
            lines[*index].label = Label::KeptMoved;
        }
    }
}

/// Relabel the lines of changed input that the other side also has.
pub fn refine(lines: &mut [ChangedLine<'_>]) {
    let mut hunks: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (index, line) in lines.iter().enumerate() {
        hunks.entry(line.hunk).or_default().push(index);
    }
    for hunk in hunks.values() {
        pair_in_hunk(
            lines,
            hunk,
            |text| (text, text.trim().to_string()),
            moved_if_distinct,
            |_, _| Some(Label::InputReindented),
        );
        pair_renamed(lines, hunk);
    }
    pair_across_hunks(lines);
}

#[cfg(test)]
#[path = "kept_test.rs"]
mod kept_test;
