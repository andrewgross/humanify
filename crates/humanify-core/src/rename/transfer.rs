//! The prior-version transfer stage (WP3.2) — TS original:
//! `src/rename/prior-transfer.ts` (`applyPriorVersionIfPresent` →
//! `TRANSFER_PIPELINE`), plus the rename half of
//! `src/prior-version/prior-version.ts` ([`evidence`]),
//! `src/rename/function-bindings.ts` ([`owned`]) and
//! `src/rename/lifecycle.ts` ([`lifecycle`]).
//!
//! Applies the names recovered from a matched prior humanified version
//! BEFORE the LLM waves, in evidence-strength order — stronger evidence
//! first, weaker tiers fill only what remains, every rename validated
//! (collisions reject; no tier overwrites another):
//!
//! 1. `statement-twin` — unique 1:1 statement-hash twins' bridged slots;
//! 2. `exact-match` — exact-matched functions' slot tables;
//! 3. `close-match` — close-matched functions' signature + aligned-body
//!    pairs;
//! 4. `binding-cascade` — module-binding renames from the binding cascade
//!    and the function var-name transfers;
//! 5. `vote-propagation` — external-reference testimony
//!    ([`crate::rename::votes`]): module bindings, cold function heads,
//!    closure captures;
//! 6. `close-match-suggestions` — set elimination as LLM hints only;
//! 7. `retry` — deferred re-attempt of collision-rejected renames
//!    ([`retry`]).
//!
//! Every name application goes through [`RenameState`]'s validated
//! applier, and every attempt lands on the strategy trail — the phase-3
//! gate (`transfers-mechanical.json`) compares that trail row for row.
//!
//! ORDER is decision input throughout (15-porting-lessons §4): the tiers
//! iterate graph-row order, match order (the cascades' `MatchMap`), close
//! assignment order and first-reference order, exactly as the TS Maps
//! iterate — which rename claims a contested name first decides who is
//! rejected and who lands through the retry pass.

pub mod carry;
pub mod dump;
pub mod evidence;
pub mod lifecycle;
pub mod owned;
pub mod retry;
pub mod rows;
mod stage;
pub mod tiers;

#[cfg(test)]
mod transfer_test;

use std::collections::HashSet;

use oxc_semantic::Semantic;

use crate::graph::UnifiedGraph;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::twins::gates::{PrivateRenameSet, TwinTransferPair};

use evidence::TransferEvidence;
use lifecycle::Lifecycle;
use retry::RejectedTransfer;
use rows::Rows;

/// Per-tier transfer counters (TS `TransferStats`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferStats {
    pub attempted: u64,
    pub applied: u64,
    pub skipped: u64,
}

/// One external-reference vote (TS `ExternalRefPair`): a matched
/// function's slot names an OUTSIDE binding — testimony for vote
/// propagation, never applied by the producing tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalRef {
    /// The minified name in the new version.
    pub old_name: String,
    /// The prior version's name.
    pub new_name: String,
    /// The producing function's session id ("statement-twin" for twins).
    pub source_function_id: String,
    /// The binding the old name resolves to — votes key by identity.
    pub binding: BindingId,
    /// Exact-match slot testimony (strong enough to pin alone).
    pub exact_slot: bool,
}

/// The mutable state the pipeline threads (TS `TransferContext` + the
/// graph nodes' mutable fields).
pub struct TransferRun<'a, 's> {
    pub semantic: &'a Semantic<'s>,
    pub graph: &'a UnifiedGraph,
    pub rows: Rows,
    pub rename: RenameState,
    /// Per function row: lifecycle state.
    pub fn_state: Vec<Lifecycle>,
    /// Per function row: `fn.priorVersionTransferred`.
    pub fn_transferred: Vec<HashSet<String>>,
    /// Per function row: `fn.priorVersionTransferredPairs` (the applied
    /// close-match pairs — first-round prompt context, WP4).
    pub fn_transferred_pairs: Vec<Option<Vec<(String, String)>>>,
    /// Per function row: `fn.priorVersionContext` is set (a pending
    /// close-matched function).
    pub fn_prior_context: Vec<bool>,
    /// Per module-binding row: lifecycle state.
    pub binding_state: Vec<Lifecycle>,
    /// Per module-binding row: `suggestedName` (an LLM hint).
    pub binding_suggested: Vec<Option<String>>,
    pub retry_queue: Vec<RejectedTransfer>,
    /// External-reference testimony per producing step.
    pub refs_twin: Vec<ExternalRef>,
    pub refs_exact: Vec<ExternalRef>,
    pub refs_close: Vec<ExternalRef>,
    /// The twins' private-name rewrites (not scope bindings — the render
    /// applies them, WP5.3).
    pub private_renames: Vec<PrivateRenameSet>,
    /// Applied binding-cascade renames (old → new), in apply order.
    pub applied_binding_renames: Vec<(String, String)>,
    /// Module renames applied by the vote ladders (old → new).
    pub applied_module_votes: Vec<(String, String)>,
    pub suggestions_applied: u64,
    pub stats_twin: TransferStats,
    pub stats_exact: TransferStats,
    pub stats_close: TransferStats,
    pub stats_retry: TransferStats,
    /// The votes dump rows (snapshotted before the ladders run).
    pub votes_dump: Vec<votes_dump::VoteDumpRow>,
}

