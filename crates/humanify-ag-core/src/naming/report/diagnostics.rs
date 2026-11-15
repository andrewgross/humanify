//! `buildDiagnosticsReport` (src/rename/diagnostics.ts) — the
//! `--diagnostics` JSON: per-identifier outcomes from the rename reports,
//! the strategy trail with its funnel (the TS `StrategyTrailReport` shape:
//! raw UTF-16 declaration spans and Babel `line:col` locs in each row's
//! anchored text), the contention events, the totals-first identifier
//! ledger and the pattern roll-ups. `placementTrails` is the split's
//! (WP5.x) and is appended by it.

use std::collections::HashMap;

use humanify_ag_model::js::math_round;
use humanify_ag_model::ordered::OrderedMap;
use humanify_ag_model::stats::{CoverageSummary, TransferStatsByTier};
use serde::Serialize;

use super::{ContentionEvent, IdentifierOutcome, RenameReport, Status};
use crate::babel_view::BabelLines;
use crate::trail::{Anchor, StrategyTrail, TrailEntry};

/// The anchored texts a trail row's spans index into.
#[derive(Clone, Copy, Default)]
pub struct AnchorTexts<'t> {
    pub fresh: &'t str,
    pub generated: Option<&'t str>,
    pub reconciled: Option<&'t str>,
    pub shipped: Option<&'t str>,
}

impl<'t> AnchorTexts<'t> {
    fn text(&self, anchor: Anchor) -> &'t str {
        match anchor {
            Anchor::Fresh => Some(self.fresh),
            Anchor::Generated => self.generated,
            Anchor::Reconciled => self.reconciled,
            Anchor::Shipped => self.shipped,
        }
        .unwrap_or(self.fresh)
    }
}

/// Byte → UTF-16 offsets of one text, for a known set of positions.
struct Utf16Map(HashMap<u32, u32>);

impl Utf16Map {
    fn build(text: &str, mut positions: Vec<u32>) -> Utf16Map {
        positions.sort_unstable();
        positions.dedup();
        let mut map = HashMap::with_capacity(positions.len());
        let (mut byte, mut units) = (0usize, 0u32);
        let mut chars = text.chars();
        for p in positions {
            while byte < p as usize {
                let Some(c) = chars.next() else { break };
                byte += c.len_utf8();
                units += c.len_utf16() as u32;
            }
            map.insert(p, units);
        }
        Utf16Map(map)
    }

    fn get(&self, p: u32) -> u32 {
        self.0[&p]
    }
}

/// One attempt of a report entry's trail.
#[derive(Clone, Serialize)]
struct AttemptRow {
    round: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    proposed: Option<String>,
    result: &'static str,
}

/// One per-identifier outcome row: the outcome's own fields, then the
/// report's shared provenance (`functionId`, `strategy`, `structuralHash`,
/// `trail`).
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeRow {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    round: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attempts: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
    function_id: String,
    strategy: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    structural_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    trail: Option<Vec<AttemptRow>>,
}

/// A row carrying `report`'s provenance, the outcome fields left empty.
fn provenance(report: &RenameReport, outcome: &IdentifierOutcome, name: &str) -> OutcomeRow {
    OutcomeRow {
        name: name.to_string(),
        function_id: report.target_id.clone(),
        strategy: report.strategy.as_str(),
        structural_hash: report.structural_hash.clone(),
        trail: outcome.trail.as_ref().map(|t| {
            t.iter()
                .map(|a| AttemptRow {
                    round: a.round,
                    proposed: a.proposed.clone(),
                    result: a.result.as_str(),
                })
                .collect()
        }),
        ..OutcomeRow::default()
    }
}

/// `Map<K, number>` bump in insertion order.
fn bump<K: PartialEq + Clone>(m: &mut Vec<(K, u64)>, k: &K) {
    match m.iter_mut().find(|(x, _)| x == k) {
        Some((_, n)) => *n += 1,
        None => m.push((k.clone(), 1)),
    }
}

#[derive(Default)]
struct Buckets {
    unchanged: Vec<OutcomeRow>,
    missing: Vec<OutcomeRow>,
    duplicate: Vec<OutcomeRow>,
    invalid: Vec<OutcomeRow>,
    renamed: Vec<OutcomeRow>,
    renamed_names: Vec<String>,
    unrenamed_names: [Vec<String>; 4],
    collision_targets: Vec<(String, u64)>,
    unchanged_names: Vec<String>,
    by_attempts: Vec<(u64, u64)>,
    missing_by_finish: Vec<(String, u64)>,
}

