//! An init's name follows its module (exp232): before the module step
//! plans which wrappers to ask, every recorded module that is the same
//! module as one of the prior release's app modules BY CONTENT takes that
//! module's init name.
//!
//! Why: the prior-version transfer names a lazy-init wrapper from the
//! function matcher's pairing of the wrapper's own arrow body, and that
//! body is the module's setup code — init calls and assignments, with
//! every name masked. Neighbouring modules' setup bodies often share one
//! shape, so the transfer can hand a wrapper its NEIGHBOUR's prior name
//! (three unchanged modules rotated their names on one measured hop), and
//! an edited setup body matches nothing, so the module step asks the model
//! for a new name although the module kept its file. Either way every
//! caller's `(0, alias.initX)()` line and the export registration change.
//!
//! The evidence here is the module's whole statement set, the same
//! rename-blind hashes the split's fossil matcher reads
//! ([`crate::place::assign::fossil_match`]): a fresh module and a prior
//! module pair only when each is the other's strictly best match by
//! multiset Jaccard and that score is at least [`CONTENT_PAIR_FLOOR`]. A
//! tie is no pairing (the content cannot tell twins apart). The name
//! always comes from the prior ledger (the prior's own spelling of the
//! wrapper); it is never invented, and nothing but the wrapper's name
//! changes.
//!
//! Precision over recall: a correction whose target name another binding
//! holds is refused, unless that holder is itself being corrected away
//! (the rotation case; a cycle is broken through the wrapper's own
//! minified name). A prior module under `vendor/` never lends its name.

use std::collections::HashMap;

use serde_json::Value;

use crate::graph::UnifiedGraph;
use crate::place::ledger::StableSplitLedger;
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::validated::scopes::BindingId;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::Tier;
use crate::twins::fossil::FossilModule;

#[cfg(test)]
mod init_identity_test;

/// The lowest multiset Jaccard at which two modules are the same module.
/// Measured on 2.1.238 -> 2.1.239 (exp232): at 0.5 every pairing whose
/// init name differed was a module the split kept in its prior file.
pub const CONTENT_PAIR_FLOOR: f64 = 0.5;

/// A hash held by more modules than this on one side does not nominate
/// candidates (it still counts in every score): a shape that common says
/// nothing about which module is which, and listing its holders for every
/// module is quadratic.
const COMMON_HASH_CAP: usize = 512;

/// The lifecycle reason of a wrapper this step named.
pub const BY_MODULE_IDENTITY: &str = "module-identity";

/// One prior module as this step reads it: its sorted statement hashes,
/// its init's name, and whether it is app code (not under `vendor/`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorModuleInit {
    pub hashes: Vec<String>,
    pub init_name: String,
    pub app: bool,
}

/// What the step did, for the run's record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InitIdentityReport {
    /// Fresh modules paired with a prior app module by content.
    pub paired: usize,
    /// Pairs whose wrapper already wore the prior module's init name.
    pub agreed: usize,
    /// `(wrapper before, wrapper after)` per wrapper this step renamed.
    pub carried: Vec<(String, String)>,
    /// `(wrapper, prior init name, why)` per correction refused.
    pub refused: Vec<(String, String, String)>,
}

/// The prior ledger's modules for this step. Empty for a ledger of
/// another hash era (its hashes are not this run's) or one without
/// modules. A module whose declared names are missing has no init name
/// to lend and is left out.
pub fn prior_module_inits(ledger: &StableSplitLedger) -> Vec<PriorModuleInit> {
    if !ledger.hashes_current() {
        return Vec::new();
    }
    ledger
        .fossil_modules
        .iter()
        .flatten()
        .filter_map(|m| {
            // The init statement closes the segment, so its name is the
            // last one the module declares.
            let init_name = m.declared.as_ref()?.last()?.clone();
            let mut hashes = m.hashes.clone();
            hashes.sort();
            Some(PriorModuleInit {
                hashes,
                init_name,
                app: !m.file.starts_with("vendor/"),
            })
        })
        .collect()
}

/// Multiset Jaccard of two SORTED hash lists.
fn sorted_jaccard(a: &[String], b: &[String]) -> f64 {
    let (mut i, mut j, mut inter) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                inter += 1;
                i += 1;
                j += 1;
            }
        }
    }
    crate::similarity::jaccard_from_counts(inter, a.len(), b.len())
}

