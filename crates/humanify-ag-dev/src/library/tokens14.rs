//! Scorer 14's NAME EVIDENCE (experiments/137-scorer-mapdefect/scorer14/
//! tokens14.mjs) over the ported acorn tokenizer (`acorn`): what no
//! minifier renames — string and template values, `.property` names and
//! object-literal keys of 4+ characters for a unit; every string, template
//! value and name of any position for an installed file, plus, from a
//! tokenizer error on, the regex fallback's quoted strings and identifiers.
//!
//! Values are JavaScript strings (UTF-16). The layer builder reads them as
//! Python did after `json.loads`: as WTF-8 bytes (`wtf8`), whose byte
//! order is Python's code-point order and in which a lone surrogate can
//! never be found inside decoded text.
//!
//! `serve` answers tokens14.mjs's own stdin/stdout protocol, byte for byte
//! (`cargo dev library tokens14 < request.json`).

use std::collections::{HashMap, HashSet};

use super::acorn::{self, Tt};

/// UTF-8 bytes as Node's `toString("utf8")` reads them (invalid
/// sequences as U+FFFD, maximal subparts), in UTF-16.
pub fn utf16_lossy(bytes: &[u8]) -> Vec<u16> {
    String::from_utf8_lossy(bytes).encode_utf16().collect()
}

/// An insertion-ordered set (JavaScript's `Set`).
#[derive(Default)]
struct OrderedSet {
    items: Vec<Vec<u16>>,
    seen: HashSet<Vec<u16>>,
}

impl OrderedSet {
    fn add(&mut self, v: &[u16]) {
        if !self.seen.contains(v) {
            self.seen.insert(v.to_vec());
            self.items.push(v.to_vec());
        }
    }
}

/// `unitEvidence(text)`.
pub fn unit_evidence(text: &[u16]) -> Vec<Vec<u16>> {
    let (all, _) = acorn::tokenize(text);
    let mut toks = OrderedSet::default();
    for (k, t) in all.iter().enumerate() {
        let prev = if k > 0 { Some(all[k - 1].tt) } else { None };
        let next = all.get(k + 1).map(|t| t.tt);
        match (t.tt, &t.value) {
            (Tt::String | Tt::Template, Some(v)) => toks.add(v),
            (Tt::Name, Some(v))
                if v.len() >= 4
                    && (matches!(prev, Some(Tt::Dot | Tt::QuestionDot))
                        || (next == Some(Tt::Colon)
                            && matches!(prev, Some(Tt::BraceL | Tt::Comma)))) =>
            {
                toks.add(v)
            }
            _ => {}
        }
    }
    toks.items
}

fn is_ascii_word(c: u16) -> bool {
    c < 128 && ((c as u8).is_ascii_alphanumeric() || c == b'_' as u16)
}

/// `.` without the `s` flag: any code unit but a line terminator.
fn is_dot(c: u16) -> bool {
    !matches!(c, 10 | 13 | 0x2028 | 0x2029)
}

/// One quoted alternative of `STR` at `at` (the opening quote `q`):
/// `q((?:[^q\\\n]|\\.){0,400})q`; (content range, end).
fn quoted(text: &[u16], at: usize, q: u16) -> Option<(usize, usize, usize)> {
    let mut k = at + 1;
    let mut n = 0;
    loop {
        let c = *text.get(k)?;
        if c == q {
            return Some((at + 1, k, k + 1));
        }
        if n == 400 {
            return None;
        }
        if c == b'\\' as u16 {
            let d = *text.get(k + 1)?;
            if !is_dot(d) {
                return None;
            }
            k += 2;
        } else if c == 10 {
            return None;
        } else {
            k += 1;
        }
        n += 1;
    }
}

