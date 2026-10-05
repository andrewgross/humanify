//! The ask trace's taxonomy pins: the wire names, the reask-class reuse
//! (the vocabulary can only drift together with reask.rs), the failure
//! mapping, and the ONE reason derivation.

use humanify_model::llm::{BatchRenameRequest, RenameFailures};

use super::{AskReason, AskScope, AskSite, RetryCause, prior_context_of, reason_of};
use crate::naming::reask::ReaskClass;
use crate::rename::validated::RejectionReason;

#[test]
fn the_reason_wire_names_are_stable_and_distinct() {
    let all = [
        AskReason::Fresh,
        AskReason::PriorHinted,
        AskReason::Shadowed,
        AskReason::Retry,
        AskReason::ModuleLane,
        AskReason::Sweep,
        AskReason::Vendor,
        AskReason::Folders,
    ];
    let names: Vec<&str> = all.iter().map(AskReason::as_str).collect();
    assert_eq!(
        names,
        [
            "fresh",
            "prior-hinted",
            "shadowed",
            "retry",
            "module-lane",
            "sweep",
            "vendor",
            "folders",
        ]
    );
}

#[test]
fn reask_classes_map_verbatim_to_causes() {
    // The vocabulary reuse is the point (naming::reask's classes, as_str'd):
    // NameTaken/InvalidSuggestion survive. Unrecoverable never re-asks at
    // the barrier/sweep (the only sites that re-ask an applier rejection),
    // but the LANE round-2 can carry one (a late `exported-name` rides the
    // duplicate preamble — its recorded cause must say unrecoverable, not
    // the generic NameTaken the failure lists derive).
    assert_eq!(
        RetryCause::from(ReaskClass::NameTaken).as_str(),
        "NameTaken"
    );
    assert_eq!(
        RetryCause::from(ReaskClass::InvalidSuggestion).as_str(),
        "InvalidSuggestion"
    );
    assert_eq!(
        RetryCause::from(ReaskClass::Unrecoverable).as_str(),
        "Unrecoverable"
    );
}

/// The lane round-2 site derivation (2026-09-29): a retry whose ids include
/// a scope-check rejection records THAT reason's class + code, in
/// preference to the writer's generic failure-list derivation; mixed
/// windows take the first recorded rejection's class and list every
/// distinct code. Recording only — the retry flow is unchanged.
#[test]
fn lane_round2_sites_record_the_scope_rejections_class_and_codes() {
    // No scope rejection in the window: the writer derives from failures.
    assert_eq!(AskSite::lane_reask(&[], 0).cause, None);
    let site = AskSite::lane_reask(&[("a".into(), RejectionReason::ShadowsChild)], 1);
    assert_eq!(site.cause, Some(RetryCause::NameTaken));
    assert_eq!(site.detail.as_deref(), Some("shadows-child"));
    assert_eq!(site.phase, 1);
    // Mixed classes: the FIRST entry's class decides; distinct codes all
    // recorded, first-seen order.
    let site = AskSite::lane_reask(
        &[
            ("a".into(), RejectionReason::ShadowsChild),
            ("b".into(), RejectionReason::ExportedName),
            ("c".into(), RejectionReason::ShadowsChild),
        ],
        0,
    );
    assert_eq!(site.cause, Some(RetryCause::NameTaken));
    assert_eq!(site.detail.as_deref(), Some("shadows-child,exported-name"));
    // An export id alone: unrecoverable — the truth, not NameTaken.
    let site = AskSite::lane_reask(&[("x".into(), RejectionReason::ExportedName)], 0);
    assert_eq!(site.cause, Some(RetryCause::Unrecoverable));
    assert_eq!(site.detail.as_deref(), Some("exported-name"));
}

