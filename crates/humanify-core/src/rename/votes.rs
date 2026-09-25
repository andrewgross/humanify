//! Vote propagation, the single-vote pin ladder, vote-suggest hints
//! (WP3.3) — TS originals: `src/rename/prior-transfer.ts`'s
//! `propagateExternalReferences` family (module-vote, module-pin,
//! fn-name-vote, fn-name-pin, closure-capture, vote-suggest) and
//! `src/rename/single-vote-pin.ts`; plus `src/rename/proximity.ts`
//! ([`proximity`], the LLM prompt's used-names window).
//!
//! External-reference testimony names what nothing hash-based can: a
//! matched function's slot that resolves OUTSIDE the function votes for
//! the prior name of the binding it reaches. Tallies are keyed by binding
//! IDENTITY and kept in first-vote order (the TS Maps); the ladders then
//! apply in that order:
//!
//! - module bindings and cold function heads rename at ≥2 agreeing votes;
//!   below the floor, the single-vote pin ladder (one exact-slot vote,
//!   one claimant across both vote maps, role corroboration, validated
//!   rename) — and a module binding the ladder refuses still gets its
//!   ranked vote as a `suggestedName` hint;
//! - closure captures (a close-matched function's local reached from a
//!   nested matched function) rename on a plain unique top vote.
//!
//! Tallies are snapshotted into the votes dump BEFORE the ladders run
//! ([`dump`]); the ladder outcome is joined from the trail at write time.

pub mod dump;
pub mod proximity;

use std::collections::HashMap;

use crate::rename::floor::is_below_floor_name;
use crate::rename::transfer::evidence::{SideRows, TransferEvidence};
use crate::rename::transfer::retry::OnApplied;
use crate::rename::transfer::rows::Rows;
use crate::rename::transfer::{ExternalRef, TransferRun};
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::{RejectionReason, RenameRequest, RenameState, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};
use crate::twins::role::{BindingRole, binding_roles_agree};

/// Per-name vote tally, exact-slot-sourced votes tracked separately (TS
/// `VoteCount`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VoteCount {
    pub total: u64,
    pub exact: u64,
}

/// A name → tally map in first-vote order (the TS `Map<string, VoteCount>`).
pub type Votes = Vec<(String, VoteCount)>;

/// Minimum agreeing votes to rename a module binding or function head.
const MIN_MODULE_BINDING_VOTES: u64 = 2;

fn tally(votes: &mut Votes, name: &str, exact: bool) {
    let i = match votes.iter().position(|(n, _)| n == name) {
        Some(i) => i,
        None => {
            votes.push((name.to_string(), VoteCount::default()));
            votes.len() - 1
        }
    };
    votes[i].1.total += 1;
    if exact {
        votes[i].1.exact += 1;
    }
}

/// TS `getTopVote`: the unique top name at or above the floor, else None.
pub fn get_top_vote<'v>(
    votes: impl IntoIterator<Item = (&'v str, u64)>,
    min_votes: u64,
) -> Option<&'v str> {
    let mut top: Option<&str> = None;
    let mut top_count = 0u64;
    let mut tied = false;
    for (name, count) in votes {
        if count > top_count {
            top = Some(name);
            top_count = count;
            tied = false;
        } else if count == top_count {
            tied = true;
        }
    }
    if tied || top_count < min_votes {
        return None;
    }
    top
}

/// TS `rankVoteSuggestion`: the unique top by (exact, then total), below-
/// floor names excluded from candidacy; a tie abstains.
pub fn rank_vote_suggestion(votes: &Votes) -> Option<String> {
    let mut best: Option<&str> = None;
    let (mut best_exact, mut best_total) = (-1i64, -1i64);
    let mut tied = false;
    for (name, count) in votes {
        if is_below_floor_name(name) {
            continue;
        }
        let (exact, total) = (count.exact as i64, count.total as i64);
        if exact > best_exact || (exact == best_exact && total > best_total) {
            best = Some(name);
            best_exact = exact;
            best_total = total;
            tied = false;
        } else if exact == best_exact && total == best_total {
            tied = true;
        }
    }
    if tied { None } else { best.map(str::to_string) }
}

/// A single-vote pin request (TS `SingleVotePinRequest`).
pub struct PinRequest<'r> {
    pub votes: &'r Votes,
    pub name_claimants: &'r HashMap<String, u64>,
    pub prior_roles: &'r HashMap<String, BindingRole>,
    pub fn_matches: &'r HashMap<String, String>,
    pub scope: BScopeId,
    pub old_name: &'r str,
    /// The tier the (caller-recorded) rename attempt counts under.
    pub tier: Tier,
}

