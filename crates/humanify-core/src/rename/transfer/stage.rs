//! `applyPriorVersionIfPresent`'s orchestration after the match stage —
//! TS `src/rename/plugin.ts` (markEvalWithTaintPreDone → markWrapperPreDone
//! → detectAndMarkLibraries → applyPriorVersionIfPresent) and
//! `prior-version.ts` (applyExactMatches' state marking, the statement
//! twins computed over the settled states).
//!
//! Library freezing reads comment regions; the four oracle dumps record
//! none (`regions.json`), and the driver's library stage (WPB.3) supplies
//! them for real runs — until the naming driver lands (WP4.3) there is no
//! caller with regions, so the freeze is not wired here.

use std::collections::{HashMap, HashSet};

use crate::modules::soundness::collect_eval_with_taint;
use crate::prior::MatchStage;
use crate::rename::validated::RenameState;
use crate::trail::Anchor;
use crate::twins::gates::{RowState, TwinGateOutput, TwinInputs, compute_gated_statement_twins};

use super::evidence::{SideRows, TransferEvidence, collect_evidence};
use super::lifecycle::Lifecycle;
use super::rows::Rows;
use super::{TransferOutcome, TransferRun, TwinTransfers, run_transfer_pipeline};

pub(super) fn apply_prior_version(
    stage: &MatchStage<'_, '_>,
) -> Result<(TransferOutcome, TwinGateOutput), String> {
    let fresh_semantic = stage.fresh.ingest.semantic();
    let prior_semantic = stage.prior.ingest.semantic();
    let fresh_state = RenameState::new(fresh_semantic, Anchor::Fresh);
    let prior_state = RenameState::new(prior_semantic, Anchor::Fresh);
    let fresh_rows = SideRows::build(
        stage.fresh.graph,
        fresh_semantic,
        stage.fresh.tables,
        stage.fresh.json,
    );
    let prior_rows = SideRows::build(
        stage.prior.graph,
        prior_semantic,
        stage.prior.tables,
        stage.prior.json,
    );
    let evidence = collect_evidence(stage, &fresh_rows, &prior_rows, &fresh_state, &prior_state)?;
    drop(prior_state);
    let rows = Rows::build(stage.fresh.graph, fresh_semantic, fresh_state.view());
    let (mut fn_state, binding_state) = pre_transfer_states(stage);

    // applyExactMatches: a frozen function keeps its freeze; only pending
    // exact matches settle as "transferred".
    let graph = stage.fresh.graph;
    for (f, pairs) in &evidence.exact {
        if fn_state[*f].is_pending() {
            fn_state[*f].mark_transferred(
                pairs.clone().unwrap_or_default(),
                &graph.functions[*f].session_id,
            );
        }
    }

    let twin_output = gate_twins(stage, &evidence, &fn_state, &binding_state)?;
    let n_fns = graph.functions.len();
    let n_bindings = graph.module_bindings.len();
    let run = TransferRun {
        semantic: fresh_semantic,
        graph,
        rows,
        rename: fresh_state,
        fn_state,
        fn_transferred: vec![HashSet::new(); n_fns],
        fn_transferred_pairs: vec![None; n_fns],
        fn_prior_context: vec![false; n_fns],
        binding_state,
        binding_suggested: vec![None; n_bindings],
        retry_queue: Vec::new(),
        refs_twin: Vec::new(),
        refs_exact: Vec::new(),
        refs_close: Vec::new(),
        private_renames: Vec::new(),
        applied_binding_renames: Vec::new(),
        applied_module_votes: Vec::new(),
        suggestions_applied: 0,
        stats_twin: Default::default(),
        stats_exact: Default::default(),
        stats_close: Default::default(),
        stats_retry: Default::default(),
        votes_dump: Vec::new(),
    };
    let twins = TwinTransfers {
        pairs: &twin_output.pairs,
        outer_refs: &twin_output.outer_refs,
        private_renames: &twin_output.private_renames,
    };
    // collectMatchedModuleBindings: read while the rows still carry their
    // minified names (a later same-named row wins, the TS Map).
    let by_name: HashMap<&str, usize> = graph
        .module_bindings
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect();
    let matched_module_bindings: Vec<(usize, String)> = evidence
        .binding_renames
        .iter()
        .filter_map(|r| {
            by_name
                .get(r.old_name.as_str())
                .map(|&i| (i, r.new_name.clone()))
        })
        .collect();
    let spans: Vec<oxc_span::Span> = stage
        .prior
        .inventory
        .statements
        .iter()
        .map(|s| s.span)
        .collect();
    let carry = super::carry::matcher_carry(stage.prior.ingest.text, &spans);
    let mut outcome = run_transfer_pipeline(run, &evidence, &twins, &fresh_rows);
    outcome.matched_module_bindings = matched_module_bindings;
    outcome.carry = carry;
    Ok((outcome, twin_output))
}

