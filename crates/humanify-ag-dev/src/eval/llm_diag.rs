//! The LLM call diagnostics of a label (exp206): what each scored run's
//! call ledger (`--stats-json` `llmCalls`, plus the `reask` backups and
//! the prompt guard's re-ask count) says about the SERVER, the MODEL'S
//! ANSWERS and the BACKUPS the pipeline took — the one reader of those
//! blocks in the harness. `eval summarize` prints it per pair and writes
//! it into `summary.json` (`llmDiagnostics`); `eval leaderboard` prints it
//! side by side.
//!
//! These are DIAGNOSTICS, never KPIs: they explain a KPI move (a slower
//! server, more collisions, fewer answers), they do not grade one. They
//! carry no band. Server columns move with the endpoint and the cache;
//! answer and backup columns are byte-identical with the model held fixed
//! (a warm cache), so between two cold runs they move with the model's
//! draws as well as with the change under test. A run scored before
//! exp206 has no ledger: every column prints `-`.
//!
//! The digest / fold / table engine is shared by every per-run stats
//! diagnostic (a `Spec`): the call ledger here (`LLM_CALLS`) and the
//! matching counts (`eval::matching::MATCHING`, exp208).

use std::path::Path;

use humanify_ag_model::js::{JsObject, JsValue, number_to_string};

use super::card::{at, number_at};

/// How a column folds across pairs.
#[derive(Clone, Copy)]
enum Fold {
    Sum,
    /// A percentile cannot be summed: the worst pair's.
    Max,
}

pub struct Col {
    pub key: &'static str,
    /// Where the number lives in a `--stats-json` record; `a+b` is the
    /// sum of two paths (absent when either is).
    path: &'static str,
    fold: Fold,
}

pub const fn sum(key: &'static str, path: &'static str) -> Col {
    Col {
        key,
        path,
        fold: Fold::Sum,
    }
}

const fn max(key: &'static str, path: &'static str) -> Col {
    Col {
        key,
        path,
        fold: Fold::Max,
    }
}

/// One family of per-run diagnostics read from `--stats-json`.
pub struct Spec {
    /// A run without this block has no digest at all (every cell `-`).
    pub present: &'static str,
    /// (group title, columns). Every column of every group is in the digest.
    pub groups: &'static [(&'static str, &'static [Col])],
    /// The line printed under every table's title.
    pub caption: &'static str,
}

/// The LLM call ledger (exp206).
pub const LLM_CALLS: Spec = Spec {
    present: "llmCalls",
    groups: GROUPS,
    caption: "DIAGNOSTICS, not KPIs: no band, no grade — they say WHY a KPI moved (server speed, the model's answers, backups taken, lost answers). '-' = scored before exp206 (no call ledger).",
};

const GROUPS: &[(&str, &[Col])] = &[
    (
        "SERVER (moves with the endpoint and the cache; ms columns: worst pair)",
        &[
            sum("calls", "llmCalls.outcomes.total.calls"),
            sum("sent", "llmCalls.transport.total.sent"),
            sum("served", "llmCalls.transport.total.servedMemo"),
            sum("cached", "llmCalls.transport.total.servedDisk"),
            sum("okRetry", "llmCalls.transport.total.okAfterRetry"),
            sum("rTimeout", "llmCalls.transport.total.retried.timeout"),
            sum("rConn", "llmCalls.transport.total.retried.connection"),
            sum("r5xx", "llmCalls.transport.total.retried.status5xx"),
            sum("r429", "llmCalls.transport.total.retried.status429"),
            max("p50ms", "llmCalls.timing.total.attemptMs.p50"),
            max("p90ms", "llmCalls.timing.total.attemptMs.p90"),
            max("p99ms", "llmCalls.timing.total.attemptMs.p99"),
            max("maxMs", "llmCalls.timing.total.attemptMs.max"),
        ],
    ),
    (
        "MODEL ANSWERS (raw verdicts per asked identifier, before any repair)",
        &[
            sum("asked", "llmCalls.outcomes.total.answers.asked"),
            sum("proposed", "llmCalls.outcomes.total.answers.proposed"),
            sum("taken", "llmCalls.outcomes.total.answers.takenName"),
            sum(
                "dupInAns",
                "llmCalls.outcomes.total.answers.duplicateInAnswer",
            ),
            sum("invalid", "llmCalls.outcomes.total.answers.invalid"),
            sum("unchanged", "llmCalls.outcomes.total.answers.unchanged"),
            sum("missing", "llmCalls.outcomes.total.answers.missing"),
            sum("hinted", "llmCalls.outcomes.total.answers.hinted"),
            sum("=hint", "llmCalls.outcomes.total.answers.equalsPriorHint"),
            sum("truncated", "llmCalls.outcomes.total.truncated"),
        ],
    ),
    (
        "BACKUPS AND FAILURES (what the pipeline did after a rejected or lost answer)",
        &[
            sum("reaskIds", "promptGuard.retryAsked"),
            sum("lateRej", "reask.lateRejections"),
            sum("handoff", "reask.collisionHandoffs"),
            sum("endHandoff", "reask.laneEndHandoffs"),
            sum("ladderEnd", "reask.ladderAtLaneEnd"),
            sum("ladderBar", "reask.ladderAtBarrier"),
            sum("sanitized", "reask.invalidSuggestionFinishes"),
            sum("sweepReask", "reask.sweepReasked"),
            sum("failed", "llmCalls.outcomes.total.failed"),
            sum("fTimeout", "llmCalls.transport.total.failed.timeout"),
            sum("fStatus", "llmCalls.transport.total.failed.status"),
        ],
    ),
];