/// TS `SingleVotePinResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinResult {
    Pinned {
        name: String,
        role_reason: &'static str,
    },
    /// `blocked` is None only when there was no single-name vote at all.
    NotPinned { blocked: Option<String> },
}

/// TS `trySingleVotePin`: the precision ladder, applying the rename on
/// success. `fresh_role` is computed lazily, once the vote gates pass.
pub fn try_single_vote_pin(
    rename: &mut RenameState,
    req: &PinRequest<'_>,
    fresh_role: impl FnOnce() -> BindingRole,
) -> PinResult {
    let blocked = |why: String| PinResult::NotPinned { blocked: Some(why) };
    if req.votes.len() != 1 {
        return PinResult::NotPinned { blocked: None };
    }
    let (name, count) = &req.votes[0];
    if count.total != 1 || count.exact != 1 {
        return blocked("non-exact-source".to_string());
    }
    if req.name_claimants.get(name).copied() != Some(1) {
        return blocked("name-conflict".to_string());
    }
    let Some(prior_role) = req.prior_roles.get(name) else {
        return blocked("no-prior-role".to_string());
    };
    let agreement = binding_roles_agree(prior_role, &fresh_role(), req.fn_matches, true);
    if !agreement.agrees {
        return blocked(format!("role-mismatch:{}", agreement.reason));
    }
    let attempt = rename.attempt_validated_rename(
        RenameRequest {
            scope: req.scope,
            old_name: req.old_name,
            new_name: name,
            expected: None,
        },
        TrailSpec::CallerRecords { tier: req.tier },
    );
    if !attempt.applied {
        let reason = attempt.reason.map_or("undefined", RejectionReason::as_str);
        return blocked(format!("validation:{reason}"));
    }
    PinResult::Pinned {
        name: name.clone(),
        role_reason: agreement.reason,
    }
}

/// One function-head vote entry (TS `FunctionNameVoteEntry`).
struct FnNameEntry {
    binding: BindingId,
    old_name: String,
    fn_row: usize,
    votes: Votes,
}

/// One closure-capture vote entry (TS `ClosureVoteEntry`).
struct ClosureEntry {
    binding: BindingId,
    old_name: String,
    owner_fn: usize,
    owner_scope: BScopeId,
    votes: Votes,
}

/// The classified tallies, each in first-vote order.
struct Tallies {
    /// (module row, votes).
    module: Vec<(usize, Votes)>,
    fn_names: Vec<FnNameEntry>,
    closures: Vec<ClosureEntry>,
}

/// How one ref is routed (the tally loop and the witness grouping share
/// it — TS `collectVoteWitnesses` re-classifies read-only).
enum Route {
    Module(usize),
    FnDecl(Option<usize>),
    Other,
}

fn route(
    run: &TransferRun<'_, '_>,
    module_by_binding: &HashMap<BindingId, usize>,
    r: &ExternalRef,
) -> Route {
    if let Some(&row) = module_by_binding.get(&r.binding) {
        return Route::Module(row);
    }
    if Rows::binding_is_function_declaration(run.rename.view(), r.binding) {
        let node = run.rename.view().binding(r.binding).path_node;
        return Route::FnDecl(run.rows.fn_by_node.get(&node).copied());
    }
    Route::Other
}

/// TS `propagateExternalReferences`: tally the refs (exact → close → twin
/// — the canonical insertion order), snapshot the dump, then run the
/// module, function-name and closure ladders in that order.
pub fn propagate_external_references(
    run: &mut TransferRun<'_, '_>,
    evidence: &TransferEvidence,
    fresh: &SideRows<'_, '_>,
) {
    let refs: Vec<ExternalRef> = run
        .refs_exact
        .iter()
        .chain(&run.refs_close)
        .chain(&run.refs_twin)
        .cloned()
        .collect();
    if refs.is_empty() {
        return;
    }
    // Pending module rows keyed by their RESOLVED binding (a ref votes for
    // a node only when it references that exact binding).
    let mut module_by_binding: HashMap<BindingId, usize> = HashMap::new();
    for (i, m) in run.rows.modules.iter().enumerate() {
        if run.binding_state[i].is_pending()
            && let Some(b) = run.rename.get_binding(m.scope, &m.name)
        {
            module_by_binding.insert(b, i);
        }
    }
    let tallies = classify(run, &refs, &module_by_binding);
    dump::record_vote_dump(run, &refs, &module_by_binding, &tallies_view(&tallies));
    let claimants = count_name_claimants(&tallies);
    apply_propagated_module_bindings(run, &tallies.module, &claimants, evidence, fresh);
    apply_propagated_function_names(run, &tallies.fn_names, &claimants, evidence, fresh);
    apply_propagated_closure_captures(run, &tallies.closures);
}

