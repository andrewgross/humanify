//! The MATCHING counts of a label (exp208): how many functions, module
//! bindings and identifiers the pipeline carried from the prior,
//! close-matched or left unmatched, against how many it ASKED the LLM about —
//! read from each scored run's `--stats-json` (`coverage`,
//! `resolutionStats`) through the shared diagnostics engine
//! (`eval::llm_diag::Spec`). `eval summarize` prints them per pair and
//! writes them into `summary.json` (`matchingDiagnostics`); `eval
//! leaderboard` prints them side by side; `eval bands` checks them across
//! same-commit repeats.
//!
//! The KPIs move between same-commit cold repeats because the LLM draws
//! different names; most of these counts do not. Two kinds:
//!
//! - EXACT (`EXACT`): decided by the code alone, before any answer the
//!   model gives. Byte-equal on every pair of seven same-commit cold
//!   repeats at two commits (40921b9f r1-r3 + s1, 111401fe r1-r3). A
//!   difference between repeats is a harness or determinism problem:
//!   `eval bands` refuses to write, and a same-commit leaderboard flags it.
//! - BANDED (the rest): they follow the names of the cold-built scratch
//!   base (the base itself is a cold LLM run) or the model's draws on the
//!   scored leg — legitimately, not a bug (exp209: carried / asked module
//!   bindings and asked identifiers follow the base's names; the model
//!   keeping a minified name moves "renamed" by 1 while "asked" holds).
//!
//! The stats' own key names mislead: `coverage.*.llm` counts ASKED AND
//! RENAMED, and `coverage.functions.failed` counts ASKED, NOTHING RENAMED
//! (not a transport failure) — so functions asked = `llm + failed`
//! (exp209). `eval bands`
//!   records the spread of their totals (`matchingBands` in the slot), and
//!   a same-commit leaderboard flags a move outside it.

use humanify_ag_model::js::{JsValue, number_to_string};

use super::card::{at, number_at};
use super::llm_diag::{Col, Spec, fold, sum};

const FUNCTIONS: &[Col] = &[
    sum("fnTotal", "coverage.functions.total"),
    sum("fnCarried", "coverage.functions.cached"),
    sum("fnClose", "coverage.functions.closeMatch"),
    sum(
        "fnAsked",
        "coverage.functions.llm+coverage.functions.failed",
    ),
    sum("fnLLMRenamed", "coverage.functions.llm"),
    sum("fnUnmatched", "resolutionStats.unmatched"),
    sum("fnAmbiguous", "resolutionStats.stillAmbiguous"),
];

const MODULE_BINDINGS: &[Col] = &[
    sum("mbCarried", "coverage.moduleBindings.cached"),
    sum("mbLLMRenamed", "coverage.moduleBindings.llm"),
];

const IDENTIFIERS: &[Col] = &[
    sum("idAsked", "coverage.identifiers.total"),
    sum("idLLMRenamed", "coverage.identifiers.llm"),
];

/// The matching counts.
pub const MATCHING: Spec = Spec {
    present: "coverage",
    groups: &[
        (
            "FUNCTIONS (total; carried from the prior; close-matched; asked the LLM = stats llm+failed; asked AND renamed by the LLM = stats llm; unmatched; still ambiguous)",
            FUNCTIONS,
        ),
        (
            "MODULE BINDINGS (carried from the prior; asked AND renamed by the LLM; statement-twin names are in neither)",
            MODULE_BINDINGS,
        ),
        (
            "IDENTIFIERS (asked the LLM; asked AND renamed by the LLM)",
            IDENTIFIERS,
        ),
    ],
    caption: "MATCHING, not KPIs. EXACT columns (fnTotal fnClose fnAsked fnUnmatched fnAmbiguous) must be EQUAL across same-commit repeats: a difference is a harness or determinism problem, not the LLM. The other columns follow the cold-built base's names and the model's draws (`eval bands` measures their spread). '-' = not in this run's stats.",
};

/// The counts the code decides alone: equal across same-commit repeats.
pub const EXACT: &[&str] = &[
    "fnTotal",
    "fnClose",
    "fnAsked",
    "fnUnmatched",
    "fnAmbiguous",
];

pub fn is_exact(key: &str) -> bool {
    EXACT.contains(&key)
}

