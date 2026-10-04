//! The sweep's prompt guard (`naming::shown`, 2026-10-04): a target the
//! shown code does not contain is refused — never asked, its refusal on
//! the trail, the identifier still a target — and an answer for an
//! identifier the ask did not show is never applied.

use std::collections::HashMap;

use humanify_model::llm::Renames;
use oxc_allocator::Allocator;

use super::{
    ReaskCtx, SweepGroup, SweepResult, apply_group_response, collect_sweep_targets,
    refuse_not_shown, split_shown,
};
use crate::ingest::Ingest;
use crate::modules::soundness::collect_eval_with_taint;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::floor::MinifiedStems;
use crate::rename::name_profile::NameProfile;
use crate::rename::validated::RenameState;
use crate::trail::{Anchor, StrategyTrail};

const TEXT: &str = "var Kq_ = one();\nvar Och_ = two();\nfunction f() { return Kq_ + Och_; }";

#[test]
fn a_target_the_shown_code_lacks_is_refused_recorded_and_stays_a_target() {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, TEXT);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let eligible = Eligibility::new(NeverRename::UNIVERSAL);
    let mut state = RenameState::with_trail(
        semantic,
        Anchor::Fresh,
        StrategyTrail::enabled(),
        NameProfile::Bun,
    );
    let targets = collect_sweep_targets(semantic, &state, &eligible, &taint, None);
    // The shown code holds `Kq_` (and `f`) only: `Och_` must be refused.
    let shown_code = "var Kq_ = one();\nfunction f() { return Kq_; }";
    let (shown, refused) = split_shown(shown_code, targets);
    let names = |v: &[super::MintedBinding]| v.iter().map(|t| t.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&refused), ["Och_"]);
    assert!(names(&shown).contains(&"Kq_".to_string()));
    let mut result = SweepResult::default();
    refuse_not_shown(&mut state, refused, &mut result);
    assert_eq!(result.not_shown, 1);
    assert_eq!(result.not_shown_examples, ["Och_"]);
    let row = state
        .trail()
        .entries()
        .iter()
        .find(|e| e.old_name == "Och_")
        .expect("the refusal is on the trail");
    assert_eq!(
        row.attempts.last().and_then(|a| a.reason.as_deref()),
        Some(crate::naming::shown::NOT_SHOWN)
    );
    // Never asked, so still a target (exhausted, not decided).
    let again = collect_sweep_targets(semantic, &state, &eligible, &taint, None);
    assert!(again.iter().any(|t| t.name == "Och_"));
}

#[test]
fn an_answer_for_an_identifier_the_ask_did_not_show_is_never_applied() {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, TEXT);
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let eligible = Eligibility::new(NeverRename::UNIVERSAL);
    let mut state = RenameState::new(semantic, Anchor::Fresh, NameProfile::Bun);
    let targets = collect_sweep_targets(semantic, &state, &eligible, &taint, None);
    let (shown, _) = split_shown("var Kq_ = one();", targets);
    let group = SweepGroup {
        code: "var Kq_ = one();".to_string(),
        targets: shown,
        used_names: Vec::new(),
    };
    // The model answers the shown target AND one it was never shown.
    let renames = Renames::from_entries(vec![
        ("Kq_".to_string(), Some("firstResult".to_string())),
        ("Och_".to_string(), Some("secondResult".to_string())),
    ]);
    let stems = MinifiedStems::empty(NameProfile::Bun);
    let none = HashMap::new();
    let ctx = ReaskCtx {
        limit: 2,
        spent: 0,
        carried: &none,
        stems: &stems,
    };
    let (named, _, _) = apply_group_response(&mut state, &group, &renames, &ctx);
    assert_eq!(named, 1, "only the shown target applies");
    let code = crate::naming::waves::render::render_program(semantic, &state);
    assert!(code.contains("var firstResult = one();"), "{code}");
    assert!(
        code.contains("var Och_ = two();"),
        "the unshown answer never applied: {code}"
    );
}
