//! The mechanical transfer tiers (WP3.2) — TS original:
//! `src/rename/prior-transfer.ts` (applyStatementTwinTransfers,
//! applyMatchedRenames / applyFunctionNameTransfers / resolvePairTarget /
//! transferOwnedPair / routeExternalPair, attachCloseMatchContext,
//! applyModuleBindingRenames / settleSameNameMatch,
//! suggestFromCloseMatchExternals).

use std::collections::HashSet;

use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::{RejectionReason, RenameRequest, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};

use super::evidence::TransferEvidence;
use super::lifecycle::{Lifecycle, TransferPair};
use super::owned::build_owned_binding_map;
use super::retry::OnApplied;
use super::rows::Rows;
use super::{ExternalRef, TransferRun, TransferStats, TwinTransfers};

/// Which pair-transfer tier is running.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PairTier {
    Exact,
    Close,
}

impl PairTier {
    fn tier(self) -> Tier {
        match self {
            PairTier::Exact => Tier::ExactMatch,
            PairTier::Close => Tier::CloseMatch,
        }
    }
}

/// Where one transfer pair's rename should go (TS `PairTarget`).
enum PairTarget {
    Scope(BScopeId),
    External(BindingId),
    Drop,
}

// ---------------------------------------------------------------------------
// statement-twin
// ---------------------------------------------------------------------------

/// TS `applyStatementTwinTransfers`: the gated pairs apply through the
/// validated owner; a pair whose binding no longer lives under its old
/// name (an earlier rename) is stale and drops silently. Outer refs become
/// demoted vote testimony; private names are node rewrites for the render.
pub fn apply_statement_twin_transfers(run: &mut TransferRun<'_, '_>, twins: &TwinTransfers<'_>) {
    run.private_renames = twins.private_renames.to_vec();
    // twinOuterRefVotes: never exact-grade (the statement hash masks WHICH
    // binding a reference resolves to).
    for r in twins.outer_refs {
        if r.old_name == r.new_name {
            continue;
        }
        let Some(binding) = run.rename.view().binding_of_symbol(r.symbol) else {
            continue;
        };
        run.refs_twin.push(ExternalRef {
            old_name: r.old_name.clone(),
            new_name: r.new_name.clone(),
            source_function_id: "statement-twin".to_string(),
            binding,
            exact_slot: false,
        });
    }
    let mut stats = TransferStats::default();
    for pair in twins.pairs {
        let Some(binding) = run.rename.view().binding_of_symbol(pair.symbol) else {
            continue;
        };
        stats.attempted += 1;
        let scope = run.rename.scope_of_binding(binding);
        if run.rename.binding_in(scope, &pair.old_name) != Some(binding) {
            stats.skipped += 1;
            continue;
        }
        let declared_fn = if Rows::binding_is_function_declaration(run.rename.view(), binding) {
            run.rows
                .fn_by_node
                .get(&run.rename.view().binding(binding).path_node)
                .copied()
        } else {
            None
        };
        let bookkeep = OnApplied::Twin {
            old_name: pair.old_name.clone(),
            scope,
            new_name: pair.new_name.clone(),
            declared_fn,
        };
        let attempt = run.rename.attempt_validated_rename(
            RenameRequest {
                scope,
                old_name: &pair.old_name,
                new_name: &pair.new_name,
                expected: None,
            },
            TrailSpec::settling(Tier::StatementTwin),
        );
        if attempt.applied {
            run.apply_on_applied(&bookkeep);
            stats.applied += 1;
        } else {
            let reason = attempt.reason.unwrap_or(RejectionReason::InvalidTarget);
            stats.skipped += 1;
            run.queue_retry(scope, &pair.old_name, &pair.new_name, reason, bookkeep);
        }
    }
    run.stats_twin = stats;
}

// ---------------------------------------------------------------------------
// exact-match / close-match
// ---------------------------------------------------------------------------

