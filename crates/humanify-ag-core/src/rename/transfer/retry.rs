//! The deferred retry of collision-rejected renames (WP3.2) — TS original:
//! `prior-transfer.ts` `queueRetry` / `retryRejectedTransfers` /
//! `restoreStrandedTemps` / `findRetryCycleEntry`.
//!
//! Phase order makes swaps and chains self-block (G→R and R→G in one scope
//! reject each other, and a token can be freed by a LATER phase), so every
//! token-collision rejection is queued and re-attempted after every phase
//! has applied: scan passes unwind chains; when a scan makes no progress a
//! closed blocked-by cycle (a pure swap) is broken by temping one member
//! (`__hf_retry_N`); a temped member whose landing then fails for a
//! positional reason is restored to its original name, or to the wanted
//! name decorated — never left as a temp silently.
//!
//! The TS entries are objects mutated in place and shared by the pending
//! list and the temped list; here they live in one arena, addressed by
//! index.

use crate::naming::validation::DECORATION_WORDS;
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::{RejectionReason, RenameRequest, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};

use super::{TransferRun, TransferStats};

/// What an applied retry must still book (the TS `onApplied` closures).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnApplied {
    /// `fn.priorVersionTransferred.add(name)`.
    FnTransferred { fn_row: usize, name: String },
    /// The statement-twin tier's bookkeeping: settle the module node of the
    /// old name, register the new name with the scope's owner, and with
    /// the declared function when the binding is a function declaration.
    Twin {
        old_name: String,
        scope: BScopeId,
        new_name: String,
        declared_fn: Option<usize>,
    },
    /// The binding cascade's: register with the owner, settle the node.
    Cascade {
        scope: BScopeId,
        new_name: String,
        old_name: String,
    },
    /// A module-vote rename: mark the binding row `propagated`.
    Propagated { row: usize },
}

/// TS `RejectedTransfer`.
#[derive(Debug, Clone)]
pub struct RejectedTransfer {
    pub scope: BScopeId,
    /// Current name of the binding; mutated when a cycle-break temps it.
    pub old_name: String,
    pub new_name: String,
    /// Identity captured at rejection time — retry only while it holds.
    pub binding: BindingId,
    pub last_reason: RejectionReason,
    pub on_applied: OnApplied,
}

/// Rejection reasons a later phase can un-block (freed tokens).
fn is_retryable(reason: RejectionReason) -> bool {
    matches!(
        reason,
        RejectionReason::TargetInScope
            | RejectionReason::TargetVisible
            | RejectionReason::ShadowsChild
    )
}

impl TransferRun<'_, '_> {
    /// TS `queueRetry`: queue a retryable rejection, keyed to the binding
    /// currently holding the old name.
    pub fn queue_retry(
        &mut self,
        scope: BScopeId,
        old_name: &str,
        new_name: &str,
        reason: RejectionReason,
        on_applied: OnApplied,
    ) {
        if !is_retryable(reason) {
            return;
        }
        let Some(binding) = self.rename.binding_in(scope, old_name) else {
            return;
        };
        self.retry_queue.push(RejectedTransfer {
            scope,
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
            binding,
            last_reason: reason,
            on_applied,
        });
    }

    /// True while the entry's binding still lives under its current old
    /// name (TS `entryStillPending`).
    fn still_pending(&self, entry: &RejectedTransfer) -> bool {
        self.rename.binding_in(entry.scope, &entry.old_name) == Some(entry.binding)
    }

    fn attempt_untrailed(&mut self, scope: BScopeId, old: &str, new: &str) -> bool {
        self.rename
            .attempt_validated_rename(
                RenameRequest {
                    scope,
                    old_name: old,
                    new_name: new,
                    expected: None,
                },
                TrailSpec::Untrailed { why: "retry-temp" },
            )
            .applied
    }
}

/// TS `retryRejectedTransfers`.
pub fn retry_rejected_transfers(
    run: &mut TransferRun<'_, '_>,
    mut arena: Vec<RejectedTransfer>,
) -> TransferStats {
    let mut stats = TransferStats {
        attempted: arena.len() as u64,
        applied: 0,
        skipped: 0,
        rejected: Vec::new(),
    };
    let mut pending: Vec<usize> = (0..arena.len())
        .filter(|&i| run.still_pending(&arena[i]))
        .collect();
    let mut temp_counter = 0u64;
    let mut temped: Vec<(usize, String)> = Vec::new();
    while !pending.is_empty() {
        let (applied, remaining) = retry_scan_pass(run, &mut arena, &pending, &mut stats);
        pending = remaining;
        if applied > 0 || pending.is_empty() {
            continue;
        }
        let Some(cycle) = find_retry_cycle_entry(run, &arena, &pending) else {
            break;
        };
        let temp = format!("__hf_retry_{temp_counter}");
        temp_counter += 1;
        let (scope, old) = (arena[cycle].scope, arena[cycle].old_name.clone());
        if !run.attempt_untrailed(scope, &old, &temp) {
            break;
        }
        temped.push((cycle, old));
        arena[cycle].old_name = temp;
    }
    restore_stranded_temps(run, &mut arena, &temped, &mut stats);
    stats.skipped = stats.attempted - stats.applied;
    stats
}

