//! Picking EXAMPLE hunks out of a walk's release diff, for a human to read
//! (the port of `experiments/walk/hunks.ts`). Presentation, not
//! measurement: every NUMBER in the walk report comes from the scorer or
//! the changed-line count. The classes here only decide which hunks are
//! worth showing and what to call them.
//!
//! - `name-only`: removed and added lines are equal once local identifiers
//!   are masked (the name-only predicate's `maskIdentifiers`);
//! - `moved`: removed and added lines are the same multiset (reordering);
//! - `added` / `removed`: only additions / only deletions;
//! - `real`: anything else (may still carry a rename inside an edit).

use humanify_ag_model::js::{cmp_utf16, locale_compare};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub file: String,
    pub header: String,
    /// Body lines with their ' ', '+', '-' prefix.
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    NameOnly,
    Moved,
    Added,
    Removed,
    Real,
}

/// JS keywords, never masked — masking them would equate different
/// control flow (034 name-only-churn.ts `KEYWORDS`).
const KEYWORDS: &[&str] = &[
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
    "typeof",
    "instanceof",
    "in",
    "of",
    "delete",
    "void",
    "null",
    "true",
    "false",
    "this",
    "async",
    "await",
    "yield",
    "throw",
    "try",
    "catch",
    "finally",
    "class",
    "extends",
    "super",
    "import",
    "export",
    "from",
    "default",
    "require",
    "module",
    "exports",
    "static",
    "get",
    "set",
];

fn ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

fn ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// Mask every local identifier as `X`: an identifier is kept when preceded
/// by `.` (a property access) or followed by `:` (an object key) — those
/// are SEMANTIC — or when it is a keyword (`(\.\s*)?ident(\s*:)?`), then
/// trim.
pub fn mask_identifiers(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '.' {
            // `.\s*ident` is kept whole.
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && ident_start(chars[j]) {
                let mut k = j;
                while k < chars.len() && ident_char(chars[k]) {
                    k += 1;
                }
                out.extend(&chars[i..k]);
                i = k;
                continue;
            }
            out.push(c);
            i += 1;
            continue;
        }
        if ident_start(c) {
            let mut k = i;
            while k < chars.len() && ident_char(chars[k]) {
                k += 1;
            }
            let name: String = chars[i..k].iter().collect();
            let mut j = k;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let key = j < chars.len() && chars[j] == ':';
            if key || KEYWORDS.contains(&name.as_str()) {
                out.push_str(&name);
            } else {
                out.push('X');
            }
            i = k;
            continue;
        }
        out.push(c);
        i += 1;
    }
    humanify_ag_model::js::trim(&out).to_string()
}

/// Split `git diff` unified output into hunks, each tagged with its file.
pub fn parse_hunks(diff: &str) -> Vec<Hunk> {
    let mut out: Vec<Hunk> = Vec::new();
    let mut file = String::new();
    let mut open = false;
    for line in diff.split('\n') {
        if line.starts_with("diff --git ") {
            open = false;
            file = line
                .rfind(" b/")
                .map(|i| line[i + 3..].to_string())
                .unwrap_or_default();
            continue;
        }
        if line.starts_with("@@") {
            out.push(Hunk {
                file: file.clone(),
                header: line.to_string(),
                lines: Vec::new(),
            });
            open = true;
            continue;
        }
        if !open || line.starts_with('\\') {
            continue;
        }
        if line.starts_with([' ', '+', '-']) {
            out.last_mut()
                .expect("an open hunk")
                .lines
                .push(line.to_string());
        }
    }
    out
}

fn sorted_body(lines: &[String], prefix: char, mask: bool) -> Vec<String> {
    let mut v: Vec<String> = lines
        .iter()
        .filter(|l| l.starts_with(prefix))
        .map(|l| {
            let body = &l[1..];
            if mask {
                mask_identifiers(body)
            } else {
                humanify_ag_model::js::trim(body).to_string()
            }
        })
        .collect();
    v.sort_by(|a, b| cmp_utf16(a, b));
    v
}

pub fn classify(h: &Hunk) -> Kind {
    let minus = sorted_body(&h.lines, '-', false);
    let plus = sorted_body(&h.lines, '+', false);
    if minus.is_empty() {
        return Kind::Added;
    }
    if plus.is_empty() {
        return Kind::Removed;
    }
    if minus == plus {
        return Kind::Moved;
    }
    if sorted_body(&h.lines, '-', true) == sorted_body(&h.lines, '+', true) {
        return Kind::NameOnly;
    }
    Kind::Real
}

pub fn changed_count(h: &Hunk) -> usize {
    h.lines
        .iter()
        .filter(|l| l.starts_with('+') || l.starts_with('-'))
        .count()
}

/// How readable an example is: a few changed lines, not a wall (vendor/
/// keeps libraries as one minified line: unreadable).
fn readability(h: &Hunk) -> i64 {
    let n = changed_count(h) as i64;
    if n < 2 || h.lines.len() > 40 || h.lines.iter().any(|l| l.encode_utf16().count() > 300) {
        return -1;
    }
    if n <= 16 { 100 - (8 - n).abs() } else { 50 - n }
}

/// Up to `per_kind` examples of each kind, readable ones first, never two
/// from the same file; ties break on file path then header.
pub fn pick_examples(hunks: &[Hunk], kinds: &[Kind], per_kind: usize) -> Vec<(Kind, Hunk)> {
    let mut out = Vec::new();
    for kind in kinds {
        let mut ranked: Vec<&Hunk> = hunks
            .iter()
            .filter(|h| readability(h) >= 0 && classify(h) == *kind)
            .collect();
        ranked.sort_by(|a, b| {
            readability(b)
                .cmp(&readability(a))
                .then_with(|| locale_compare(&a.file, &b.file))
                .then_with(|| locale_compare(&a.header, &b.header))
        });
        let mut seen: Vec<&str> = Vec::new();
        for h in ranked {
            if seen.contains(&h.file.as_str()) {
                continue;
            }
            seen.push(&h.file);
            out.push((*kind, h.clone()));
            if seen.len() >= per_kind {
                break;
            }
        }
    }
    out
}

/// Changed lines per hunk kind — the example section's framing only.
pub fn tally(hunks: &[Hunk]) -> [(Kind, usize); 5] {
    let mut t = [
        (Kind::NameOnly, 0),
        (Kind::Moved, 0),
        (Kind::Added, 0),
        (Kind::Removed, 0),
        (Kind::Real, 0),
    ];
    for h in hunks {
        let k = classify(h);
        if let Some(slot) = t.iter_mut().find(|(kind, _)| *kind == k) {
            slot.1 += changed_count(h);
        }
    }
    t
}

pub fn tally_of(t: &[(Kind, usize); 5], k: Kind) -> usize {
    t.iter().find(|(x, _)| *x == k).map_or(0, |(_, n)| *n)
}

#[cfg(test)]
#[path = "hunks_test.rs"]
mod hunks_test;