impl Spec {
    pub fn cols(&self) -> impl Iterator<Item = &'static Col> {
        self.groups.iter().flat_map(|(_, c)| c.iter())
    }
}

/// The number a column reads from a stats record (`a+b` sums two paths).
fn read(stats: &JsValue, path: &str) -> Option<f64> {
    path.split('+')
        .map(|p| number_at(stats, p))
        .sum::<Option<f64>>()
}

/// One run's digest from its `--stats-json` record; None for a run
/// without the spec's block (for the call ledger: scored before exp206).
pub fn digest(spec: &Spec, stats: &JsValue) -> Option<JsValue> {
    at(stats, spec.present)?;
    let mut o = JsObject::new();
    for c in spec.cols() {
        if let Some(n) = read(stats, c.path) {
            o.insert(c.key, JsValue::Number(n));
        }
    }
    Some(JsValue::Object(o))
}

/// `{pair, ...digest}` for every card whose scored run left a ledger, read
/// from the label's `<to>.stats.json`.
pub fn pair_digests(spec: &Spec, dir: &Path, cards: &[JsValue]) -> Vec<JsValue> {
    cards
        .iter()
        .filter_map(|card| {
            let pair = at(card, "pair")?.as_str()?;
            let to = pair.rsplit("->").next()?;
            let text = std::fs::read_to_string(dir.join(format!("{to}.stats.json"))).ok()?;
            let d = digest(spec, &JsValue::parse(&text).ok()?)?;
            let mut o = JsObject::new();
            o.insert("pair", JsValue::str(pair));
            for (k, v) in d.as_object()?.entries() {
                o.insert(k.clone(), v.clone());
            }
            Some(JsValue::Object(o))
        })
        .collect()
}

/// A label's digests folded over `pairs` (sum, or the worst pair for a
/// percentile). None when any of those pairs has no digest — a partial
/// total would read as a real drop.
pub fn fold(spec: &Spec, digests: &[JsValue], pairs: &[String]) -> Option<JsValue> {
    let chosen: Vec<&JsValue> = pairs
        .iter()
        .map(|p| {
            digests
                .iter()
                .find(|d| at(d, "pair").and_then(JsValue::as_str) == Some(p))
        })
        .collect::<Option<_>>()?;
    let mut o = JsObject::new();
    for c in spec.cols() {
        let values: Vec<f64> = chosen.iter().filter_map(|d| number_at(d, c.key)).collect();
        if values.len() != chosen.len() {
            continue;
        }
        let v = match c.fold {
            Fold::Sum => values.iter().sum(),
            Fold::Max => values.iter().copied().fold(0.0, f64::max),
        };
        o.insert(c.key, JsValue::Number(v));
    }
    Some(JsValue::Object(o))
}

fn width(key: &str) -> usize {
    key.len().max(7)
}

fn value_cell(v: Option<f64>, base: Option<f64>, is_base: bool) -> String {
    let Some(v) = v else { return "-".into() };
    let s = number_to_string(v);
    match base {
        Some(b) if !is_base && v != b => {
            let d = v - b;
            let sign = if d > 0.0 { "+" } else { "" };
            format!("{s} ({sign}{})", number_to_string(d))
        }
        _ => s,
    }
}

/// The diagnostics tables: one row per `(name, digest)` (None prints
/// `-`). With `deltas`, the first row is the baseline the others' deltas
/// are against (the leaderboard); without, plain values (summarize's
/// per-pair rows).
pub fn tables(
    spec: &Spec,
    title: &str,
    rows: &[(String, Option<JsValue>)],
    deltas: bool,
) -> Vec<String> {
    let mut out = vec![format!("\n=== {title} ===")];
    out.push(spec.caption.into());
    let name_w = rows
        .iter()
        .map(|(n, _)| n.chars().count())
        .max()
        .unwrap_or(0)
        .max(16);
    let base = rows
        .first()
        .and_then(|(_, d)| d.as_ref())
        .filter(|_| deltas);
    for (group, columns) in spec.groups {
        out.push(format!("-- {group}"));
        let widths: Vec<usize> = columns
            .iter()
            .map(|c| {
                let cells = rows.iter().enumerate().map(|(i, (_, d))| {
                    let get = |d: Option<&JsValue>| d.and_then(|d| number_at(d, c.key));
                    value_cell(get(d.as_ref()), get(base), i == 0)
                        .chars()
                        .count()
                });
                cells.max().unwrap_or(0).max(width(c.key))
            })
            .collect();
        let mut head = vec![format!("{:<name_w$}", "")];
        head.extend(
            columns
                .iter()
                .zip(&widths)
                .map(|(c, w)| format!("{:>w$}", c.key)),
        );
        out.push(head.join(" "));
        for (i, (name, d)) in rows.iter().enumerate() {
            let get = |d: Option<&JsValue>, k: &str| d.and_then(|d| number_at(d, k));
            let mut row = vec![format!("{name:<name_w$}")];
            row.extend(columns.iter().zip(&widths).map(|(c, w)| {
                format!(
                    "{:>w$}",
                    value_cell(get(d.as_ref(), c.key), get(base, c.key), i == 0)
                )
            }));
            out.push(row.join(" "));
        }
    }
    out
}

#[cfg(test)]
#[path = "llm_diag_test.rs"]
mod llm_diag_test;
