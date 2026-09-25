//! The votes dump (07 §2 votes.json) — TS original: `prior-transfer.ts`
//! `recordVoteDump` / `collectVoteWitnesses` and `src/dump/write.ts`
//! `writeVotes` / `voteOutcomeBySpan`.
//!
//! Per-target tallies with witnesses, snapshotted BEFORE the ladders run;
//! the ladder OUTCOME is joined from the strategy trail at write time (the
//! last vote-ladder attempt on the target's row: module-vote, module-pin,
//! fn-name-vote, fn-name-pin, vote-suggest). Closure tallies carry totals
//! only (exact recorded as 0) and closure-capture is not a ladder tier, so
//! closure rows carry no outcome.

use std::collections::HashMap;

use humanify_model::dump::{SpanKey, VoteRow, VoteTally, VoteWitness};
use oxc_span::Span;

use crate::rename::transfer::{ExternalRef, TransferRun};
use crate::rename::validated::scopes::BindingId;
use crate::trail::{Anchor, Outcome, StrategyTrail, Tier};

use super::{Route, Votes, route};

/// One snapshotted target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteDumpRow {
    pub target_kind: &'static str,
    /// The target identifier's span in the fresh text.
    pub span: Span,
    /// (name, total, exact).
    pub tally: Vec<(String, u64, u64)>,
    /// (oldName, sourceFunctionId, exactSlot), in ref order.
    pub witnesses: Vec<(String, String, bool)>,
}

/// The classified tallies, as the dump reads them.
pub(super) struct TallyView<'t> {
    pub module: Vec<(usize, &'t Votes)>,
    pub fn_names: Vec<(BindingId, usize, &'t Votes)>,
    pub closures: Vec<(BindingId, &'t Votes)>,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
enum WitnessKey {
    Module(usize),
    Fn(usize),
    Binding(BindingId),
}

/// TS `recordVoteDump`.
pub(super) fn record_vote_dump(
    run: &mut TransferRun<'_, '_>,
    refs: &[ExternalRef],
    module_by_binding: &HashMap<BindingId, usize>,
    tallies: &TallyView<'_>,
) {
    let mut witnesses: HashMap<WitnessKey, Vec<(String, String, bool)>> = HashMap::new();
    for r in refs {
        let key = match route(run, module_by_binding, r) {
            Route::Module(row) => WitnessKey::Module(row),
            Route::FnDecl(Some(f)) => WitnessKey::Fn(f),
            Route::FnDecl(None) => continue,
            Route::Other => WitnessKey::Binding(r.binding),
        };
        witnesses.entry(key).or_default().push((
            r.old_name.clone(),
            r.source_function_id.clone(),
            r.exact_slot,
        ));
    }
    let take = |key: WitnessKey| witnesses.get(&key).cloned().unwrap_or_default();
    let mut rows = Vec::new();
    for (row, votes) in &tallies.module {
        rows.push(VoteDumpRow {
            target_kind: "module",
            span: run.rows.modules[*row].id_span,
            tally: sorted_tally(votes, true),
            witnesses: take(WitnessKey::Module(*row)),
        });
    }
    for (binding, fn_row, votes) in &tallies.fn_names {
        rows.push(VoteDumpRow {
            target_kind: "fn",
            span: run.rename.view().binding(*binding).id_span,
            tally: sorted_tally(votes, true),
            witnesses: take(WitnessKey::Fn(*fn_row)),
        });
    }
    for (binding, votes) in &tallies.closures {
        rows.push(VoteDumpRow {
            target_kind: "closure",
            span: run.rename.view().binding(*binding).id_span,
            tally: sorted_tally(votes, false),
            witnesses: take(WitnessKey::Binding(*binding)),
        });
    }
    run.votes_dump.extend(rows);
}

/// A vote map → the tally array, sorted by name (JS `<` — UTF-16 order).
fn sorted_tally(votes: &Votes, with_exact: bool) -> Vec<(String, u64, u64)> {
    let mut out: Vec<(String, u64, u64)> = votes
        .iter()
        .map(|(n, c)| (n.clone(), c.total, if with_exact { c.exact } else { 0 }))
        .collect();
    out.sort_by(|a, b| humanify_model::js::cmp_utf16(&a.0, &b.0));
    out
}

/// The vote-ladder tiers whose trail entry carries a target's outcome.
fn is_ladder_tier(tier: Tier) -> bool {
    matches!(
        tier,
        Tier::ModuleVote | Tier::ModulePin | Tier::FnNameVote | Tier::FnNamePin | Tier::VoteSuggest
    )
}

/// TS `writeVotes`: the dump rows with the ladder outcome joined from the
/// trail (fresh-anchored rows only), sorted by target span.
pub fn vote_rows(dump: &[VoteDumpRow], trail: &StrategyTrail) -> Vec<VoteRow> {
    let mut outcome_by_span: HashMap<(u32, u32), String> = HashMap::new();
    for entry in trail.entries() {
        if entry.target.anchor != Anchor::Fresh {
            continue;
        }
        let Some(last) = entry.attempts.iter().rev().find(|a| is_ladder_tier(a.tier)) else {
            continue;
        };
        let outcome = if last.outcome == Outcome::Abstained {
            format!(
                "{}:{}",
                last.outcome.as_str(),
                last.reason.as_deref().unwrap_or("")
            )
        } else {
            last.outcome.as_str().to_string()
        };
        let span = entry.target.decl_span;
        outcome_by_span.insert((span.start, span.end), outcome);
    }
    let mut rows: Vec<VoteRow> = dump
        .iter()
        .map(|row| VoteRow {
            target: SpanKey {
                text: "fresh".to_string(),
                start: i64::from(row.span.start),
                end: i64::from(row.span.end),
            },
            target_kind: row.target_kind.to_string(),
            outcome: outcome_by_span
                .get(&(row.span.start, row.span.end))
                .cloned(),
            tally: row
                .tally
                .iter()
                .map(|(name, total, exact)| VoteTally {
                    name: name.clone(),
                    total: *total,
                    exact: *exact,
                })
                .collect(),
            witnesses: row
                .witnesses
                .iter()
                .map(|(old, source, exact)| VoteWitness {
                    source_function_id: source.clone(),
                    old_name: old.clone(),
                    exact_slot: *exact,
                })
                .collect(),
        })
        .collect();
    rows.sort_by(|a, b| a.target.cmp(&b.target));
    rows
}