/// TS `applyMatchedRenames`: every exact-matched ("transferred") function's
/// slot pairs, in graph-row order.
pub fn apply_matched_renames(run: &mut TransferRun<'_, '_>) {
    let mut stats = TransferStats::default();
    for f in 0..run.fn_state.len() {
        let Lifecycle::Transferred(pairs) = &run.fn_state[f] else {
            continue;
        };
        let pairs = pairs.clone();
        let mut refs = std::mem::take(&mut run.refs_exact);
        apply_function_name_transfers(run, f, &pairs, PairTier::Exact, &mut stats, &mut refs);
        run.refs_exact = refs;
    }
    run.stats_exact = stats;
}

/// TS `attachCloseMatchContext`: close-matched, still-pending functions
/// transfer their pairs and keep the prior context (the LLM names them).
pub fn attach_close_match_context(run: &mut TransferRun<'_, '_>, evidence: &TransferEvidence) {
    let mut stats = TransferStats::default();
    for info in &evidence.close {
        let f = info.fresh_fn;
        if run.fn_state[f].is_settled() {
            continue;
        }
        let mut refs = std::mem::take(&mut run.refs_close);
        let transferred = apply_function_name_transfers(
            run,
            f,
            &info.name_transfers,
            PairTier::Close,
            &mut stats,
            &mut refs,
        );
        run.refs_close = refs;
        if !transferred.is_empty() {
            run.fn_transferred[f].extend(transferred.iter().cloned());
            run.fn_transferred_pairs[f] = Some(
                info.name_transfers
                    .iter()
                    .filter(|p| transferred.contains(&p.new_name))
                    .map(|p| (p.old_name.clone(), p.new_name.clone()))
                    .collect(),
            );
        }
        run.fn_prior_context[f] = true;
    }
    run.stats_close = stats;
}

/// TS `applyFunctionNameTransfers`: transfer pairs into a function's owned
/// bindings; non-owned names become external refs for vote propagation.
/// Returns the target names actually applied.
fn apply_function_name_transfers(
    run: &mut TransferRun<'_, '_>,
    f: usize,
    pairs: &[TransferPair],
    tier: PairTier,
    stats: &mut TransferStats,
    external_refs: &mut Vec<ExternalRef>,
) -> HashSet<String> {
    // Only positional pairs need the owned-name map — built lazily ONCE per
    // call, never refreshed by the renames this call applies.
    let mut owned: Option<Vec<(String, BScopeId)>> = None;
    let mut transferred = HashSet::new();
    for pair in pairs {
        if pair.old_name == pair.new_name {
            continue;
        }
        stats.attempted += 1;
        match resolve_pair_target(run, f, pair, &mut owned) {
            PairTarget::External(binding) => {
                stats.skipped += 1;
                route_external_pair(run, f, pair, tier, binding, external_refs);
            }
            PairTarget::Drop => stats.skipped += 1,
            PairTarget::Scope(scope) => {
                transfer_owned_pair(run, f, pair, tier, scope, stats, &mut transferred);
            }
        }
    }
    transferred
}

/// TS `isOwnedBinding`: the function's own name binding (its path is the
/// function), or a binding whose nearest function scope is the function's.
fn is_owned_binding(run: &TransferRun<'_, '_>, binding: BindingId, f: usize) -> bool {
    let view = run.rename.view();
    let b = view.binding(binding);
    if b.path_node == run.rows.fns[f].node {
        return true;
    }
    view.function_parent(b.owner) == Some(run.rows.fns[f].scope)
}

/// TS `resolvePairTarget`.
fn resolve_pair_target(
    run: &TransferRun<'_, '_>,
    f: usize,
    pair: &TransferPair,
    owned: &mut Option<Vec<(String, BScopeId)>>,
) -> PairTarget {
    if let Some(binding) = pair.binding {
        if !is_owned_binding(run, binding, f) {
            return PairTarget::External(binding);
        }
        let scope = run.rename.scope_of_binding(binding);
        if run.rename.binding_in(scope, &pair.old_name) != Some(binding) {
            return PairTarget::Drop;
        }
        return PairTarget::Scope(scope);
    }
    let map = owned.get_or_insert_with(|| build_owned_binding_map(&run.rename, &run.rows.fns[f]));
    if let Some((_, scope)) = map.iter().find(|(name, _)| *name == pair.old_name) {
        return PairTarget::Scope(*scope);
    }
    match resolve_referenced_outer_binding(run, f, &pair.old_name) {
        Some(outer) => PairTarget::External(outer),
        None => PairTarget::Drop,
    }
}

