//! What the matcher decided for every prior function, written as a file
//! of the run (`.humanify/match-decisions.json`).

use std::collections::HashMap;

use crate::babel_view::{line_of, line_starts};

pub const MATCH_DECISIONS_PATH: &str = ".humanify/match-decisions.json";

/// One text's functions: each id with the byte its function starts at.
pub struct Functions<'a> {
    pub text: &'a str,
    pub starts: Vec<(&'a str, u32)>,
}

/// The matcher's outcome, by function id. Pairs are (prior, fresh).
#[derive(Default)]
pub struct Decided<'a> {
    pub exact: Vec<(&'a str, &'a str)>,
    pub close: Vec<(&'a str, &'a str)>,
    pub ambiguous: Vec<&'a str>,
    pub unmatched: Vec<&'a str>,
}

/// `fresh` and `prior` hold the 1-based line of each function in text
/// order; every other field holds positions in those two lists, pairs as
/// (prior, fresh).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct MatchDecisions {
    pub version: u32,
    pub fresh: Vec<u32>,
    pub prior: Vec<u32>,
    pub exact: Vec<(u32, u32)>,
    pub close: Vec<(u32, u32)>,
    pub ambiguous: Vec<u32>,
    pub unmatched: Vec<u32>,
}

pub fn record(
    fresh: &Functions<'_>,
    prior: &Functions<'_>,
    decided: &Decided<'_>,
) -> MatchDecisions {
    let (fresh_lines, fresh_at) = in_text_order(fresh);
    let (prior_lines, prior_at) = in_text_order(prior);
    let pairs = |list: &[(&str, &str)]| {
        list.iter()
            .filter_map(|(p, f)| Some((*prior_at.get(p)?, *fresh_at.get(f)?)))
            .collect()
    };
    let singles = |list: &[&str]| {
        list.iter()
            .filter_map(|p| prior_at.get(p).copied())
            .collect()
    };
    MatchDecisions {
        version: 1,
        exact: pairs(&decided.exact),
        close: pairs(&decided.close),
        ambiguous: singles(&decided.ambiguous),
        unmatched: singles(&decided.unmatched),
        fresh: fresh_lines,
        prior: prior_lines,
    }
}

/// Each function's line in text order, and each id's position in that list.
fn in_text_order<'a>(functions: &Functions<'a>) -> (Vec<u32>, HashMap<&'a str, u32>) {
    let starts = line_starts(functions.text);
    let mut ordered = functions.starts.clone();
    ordered.sort_by_key(|&(id, start)| (start, id));
    let lines = ordered
        .iter()
        .map(|&(_, start)| line_of(&starts, start) + 1)
        .collect();
    let at = ordered
        .iter()
        .enumerate()
        .map(|(i, &(id, _))| (id, i as u32))
        .collect();
    (lines, at)
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
        unmatched: result.unmatched.iter().map(String::as_str).collect(),
    };
    record(&side(&stage.fresh), &side(&stage.prior), &decided)
}

fn side<'a>(s: &'a crate::prior::StageSide<'_, '_>) -> Functions<'a> {
    Functions {
        text: s.ingest.semantic().source_text(),
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
