//! What the matcher decided for every function, as the layout record
//! reads it.

use std::collections::HashMap;

/// One text's functions: each id with the byte its function starts at.
pub struct Functions<'a> {
    pub starts: Vec<(&'a str, u32)>,
}

/// The matcher's outcome, by function id. Pairs are (prior, fresh).
#[derive(Default)]
pub struct Decided<'a> {
    pub exact: Vec<(&'a str, &'a str)>,
    pub close: Vec<(&'a str, &'a str)>,
    pub ambiguous: Vec<&'a str>,
}

/// `fresh` holds the byte each fresh function starts at, in text order;
/// `prior` counts the prior text's functions. Every other field holds
/// positions in text order, pairs as (prior, fresh).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MatchDecisions {
    pub fresh: Vec<u32>,
    pub prior: u32,
    pub exact: Vec<(u32, u32)>,
    pub close: Vec<(u32, u32)>,
    pub ambiguous: Vec<u32>,
}

pub fn record(
    fresh: &Functions<'_>,
    prior: &Functions<'_>,
    decided: &Decided<'_>,
) -> MatchDecisions {
    let (fresh_starts, fresh_at) = in_text_order(fresh);
    let (prior_starts, prior_at) = in_text_order(prior);
    let pairs = |list: &[(&str, &str)]| {
        list.iter()
            .filter_map(|(p, f)| Some((*prior_at.get(p)?, *fresh_at.get(f)?)))
            .collect()
    };
    MatchDecisions {
        exact: pairs(&decided.exact),
        close: pairs(&decided.close),
        ambiguous: (decided.ambiguous.iter())
            .filter_map(|p| prior_at.get(p).copied())
            .collect(),
        fresh: fresh_starts,
        prior: prior_starts.len() as u32,
    }
}

/// Each function's start in text order, and each id's position in that list.
fn in_text_order<'a>(functions: &Functions<'a>) -> (Vec<u32>, HashMap<&'a str, u32>) {
    let mut ordered = functions.starts.clone();
    ordered.sort_by_key(|&(id, start)| (start, id));
    let starts = ordered.iter().map(|&(_, start)| start).collect();
    let at = ordered
        .iter()
        .enumerate()
        .map(|(i, &(id, _))| (id, i as u32))
        .collect();
    (starts, at)
}

/// The decisions of one finished match stage.
pub fn of_stage(stage: &crate::prior::MatchStage<'_, '_>) -> MatchDecisions {
    let result = stage.function_result;
    let decided = Decided {
        exact: result
            .matches
            .iter_ordered()
            .map(|(p, f)| (p.as_str(), f.as_str()))
            .collect(),
        close: stage
            .close_pairs
            .iter()
            .map(|c| (c.prior_id.as_str(), c.fresh_id.as_str()))
            .collect(),
        ambiguous: result.ambiguous.iter().map(|(p, _)| p.as_str()).collect(),
    };
    record(&side(&stage.fresh), &side(&stage.prior), &decided)
}

fn side<'a>(s: &'a crate::prior::StageSide<'_, '_>) -> Functions<'a> {
    Functions {
        starts: s
            .graph
            .functions
            .iter()
            .map(|f| (f.session_id.as_str(), f.span.start))
            .collect(),
    }
}

#[cfg(test)]
#[path = "decisions/decisions_test.rs"]
mod decisions_test;