/// hash -> the modules holding it, in module order (lookup only).
fn holders<'a>(side: &[&'a [String]]) -> HashMap<&'a str, Vec<usize>> {
    let mut map: HashMap<&str, Vec<usize>> = HashMap::new();
    for (k, hashes) in side.iter().enumerate() {
        for h in hashes.iter() {
            let list = map.entry(h.as_str()).or_default();
            if list.last() != Some(&k) {
                list.push(k);
            }
        }
    }
    map
}

/// The strictly best module of `other` for `hashes`, at or above the
/// floor; `None` on a tie or below it.
fn strict_best(
    hashes: &[String],
    other: &[&[String]],
    index: &HashMap<&str, Vec<usize>>,
) -> Option<usize> {
    let mut candidates: Vec<usize> = hashes
        .iter()
        .filter_map(|h| index.get(h.as_str()))
        .filter(|list| list.len() <= COMMON_HASH_CAP)
        .flatten()
        .copied()
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    let (mut best, mut best_score, mut second) = (None, 0.0f64, 0.0f64);
    for c in candidates {
        let score = sorted_jaccard(hashes, other[c]);
        if score > best_score {
            second = best_score;
            best = Some(c);
            best_score = score;
        } else if score > second {
            second = score;
        }
    }
    (best_score >= CONTENT_PAIR_FLOOR && best_score > second)
        .then_some(best)
        .flatten()
}

/// Mutual strict-best pairs `(fresh, prior)` by content, fresh order.
/// Each side's lists must be SORTED.
pub fn pair_modules_by_content(fresh: &[&[String]], prior: &[&[String]]) -> Vec<(usize, usize)> {
    let prior_index = holders(prior);
    let fresh_index = holders(fresh);
    (0..fresh.len())
        .filter_map(|f| {
            let p = strict_best(fresh[f], prior, &prior_index)?;
            (strict_best(prior[p], fresh, &fresh_index) == Some(f)).then_some((f, p))
        })
        .collect()
}

/// One wrapper to rename: its binding, graph row and the prior's name.
struct Correction {
    binding: BindingId,
    row: usize,
    target: String,
}

/// Name every recorded module's wrapper after the prior module it is by
/// content. `modules` are the era's recorded modules (hashes filled),
/// `wrappers` their wrapper bindings (as [`super::plan_module_naming`]
/// resolves them).
pub fn carry_init_names(
    graph: &UnifiedGraph,
    state: &mut RenameState,
    binding_state: &mut [Lifecycle],
    modules: &[FossilModule],
    wrappers: &[Option<BindingId>],
    prior: &[PriorModuleInit],
) -> InitIdentityReport {
    let mut report = InitIdentityReport::default();
    if prior.is_empty() || modules.is_empty() {
        return report;
    }
    let fresh_side: Vec<&[String]> = modules.iter().map(|m| m.hashes.as_slice()).collect();
    let prior_side: Vec<&[String]> = prior.iter().map(|m| m.hashes.as_slice()).collect();
    let row_of: HashMap<BindingId, usize> = graph
        .module_bindings
        .iter()
        .enumerate()
        .filter_map(|(row, b)| Some((state.view().binding_of_symbol(b.symbol)?, row)))
        .collect();
    let mut corrections: Vec<Correction> = Vec::new();
    for (f, p) in pair_modules_by_content(&fresh_side, &prior_side) {
        let (Some(binding), true) = (wrappers[f], prior[p].app) else {
            continue;
        };
        let Some(&row) = row_of.get(&binding) else {
            continue;
        };
        report.paired += 1;
        if state.name_of(binding) == prior[p].init_name {
            report.agreed += 1;
            continue;
        }
        corrections.push(Correction {
            binding,
            row,
            target: prior[p].init_name.clone(),
        });
    }
    refuse_taken_targets(state, &mut corrections, &mut report);
    apply_corrections(graph, state, binding_state, corrections, &mut report);
    report
}

/// Drop every correction whose target another binding holds, unless that
/// holder is itself corrected away.
fn refuse_taken_targets(
    state: &RenameState,
    corrections: &mut Vec<Correction>,
    report: &mut InitIdentityReport,
) {
    loop {
        let moving: Vec<BindingId> = corrections.iter().map(|c| c.binding).collect();
        let before = corrections.len();
        corrections.retain(|c| {
            let scope = state.scope_of_binding(c.binding);
            let holder = state.binding_in(scope, &c.target);
            let ok = holder.is_none_or(|h| moving.contains(&h));
            if !ok {
                report.refused.push((
                    state.name_of(c.binding).to_string(),
                    c.target.clone(),
                    "target-held".to_string(),
                ));
            }
            ok
        });
        // A refused holder stays where it is, so whoever needed its name
        // is refused on the next round.
        if corrections.len() == before {
            return;
        }
    }
}