fn classify(reports: &[RenameReport]) -> Buckets {
    let mut b = Buckets::default();
    for report in reports {
        for (name, outcome) in report.outcomes.iter() {
            let row = provenance(report, outcome, name);
            match &outcome.status {
                Status::Renamed { new_name, round } => {
                    b.renamed.push(OutcomeRow {
                        new_name: Some(new_name.clone()),
                        round: Some(*round),
                        ..row
                    });
                    b.renamed_names.push(new_name.clone());
                }
                Status::Unchanged {
                    attempts,
                    suggestion,
                } => {
                    b.unchanged.push(OutcomeRow {
                        suggestion: suggestion.clone(),
                        reason: Some("LLM returned original name"),
                        attempts: Some(*attempts),
                        ..row
                    });
                    b.unchanged_names.push(name.clone());
                    b.unrenamed_names[0].push(name.clone());
                    bump(&mut b.by_attempts, attempts);
                }
                Status::Missing {
                    attempts,
                    last_finish_reason,
                } => {
                    b.missing.push(OutcomeRow {
                        reason: Some("LLM did not return this identifier"),
                        attempts: Some(*attempts),
                        detail: last_finish_reason
                            .as_deref()
                            .filter(|r| !r.is_empty())
                            .map(|r| format!("finish_reason: {r}")),
                        ..row
                    });
                    b.unrenamed_names[1].push(name.clone());
                    bump(&mut b.by_attempts, attempts);
                    let fr = last_finish_reason
                        .clone()
                        .unwrap_or_else(|| "unknown".into());
                    bump(&mut b.missing_by_finish, &fr);
                }
                Status::Duplicate {
                    conflicted_with,
                    attempts,
                    suggestion,
                } => {
                    b.duplicate.push(OutcomeRow {
                        suggestion: suggestion.clone(),
                        reason: Some("Name collision unresolved"),
                        attempts: Some(*attempts),
                        detail: Some(format!("conflicted with: {conflicted_with}")),
                        ..row
                    });
                    b.unrenamed_names[2].push(name.clone());
                    bump(&mut b.collision_targets, conflicted_with);
                    bump(&mut b.by_attempts, attempts);
                }
                Status::Invalid {
                    attempts,
                    suggestion,
                } => {
                    b.invalid.push(OutcomeRow {
                        suggestion: suggestion.clone(),
                        reason: Some("Invalid identifier returned"),
                        attempts: Some(*attempts),
                        ..row
                    });
                    b.unrenamed_names[3].push(name.clone());
                    bump(&mut b.by_attempts, attempts);
                }
            }
        }
    }
    b
}

/// Rows recorded into the run's trail by a pass over ANOTHER text than
/// the four anchored ones (the post-split reconcile, per split file).
#[derive(Clone, Debug, Default)]
pub struct ExtraText {
    /// The split file's tree-relative path — the rows' text label (07 §1's
    /// path key space; finding #50: these rows used to say "generated").
    pub file: String,
    /// The text the rows' spans index (the file as the pass parsed it).
    pub text: String,
    /// The rows, in record order.
    pub rows: Vec<TrailEntry>,
}

/// Every extra text's rows, in record order.
pub type ExtraTrail = [ExtraText];

