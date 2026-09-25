//! Content-anchor file inheritance — TS `src/split/content-anchor.ts`:
//! identify a statement across releases by what it SAYS (its rare string
//! literals) when neither its hash nor its name can.
//!
//! Precision over recall, four gates, each abstaining rather than guessing:
//! rare on BOTH sides; every rare literal points at the SAME prior
//! statement; at least half the identifier tokens shared; an uncontested
//! claim. Pure and order-independent by construction.
//!
//! The two TS regexes are non-unicode JS `RegExp`s, ported as explicit
//! scanners with those semantics (lesson 15): `RARE_LITERAL`'s `{12,}`
//! counts UTF-16 code units, and `WORD` is ASCII-only.

use std::collections::{HashMap, HashSet};

/// A prior-release top-level statement and the file it was emitted into.
#[derive(Clone, Debug)]
pub struct PriorStatement<'a> {
    pub text: &'a str,
    pub file: &'a str,
}

/// `MIN_TOKEN_OVERLAP`.
const MIN_TOKEN_OVERLAP: f64 = 0.5;
/// `NEAR_IDENTICAL_MAX_EDIT`.
pub const NEAR_IDENTICAL_MAX_EDIT: f64 = 0.1;

/// The closing-quote scan of one `"…"` / `'…'` alternative starting at
/// char index `open`: `Some((capture, index past the close))` when the run
/// of non-`[quote \ \n]` chars is 12+ UTF-16 units and ends at the quote.
fn quoted_run(chars: &[char], open: usize) -> Option<(String, usize)> {
    let quote = chars[open];
    let mut j = open + 1;
    let mut units = 0;
    while j < chars.len() && chars[j] != quote && chars[j] != '\\' && chars[j] != '\n' {
        units += chars[j].len_utf16();
        j += 1;
    }
    (j < chars.len() && chars[j] == quote && units >= 12)
        .then(|| (chars[open + 1..j].iter().collect(), j + 1))
}

/// `literalsOf`: the distinct captures of
/// `/"([^"\\\n]{12,})"|'([^'\\\n]{12,})'/g`, in first-seen order.
pub fn literals_of(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut i = 0;
    while i < chars.len() {
        if matches!(chars[i], '"' | '\'')
            && let Some((capture, next)) = quoted_run(&chars, i)
        {
            if seen.insert(capture.clone()) {
                out.push(capture);
            }
            i = next;
            continue;
        }
        i += 1;
    }
    out
}

