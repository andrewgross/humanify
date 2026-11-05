//! The retry policy's classification matrix: every RejectionReason lands
//! in exactly one class, and only the unrecoverable class keeps its
//! rejections loud (no re-ask).

use super::{REASK_LIMIT, ReaskClass, barrier_class, class_of, reask_again, should_reask};
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

/// The bound (2026-09-29): TWO disclosed re-asks by default — the budget
/// `--rename-retries` sizes; never a loop.
#[test]
fn the_default_reask_budget_is_two() {
    assert_eq!(REASK_LIMIT, 2);
}

/// The wave barrier's class: a used-set collision (no validated reason)
/// reads as name-taken; a validated rejection as its own reason's class.
#[test]
fn the_barrier_classes_collisions_and_rejections() {
    assert_eq!(barrier_class(None), ReaskClass::NameTaken);
    assert_eq!(
        barrier_class(Some(RejectionReason::TargetInScope)),
        ReaskClass::NameTaken
    );
    assert_eq!(
        barrier_class(Some(RejectionReason::InvalidTarget)),
        ReaskClass::InvalidSuggestion
    );
    assert_eq!(
        barrier_class(Some(RejectionReason::ExportedName)),
        ReaskClass::Unrecoverable
    );
}

/// The budget decision: a reaskable class gets one more disclosed re-ask
/// while the identifier has `--rename-retries` budget left; an
/// unrecoverable class never does, whatever the budget.
#[test]
fn a_reaskable_class_reasks_again_until_the_budget_is_spent() {
    assert!(reask_again(2, 0, ReaskClass::NameTaken), "the first re-ask");
    assert!(
        reask_again(2, 1, ReaskClass::NameTaken),
        "the second re-ask"
    );
    assert!(
        !reask_again(2, 2, ReaskClass::NameTaken),
        "the budget is spent: never a loop"
    );
    assert!(
        reask_again(1, 0, ReaskClass::InvalidSuggestion),
        "a single-retry budget still allows the one re-ask"
    );
    assert!(!reask_again(1, 1, ReaskClass::NameTaken));
    assert!(
        !reask_again(0, 0, ReaskClass::NameTaken),
        "a zero budget never re-asks"
    );
    for spent in 0..3 {
        assert!(
            !reask_again(2, spent, ReaskClass::Unrecoverable),
            "an unrecoverable class stays loud at any budget ({spent})"
        );
    }
}