fn classify(
    run: &TransferRun<'_, '_>,
    refs: &[ExternalRef],
    module_by_binding: &HashMap<BindingId, usize>,
) -> Tallies {
    let mut tallies = Tallies {
        module: Vec::new(),
        fn_names: Vec::new(),
        closures: Vec::new(),
    };
    let mut module_index: HashMap<usize, usize> = HashMap::new();
    let mut fn_index: HashMap<BindingId, usize> = HashMap::new();
    let mut closure_index: HashMap<BindingId, usize> = HashMap::new();
    for r in refs {
        match route(run, module_by_binding, r) {
            Route::Module(row) => {
                let i = *module_index.entry(row).or_insert_with(|| {
                    tallies.module.push((row, Vec::new()));
                    tallies.module.len() - 1
                });
                tally(&mut tallies.module[i].1, &r.new_name, r.exact_slot);
            }
            Route::FnDecl(fn_row) => {
                // Only pending functions: a settled one carries its name.
                let Some(f) = fn_row.filter(|&f| run.fn_state[f].is_pending()) else {
                    continue;
                };
                let i = *fn_index.entry(r.binding).or_insert_with(|| {
                    tallies.fn_names.push(FnNameEntry {
                        binding: r.binding,
                        old_name: r.old_name.clone(),
                        fn_row: f,
                        votes: Vec::new(),
                    });
                    tallies.fn_names.len() - 1
                });
                tally(&mut tallies.fn_names[i].votes, &r.new_name, r.exact_slot);
            }
            Route::Other => {
                // classifyClosureCapture: the binding's nearest function
                // must be a close-matched (context-carrying) function.
                let view = run.rename.view();
                let owner_scope = view.binding(r.binding).owner;
                let Some(owner_fn) = view
                    .function_parent(owner_scope)
                    .and_then(|s| run.rows.fn_by_scope.get(&s).copied())
                else {
                    continue;
                };
                if !run.fn_prior_context[owner_fn] {
                    continue;
                }
                let i = *closure_index.entry(r.binding).or_insert_with(|| {
                    tallies.closures.push(ClosureEntry {
                        binding: r.binding,
                        old_name: r.old_name.clone(),
                        owner_fn,
                        owner_scope,
                        votes: Vec::new(),
                    });
                    tallies.closures.len() - 1
                });
                // Closure tallies count totals only.
                tally(&mut tallies.closures[i].votes, &r.new_name, false);
            }
        }
    }
    tallies
}

/// The tallies as the dump reads them.
fn tallies_view(t: &Tallies) -> dump::TallyView<'_> {
    dump::TallyView {
        module: t.module.iter().map(|(row, v)| (*row, v)).collect(),
        fn_names: t
            .fn_names
            .iter()
            .map(|e| (e.binding, e.fn_row, &e.votes))
            .collect(),
        closures: t.closures.iter().map(|e| (e.binding, &e.votes)).collect(),
    }
}

/// TS `countNameClaimants`: distinct nodes each proposed name has votes
/// on, across BOTH the module and the function-head vote maps.
fn count_name_claimants(t: &Tallies) -> HashMap<String, u64> {
    let mut claimants: HashMap<String, u64> = HashMap::new();
    for (_, votes) in &t.module {
        for (name, _) in votes {
            *claimants.entry(name.clone()).or_insert(0) += 1;
        }
    }
    for e in &t.fn_names {
        for (name, _) in &e.votes {
            *claimants.entry(name.clone()).or_insert(0) += 1;
        }
    }
    claimants
}

fn totals(votes: &Votes) -> impl Iterator<Item = (&str, u64)> {
    votes.iter().map(|(n, c)| (n.as_str(), c.total))
}

/// Record a module-node attempt on the binding currently holding the
/// minified name (TS `recordModuleNodeTrail` — a missing binding records
/// nothing).
fn record_on(
    run: &mut TransferRun<'_, '_>,
    binding: Option<BindingId>,
    old_name: &str,
    attempt: Attempt,
) {
    if let Some(b) = binding {
        run.rename.record(b, old_name, attempt, false);
    }
}

fn rename_attempt(
    tier: Tier,
    applied: bool,
    reason: Option<RejectionReason>,
    new: &str,
) -> Attempt {
    let a = Attempt::new(
        tier,
        if applied {
            Outcome::Applied
        } else {
            Outcome::Rejected
        },
    )
    .proposed(new);
    match reason {
        Some(r) if !applied => a.reason(r.as_str()),
        _ => a,
    }
}