/// Apply the corrections: each one whose target is free, repeatedly; a
/// cycle (every remaining target held by another remaining wrapper) is
/// broken by moving one wrapper to its own minified name first.
fn apply_corrections(
    graph: &UnifiedGraph,
    state: &mut RenameState,
    binding_state: &mut [Lifecycle],
    mut pending: Vec<Correction>,
    report: &mut InitIdentityReport,
) {
    let mut before_names: HashMap<BindingId, String> = pending
        .iter()
        .map(|c| (c.binding, state.name_of(c.binding).to_string()))
        .collect();
    while !pending.is_empty() {
        let mut progressed = false;
        let mut k = 0;
        while k < pending.len() {
            let c = &pending[k];
            let scope = state.scope_of_binding(c.binding);
            if state.binding_in(scope, &c.target).is_some() {
                k += 1;
                continue;
            }
            let c = pending.remove(k);
            progressed = true;
            let old = before_names
                .remove(&c.binding)
                .unwrap_or_else(|| state.name_of(c.binding).to_string());
            rename_wrapper(state, binding_state, &c, &old, report);
        }
        if !progressed {
            break_one_cycle(graph, state, &mut pending, &mut before_names, report);
        }
    }
}

/// Nothing applied this round: every remaining target is held. When the
/// first remaining wrapper's target is held by another remaining wrapper
/// (a cycle), move it to the minified name it was born with so the chain
/// can run; otherwise (its holder was refused) refuse it, back under the
/// name it had if it was moved.
fn break_one_cycle(
    graph: &UnifiedGraph,
    state: &mut RenameState,
    pending: &mut Vec<Correction>,
    before_names: &mut HashMap<BindingId, String>,
    report: &mut InitIdentityReport,
) {
    let Some(c) = pending.first() else {
        return;
    };
    let scope = state.scope_of_binding(c.binding);
    let holder = state.binding_in(scope, &c.target);
    let current = state.name_of(c.binding).to_string();
    let minified = graph.module_bindings[c.row].name.clone();
    let in_cycle = holder.is_some_and(|h| pending.iter().any(|o| o.binding == h));
    let untrailed = TrailSpec::Untrailed {
        why: "module-identity-cycle",
    };
    if in_cycle && current != minified {
        let request = RenameRequest {
            scope,
            old_name: &current,
            new_name: &minified,
            expected: Some(c.binding),
        };
        if state.attempt_validated_rename(request, untrailed).applied {
            return;
        }
    }
    let c = pending.remove(0);
    let before = before_names.remove(&c.binding).unwrap_or(current.clone());
    if before != current {
        let request = RenameRequest {
            scope,
            old_name: &current,
            new_name: &before,
            expected: Some(c.binding),
        };
        let _restored = state.attempt_validated_rename(request, untrailed);
    }
    report
        .refused
        .push((before, c.target, "target-held".to_string()));
}

/// Rename one wrapper to its prior name and settle its row.
fn rename_wrapper(
    state: &mut RenameState,
    binding_state: &mut [Lifecycle],
    c: &Correction,
    shown_old: &str,
    report: &mut InitIdentityReport,
) {
    let current = state.name_of(c.binding).to_string();
    let pending = binding_state[c.row].is_pending();
    let spec = if pending {
        TrailSpec::settling(Tier::ModuleIdentity)
    } else {
        TrailSpec::post_pass(Tier::ModuleIdentity)
    };
    let request = RenameRequest {
        scope: state.scope_of_binding(c.binding),
        old_name: &current,
        new_name: &c.target,
        expected: Some(c.binding),
    };
    let attempt = state.attempt_validated_rename(request, spec);
    if !attempt.applied {
        let why = attempt.reason.map_or("unknown", |r| r.as_str());
        report
            .refused
            .push((current, c.target.clone(), why.to_string()));
        return;
    }
    if pending {
        binding_state[c.row].mark_skipped(BY_MODULE_IDENTITY, &c.target);
    }
    report
        .carried
        .push((shown_old.to_string(), c.target.clone()));
}

/// The statement hashes of the era's body, for the recorded modules'
/// content (the same rename-blind form the split hashes with).
pub fn body_hashes(body: &[Value]) -> Vec<String> {
    crate::par::map_ordered(body, crate::hash::statement_hash::statement_hash)
}