/// The strategy trail as `StrategyTrailReport` (`trails` + `funnel`): the
/// run's rows, then the extra rows (the TS's one trail keyed by binding
/// NODE: a later pass's rows are new entries, appended), each converted in
/// its own text.
pub fn trail_report(
    trail: &StrategyTrail,
    texts: &AnchorTexts<'_>,
    extra: &ExtraTrail,
) -> TrailReport {
    let anchors = [
        Anchor::Fresh,
        Anchor::Generated,
        Anchor::Reconciled,
        Anchor::Shipped,
    ];
    let mut maps: Vec<(Anchor, Utf16Map, BabelLines<'_>)> = Vec::new();
    for anchor in anchors {
        let text = texts.text(anchor);
        let positions =
            entry_positions(trail.entries().iter().filter(|e| e.target.anchor == anchor));
        if positions.is_empty() {
            continue;
        }
        maps.push((
            anchor,
            Utf16Map::build(text, positions),
            BabelLines::new(text),
        ));
    }
    let mut trails: Vec<TrailRow> = trail
        .entries()
        .iter()
        .map(|e| {
            let (_, map, lines) = maps
                .iter()
                .find(|(a, _, _)| *a == e.target.anchor)
                .expect("an anchored text for every row");
            trail_entry(e, e.target.anchor.as_str(), map, lines)
        })
        .collect();
    for x in extra {
        let map = Utf16Map::build(&x.text, entry_positions(x.rows.iter()));
        let lines = BabelLines::new(&x.text);
        trails.extend(x.rows.iter().map(|e| trail_entry(e, &x.file, &map, &lines)));
    }
    let funnel = funnel_of(all_entries(trail, extra))
        .into_iter()
        .map(|(tier, outcomes)| {
            let row = outcomes
                .into_iter()
                .map(|(outcome, n)| (outcome.as_str().to_string(), n))
                .collect();
            (tier.as_str().to_string(), OrderedMap(row))
        })
        .collect();
    TrailReport {
        trails,
        funnel: OrderedMap(funnel),
    }
}

/// The strategy trail (`trails` + `funnel`: per tier, per outcome, the
/// attempt count, both in first-seen order).
#[derive(Serialize)]
pub struct TrailReport {
    pub trails: Vec<TrailRow>,
    pub funnel: OrderedMap<OrderedMap<u64>>,
}

/// One trail row; `declText` labels the text its spans index (an anchor,
/// or a split file's path). Spans are UTF-16 offsets.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailRow {
    pub old_name: String,
    pub loc: String,
    pub decl_span: Utf16Span,
    pub decl_text: String,
    pub trail: Vec<TrailAttempt>,
    pub post_settle_attempts: u32,
    pub post_settle_votes: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settled_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_by: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_name: Option<String>,
}

#[derive(Serialize)]
pub struct Utf16Span {
    pub start: u32,
    pub end: u32,
}

/// One attempt of a trail row.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailAttempt {
    pub strategy: &'static str,
    pub outcome: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_block: Option<String>,
}

/// Every position a row's report reads (declaration + scope blocks).
fn entry_positions<'e>(entries: impl Iterator<Item = &'e TrailEntry>) -> Vec<u32> {
    let mut positions: Vec<u32> = Vec::new();
    for e in entries {
        positions.push(e.target.decl_span.start);
        positions.push(e.target.decl_span.end);
        for a in &e.attempts {
            if let Some(s) = a.scope_block {
                positions.push(s.start);
                positions.push(s.end);
            }
        }
    }
    positions
}

/// The run's rows then the extra rows, in record order.
fn all_entries<'e>(
    trail: &'e StrategyTrail,
    extra: &'e ExtraTrail,
) -> impl Iterator<Item = &'e TrailEntry> + Clone {
    trail
        .entries()
        .iter()
        .chain(extra.iter().flat_map(|x| x.rows.iter()))
}

/// `report().funnel`: per tier (first-seen order), per outcome (first-seen
/// order), the attempt count.
fn funnel_of<'e>(
    entries: impl Iterator<Item = &'e TrailEntry>,
) -> Vec<(crate::trail::Tier, Vec<(crate::trail::Outcome, u64)>)> {
    let mut funnel: Vec<(crate::trail::Tier, Vec<(crate::trail::Outcome, u64)>)> = Vec::new();
    for attempt in entries.flat_map(|e| &e.attempts) {
        let at = match funnel.iter().position(|(t, _)| *t == attempt.tier) {
            Some(i) => i,
            None => {
                funnel.push((attempt.tier, Vec::new()));
                funnel.len() - 1
            }
        };
        let row = &mut funnel[at].1;
        match row.iter_mut().find(|(o, _)| *o == attempt.outcome) {
            Some((_, n)) => *n += 1,
            None => row.push((attempt.outcome, 1)),
        }
    }
    funnel
}

/// One trail row, its spans converted to UTF-16 offsets in `text`.
fn trail_entry(e: &TrailEntry, text: &str, map: &Utf16Map, lines: &BabelLines<'_>) -> TrailRow {
    let (line, col) = lines.loc(e.target.decl_span.start);
    TrailRow {
        old_name: e.old_name.clone(),
        loc: format!("{line}:{col}"),
        decl_span: Utf16Span {
            start: map.get(e.target.decl_span.start),
            end: map.get(e.target.decl_span.end),
        },
        decl_text: text.to_string(),
        trail: e
            .attempts
            .iter()
            .map(|a| TrailAttempt {
                strategy: a.tier.as_str(),
                outcome: a.outcome.as_str(),
                reason: a.reason.clone(),
                new_name: a.proposed_name.clone(),
                ref_count: a.ref_count,
                scope_block: a
                    .scope_block
                    .map(|s| format!("{}:{}", map.get(s.start), map.get(s.end))),
            })
            .collect(),
        post_settle_attempts: e.post_settle_attempts,
        post_settle_votes: e.post_settle_votes,
        settled_by: e.settled_by.map(|s| s.as_str()),
        terminal_by: e.terminal_by.map(|t| t.as_str()),
        final_name: e.terminal_by.and(e.final_name.clone()),
    }
}