/// A label's per-pair matching digests from its `summary.json`
/// (`matchingDiagnostics`); empty for a label summarized before exp208.
pub fn summary_digests(summary: &JsValue) -> Vec<JsValue> {
    match at(summary, "matchingDiagnostics") {
        Some(JsValue::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

/// What a set of same-commit repeats says about their matching counts.
pub struct Repeats {
    /// Per column, the largest pairwise disagreement of the repeats'
    /// totals over the pairs.
    pub spreads: Vec<(String, f64)>,
    /// One line per (exact column, pair) whose repeats differ.
    pub not_equal: Vec<String>,
}

fn pair_digest<'a>(digests: &'a [JsValue], pair: &str) -> Option<&'a JsValue> {
    digests
        .iter()
        .find(|d| at(d, "pair").and_then(JsValue::as_str) == Some(pair))
}

fn spread(values: &[f64]) -> f64 {
    let hi = values.iter().copied().fold(f64::MIN, f64::max);
    let lo = values.iter().copied().fold(f64::MAX, f64::min);
    hi - lo
}

/// The exact columns' per-pair disagreements among `labels`.
fn exact_mismatches(labels: &[(String, Vec<JsValue>)], pairs: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for key in EXACT {
        for pair in pairs {
            let values: Vec<(&str, f64)> = labels
                .iter()
                .filter_map(|(name, d)| {
                    Some((name.as_str(), number_at(pair_digest(d, pair)?, key)?))
                })
                .collect();
            let nums: Vec<f64> = values.iter().map(|(_, v)| *v).collect();
            if nums.len() > 1 && spread(&nums) != 0.0 {
                let said: Vec<String> = values
                    .iter()
                    .map(|(n, v)| format!("{n}={}", number_to_string(*v)))
                    .collect();
                out.push(format!(
                    "!! MATCHING NOT EQUAL: {key} on {pair}: {}",
                    said.join(" ")
                ));
            }
        }
    }
    out
}

/// The repeats' matching spreads and exact mismatches over `pairs`; None
/// when any repeat lacks matching counts for any of those pairs.
pub fn repeats(labels: &[(String, Vec<JsValue>)], pairs: &[String]) -> Option<Repeats> {
    let totals: Vec<JsValue> = labels
        .iter()
        .map(|(_, d)| fold(&MATCHING, d, pairs))
        .collect::<Option<_>>()?;
    let spreads = MATCHING
        .cols()
        .filter_map(|c| {
            let values: Vec<f64> = totals.iter().filter_map(|t| number_at(t, c.key)).collect();
            (values.len() == totals.len()).then(|| (c.key.to_string(), spread(&values)))
        })
        .collect();
    Some(Repeats {
        spreads,
        not_equal: exact_mismatches(labels, pairs),
    })
}

/// The leaderboard's matching flags: against the first row, every exact
/// column that moved and every banded column that moved past its band.
/// Only labels at the same commit are judged — across commits the counts
/// move with the code.
pub fn flags(
    rows: &[(String, Option<JsValue>)],
    band: &dyn Fn(&str) -> Option<f64>,
    same_commit: bool,
) -> Vec<String> {
    let Some((base_name, Some(base))) = rows.first() else {
        return Vec::new();
    };
    if !same_commit {
        return vec![
            "  matching: the labels are at different commits — the counts move with the code; not judged.".into(),
        ];
    }
    let mut out = Vec::new();
    for (name, d) in &rows[1..] {
        let Some(d) = d else { continue };
        for c in MATCHING.cols() {
            let (Some(v), Some(b)) = (number_at(d, c.key), number_at(base, c.key)) else {
                continue;
            };
            let delta = v - b;
            if delta == 0.0 {
                continue;
            }
            let said = format!(
                "{name} {} {} vs {base_name} {}",
                c.key,
                number_to_string(v),
                number_to_string(b)
            );
            if is_exact(c.key) {
                out.push(format!(
                    "!! MATCHING NOT EQUAL at the same commit: {said} — must be EQUAL: a harness or determinism problem, not the LLM."
                ));
            } else if let Some(w) = band(c.key).filter(|w| delta.abs() > *w) {
                out.push(format!(
                    "!! MATCHING outside its band at the same commit: {said} (band ±{})",
                    number_to_string(w)
                ));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "matching_test.rs"]
mod matching_test;