/// TS `applyPropagatedModuleBindings`.
fn apply_propagated_module_bindings(
    run: &mut TransferRun<'_, '_>,
    module_votes: &[(usize, Votes)],
    claimants: &HashMap<String, u64>,
    evidence: &TransferEvidence,
    fresh: &SideRows<'_, '_>,
) {
    for (row, votes) in module_votes {
        let row = *row;
        let (name, scope, id_span) = {
            let m = &run.rows.modules[row];
            (m.name.clone(), m.scope, m.id_span)
        };
        let Some(top) = get_top_vote(totals(votes), MIN_MODULE_BINDING_VOTES).map(str::to_string)
        else {
            if !try_module_single_vote_pin(run, row, votes, claimants, evidence, fresh) {
                suggest_from_votes(run, row, votes);
            }
            continue;
        };
        let trail_binding = run.rename.get_binding(scope, &name);
        // Identity, era-stable: the holder's declaration identifier must be
        // the NODE's own — a re-adopted minified name fails here.
        if let Some(tb) = trail_binding
            && run.rename.view().binding(tb).id_span != id_span
        {
            let a = Attempt::new(Tier::ModuleVote, Outcome::Rejected)
                .reason("stale-binding")
                .proposed(top.clone());
            record_on(run, Some(tb), &name, a);
            continue;
        }
        let attempt = run.rename.attempt_validated_rename(
            RenameRequest {
                scope,
                old_name: &name,
                new_name: &top,
                expected: None,
            },
            TrailSpec::CallerRecords {
                tier: Tier::ModuleVote,
            },
        );
        let a = rename_attempt(Tier::ModuleVote, attempt.applied, attempt.reason, &top);
        record_on(run, trail_binding, &name, a);
        if !attempt.applied {
            let reason = attempt.reason.unwrap_or(RejectionReason::InvalidTarget);
            run.queue_retry(scope, &name, &top, reason, OnApplied::Propagated { row });
            continue;
        }
        run.mark_binding_propagated(row);
        run.applied_module_votes.push((name, top));
    }
}

/// TS `tryModuleSingleVotePin`.
fn try_module_single_vote_pin(
    run: &mut TransferRun<'_, '_>,
    row: usize,
    votes: &Votes,
    claimants: &HashMap<String, u64>,
    evidence: &TransferEvidence,
    fresh: &SideRows<'_, '_>,
) -> bool {
    let (name, scope) = {
        let m = &run.rows.modules[row];
        (m.name.clone(), m.scope)
    };
    let trail_binding = run.rename.get_binding(scope, &name);
    let req = PinRequest {
        votes,
        name_claimants: claimants,
        prior_roles: &evidence.prior_binding_roles,
        fn_matches: &evidence.fn_matches,
        scope,
        old_name: &name,
        tier: Tier::ModulePin,
    };
    match try_single_vote_pin(&mut run.rename, &req, || fresh.binding_role(row)) {
        PinResult::NotPinned { blocked } => {
            if let Some(why) = blocked {
                let a = Attempt::new(Tier::ModulePin, Outcome::Abstained).reason(why);
                record_on(run, trail_binding, &name, a);
            }
            false
        }
        PinResult::Pinned { name: pinned, .. } => {
            let a = Attempt::new(Tier::ModulePin, Outcome::Applied).proposed(pinned.clone());
            record_on(run, trail_binding, &name, a);
            run.mark_binding_propagated(row);
            run.applied_module_votes.push((name, pinned));
            true
        }
    }
}

/// TS `suggestFromVotes`: a binding that cleared neither the floor nor the
/// pin ladder gets its ranked vote as the batch's `suggestedName` hint.
fn suggest_from_votes(run: &mut TransferRun<'_, '_>, row: usize, votes: &Votes) {
    if run.binding_suggested[row].is_some() {
        return;
    }
    let Some(suggestion) = rank_vote_suggestion(votes) else {
        return;
    };
    run.binding_suggested[row] = Some(suggestion.clone());
    let (name, scope) = {
        let m = &run.rows.modules[row];
        (m.name.clone(), m.scope)
    };
    let trail_binding = run.rename.get_binding(scope, &name);
    let a = Attempt::new(Tier::VoteSuggest, Outcome::Vote).proposed(suggestion);
    record_on(run, trail_binding, &name, a);
}