/// `creditPools` + `buildTerminalState` + `buildIdentifierLedger`.
fn identifier_ledger<'e>(
    coverage: &CoverageSummary,
    entries: impl Iterator<Item = &'e TrailEntry> + Clone,
    renamed: &[String],
    unrenamed: &[String],
) -> IdentifierLedger {
    let mut transfer_settled: Vec<(String, u64)> = Vec::new();
    let mut named_by_tier: Vec<(String, u64)> = Vec::new();
    let mut credits: Vec<(String, u64)> = Vec::new();
    for e in entries.clone() {
        if let Some(s) = e.settled_by {
            bump(&mut transfer_settled, &s.as_str().to_string());
        }
        if let Some(t) = e.terminal_by.or(e.settled_by) {
            bump(&mut named_by_tier, &t.as_str().to_string());
        }
    }
    let mut credit_map: HashMap<String, u64> = HashMap::new();
    for e in entries {
        let last = e
            .attempts
            .iter()
            .rev()
            .find(|a| a.outcome == crate::trail::Outcome::Applied);
        match last
            .and_then(|a| a.proposed_name.as_deref())
            .filter(|n| !n.is_empty())
        {
            Some(n) => *credit_map.entry(n.to_string()).or_default() += 1,
            None if !e.attempts.is_empty() => {
                *credit_map.entry(e.old_name.clone()).or_default() += 1;
            }
            None => {}
        }
    }
    for n in renamed.iter().chain(unrenamed) {
        *credit_map.entry(n.clone()).or_default() += 1;
    }
    credits.clear();
    let census = coverage.minted_census.as_ref();
    let mut accounted = 0u64;
    let mut unaccounted: Vec<String> = Vec::new();
    for name in census.and_then(|c| c.names.clone()).unwrap_or_default() {
        match credit_map.get_mut(&name) {
            Some(n) if *n > 0 => {
                *n -= 1;
                accounted += 1;
            }
            _ => unaccounted.push(name),
        }
    }
    let total = census.and_then(|c| c.total_bindings);
    let ids = &coverage.identifiers;
    IdentifierLedger {
        total_bindings: total,
        transfer_settled: OrderedMap(transfer_settled),
        llm_named: ids.llm,
        library_prefix: ids.library_prefix,
        fallback: ids.fallback,
        not_renamed: ids.not_renamed,
        remaining_minted: census.map(|c| c.total),
        terminal_state: TerminalState {
            total_bindings: total,
            named_by_tier: OrderedMap(named_by_tier),
            llm_named: ids.llm,
            minted_accounted: accounted,
            minted_unaccounted: unaccounted,
        },
    }
}

/// The totals-first identifier ledger.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifierLedger {
    #[serde(skip_serializing_if = "Option::is_none")]
    total_bindings: Option<u64>,
    transfer_settled: OrderedMap<u64>,
    llm_named: u64,
    library_prefix: u64,
    fallback: u64,
    not_renamed: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    remaining_minted: Option<u64>,
    terminal_state: TerminalState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalState {
    #[serde(skip_serializing_if = "Option::is_none")]
    total_bindings: Option<u64>,
    named_by_tier: OrderedMap<u64>,
    llm_named: u64,
    minted_accounted: u64,
    minted_unaccounted: Vec<String>,
}

/// The pattern roll-ups.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Patterns {
    top_collision_targets: Vec<CollisionTarget>,
    unchanged_identifiers: Vec<String>,
    lowest_coverage_functions: Vec<FunctionCoverage>,
    /// Keyed by attempt count, ascending.
    failures_by_attempts: OrderedMap<u64>,
    missing_by_finish_reason: OrderedMap<u64>,
}