#[test]
fn lane_reask_causes_derive_from_the_failure_lists_in_render_order() {
    let mk = |d: &[&str], i: &[&str], m: &[&str], u: &[&str]| RenameFailures {
        duplicates: d.iter().map(|s| s.to_string()).collect(),
        invalid: i.iter().map(|s| s.to_string()).collect(),
        missing: m.iter().map(|s| s.to_string()).collect(),
        unchanged: u.iter().map(|s| s.to_string()).collect(),
        stray_keys: Vec::new(),
    };
    assert_eq!(
        RetryCause::of_failures(&mk(&["a"], &[], &[], &[])),
        Some(RetryCause::NameTaken)
    );
    assert_eq!(
        RetryCause::of_failures(&mk(&[], &["b"], &[], &[])),
        Some(RetryCause::InvalidSuggestion)
    );
    assert_eq!(
        RetryCause::of_failures(&mk(&[], &[], &["c"], &[])),
        Some(RetryCause::Missing)
    );
    assert_eq!(
        RetryCause::of_failures(&mk(&[], &[], &[], &["d"])),
        Some(RetryCause::Unchanged)
    );
    // Duplicates win when several lists are populated (the retry prompt
    // renders the collision preamble first).
    assert_eq!(
        RetryCause::of_failures(&mk(&["a"], &["b"], &[], &[])),
        Some(RetryCause::NameTaken)
    );
    assert_eq!(RetryCause::of_failures(&mk(&[], &[], &[], &[])), None);
}

#[test]
fn a_fn_ask_reads_by_phase_prior_and_retry() {
    let req = |retry: bool| {
        let mut r = BatchRenameRequest {
            code: "function f(a){}".into(),
            identifiers: vec!["a".into()],
            ..BatchRenameRequest::default()
        };
        if retry {
            r.is_retry = Some(true);
        }
        r
    };
    let site = |phase: u8, prior: bool| AskSite {
        phase,
        prior,
        cause: None,
        detail: None,
    };
    use AskScope::Fn;
    assert_eq!(
        reason_of(&site(0, false), Fn, &req(false)),
        AskReason::Fresh
    );
    assert_eq!(
        reason_of(&site(0, true), Fn, &req(false)),
        AskReason::PriorHinted
    );
    assert_eq!(
        reason_of(&site(1, false), Fn, &req(false)),
        AskReason::Shadowed
    );
    // A re-ask is a re-ask first; phase and prior stay on the record.
    assert_eq!(reason_of(&site(1, true), Fn, &req(true)), AskReason::Retry);
    // Fresh modules binding groups read as their own lane variant even
    // though they carry prior suggestions.
    assert_eq!(
        reason_of(&site(0, true), AskScope::Module, &req(false)),
        AskReason::ModuleLane
    );
    assert_eq!(
        reason_of(&site(0, false), AskScope::Module, &req(true)),
        AskReason::Retry
    );
    assert_eq!(
        reason_of(&site(0, false), AskScope::Sweep, &req(false)),
        AskReason::Sweep
    );
    assert_eq!(
        reason_of(&site(0, false), AskScope::Sweep, &req(true)),
        AskReason::Retry
    );
    assert_eq!(
        reason_of(&site(0, false), AskScope::Vendor, &req(false)),
        AskReason::Vendor
    );
    assert_eq!(
        reason_of(&site(0, false), AskScope::Folders, &req(false)),
        AskReason::Folders
    );
}

#[test]
fn prior_context_reads_the_requests_prior_fields() {
    // The request's PRIOR-VERSION fields are prior context for the record
    // (a close-matched fn ask carries priorVersionCode and hints); a module
    // lane's prior suggestions live in the prompt TEXT, so its site flag
    // records them — the writer ORs the two.
    let mut r = BatchRenameRequest::default();
    assert!(!prior_context_of(&r));
    r.prior_version_code = Some("function f(x){}".into());
    assert!(prior_context_of(&r));
    r.prior_version_code = None;
    r.prior_name_hints = Some(humanify_model::llm::StrMap(vec![(
        "a".into(),
        "alpha".into(),
    )]));
    assert!(prior_context_of(&r));
    // `alreadyRenamed` names what THIS RUN renamed earlier (every retry
    // carries it) — same-run context, not a prior version: it must NOT
    // read as prior context, or every re-ask in a fresh run would.
    r.prior_name_hints = None;
    r.already_renamed = Some(humanify_model::llm::StrMap(vec![(
        "b".into(),
        "beta".into(),
    )]));
    assert!(!prior_context_of(&r), "alreadyRenamed is not prior context");
    assert!(
        AskSite {
            prior: true,
            ..AskSite::fresh(0)
        }
        .prior
    );
}