pub use crate::rename::votes::dump as votes_dump;

/// The statement-twin tier's inputs (the gated twins' output).
pub struct TwinTransfers<'t> {
    pub pairs: &'t [TwinTransferPair],
    pub outer_refs: &'t [TwinTransferPair],
    pub private_renames: &'t [PrivateRenameSet],
}

impl<'a, 's> TransferRun<'a, 's> {
    /// TS `registerTransferredWithOwner`: the function owning the scope
    /// must not re-rename the transferred name in its LLM pass.
    pub fn register_with_owner(&mut self, scope: BScopeId, new_name: &str) {
        if let Some(f) = self.rows.owner_fn_of_scope(self.rename.view(), scope) {
            self.fn_transferred[f].insert(new_name.to_string());
        }
    }

    /// TS `settleModuleBindingNode`: settle the graph node keyed by the
    /// binding's ORIGINAL name.
    pub fn settle_module_binding_node(&mut self, old_name: &str) {
        if let Some(&i) = self.rows.module_by_name.get(old_name)
            && self.binding_state[i].is_pending()
        {
            self.binding_state[i].mark_skipped("prior-version-match", old_name);
        }
    }

    /// Mark a module-binding row `propagated` when still pending.
    pub fn mark_binding_propagated(&mut self, row: usize) {
        if self.binding_state[row].is_pending() {
            let name = self.rows.modules[row].name.clone();
            self.binding_state[row].mark_skipped("propagated", &name);
        }
    }
}

/// What the stage hands on.
pub struct TransferOutcome {
    pub rename: RenameState,
    pub fn_state: Vec<Lifecycle>,
    pub fn_transferred: Vec<HashSet<String>>,
    /// Per function row: `fn.priorVersionTransferredPairs` (first-round
    /// `alreadyRenamed` context of the waves).
    pub fn_transferred_pairs: Vec<Option<Vec<(String, String)>>>,
    /// Per function row: the close-matched PRIOR function's session id
    /// when `fn.priorVersionContext` is set (a pending close match).
    pub fn_close_prior: Vec<Option<String>>,
    pub binding_state: Vec<Lifecycle>,
    pub binding_suggested: Vec<Option<String>>,
    pub private_renames: Vec<PrivateRenameSet>,
    pub votes_dump: Vec<votes_dump::VoteDumpRow>,
    pub stats_twin: TransferStats,
    pub stats_exact: TransferStats,
    pub stats_close: TransferStats,
    pub stats_retry: TransferStats,
    /// TS `matchedModuleBindings`: (module row, the prior name it matched)
    /// for every binding rename — the split's identity map reads the row's
    /// FINAL name once every pass has run ([`carry::build_prior_match_map`]).
    pub matched_module_bindings: Vec<(usize, String)>,
    /// What the match collected for the split (TS `MatcherCarry`).
    pub carry: carry::MatcherCarry,
}

/// One named, documented pass of the mechanical transfer phase (TS
/// `TransferStep`; `name` matches the strategy-trail label; the
/// description renders into docs/naming-pipeline.md).
pub struct TransferStep {
    pub name: &'static str,
    pub description: &'static str,
}

