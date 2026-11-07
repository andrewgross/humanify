//! Two LINE-level advisory views of a split tree's diff, reported beside
//! the composition (TS: `034/build-constant-churn.ts`,
//! `034/name-only-churn.ts`). Both are multiset differences per common
//! file — a line whose text is unchanged cannot be a changed line however
//! the hunks fall — so neither depends on a diff algorithm.
//!
//! - build-constant churn: changed lines whose key is one of the
//!   build-stamp fields (`VERSION`, `BUILD_TIME`, `GIT_SHA`), present on
//!   both sides of a file and changed in at least two files — a bundler
//!   inlined one constants object at every use site; subtracted from the
//!   headline (`churnLinesExBuild`).
//! - name-only churn: changed lines whose code is identical once local
//!   identifiers are masked (property names and object keys stay: they
//!   are semantic).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use humanify_ag_model::js::{is_js_whitespace, trim};

use super::text::{list_js, read};

/// Build-constant churn of two trees.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildConstantChurn {
    /// git lines (deletions + insertions) of modified build-stamp fields.
    pub lines: usize,
    pub files: usize,
    /// Per key, in first-counted order.
    pub by_key: Vec<(String, usize)>,
}

/// The build-stamp fields, by name (a fixed list on purpose: a pattern
/// absorbed genuinely new feature flags).
const BUILD_STAMP_FIELDS: [&str; 3] = ["VERSION", "BUILD_TIME", "GIT_SHA"];

/// A key must change in at least this many files.
const MIN_FILES: usize = 2;

/// `/^\s*([A-Za-z][A-Za-z0-9_]*):\s*.+$/`'s key.
fn const_field_key(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(is_js_whitespace);
    let first = rest.chars().next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let key = &rest[..end];
    let after = rest[end..].strip_prefix(':')?;
    // `\s*.+$`: something follows the colon, and no line terminator
    // (`.` never matches one, `$` is the end of the line).
    let ok = !after.is_empty() && !after.contains(['\r', '\u{2028}', '\u{2029}', '\n']);
    ok.then_some(key)
}

/// Lines of `a` not covered by `b`, as a multiset (text -> count).
fn surplus_counts<'a>(a: &[&'a str], b: &[&'a str]) -> BTreeMap<&'a str, usize> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for l in a {
        *counts.entry(l).or_default() += 1;
    }
    for l in b {
        if let Some(n) = counts.get_mut(l) {
            if *n == 1 {
                counts.remove(l);
            } else {
                *n -= 1;
            }
        }
    }
    counts
}

/// The common files of two trees, sorted, with both texts.
fn common_files(prior: &Path, fresh: &Path) -> Result<Vec<(String, String, String)>, String> {
    let mut all: BTreeSet<String> = list_js(prior)?.into_iter().collect();
    all.extend(list_js(fresh)?);
    let mut out = Vec::new();
    for rel in all {
        let (p, f) = (prior.join(&rel), fresh.join(&rel));
        if p.is_file() && f.is_file() {
            out.push((rel, read(&p)?, read(&f)?));
        }
    }
    Ok(out)
}

/// Count build-constant churn between two emitted trees.
pub fn build_constant_churn(prior: &Path, fresh: &Path) -> Result<BuildConstantChurn, String> {
    // key -> file -> paired lines * 2
    let mut per_key: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    for (rel, p, f) in common_files(prior, fresh)? {
        let pl: Vec<&str> = p.split('\n').collect();
        let fl: Vec<&str> = f.split('\n').collect();
        let gone = surplus_counts(&pl, &fl);
        let came = surplus_counts(&fl, &pl);
        if gone.is_empty() && came.is_empty() {
            continue;
        }
        let key_count = |m: &BTreeMap<&str, usize>| {
            let mut out: HashMap<&str, usize> = HashMap::new();
            for (text, n) in m {
                if let Some(k) = const_field_key(text)
                    && let Some(field) = BUILD_STAMP_FIELDS.iter().find(|f| **f == k)
                {
                    *out.entry(field).or_default() += n;
                }
            }
            out
        };
        let gone_keys = key_count(&gone);
        let came_keys = key_count(&came);
        for field in BUILD_STAMP_FIELDS {
            let paired = gone_keys
                .get(field)
                .copied()
                .unwrap_or(0)
                .min(came_keys.get(field).copied().unwrap_or(0));
            if paired > 0 {
                per_key
                    .entry(field)
                    .or_default()
                    .insert(rel.clone(), paired * 2);
            }
        }
    }
    let mut out = BuildConstantChurn::default();
    let mut touched: BTreeSet<&String> = BTreeSet::new();
    for field in BUILD_STAMP_FIELDS {
        let Some(per_file) = per_key.get(field) else {
            continue;
        };
        if per_file.len() < MIN_FILES {
            continue;
        }
        let n: usize = per_file.values().sum();
        out.by_key.push((field.to_string(), n));
        out.lines += n;
        touched.extend(per_file.keys());
    }
    out.files = touched.len();
    Ok(out)
}