/// The plugin's freezes before the transfer stage: functions on a
/// with/direct-eval site's scope chain (and every module binding when any
/// site exists), then the wrapper IIFE.
fn pre_transfer_states(stage: &MatchStage<'_, '_>) -> (Vec<Lifecycle>, Vec<Lifecycle>) {
    let graph = stage.fresh.graph;
    let mut fn_state: Vec<Lifecycle> = vec![Lifecycle::Pending; graph.functions.len()];
    let mut binding_state: Vec<Lifecycle> = vec![Lifecycle::Pending; graph.module_bindings.len()];
    let taint = collect_eval_with_taint(stage.fresh.ingest.semantic());
    if taint.site_count > 0 {
        let tainted: HashSet<(u32, u32)> = taint
            .tainted_functions
            .iter()
            .map(|s| (s.start, s.end))
            .collect();
        for (i, f) in graph.functions.iter().enumerate() {
            if tainted.contains(&(f.span.start, f.span.end)) {
                fn_state[i].mark_skipped("eval-with-taint", &f.session_id);
            }
        }
        if taint.module_tainted {
            for (i, b) in graph.module_bindings.iter().enumerate() {
                binding_state[i].mark_skipped("eval-with-taint", &b.session_id);
            }
        }
    }
    if let Some(wrapper) = stage.fresh.wrapper
        && let Some(i) = graph.functions.iter().position(|f| f.span == wrapper.span)
        && fn_state[i].is_pending()
    {
        fn_state[i].mark_skipped("wrapper-iife", &graph.functions[i].session_id);
    }
    (fn_state, binding_state)
}

/// `computeStatementTwinTransfers` over the settled states: the claimed
/// set is the binding CASCADE's old names (snapshotted before the var-name
/// transfers were appended), the identity pairs the whole rename list.
fn gate_twins(
    stage: &MatchStage<'_, '_>,
    evidence: &TransferEvidence,
    fn_state: &[Lifecycle],
    binding_state: &[Lifecycle],
) -> Result<TwinGateOutput, String> {
    let graph = stage.fresh.graph;
    let claimed: HashSet<String> = evidence.binding_renames[..evidence.cascade_renames]
        .iter()
        .map(|r| r.old_name.clone())
        .collect();
    let identity_pairs: Vec<(String, String)> = evidence
        .binding_renames
        .iter()
        .map(|r| (r.old_name.clone(), r.new_name.clone()))
        .collect();
    let fn_states: HashMap<String, RowState> = graph
        .functions
        .iter()
        .zip(fn_state)
        .map(|(f, s)| (f.session_id.clone(), row_state(s)))
        .collect();
    let binding_states: HashMap<String, RowState> = graph
        .module_bindings
        .iter()
        .zip(binding_state)
        .map(|(b, s)| (b.session_id.clone(), row_state(s)))
        .collect();
    compute_gated_statement_twins(
        &stage.prior.gate_side(),
        &stage.fresh.gate_side(),
        &TwinInputs {
            fn_matches: &evidence.fn_matches,
            claimed_old_names: &claimed,
            binding_identity_pairs: &identity_pairs,
            fn_states: &fn_states,
            binding_states: &binding_states,
        },
    )
}

fn row_state(s: &Lifecycle) -> RowState {
    match s {
        Lifecycle::Pending => RowState::Pending,
        Lifecycle::Transferred(_) => RowState::ExactMatched,
        Lifecycle::Skipped(_) => RowState::Settled,
    }
}