/// TS `retryScanPass`: apply what now validates, keep the rest.
fn retry_scan_pass(
    run: &mut TransferRun<'_, '_>,
    arena: &mut [RejectedTransfer],
    pending: &[usize],
    stats: &mut TransferStats,
) -> (u64, Vec<usize>) {
    let mut applied = 0u64;
    let mut remaining = Vec::new();
    for &i in pending {
        if !run.still_pending(&arena[i]) {
            continue;
        }
        let entry = &arena[i];
        let attempt = run.rename.attempt_validated_rename(
            RenameRequest {
                scope: entry.scope,
                old_name: &entry.old_name,
                new_name: &entry.new_name,
                expected: None,
            },
            TrailSpec::CallerRecords { tier: Tier::Retry },
        );
        if attempt.applied {
            applied += 1;
            stats.applied += 1;
            let on_applied = entry.on_applied.clone();
            let (binding, old, new) = (
                entry.binding,
                entry.old_name.clone(),
                entry.new_name.clone(),
            );
            run.apply_on_applied(&on_applied);
            run.rename.record(
                binding,
                &old,
                Attempt::new(Tier::Retry, Outcome::Applied).proposed(new),
                false,
            );
        } else {
            if let Some(reason) = attempt.reason {
                arena[i].last_reason = reason;
            }
            remaining.push(i);
        }
    }
    (applied, remaining)
}

/// TS `findRetryCycleEntry`: an entry on a closed blocked-by cycle — from
/// each start, follow the pending entry whose SUBJECT binding holds the
/// token the previous entry wants. Only token-collision rejections take
/// part (a shadows-child block is positional; temping frees nothing).
fn find_retry_cycle_entry(
    run: &TransferRun<'_, '_>,
    arena: &[RejectedTransfer],
    pending: &[usize],
) -> Option<usize> {
    // `bySubject`: a Map — a repeated subject keeps its first position and
    // takes the later entry.
    let mut by_subject: Vec<(BindingId, usize)> = Vec::new();
    for &i in pending {
        let entry = &arena[i];
        if entry.last_reason == RejectionReason::ShadowsChild {
            continue;
        }
        if let Some(subject) = run.rename.binding_in(entry.scope, &entry.old_name) {
            match by_subject.iter_mut().find(|(s, _)| *s == subject) {
                Some(slot) => slot.1 = i,
                None => by_subject.push((subject, i)),
            }
        }
    }
    let lookup = |b: BindingId| by_subject.iter().find(|(s, _)| *s == b).map(|(_, i)| *i);
    for &(_, start) in &by_subject {
        let mut seen = vec![start];
        let mut current = start;
        loop {
            let entry = &arena[current];
            let holder = run.rename.get_binding(entry.scope, &entry.new_name);
            let next = holder.and_then(lookup);
            match next {
                Some(n) if !seen.contains(&n) => {
                    seen.push(n);
                    current = n;
                }
                Some(n) => {
                    if n == start {
                        return Some(start);
                    }
                    break;
                }
                None => break,
            }
        }
    }
    None
}

/// TS `restoreStrandedTemps`: a temped member whose landing failed is
/// restored — its original name first, then the wanted name decorated
/// through the owner ladder; a still-stranded temp is recorded.
fn restore_stranded_temps(
    run: &mut TransferRun<'_, '_>,
    arena: &mut [RejectedTransfer],
    temped: &[(usize, String)],
    stats: &mut TransferStats,
) {
    for (i, original) in temped {
        let i = *i;
        if !run.still_pending(&arena[i]) || !arena[i].old_name.starts_with("__hf_retry_") {
            continue;
        }
        let (scope, temp, wanted) = (
            arena[i].scope,
            arena[i].old_name.clone(),
            arena[i].new_name.clone(),
        );
        let restored = if run.attempt_untrailed(scope, &temp, original) {
            Some(original.clone())
        } else {
            DECORATION_WORDS
                .iter()
                .map(|w| format!("{wanted}{w}"))
                .find(|candidate| run.attempt_untrailed(scope, &temp, candidate))
        };
        let attempt = match &restored {
            Some(name) => Attempt::new(Tier::Retry, Outcome::Applied).proposed(name.clone()),
            None => Attempt::new(Tier::Retry, Outcome::Rejected)
                .reason("stranded-temp")
                .proposed(wanted.clone()),
        };
        run.rename.record(arena[i].binding, &temp, attempt, false);
        if let Some(name) = restored {
            arena[i].old_name = name;
            stats.applied += 1;
        }
    }
}
