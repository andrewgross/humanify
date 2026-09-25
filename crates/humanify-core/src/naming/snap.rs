//! Snap LLM name suggestions to prior-version names — TS original:
//! `src/rename/prior-name-snap.ts` (ledger row WP4.5; the waves are its
//! first consumer, the prior-name-snap pass reuses it).
//!
//! A close-matched function's suggestion that shares its STEM (the name
//! minus trailing decorations, lowercased) with exactly ONE prior name is
//! snapped to that prior name; an exact-slot snap (`priorNameSnaps`, the
//! content-gated channel) wins over both the LLM's pick and the stem.

use std::collections::HashMap;

use super::validation::DECORATION_WORDS;

/// LLM-authored decorations stripped but never produced.
const STRIP_ONLY_WORDS: [&str; 2] = ["Instance", "Obj"];
/// Ladder words deliberately NOT stripped (`Result` is usually semantic).
const LADDER_ONLY_WORDS: [&str; 1] = ["Result"];

fn strip_words() -> Vec<&'static str> {
    DECORATION_WORDS
        .iter()
        .copied()
        .filter(|w| !LADDER_ONLY_WORDS.contains(w))
        .chain(STRIP_ONLY_WORDS)
        .collect()
}

/// The start of the leftmost suffix matching
/// `(?:<words>|_?\d+)+$` — a JS `replace` removes the first (leftmost)
/// match, i.e. the smallest start whose suffix decomposes fully.
fn decoration_suffix_start(name: &str, words: &[&str]) -> Option<usize> {
    let b = name.as_bytes();
    let n = b.len();
    // can[p]: name[p..] is a (possibly empty) concatenation of units.
    let mut can = vec![false; n + 1];
    can[n] = true;
    for p in (0..n).rev() {
        if !name.is_char_boundary(p) {
            continue;
        }
        let mut ok = words
            .iter()
            .any(|w| name[p..].starts_with(w) && can[p + w.len()]);
        if !ok {
            // `_?\d+`
            let mut q = p;
            if b[q] == b'_' {
                q += 1;
            }
            let mut r = q;
            while r < n && b[r].is_ascii_digit() {
                r += 1;
                if can[r] {
                    ok = true;
                    break;
                }
            }
        }
        can[p] = ok;
    }
    (0..n).find(|&p| can[p] && name.is_char_boundary(p))
}

/// `nameStem`: the name minus trailing decorations, lowercased (empty when
/// all decoration).
pub fn name_stem(name: &str) -> String {
    let words = strip_words();
    let kept = match decoration_suffix_start(name, &words) {
        Some(p) => &name[..p],
        None => name,
    };
    kept.to_lowercase()
}

/// `buildPriorStemIndex`: stem → prior name, UNIQUE stems only.
pub fn build_prior_stem_index(prior_names: &[String]) -> HashMap<String, String> {
    let mut by_stem: HashMap<String, Option<String>> = HashMap::new();
    for name in prior_names {
        let stem = name_stem(name);
        if stem.is_empty() {
            continue;
        }
        let entry = if by_stem.contains_key(&stem) {
            None
        } else {
            Some(name.clone())
        };
        by_stem.insert(stem, entry);
    }
    by_stem
        .into_iter()
        .filter_map(|(stem, name)| name.map(|n| (stem, n)))
        .collect()
}

/// `snapToKnownPrior`: the prior when the suggestion merely re-decorates it.
pub fn snap_to_known_prior(prior: &str, suggestion: &str) -> String {
    if name_stem(prior) == name_stem(suggestion) {
        prior.to_string()
    } else {
        suggestion.to_string()
    }
}

/// `snapSuggestionToPrior`: the exact-slot snap first, then the stem index.
pub fn snap_suggestion_to_prior(
    suggestion: &str,
    stem_index: &HashMap<String, String>,
    old_name: Option<&str>,
    snaps: Option<&[(String, String)]>,
) -> String {
    if let (Some(old), Some(snaps)) = (old_name, snaps)
        && !old.is_empty()
        && let Some((_, prior)) = snaps.iter().find(|(k, _)| k == old)
        && !prior.is_empty()
    {
        return prior.clone();
    }
    stem_index
        .get(&name_stem(suggestion))
        .cloned()
        .unwrap_or_else(|| suggestion.to_string())
}

#[cfg(test)]
mod snap_test;