/// TS `resolveReferencedOuterBinding`: the binding the name resolves to
/// from the function's scope, counted only when one of its reads or
/// writes sits strictly inside the function.
fn resolve_referenced_outer_binding(
    run: &TransferRun<'_, '_>,
    f: usize,
    name: &str,
) -> Option<BindingId> {
    let fn_row = &run.rows.fns[f];
    let binding = run.rename.get_binding(fn_row.scope, name)?;
    let b = run.rename.view().binding(binding);
    let inside = b.refs.iter().chain(&b.violations).any(|site| {
        site.node != fn_row.node
            && site.span.start >= fn_row.span.start
            && site.span.end <= fn_row.span.end
    });
    inside.then_some(binding)
}

/// TS `routeExternalPair`: vote testimony, never applied here.
fn route_external_pair(
    run: &mut TransferRun<'_, '_>,
    f: usize,
    pair: &TransferPair,
    tier: PairTier,
    binding: BindingId,
    external_refs: &mut Vec<ExternalRef>,
) {
    external_refs.push(ExternalRef {
        old_name: pair.old_name.clone(),
        new_name: pair.new_name.clone(),
        source_function_id: run.rows.fns[f].session_id.clone(),
        binding,
        exact_slot: tier == PairTier::Exact && pair.binding.is_some(),
    });
    run.rename.record(
        binding,
        &pair.old_name,
        Attempt::new(tier.tier(), Outcome::Vote)
            .reason("external-reference")
            .proposed(pair.new_name.clone()),
        false,
    );
}

/// TS `transferOwnedPair`: validated rename, trail, retry bookkeeping.
fn transfer_owned_pair(
    run: &mut TransferRun<'_, '_>,
    f: usize,
    pair: &TransferPair,
    tier: PairTier,
    scope: BScopeId,
    stats: &mut TransferStats,
    transferred: &mut HashSet<String>,
) {
    let attempt = run.rename.attempt_validated_rename(
        RenameRequest {
            scope,
            old_name: &pair.old_name,
            new_name: &pair.new_name,
            expected: None,
        },
        TrailSpec::settling(tier.tier()),
    );
    if attempt.applied {
        transferred.insert(pair.new_name.clone());
        stats.applied += 1;
        return;
    }
    let reason = attempt.reason.unwrap_or(RejectionReason::InvalidTarget);
    stats.skipped += 1;
    run.queue_retry(
        scope,
        &pair.old_name,
        &pair.new_name,
        reason,
        OnApplied::FnTransferred {
            fn_row: f,
            name: pair.new_name.clone(),
        },
    );
}

// ---------------------------------------------------------------------------
// binding-cascade
// ---------------------------------------------------------------------------

/// TS `applyModuleBindingRenames`: the cascade's matched module bindings
/// and the function var-name transfers, in list order. A same-name match
/// settles WITHOUT a rename (exp066).
pub fn apply_module_binding_renames(run: &mut TransferRun<'_, '_>, evidence: &TransferEvidence) {
    for r in &evidence.binding_renames {
        let trail_binding = run.rename.binding_in(r.scope, &r.old_name);
        if r.old_name == r.new_name {
            settle_same_name_match(run, &r.old_name, r.scope, trail_binding);
            run.applied_binding_renames
                .push((r.old_name.clone(), r.new_name.clone()));
            continue;
        }
        let attempt = run.rename.attempt_validated_rename(
            RenameRequest {
                scope: r.scope,
                old_name: &r.old_name,
                new_name: &r.new_name,
                expected: r.binding,
            },
            TrailSpec::settling(Tier::BindingCascade),
        );
        let bookkeep = OnApplied::Cascade {
            scope: r.scope,
            new_name: r.new_name.clone(),
            old_name: r.old_name.clone(),
        };
        if !attempt.applied {
            let reason = attempt.reason.unwrap_or(RejectionReason::InvalidTarget);
            run.queue_retry(r.scope, &r.old_name, &r.new_name, reason, bookkeep);
            continue;
        }
        run.applied_binding_renames
            .push((r.old_name.clone(), r.new_name.clone()));
        run.apply_on_applied(&bookkeep);
    }
}