#[derive(Serialize)]
struct CollisionTarget {
    name: String,
    count: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FunctionCoverage {
    function_id: String,
    total: u64,
    renamed: u64,
    pct: f64,
}

fn patterns(reports: &[RenameReport], b: &Buckets) -> Patterns {
    let mut targets = b.collision_targets.clone();
    targets.sort_by_key(|x| std::cmp::Reverse(x.1));
    let mut lowest: Vec<FunctionCoverage> = reports
        .iter()
        .filter(|r| r.total_identifiers > 0)
        .map(|r| FunctionCoverage {
            function_id: r.target_id.clone(),
            total: r.total_identifiers as u64,
            renamed: r.renamed_count as u64,
            pct: math_round(r.renamed_count as f64 / r.total_identifiers as f64 * 100.0),
        })
        .collect();
    lowest.sort_by(|x, y| x.pct.total_cmp(&y.pct));
    lowest.truncate(20);
    let mut by_attempts = b.by_attempts.clone();
    by_attempts.sort_by_key(|(a, _)| *a);
    Patterns {
        top_collision_targets: targets
            .into_iter()
            .take(20)
            .map(|(name, count)| CollisionTarget { name, count })
            .collect(),
        unchanged_identifiers: b.unchanged_names.clone(),
        lowest_coverage_functions: lowest,
        failures_by_attempts: OrderedMap(
            by_attempts
                .into_iter()
                .map(|(a, n)| (a.to_string(), n))
                .collect(),
        ),
        missing_by_finish_reason: OrderedMap(b.missing_by_finish.clone()),
    }
}

/// What `buildDiagnosticsReport` reads.
pub struct DiagnosticsInputs<'a, 't> {
    pub timestamp: String,
    pub reports: &'a [RenameReport],
    pub coverage: &'a CoverageSummary,
    pub transfer_stats: Option<&'a TransferStatsByTier>,
    pub trail: &'a StrategyTrail,
    pub texts: AnchorTexts<'t>,
    pub contention: &'a [ContentionEvent],
    /// The post-split reconcile's rows (per split file), appended.
    pub extra_trail: &'a ExtraTrail,
}

/// The `--diagnostics` record. The CLI sets `placement_trails` (the
/// split's) and appends its run-level blocks to `extra`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport<'a> {
    pub timestamp: String,
    pub coverage: &'a CoverageSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_stats: Option<&'a TransferStatsByTier>,
    pub unrenamed: Unrenamed,
    pub renamed: Vec<OutcomeRow>,
    pub strategy_trails: TrailReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placement_trails: Option<serde_json::Value>,
    pub name_contention: NameContention,
    pub identifier_ledger: IdentifierLedger,
    pub patterns: Patterns,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// The identifiers left unrenamed, by outcome.
#[derive(Serialize)]
pub struct Unrenamed {
    pub unchanged: Vec<OutcomeRow>,
    pub missing: Vec<OutcomeRow>,
    pub duplicate: Vec<OutcomeRow>,
    pub invalid: Vec<OutcomeRow>,
}

#[derive(Serialize)]
pub struct NameContention {
    pub events: Vec<ContentionRow>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentionRow {
    requested: String,
    resolved_to: String,
    old_name: String,
    site: &'static str,
}

/// `buildDiagnosticsReport`.
pub fn build_diagnostics_report<'a>(inp: &DiagnosticsInputs<'a, '_>) -> DiagnosticsReport<'a> {
    let b = classify(inp.reports);
    let unrenamed_names: Vec<String> = b.unrenamed_names.iter().flatten().cloned().collect();
    let events = inp
        .contention
        .iter()
        .map(|e| ContentionRow {
            requested: e.requested.clone(),
            resolved_to: e.resolved_to.clone(),
            old_name: e.old_name.clone(),
            site: e.site,
        })
        .collect();
    let identifier_ledger = identifier_ledger(
        inp.coverage,
        all_entries(inp.trail, inp.extra_trail),
        &b.renamed_names,
        &unrenamed_names,
    );
    let patterns = patterns(inp.reports, &b);
    DiagnosticsReport {
        timestamp: inp.timestamp.clone(),
        coverage: inp.coverage,
        transfer_stats: inp.transfer_stats,
        strategy_trails: trail_report(inp.trail, &inp.texts, inp.extra_trail),
        unrenamed: Unrenamed {
            unchanged: b.unchanged,
            missing: b.missing,
            duplicate: b.duplicate,
            invalid: b.invalid,
        },
        renamed: b.renamed,
        placement_trails: None,
        name_contention: NameContention { events },
        identifier_ledger,
        patterns,
        extra: serde_json::Map::new(),
    }
}