/// TS `TRANSFER_PIPELINE`: the mechanical transfer phase, in
/// evidence-strength order (descriptions verbatim from the TS registry).
pub const TRANSFER_PIPELINE: [TransferStep; 7] = [
    TransferStep {
        name: "statement-twin",
        description: "Unique 1:1 statementHash twins are whole-statement identity (literals included) — bridged slots outrank ordinal exact matches crossed by bundle reorders and close-match guesses; includes positional PrivateName rewrites, outer slots become demoted vote testimony.",
    },
    TransferStep {
        name: "exact-match",
        description: "Exact-matched functions' slot tables (byte-identical modulo names) rename params and locals to prior names; references to outside bindings route to vote propagation with exact-grade testimony.",
    },
    TransferStep {
        name: "close-match",
        description: "Close-matched functions transfer positional signature pairs and statement-aligned body locals; head names stay LLM suggestions (content changed), externals route to votes at non-exact grade.",
    },
    TransferStep {
        name: "binding-cascade",
        description: "Module-binding renames matched by the cascade (same tiers as functions, alternating rounds, identity-corroborated) apply through validated renames.",
    },
    TransferStep {
        name: "vote-propagation",
        description: "External-reference testimony names what nothing hash-based can: module bindings and cold function heads at a >=2 agreeing-vote floor, the single-vote pin ladder below it (exact testimony + injectivity + role corroboration), and closure captures.",
    },
    TransferStep {
        name: "close-match-suggestions",
        description: "Resolved binding names from every earlier pass are injected into close-match LLM context as suggestions — hints, never mechanical renames.",
    },
    TransferStep {
        name: "retry",
        description: "Deferred re-attempt of collision-rejected renames from every pass — swaps and chains unwind as earlier phases free tokens, pure cycles break via a temp name.",
    },
];

/// Run `TRANSFER_PIPELINE` in registry order over a prepared run.
pub fn run_transfer_pipeline(
    mut run: TransferRun<'_, '_>,
    evidence: &TransferEvidence,
    twins: &TwinTransfers<'_>,
    fresh_rows: &evidence::SideRows<'_, '_>,
) -> TransferOutcome {
    for step in &TRANSFER_PIPELINE {
        match step.name {
            "statement-twin" => tiers::apply_statement_twin_transfers(&mut run, twins),
            "exact-match" => tiers::apply_matched_renames(&mut run),
            "close-match" => tiers::attach_close_match_context(&mut run, evidence),
            "binding-cascade" => tiers::apply_module_binding_renames(&mut run, evidence),
            "vote-propagation" => {
                crate::rename::votes::propagate_external_references(&mut run, evidence, fresh_rows);
            }
            "close-match-suggestions" => {
                tiers::suggest_from_close_match_externals(&mut run, evidence)
            }
            "retry" => {
                let queue = std::mem::take(&mut run.retry_queue);
                run.stats_retry = retry::retry_rejected_transfers(&mut run, queue);
            }
            other => unreachable!("unregistered transfer step {other}"),
        }
    }
    let mut fn_close_prior: Vec<Option<String>> = vec![None; run.fn_state.len()];
    for info in &evidence.close {
        if run.fn_prior_context[info.fresh_fn] && fn_close_prior[info.fresh_fn].is_none() {
            fn_close_prior[info.fresh_fn] = Some(info.prior_id.clone());
        }
    }
    TransferOutcome {
        rename: run.rename,
        fn_state: run.fn_state,
        fn_transferred: run.fn_transferred,
        fn_transferred_pairs: run.fn_transferred_pairs,
        fn_close_prior,
        binding_state: run.binding_state,
        binding_suggested: run.binding_suggested,
        private_renames: run.private_renames,
        votes_dump: run.votes_dump,
        stats_twin: run.stats_twin,
        stats_exact: run.stats_exact,
        stats_close: run.stats_close,
        stats_retry: run.stats_retry,
        matched_module_bindings: Vec::new(),
        carry: carry::MatcherCarry::default(),
    }
}

/// TS `applyPriorVersionIfPresent` after the match (the plugin's
/// pre-transfer freezes included): settle the exact matches, gate the
/// statement twins over those states, run `TRANSFER_PIPELINE`. Returns the
/// outcome and the twins' gate output (the dump's `twin-gates.json`).
pub fn apply_prior_version(
    stage: &crate::prior::MatchStage<'_, '_>,
) -> Result<(TransferOutcome, crate::twins::gates::TwinGateOutput), String> {
    stage::apply_prior_version(stage)
}

/// The statement twins over the settled states alone — the SAME inputs
/// [`apply_prior_version`] gates them over (one owner: the M1 matches
/// dump reads this rather than re-deriving the inputs).
pub fn statement_twins(
    stage: &crate::prior::MatchStage<'_, '_>,
) -> Result<crate::twins::gates::TwinGateOutput, String> {
    stage::statement_twins(stage)
}
