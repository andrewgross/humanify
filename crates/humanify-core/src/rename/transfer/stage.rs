//! `applyPriorVersionIfPresent`'s orchestration after the match stage —
//! TS `src/rename/plugin.ts` (markEvalWithTaintPreDone → markWrapperPreDone
//! → detectAndMarkLibraries → applyPriorVersionIfPresent) and
//! `prior-version.ts` (applyExactMatches' state marking, the statement
//! twins computed over the settled states).
//!
//! The settle step ([`settle`]) is the ONE owner of what the statement
//! twins read: the M1 matches dump calls [`statement_twins`], the transfer
//! run [`apply_prior_version`], both through it.
//!
//! Library freezing reads comment regions, which no stage classifies yet:
//! the naming driver passes [`PreFreeze::library`] from its library HOOK
//! (`naming::driver::library::LibraryHook`, None today). The regime is
//! constructed and diverges (twins::gates' posture note), but no oracle can
//! gate it until findings #32 (the TS classifies beautified offsets against
//! raw-text regions) and #33 (the TS dump writer throws on a mixed file)
//! are fixed TS-first.

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

/// The plugin's freezes that are not the transfer stage's own decisions:
/// the library functions (`markLibraryFunctionsPreDone`, fn rows in
/// classification order) — frozen after eval-with taint and the wrapper.
#[derive(Clone, Debug, Default)]
pub struct PreFreeze {
    pub library: Vec<usize>,
}

pub(super) fn apply_prior_version(
    stage: &MatchStage<'_, '_>,
    freeze: &PreFreeze,
) -> Result<(TransferOutcome, TwinGateOutput), String> {
    let fresh_semantic = stage.fresh.ingest.semantic();
    let fresh_rows = SideRows::build(
        stage.fresh.graph,
        fresh_semantic,
        stage.fresh.tables,
        stage.fresh.json,
    );
    let Settled {
        evidence,
        fresh_state,
        fn_state,
        binding_state,
    } = settle(stage, &fresh_rows, freeze)?;
    let twin_output = gate_twins(stage, &evidence, &fn_state, &binding_state)?;
    let rows = Rows::build(stage.fresh.graph, fresh_semantic, fresh_state.view());
    let graph = stage.fresh.graph;
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
    outcome.counts.functions_matched = evidence.exact.iter().filter(|(_, p)| p.is_some()).count();
    outcome.counts.functions_already_named =
        evidence.exact.iter().filter(|(_, p)| p.is_none()).count();
    outcome.counts.close_match_count = evidence
        .close
        .iter()
        .map(|c| c.fresh_fn)
        .collect::<HashSet<usize>>()
        .len();
    outcome.matched_module_bindings = matched_module_bindings;
    outcome.carry = carry;
    Ok((outcome, twin_output))
}

/// The statement twins alone, gated over exactly the inputs
/// [`apply_prior_version`] gates them over — the M1 matches dump's
/// `twin-gates.json` reads this, so the dump and the transfer run cannot
/// answer "what do the twins see" differently (they did until 2026-09-25:
/// the dump re-derived the inputs, missing the close half of the fn-var
/// transfers and every freeze).
pub(super) fn statement_twins(stage: &MatchStage<'_, '_>) -> Result<TwinGateOutput, String> {
    let fresh_rows = SideRows::build(
        stage.fresh.graph,
        stage.fresh.ingest.semantic(),
        stage.fresh.tables,
        stage.fresh.json,
    );
    let settled = settle(stage, &fresh_rows, &PreFreeze::default())?;
    gate_twins(
        stage,
        &settled.evidence,
        &settled.fn_state,
        &settled.binding_state,
    )
}

/// What `matchPriorVersion` has settled when the statement twins run
/// (prior-version.ts :360-431): the transfer evidence — the binding
/// cascade's renames with the fn-var renames appended, exact THEN
/// corroborated close (the TS `moduleBindingRenames` ALIASES
/// `bindingCascade.renames`, so the push lands in the array the twins
/// read) — and the fresh rows' lifecycle states: the plugin's freezes,
/// then applyExactMatches' `transferred` marks on still-pending matches.
struct Settled {
    evidence: TransferEvidence,
    fresh_state: RenameState,
    fn_state: Vec<Lifecycle>,
    binding_state: Vec<Lifecycle>,
}

fn settle(
    stage: &MatchStage<'_, '_>,
    fresh_rows: &SideRows<'_, '_>,
    freeze: &PreFreeze,
) -> Result<Settled, String> {
    let fresh_state = RenameState::new(stage.fresh.ingest.semantic(), Anchor::Fresh);
    let prior_semantic = stage.prior.ingest.semantic();
    let prior_state = RenameState::new(prior_semantic, Anchor::Fresh);
    let prior_rows = SideRows::build(
        stage.prior.graph,
        prior_semantic,
        stage.prior.tables,
        stage.prior.json,
    );
    let evidence = collect_evidence(stage, fresh_rows, &prior_rows, &fresh_state, &prior_state)?;
    drop(prior_state);
    let (mut fn_state, binding_state) = pre_transfer_states(
        stage.fresh.graph,
        stage.fresh.ingest.semantic(),
        stage.fresh.wrapper.map(|w| w.span),
        freeze,
    );
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
    Ok(Settled {
        evidence,
        fresh_state,
        fn_state,
        binding_state,
    })
}

/// The plugin's freezes before the transfer stage (and before the waves
/// when there is no prior): functions on a with/direct-eval site's scope
/// chain (and every module binding when any site exists), then the
/// wrapper IIFE, then the library functions still pending.
pub fn pre_transfer_states(
    graph: &crate::graph::UnifiedGraph,
    semantic: &oxc_semantic::Semantic<'_>,
    wrapper: Option<oxc_span::Span>,
    freeze: &PreFreeze,
) -> (Vec<Lifecycle>, Vec<Lifecycle>) {
    let mut fn_state: Vec<Lifecycle> = vec![Lifecycle::Pending; graph.functions.len()];
    let mut binding_state: Vec<Lifecycle> = vec![Lifecycle::Pending; graph.module_bindings.len()];
    let taint = collect_eval_with_taint(semantic);
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
    if let Some(wrapper) = wrapper
        && let Some(i) = graph.functions.iter().position(|f| f.span == wrapper)
        && fn_state[i].is_pending()
    {
        fn_state[i].mark_skipped("wrapper-iife", &graph.functions[i].session_id);
    }
    // A library function already frozen by eval-taint keeps that reason.
    for &i in &freeze.library {
        if fn_state[i].is_pending() {
            fn_state[i].mark_skipped("library", &graph.functions[i].session_id);
        }
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
        Lifecycle::Skipped(_) | Lifecycle::LlmDone | Lifecycle::Failed => RowState::Settled,
    }
}