/// TS `applyPropagatedFunctionNames`.
fn apply_propagated_function_names(
    run: &mut TransferRun<'_, '_>,
    entries: &[FnNameEntry],
    claimants: &HashMap<String, u64>,
    evidence: &TransferEvidence,
    fresh: &SideRows<'_, '_>,
) {
    for entry in entries {
        let Some(top) =
            get_top_vote(totals(&entry.votes), MIN_MODULE_BINDING_VOTES).map(str::to_string)
        else {
            try_function_name_pin(run, entry, claimants, evidence, fresh);
            continue;
        };
        let scope = run.rename.scope_of_binding(entry.binding);
        let attempt = run.rename.attempt_validated_rename(
            RenameRequest {
                scope,
                old_name: &entry.old_name,
                new_name: &top,
                expected: Some(entry.binding),
            },
            TrailSpec::CallerRecords {
                tier: Tier::FnNameVote,
            },
        );
        let a = rename_attempt(Tier::FnNameVote, attempt.applied, attempt.reason, &top);
        run.rename.record(entry.binding, &entry.old_name, a, false);
        if !attempt.applied {
            let reason = attempt.reason.unwrap_or(RejectionReason::InvalidTarget);
            run.queue_retry(
                scope,
                &entry.old_name,
                &top,
                reason,
                OnApplied::FnTransferred {
                    fn_row: entry.fn_row,
                    name: top.clone(),
                },
            );
            continue;
        }
        run.fn_transferred[entry.fn_row].insert(top);
    }
}

/// TS `tryFunctionNamePin`: close-matched functions are excluded (their
/// content changed — the prior name stays an LLM suggestion).
fn try_function_name_pin(
    run: &mut TransferRun<'_, '_>,
    entry: &FnNameEntry,
    claimants: &HashMap<String, u64>,
    evidence: &TransferEvidence,
    fresh: &SideRows<'_, '_>,
) {
    if evidence
        .close_matched_ids
        .contains(&run.rows.fns[entry.fn_row].session_id)
    {
        return;
    }
    let scope = run.rename.scope_of_binding(entry.binding);
    let req = PinRequest {
        votes: &entry.votes,
        name_claimants: claimants,
        prior_roles: &evidence.prior_function_roles,
        fn_matches: &evidence.fn_matches,
        scope,
        old_name: &entry.old_name,
        tier: Tier::FnNamePin,
    };
    let fn_row = entry.fn_row;
    let result = try_single_vote_pin(&mut run.rename, &req, || {
        fresh.function_role(fn_row).unwrap_or_default()
    });
    match result {
        PinResult::NotPinned { blocked } => {
            if let Some(why) = blocked {
                let a = Attempt::new(Tier::FnNamePin, Outcome::Abstained).reason(why);
                run.rename.record(entry.binding, &entry.old_name, a, false);
            }
        }
        PinResult::Pinned { name, .. } => {
            let a = Attempt::new(Tier::FnNamePin, Outcome::Applied).proposed(name.clone());
            run.rename.record(entry.binding, &entry.old_name, a, false);
            run.fn_transferred[entry.fn_row].insert(name);
        }
    }
}

/// TS `applyPropagatedClosureCaptures`: a plain unique top vote renames a
/// close-matched function's captured local, unless the function already
/// holds that name from an earlier tier.
fn apply_propagated_closure_captures(run: &mut TransferRun<'_, '_>, entries: &[ClosureEntry]) {
    for entry in entries {
        let Some(top) = get_top_vote(totals(&entry.votes), 1).map(str::to_string) else {
            continue;
        };
        if run.fn_transferred[entry.owner_fn].contains(&top) {
            continue;
        }
        let attempt = run.rename.attempt_validated_rename(
            RenameRequest {
                scope: entry.owner_scope,
                old_name: &entry.old_name,
                new_name: &top,
                expected: Some(entry.binding),
            },
            TrailSpec::CallerRecords {
                tier: Tier::ClosureCapture,
            },
        );
        let a = rename_attempt(Tier::ClosureCapture, attempt.applied, attempt.reason, &top);
        run.rename.record(entry.binding, &entry.old_name, a, false);
        if !attempt.applied {
            let reason = attempt.reason.unwrap_or(RejectionReason::InvalidTarget);
            run.queue_retry(
                entry.owner_scope,
                &entry.old_name,
                &top,
                reason,
                OnApplied::FnTransferred {
                    fn_row: entry.owner_fn,
                    name: top.clone(),
                },
            );
            continue;
        }
        run.fn_transferred[entry.owner_fn].insert(top);
    }
}

#[cfg(test)]
mod votes_test;