/// `tokensOf`: the `/[A-Za-z_$][\w$]*/g` words longer than 2, as a set.
pub fn tokens_of(text: &str) -> HashSet<&str> {
    let bytes = text.as_bytes();
    let is_start = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b == b'$';
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$';
    let mut out = HashSet::new();
    let mut i = 0;
    while i < bytes.len() {
        if !is_start(bytes[i]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < bytes.len() && is_word(bytes[j]) {
            j += 1;
        }
        if j - i > 2 {
            out.insert(&text[i..j]);
        }
        i = j;
    }
    out
}

/// `tokenOverlap`: the share of the larger token set both have.
fn token_overlap(a: &HashSet<&str>, b: &HashSet<&str>) -> f64 {
    let inter = b.iter().filter(|w| a.contains(*w)).count();
    inter as f64 / a.len().max(b.len()).max(1) as f64
}

/// `uniqueLiteralOwners`: literal → the single statement carrying it.
fn unique_literal_owners(literal_sets: &[Vec<String>]) -> HashMap<&str, usize> {
    let mut owner: HashMap<&str, Option<usize>> = HashMap::new();
    for (i, set) in literal_sets.iter().enumerate() {
        for lit in set {
            owner
                .entry(lit.as_str())
                .and_modify(|o| {
                    if *o != Some(i) {
                        *o = None;
                    }
                })
                .or_insert(Some(i));
        }
    }
    owner
        .into_iter()
        .filter_map(|(lit, o)| o.map(|i| (lit, i)))
        .collect()
}

/// `soleCandidate`: the one prior statement every rare literal of this
/// fresh statement points at, or `None`.
fn sole_candidate(
    fresh_literals: &[String],
    fresh_owners: &HashMap<&str, usize>,
    prior_owners: &HashMap<&str, usize>,
    fresh_idx: usize,
) -> Option<usize> {
    let mut found: Option<usize> = None;
    for lit in fresh_literals {
        if fresh_owners.get(lit.as_str()) != Some(&fresh_idx) {
            continue;
        }
        let Some(&prior_idx) = prior_owners.get(lit.as_str()) else {
            continue;
        };
        match found {
            None => found = Some(prior_idx),
            Some(f) if f != prior_idx => return None,
            Some(_) => {}
        }
    }
    found
}

/// `contentAnchorPairs`: fresh statement index → the prior statement index
/// it is, for the pairs that pass every gate.
pub fn content_anchor_pairs(prior: &[PriorStatement], fresh: &[&str]) -> HashMap<usize, usize> {
    let mut verdicts = HashMap::new();
    if prior.is_empty() || fresh.is_empty() {
        return verdicts;
    }
    let prior_literals: Vec<Vec<String>> = prior.iter().map(|s| literals_of(s.text)).collect();
    let fresh_literals: Vec<Vec<String>> = fresh.iter().map(|t| literals_of(t)).collect();
    let prior_owners = unique_literal_owners(&prior_literals);
    let fresh_owners = unique_literal_owners(&fresh_literals);
    if prior_owners.is_empty() {
        return verdicts;
    }
    // Claims first; only the uncontested survive. Claim order is fresh
    // order, so the claimant lists are in fresh order.
    let mut claimed_by: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut claim_order: Vec<usize> = Vec::new();
    for (i, text) in fresh.iter().enumerate() {
        let Some(prior_idx) = sole_candidate(&fresh_literals[i], &fresh_owners, &prior_owners, i)
        else {
            continue;
        };
        if token_overlap(&tokens_of(prior[prior_idx].text), &tokens_of(text)) < MIN_TOKEN_OVERLAP {
            continue;
        }
        let list = claimed_by.entry(prior_idx).or_insert_with(|| {
            claim_order.push(prior_idx);
            Vec::new()
        });
        list.push(i);
    }
    for prior_idx in claim_order {
        if let [only] = claimed_by[&prior_idx].as_slice() {
            verdicts.insert(*only, prior_idx);
        }
    }
    verdicts
}

/// `changedLineFraction`: the share of the fresh statement's lines that do
/// not appear in its prior twin.
pub fn changed_line_fraction(fresh_text: &str, prior_text: &str) -> f64 {
    let fresh_lines: Vec<&str> = fresh_text.split('\n').collect();
    let prior_lines: HashSet<&str> = prior_text.split('\n').collect();
    let changed = fresh_lines
        .iter()
        .filter(|l| !prior_lines.contains(*l))
        .count();
    changed as f64 / fresh_lines.len().max(1) as f64
}

/// `AnchorVerdict`.
#[derive(Clone, Debug, PartialEq)]
pub struct AnchorVerdict {
    pub file: String,
    pub near_identical: bool,
}

/// `contentAnchorVerdicts`: fresh index → verdict, for the identified ones.
pub fn content_anchor_verdicts(
    prior: &[PriorStatement],
    fresh: &[&str],
) -> HashMap<usize, AnchorVerdict> {
    content_anchor_pairs(prior, fresh)
        .into_iter()
        .map(|(fresh_idx, prior_idx)| {
            let near = changed_line_fraction(fresh[fresh_idx], prior[prior_idx].text)
                <= NEAR_IDENTICAL_MAX_EDIT;
            (
                fresh_idx,
                AnchorVerdict {
                    file: prior[prior_idx].file.to_string(),
                    near_identical: near,
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod anchor_test;