/// JS keywords never masked.
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

fn ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}
fn ident_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Skip `\s*` from byte `i`; the index after it.
fn skip_ws(s: &str, mut i: usize) -> usize {
    while let Some(c) = s[i..].chars().next() {
        if !is_js_whitespace(c) {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// Mask local identifiers: every identifier that is neither preceded by
/// `.` nor followed by `:` and is not a keyword becomes `X`; trimmed.
/// (`/(\.\s*)?([A-Za-z_$][A-Za-z0-9_$]*)(\s*:)?/g`.)
pub fn mask_identifiers(line: &str) -> String {
    let b = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    while i < b.len() {
        // A match at i: an optional `.\s*`, then an identifier.
        let (dot, at) = if b[i] == b'.' {
            let j = skip_ws(line, i + 1);
            if j < b.len() && ident_start(b[j]) {
                (true, j)
            } else {
                (false, i)
            }
        } else {
            (false, i)
        };
        if at < b.len() && ident_start(b[at]) {
            let mut end = at + 1;
            while end < b.len() && ident_cont(b[end]) {
                end += 1;
            }
            let name = &line[at..end];
            let k = skip_ws(line, end);
            let colon = k < b.len() && b[k] == b':';
            let stop = if colon { k + 1 } else { end };
            if dot || colon || KEYWORDS.contains(&name) {
                out.push_str(&line[i..stop]);
            } else {
                out.push('X');
            }
            i = stop;
        } else {
            let c = line[i..].chars().next().unwrap_or(' ');
            out.push(c);
            i += c.len_utf8();
        }
    }
    trim(&out).to_string()
}

/// Name-only churn of two trees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NameOnlyChurn {
    pub lines: usize,
    pub files: usize,
    pub changed_lines: usize,
}

/// Entries of `a` not covered by `b`, in `a`'s order.
fn surplus_list<'a>(a: &[&'a str], b: &[&'a str]) -> Vec<&'a str> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for l in b {
        *counts.entry(l).or_default() += 1;
    }
    let mut out = Vec::new();
    for l in a {
        match counts.get_mut(l) {
            Some(n) if *n > 0 => *n -= 1,
            _ => out.push(*l),
        }
    }
    out
}

/// Count name-only churn between two emitted trees.
pub fn name_only_churn(prior: &Path, fresh: &Path) -> Result<NameOnlyChurn, String> {
    let mut out = NameOnlyChurn::default();
    for (_, p, f) in common_files(prior, fresh)? {
        let pl: Vec<&str> = p.split('\n').collect();
        let fl: Vec<&str> = f.split('\n').collect();
        let gone = surplus_list(&pl, &fl);
        let came = surplus_list(&fl, &pl);
        out.changed_lines += gone.len() + came.len();
        if gone.is_empty() || came.is_empty() {
            continue;
        }
        let mut masked: HashMap<String, usize> = HashMap::new();
        for l in &gone {
            let k = mask_identifiers(l);
            if !k.is_empty() {
                *masked.entry(k).or_default() += 1;
            }
        }
        let mut file_lines = 0;
        for l in &came {
            if let Some(n) = masked.get_mut(&mask_identifiers(l))
                && *n > 0
            {
                *n -= 1;
                file_lines += 2;
            }
        }
        if file_lines > 0 {
            out.lines += file_lines;
            out.files += 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "line_churn_test.rs"]
mod line_churn_test;
