//! The votes record (the dump's votes section): per-target tallies with
//! witnesses, snapshotted BEFORE the ladders run. The ladder's outcome is
//! the target's row in the transfers section. Closure tallies carry
//! totals only (exact recorded as 0).

use std::collections::HashMap;

use oxc_span::Span;

use crate::rename::transfer::{ExternalRef, TransferRun};
use crate::rename::validated::scopes::BindingId;

use super::{Route, Votes, route};

/// One snapshotted target.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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

/// A vote map → the tally array, sorted by name.
fn sorted_tally(votes: &Votes, with_exact: bool) -> Vec<(String, u64, u64)> {
    let mut out: Vec<(String, u64, u64)> = votes
        .iter()
        .map(|(n, c)| (n.clone(), c.total, if with_exact { c.exact } else { 0 }))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
