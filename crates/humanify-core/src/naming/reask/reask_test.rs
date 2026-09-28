//! The retry policy's classification matrix: every RejectionReason lands
//! in exactly one class, and only the unrecoverable class keeps its
//! rejections loud (no re-ask).

use super::{REASK_LIMIT, ReaskClass, barrier_reask, class_of, should_reask};
use crate::rename::validated::RejectionReason;

#[test]
fn taken_names_and_invalid_suggestions_are_reaskable() {
    assert_eq!(
        class_of(RejectionReason::TargetInScope),
        ReaskClass::NameTaken
    );
    assert_eq!(
        class_of(RejectionReason::TargetVisible),
        ReaskClass::NameTaken
    );
    assert_eq!(
        class_of(RejectionReason::ShadowsChild),
        ReaskClass::NameTaken
    );
    assert_eq!(
        class_of(RejectionReason::TargetFreeName),
        ReaskClass::NameTaken
    );
    assert_eq!(
        class_of(RejectionReason::InvalidTarget),
        ReaskClass::InvalidSuggestion
    );
    assert!(should_reask(ReaskClass::NameTaken));
    assert!(should_reask(ReaskClass::InvalidSuggestion));
}

#[test]
fn state_bugs_exports_and_shadow_captures_stay_loud() {
    for reason in [
        RejectionReason::NoBinding,
        RejectionReason::StaleBinding,
        RejectionReason::ExportedName,
        RejectionReason::CaptureInSubtree,
    ] {
        assert_eq!(class_of(reason), ReaskClass::Unrecoverable, "{reason:?}");
        assert!(!should_reask(class_of(reason)));
    }
}

/// The bound: ONE re-ask, never a loop.
#[test]
fn the_reask_bound_is_one() {
    assert_eq!(REASK_LIMIT, 1);
}

/// The wave barrier's seeding decision: a used-set collision (no
/// validated reason) re-asks; a reaskable validated rejection re-asks; an
/// unrecoverable one requeues nothing.
#[test]
fn the_barrier_seeds_only_reaskable_rejections() {
    assert!(
        barrier_reask(true, None),
        "a live used-set collision re-asks"
    );
    assert!(barrier_reask(false, Some(RejectionReason::TargetInScope)));
    assert!(!barrier_reask(false, Some(RejectionReason::ExportedName)));
    assert!(
        !barrier_reask(false, Some(RejectionReason::NoBinding)),
        "an internal state bug stays loud, not re-asked"
    );
}