/// TS `settleSameNameMatch`: the prior kept this exact token — settle it
/// without a rename and register the carry.
fn settle_same_name_match(
    run: &mut TransferRun<'_, '_>,
    name: &str,
    scope: BScopeId,
    trail_binding: Option<BindingId>,
) {
    if let Some(b) = trail_binding {
        run.rename.record(
            b,
            name,
            Attempt::new(Tier::BindingCascade, Outcome::Applied)
                .reason("same-name-settle")
                .proposed(name),
            false,
        );
        run.rename.record_carried(b);
    }
    run.register_with_owner(scope, name);
    run.settle_module_binding_node(name);
}

// ---------------------------------------------------------------------------
// close-match-suggestions
// ---------------------------------------------------------------------------

/// TS `suggestFromCloseMatchExternals`: set elimination on close-match
/// external references — a 1:1 remainder becomes the module binding's
/// `suggestedName` (an LLM hint, never a rename).
pub fn suggest_from_close_match_externals(
    run: &mut TransferRun<'_, '_>,
    evidence: &TransferEvidence,
) {
    let resolved_humanified: HashSet<&str> = run
        .applied_binding_renames
        .iter()
        .chain(&run.applied_module_votes)
        .map(|(_, new)| new.as_str())
        .collect();
    // collectUnmatchedModuleBindings: pending rows keyed by name.
    let unmatched: std::collections::HashMap<&str, usize> = run
        .rows
        .modules
        .iter()
        .enumerate()
        .filter(|(i, _)| run.binding_state[*i].is_pending())
        .map(|(i, m)| (m.name.as_str(), i))
        .collect();
    let mut suggestions: Vec<(usize, String)> = Vec::new();
    for info in &evidence.close {
        let new_remaining: Vec<&String> = info
            .new_externals
            .iter()
            .filter(|n| unmatched.contains_key(n.as_str()))
            .collect();
        let prior_remaining: Vec<&String> = info
            .prior_externals
            .iter()
            .filter(|n| !resolved_humanified.contains(n.as_str()))
            .collect();
        if new_remaining.len() != 1 || prior_remaining.len() != 1 {
            continue;
        }
        let row = unmatched[new_remaining[0].as_str()];
        if run.binding_suggested[row].is_some() || suggestions.iter().any(|(r, _)| *r == row) {
            continue;
        }
        suggestions.push((row, prior_remaining[0].clone()));
    }
    for (row, name) in suggestions {
        run.binding_suggested[row] = Some(name);
        run.suggestions_applied += 1;
    }
}

impl TransferRun<'_, '_> {
    /// Run a tier's post-apply bookkeeping (shared with the retry pass).
    pub fn apply_on_applied(&mut self, on_applied: &OnApplied) {
        match on_applied {
            OnApplied::FnTransferred { fn_row, name } => {
                self.fn_transferred[*fn_row].insert(name.clone());
            }
            OnApplied::Twin {
                old_name,
                scope,
                new_name,
                declared_fn,
            } => {
                self.settle_module_binding_node(old_name);
                self.register_with_owner(*scope, new_name);
                if let Some(f) = declared_fn {
                    self.fn_transferred[*f].insert(new_name.clone());
                }
            }
            OnApplied::Cascade {
                scope,
                new_name,
                old_name,
            } => {
                self.register_with_owner(*scope, new_name);
                self.settle_module_binding_node(old_name);
            }
            OnApplied::Propagated { row } => self.mark_binding_propagated(*row),
        }
    }
}