/// `text.matchAll(STR)`, STR = `"((?:[^"\\\n]|\\.){0,400})"|'((?:[^'\\\n]|\\.){0,400})'|([A-Za-z_$][\w$]{3,})`:
/// each match's captured group.
fn fallback_matches(text: &[u16], out: &mut OrderedSet) {
    let mut k = 0;
    while k < text.len() {
        let c = text[k];
        if c == b'"' as u16 || c == b'\'' as u16 {
            if let Some((a, b, end)) = quoted(text, k, c) {
                out.add(&text[a..b]);
                k = end;
                continue;
            }
        } else if c < 128
            && ((c as u8).is_ascii_alphabetic() || c == b'_' as u16 || c == b'$' as u16)
        {
            let mut j = k + 1;
            while j < text.len() && (is_ascii_word(text[j]) || text[j] == b'$' as u16) {
                j += 1;
            }
            if j - k >= 4 {
                out.add(&text[k..j]);
                k = j;
                continue;
            }
        }
        k += 1;
    }
}

/// `fileTokens(text)`.
pub fn file_tokens(text: &[u16]) -> Vec<Vec<u16>> {
    let (all, err) = acorn::tokenize(text);
    let mut toks = OrderedSet::default();
    for t in &all {
        if let (Tt::String | Tt::Template | Tt::Name | Tt::Kw(_), Some(v)) = (t.tt, &t.value) {
            toks.add(v);
        }
    }
    if let Some(e) = err {
        fallback_matches(&text[e.min(text.len())..], &mut toks);
    }
    toks.items
}

/// A JavaScript string as WTF-8: UTF-8, a lone surrogate as its own
/// three-byte sequence.
pub fn wtf8(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(units.len());
    let mut k = 0;
    while k < units.len() {
        let c = units[k] as u32;
        let cp = if (0xd800..0xdc00).contains(&c)
            && units
                .get(k + 1)
                .is_some_and(|&d| (0xdc00..0xe000).contains(&(d as u32)))
        {
            let d = units[k + 1] as u32;
            k += 1;
            0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00)
        } else {
            c
        };
        k += 1;
        match cp {
            0..=0x7f => out.push(cp as u8),
            0x80..=0x7ff => {
                out.push(0xc0 | (cp >> 6) as u8);
                out.push(0x80 | (cp & 0x3f) as u8);
            }
            0x800..=0xffff => {
                out.push(0xe0 | (cp >> 12) as u8);
                out.push(0x80 | ((cp >> 6) & 0x3f) as u8);
                out.push(0x80 | (cp & 0x3f) as u8);
            }
            _ => {
                out.push(0xf0 | (cp >> 18) as u8);
                out.push(0x80 | ((cp >> 12) & 0x3f) as u8);
                out.push(0x80 | ((cp >> 6) & 0x3f) as u8);
                out.push(0x80 | (cp & 0x3f) as u8);
            }
        }
    }
    out
}

/// The `files` mode: per wanted token the first `cap` files holding it,
/// and the tokens held by more.
pub struct Hits {
    pub hits: Vec<Vec<usize>>,
    pub many: Vec<usize>,
}

/// tokens14.mjs `files` mode over file BYTES (`read(i)`; None: unreadable).
pub fn file_hits(
    n_files: usize,
    read: impl Fn(usize) -> Option<Vec<u8>> + Sync + Send,
    want: &[Vec<u8>],
    cap: usize,
) -> Hits {
    let index: HashMap<&[u8], usize> = want
        .iter()
        .enumerate()
        .map(|(i, t)| (t.as_slice(), i))
        .collect();
    // tokenizing is per file and independent: in parallel, merged in file order
    let ids: Vec<usize> = (0..n_files).collect();
    let per_file = humanify_ag_core::par::map_ordered(&ids, |&fi| -> Vec<usize> {
        let Some(bytes) = read(fi) else {
            return Vec::new();
        };
        file_tokens(&utf16_lossy(&bytes))
            .iter()
            .filter_map(|t| index.get(wtf8(t).as_slice()).copied())
            .collect()
    });
    let mut hits = vec![Vec::new(); want.len()];
    let mut many = Vec::new();
    let mut in_many = vec![false; want.len()];
    for (fi, held) in per_file.into_iter().enumerate() {
        for i in held {
            if hits[i].len() < cap {
                hits[i].push(fi);
            } else if !in_many[i] {
                in_many[i] = true;
                many.push(i);
            }
        }
    }
    Hits { hits, many }
}

#[cfg(test)]
#[path = "tokens14_test.rs"]
mod tokens14_test;
