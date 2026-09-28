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
//!   rename target (`invalid-target`). The lane loop already re-asks
//!   these; a site without that loop (the sweep) may re-ask once.
//! - [`ReaskClass::Unrecoverable`] — no suggestion can fix it:
//!   `no-binding` / `stale-binding` are internal state bugs (retrying
//!   hides them — they stay loud), `exported-name` rejects EVERY name for
//!   that binding (finding #55), and `capture-in-subtree` belongs to the
//!   deliberate-shadow path whose target is fixed by its owner binding.
//!
//! The bound is ONE re-ask ([`REASK_LIMIT`]); a second collision means
//! the ask keeps its recorder row and gives up — never a loop.
//!
//! NOT the same question as `rename::transfer::retry::is_retryable`
//! (declared difference, docs/responsibility.md): that one asks "can a
//! LATER RETRY OF THE SAME NAME land" (the transfer's deterministic prior
//! names — a fixed name never escapes `target-free-name`, so it is not
//! retryable there); this module asks "can a DIFFERENT SUGGESTION fix
//! it" (an LLM re-ask — a fresh name can escape more classes).

use crate::rename::validated::RejectionReason;

/// How many times a rejected suggestion may be re-asked.
pub const REASK_LIMIT: usize = 1;

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

/// Whether a rejected suggestion of this class gets the one bounded re-ask.
pub fn should_reask(class: ReaskClass) -> bool {
    !matches!(class, ReaskClass::Unrecoverable)
}

/// The wave barrier's seeding decision: `taken` is the used-set collision
/// (no validated reason — the name is in the live used set, re-ask), and
/// `reason` is the validated applier's rejection when the apply ran.
/// A rejected entry seeds a disclosed re-ask exactly when its rejection
/// is reaskable; unrecoverable classes stay loud and requeue nothing.
pub fn barrier_reask(taken: bool, reason: Option<RejectionReason>) -> bool {
    match reason {
        Some(r) => should_reask(class_of(r)),
        None => taken,
    }
}

#[cfg(test)]
mod reask_test;
