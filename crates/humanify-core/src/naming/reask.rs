//! What happens to a rejected rename suggestion — ONE retry policy every
//! LLM ask site shares (the wave barrier and the coverage sweep; see
//! docs/responsibility.md, "what happens to a rejected suggestion").
//!
//! A rejection means one of three things, and only the first two are
//! worth an LLM's time:
//!
//! - [`ReaskClass::NameTaken`] — the suggestion is a legal name that is
//!   already in use (`target-in-scope`, `target-visible`,
//!   `shadows-child`, `target-free-name`). A re-ask that DISCLOSES the
//!   previous suggestion can fix it.
//! - [`ReaskClass::InvalidSuggestion`] — the suggestion is not a legal
//!   rename target (`invalid-target`), or it borrows one of the program's
//!   minified names as a word (`borrowed-minified-stem`, 2026-10-03 —
//!   refused at the barrier and in the sweep; a budget that dies on it
//!   leaves the binding unrenamed and EXHAUSTED, never decorated). The
//!   lane loop already re-asks invalid targets; a site without that loop
//!   (the sweep) may re-ask too.
//! - [`ReaskClass::Unrecoverable`] — no suggestion can fix it:
//!   `no-binding` / `stale-binding` are internal state bugs (retrying
//!   hides them — they stay loud), `exported-name` rejects EVERY name for
//!   that binding (finding #55), and `capture-in-subtree` belongs to the
//!   deliberate-shadow path whose target is fixed by its owner binding.
//!
//! The budget (2026-09-29, Andrew's call): a reaskable rejection gets at
//! most TWO disclosed re-asks by default ([`REASK_LIMIT`] — the value
//! `--rename-retries` sizes), and each re-ask ACCUMULATES the disclosure:
//! every prior suggestion and why it was rejected travels in the retry's
//! do-not-suggest block, so a stubborn collision cannot be re-offered a
//! name it already tried. A budget exhausted at the second collision (or
//! disabled with `--rename-retries 0`) gives up — never a loop — and the
//! site's deterministic repair (the suffix ladder) settles the name.
//!
//! The budget is ADDITIVE to the lane's per-identifier call cap
//! (`--max-retries`, `naming::waves::batch`): a collision re-ask rides
//! its own wave-step round / sweep round and consumes no lane attempts,
//! so the default run gives a colliding identifier its initial ask plus
//! BOTH disclosed re-asks with no other flags.
//!
//! NOT the same question as `rename::transfer::retry::is_retryable`
//! (declared difference, docs/responsibility.md): that one asks "can a
//! LATER RETRY OF THE SAME NAME land" (the transfer's deterministic prior
//! names — a fixed name never escapes `target-free-name`, so it is not
//! retryable there); this module asks "can a DIFFERENT SUGGESTION fix
//! it" (an LLM re-ask — a fresh name can escape more classes).

use crate::rename::validated::RejectionReason;

/// How many times a rejected suggestion may be re-asked by DEFAULT — the
/// budget `--rename-retries` overrides per run.
pub const REASK_LIMIT: usize = 2;

/// Why a rename was rejected, as the retry policy reads it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReaskClass {
    /// The name is taken or read as a global — a disclosed re-ask can fix it.
    NameTaken,
    /// The suggestion was not a legal rename target — a re-ask can fix it.
    InvalidSuggestion,
    /// No suggestion can fix it — record and stay loud, never re-ask.
    Unrecoverable,
}

/// Classify a rejection reason (the one owner of the taken/unusable vs
/// unrecoverable split).
pub fn class_of(reason: RejectionReason) -> ReaskClass {
    match reason {
        RejectionReason::TargetInScope
        | RejectionReason::TargetVisible
        | RejectionReason::ShadowsChild
        | RejectionReason::TargetFreeName => ReaskClass::NameTaken,
        RejectionReason::InvalidTarget => ReaskClass::InvalidSuggestion,
        RejectionReason::NoBinding
        | RejectionReason::StaleBinding
        | RejectionReason::ExportedName
        | RejectionReason::CaptureInSubtree => ReaskClass::Unrecoverable,
    }
}

/// Whether a rejected suggestion of this class gets a disclosed re-ask at
/// all (the class half of the decision; [`reask_again`] adds the budget).
pub fn should_reask(class: ReaskClass) -> bool {
    !matches!(class, ReaskClass::Unrecoverable)
}

/// Whether a rejected suggestion of this class gets ONE MORE disclosed
/// re-ask: the class must be reaskable AND the identifier must still have
/// budget. `limit` is the run's `--rename-retries` ( [`REASK_LIMIT`] by
/// default); `spent` is how many re-asks the identifier has already had.
/// `spent == 0` with a positive limit is the first seeding.
pub fn reask_again(limit: usize, spent: usize, class: ReaskClass) -> bool {
    should_reask(class) && spent < limit
}

/// The class a wave-barrier rejection reads as: the validated applier's
/// rejection's own class; `None` is the used-set collision — the name is
/// in the live used set, no applier code — which reads `NameTaken`. (The
/// barrier calls this only on a FAILED apply, where `reason == None` is
/// exactly that collision: a successful apply never reaches the policy.)
pub fn barrier_class(reason: Option<RejectionReason>) -> ReaskClass {
    match reason {
        Some(r) => class_of(r),
        None => ReaskClass::NameTaken,
    }
}

#[cfg(test)]
mod reask_test;
