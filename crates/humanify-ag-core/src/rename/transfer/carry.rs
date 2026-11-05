//! What the match stage collects FOR THE SPLIT (WP3.2) — TS originals:
//! `src/split/prior-carry.ts` (`MatcherCarry` / `PriorCarry`, forwarded
//! whole through the rename layers) and `src/rename/prior-match-map.ts`
//! (`buildPriorMatchMap`, the split's binding-identity map).
//!
//! The carry is read while the prior AST is alive (the only moment it can
//! be — re-parsing the prior at split time held two bundle graphs at once,
//! the measured 2.1.216 split OOM); the match map is completed after every
//! rename pass, because it is keyed by the FINAL shipped name.

use std::collections::HashMap;

/// TS `MatcherCarry`: the source text of every prior TOP-LEVEL statement,
/// in bundle order (the prior split ledger's `order` indexes it). Empty ⇒
/// the content-anchor tier abstains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatcherCarry {
    pub statement_texts: Vec<String>,
}

/// TS `PriorCarry`: the matcher's carry plus the binding-identity map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PriorCarry {
    pub matcher: MatcherCarry,
    /// new final name → the matched prior counterpart's name.
    pub match_map: Vec<(String, String)>,
}

/// TS `emptyMatcherCarry`.
pub fn empty_matcher_carry() -> MatcherCarry {
    MatcherCarry::default()
}

/// The carry of one prior text: each top-level statement's source slice
/// (spans are the inventory's — the wrapper body's statements when a
/// wrapper exists, the program body's otherwise: TS `topLevelStatements`).
pub fn matcher_carry(prior_text: &str, statement_spans: &[oxc_span::Span]) -> MatcherCarry {
    MatcherCarry {
        statement_texts: statement_spans
            .iter()
            .map(|s| {
                prior_text
                    .get(s.start as usize..s.end as usize)
                    .unwrap_or("")
                    .to_string()
            })
            .collect(),
    }
}

/// TS `buildPriorMatchMap`: `{final name → prior name}` from the matched
/// module bindings (final name = the binding's name after every rename
/// pass). Only FLIPPED bindings count (a pinned name needs nothing to
/// inherit); a final name seen with two different priors is poisoned and
/// dropped. Insertion order is kept (the TS Map).
pub fn build_prior_match_map<'a>(
    refs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(String, String)> {
    let mut order: Vec<String> = Vec::new();
    let mut resolved: HashMap<String, Option<String>> = HashMap::new();
    for (final_name, prior_name) in refs {
        if final_name == prior_name {
            continue;
        }
        match resolved.get(final_name) {
            None => {
                order.push(final_name.to_string());
                resolved.insert(final_name.to_string(), Some(prior_name.to_string()));
            }
            Some(Some(prev)) if prev != prior_name => {
                resolved.insert(final_name.to_string(), None);
            }
            Some(_) => {}
        }
    }
    order
        .into_iter()
        .filter_map(|name| {
            let prior = resolved.remove(&name).flatten()?;
            Some((name, prior))
        })
        .collect()
}

#[cfg(test)]
mod carry_test;
