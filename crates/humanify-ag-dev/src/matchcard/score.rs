//! The matcher's decisions held against the input comparison.

use std::collections::{BTreeMap, BTreeSet};

/// One function of one version.
pub struct Located {
    /// The unchanged input unit it sits in, by pair number.
    pub pair: Option<u32>,
    /// Its place among the functions of its input file.
    pub order: u64,
}

/// What the matcher decided: `(prior, fresh)` exact and close pairs, and
/// the prior functions it left ambiguous.
pub struct Matcher {
    pub exact: Vec<(usize, usize)>,
    pub close: Vec<(usize, usize)>,
    pub ambiguous: BTreeSet<usize>,
}

/// Counts over the new version's functions.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Card {
    /// In unchanged input, with a known partner in the prior.
    pub unchanged: u64,
    pub correct: u64,
    pub wrong: u64,
    /// Close-matched to its partner.
    pub missed_close: u64,
    /// It or its partner close-matched to another function.
    pub missed_close_other: u64,
    pub missed_ambiguous: u64,
    pub missed_unmatched: u64,
    /// In an unchanged unit whose two sides hold different numbers of
    /// functions.
    pub not_judged: u64,
    /// Not in unchanged input: matched or not.
    pub other_matched: u64,
    pub other_unmatched: u64,
}

/// Per pair number, the side's functions in it, in source order.
fn by_pair(side: &[Located]) -> BTreeMap<u32, Vec<usize>> {
    let mut out: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, function) in side.iter().enumerate() {
        if let Some(pair) = function.pair {
            out.entry(pair).or_default().push(i);
        }
    }
    for functions in out.values_mut() {
        functions.sort_by_key(|i| side[*i].order);
    }
    out
}

/// What the input comparison says about a fresh function.
#[derive(Clone, Copy)]
enum Truth {
    /// Its partner in the prior.
    Partner(usize),
    NotJudged,
    Other,
}

fn truths(fresh: &[Located], prior: &[Located]) -> Vec<Truth> {
    let (fresh_units, prior_units) = (by_pair(fresh), by_pair(prior));
    let mut out = vec![Truth::Other; fresh.len()];
    for (pair, functions) in &fresh_units {
        let partners = prior_units.get(pair).filter(|p| p.len() == functions.len());
        for (k, i) in functions.iter().enumerate() {
            out[*i] = partners.map_or(Truth::NotJudged, |p| Truth::Partner(p[k]));
        }
    }
    out
}

pub fn score(fresh: &[Located], prior: &[Located], matcher: &Matcher) -> Card {
    let matched: BTreeMap<usize, usize> = matcher.exact.iter().map(|(p, f)| (*f, *p)).collect();
    let close: BTreeSet<(usize, usize)> = matcher.close.iter().copied().collect();
    let close_prior: BTreeSet<usize> = close.iter().map(|c| c.0).collect();
    let close_fresh: BTreeSet<usize> = close.iter().map(|c| c.1).collect();
    let mut card = Card::default();
    for (i, truth) in truths(fresh, prior).into_iter().enumerate() {
        let Truth::Partner(partner) = truth else {
            let counter = match (truth, matched.contains_key(&i)) {
                (Truth::NotJudged, _) => &mut card.not_judged,
                (_, true) => &mut card.other_matched,
                (_, false) => &mut card.other_unmatched,
            };
            *counter += 1;
            continue;
        };
        card.unchanged += 1;
        let counter = match matched.get(&i) {
            Some(prior) if *prior == partner => &mut card.correct,
            Some(_) => &mut card.wrong,
            None if close.contains(&(partner, i)) => &mut card.missed_close,
            None if close_prior.contains(&partner) || close_fresh.contains(&i) => {
                &mut card.missed_close_other
            }
            None if matcher.ambiguous.contains(&partner) => &mut card.missed_ambiguous,
            None => &mut card.missed_unmatched,
        };
        *counter += 1;
    }
    card
}

#[cfg(test)]
#[path = "score_test.rs"]
mod score_test;
