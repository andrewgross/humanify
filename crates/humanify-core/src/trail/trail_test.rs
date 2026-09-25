//! The strategy trail's tests, ported case-for-case from
//! `src/rename/strategy-trail.test.ts`. The TS keys entries by a Babel
//! `Binding`'s declaration identifier NODE; the Rust key is that
//! identifier's span in its anchored text, so the fixtures resolve bindings
//! through the Babel scope view and key by their `id_span`.
//!
//! Not ported here: "captures transfer-tier attempts through the real
//! pipeline" runs `createRenamePlugin` end to end (module-vote routing) —
//! it needs the transfer tiers (WP3.2) and the naming processor (WP4.3),
//! and ports with them.

use crate::rename::validated::scopes::BabelScopes;
use crate::rename::validated::test_support::with_semantic;
use crate::trail::{Anchor, Attempt, Outcome, StrategyTrail, Tier, TrailTarget};

/// The fresh-anchored target of the nth binding named `name`, ordered by
/// declaration position.
fn target_of(code: &str, name: &str, nth: usize) -> TrailTarget {
    with_semantic(code, true, |semantic| {
        let view = BabelScopes::build(semantic);
        let mut spans: Vec<_> = view
            .bindings
            .iter()
            .filter(|b| b.name == name)
            .map(|b| b.id_span)
            .collect();
        spans.sort_by_key(|s| s.start);
        TrailTarget {
            anchor: Anchor::Fresh,
            decl_span: spans[nth],
        }
    })
}

fn armed() -> StrategyTrail {
    StrategyTrail::enabled()
}

/// TS: "records attempts in order and marks the settling strategy".
#[test]
fn records_attempts_in_order_and_marks_the_settling_strategy() {
    let target = target_of("function q7(v) { return v; } q7(1);", "q7", 0);
    let mut trail = armed();
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::StatementTwin, Outcome::Abstained).reason("family-bucket"),
    );
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::FnNamePin, Outcome::Applied).proposed("packItem"),
    );
    let entries = trail.entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].old_name, "q7");
    assert_eq!(entries[0].settled_by, Some(Tier::FnNamePin));
    let shape: Vec<String> = entries[0]
        .attempts
        .iter()
        .map(|a| format!("{}:{}", a.tier.as_str(), a.outcome.as_str()))
        .collect();
    assert_eq!(shape, ["statement-twin:abstained", "fn-name-pin:applied"]);
}

/// TS: "stops recording once settled, counting post-settle attempts".
#[test]
fn stops_recording_once_settled_counting_post_settle_attempts() {
    let target = target_of("function q7(v) { return v; }", "q7", 0);
    let mut trail = armed();
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::ExactMatch, Outcome::Applied).proposed("packItem"),
    );
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::ModuleVote, Outcome::Applied).proposed("packOther"),
    );
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::ExactMatch, Outcome::Vote)
            .reason("external-reference")
            .proposed("packItem"),
    );
    let entry = &trail.entries()[0];
    assert_eq!(entry.attempts.len(), 1);
    assert_eq!(entry.post_settle_attempts, 1);
    assert_eq!(
        entry.post_settle_votes, 1,
        "post-settle votes are expected testimony, tracked separately"
    );
}

/// TS: "keeps shadowed same-name bindings apart".
#[test]
fn keeps_shadowed_same_name_bindings_apart() {
    let code = "var e = 1; function f() { var e = 2; return e; } f();";
    let outer = target_of(code, "e", 0);
    let inner = target_of(code, "e", 1);
    let mut trail = armed();
    trail.record(
        outer,
        "e",
        Attempt::new(Tier::BindingCascade, Outcome::Applied).proposed("outerE"),
    );
    trail.record(
        inner,
        "e",
        Attempt::new(Tier::ExactMatch, Outcome::Rejected).reason("target-in-scope"),
    );
    assert_eq!(trail.entries().len(), 2);
}

/// TS: "is a no-op when disabled".
#[test]
fn is_a_no_op_when_disabled() {
    let target = target_of("function q7(v) { return v; }", "q7", 0);
    let mut trail = armed();
    trail.reset(false);
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::ExactMatch, Outcome::Applied).proposed("x"),
    );
    assert!(trail.entries().is_empty());
}

/// TS: "post-pass applies append past settling and set terminalBy".
#[test]
fn post_pass_applies_append_past_settling_and_set_terminal_by() {
    let target = target_of("function q7(v) { return v; }", "q7", 0);
    let mut trail = armed();
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::ExactMatch, Outcome::Applied).proposed("initializeApp_"),
    );
    trail.record_post_pass(
        target,
        "initializeApp_",
        Attempt::new(Tier::DecorationRetry, Outcome::Applied).proposed("initializeApp"),
    );
    let entry = &trail.entries()[0];
    assert_eq!(entry.attempts.len(), 2);
    assert_eq!(entry.settled_by, Some(Tier::ExactMatch));
    assert_eq!(entry.terminal_by, Some(Tier::DecorationRetry));
    assert_eq!(entry.post_settle_attempts, 0, "not a clobber");
}

/// TS: "post-pass entries for unseen bindings create their own trail".
#[test]
fn post_pass_entries_for_unseen_bindings_create_their_own_trail() {
    let target = target_of("var iIn = 1; console.log(iIn);", "iIn", 0);
    let mut trail = armed();
    trail.record_post_pass(
        target,
        "iIn",
        Attempt::new(Tier::ReconcileAsymmetric, Outcome::Applied).proposed("T7Class"),
    );
    let entry = &trail.entries()[0];
    assert_eq!(entry.old_name, "iIn");
    assert_eq!(entry.settled_by, None, "no transfer tier settled");
    assert_eq!(entry.terminal_by, Some(Tier::ReconcileAsymmetric));
}

/// TS: "post-pass abstains are recorded without touching terminal state".
#[test]
fn post_pass_abstains_are_recorded_without_touching_terminal_state() {
    let target = target_of("var q8 = 1; console.log(q8);", "q8", 0);
    let mut trail = armed();
    trail.record_post_pass(
        target,
        "q8",
        Attempt::new(Tier::CoverageSweep, Outcome::Abstained).reason("llm-declined"),
    );
    let entry = &trail.entries()[0];
    assert_eq!(entry.terminal_by, None);
    assert_eq!(entry.attempts.len(), 1);
    assert_eq!(
        trail.funnel(),
        vec![(Tier::CoverageSweep, vec![(Outcome::Abstained, 1)])]
    );
}

/// TS: "record() sets terminalBy on the settling apply".
#[test]
fn record_sets_terminal_by_on_the_settling_apply() {
    let target = target_of("function q7(v) { return v; }", "q7", 0);
    let mut trail = armed();
    trail.record(
        target,
        "q7",
        Attempt::new(Tier::ExactMatch, Outcome::Applied).proposed("packItem"),
    );
    assert_eq!(trail.entries()[0].terminal_by, Some(Tier::ExactMatch));
}

/// TS: "rolls attempts up into a per-strategy funnel".
#[test]
fn rolls_attempts_up_into_a_per_strategy_funnel() {
    let a = target_of("function q7(v) { return v; }", "q7", 0);
    let b = target_of("function x(y) {} function w3(v) { return v; }", "w3", 0);
    let mut trail = armed();
    trail.record(
        a,
        "q7",
        Attempt::new(Tier::StatementTwin, Outcome::Applied).proposed("packItem"),
    );
    trail.record(
        b,
        "w3",
        Attempt::new(Tier::StatementTwin, Outcome::Rejected).reason("target-in-scope"),
    );
    trail.record(
        b,
        "w3",
        Attempt::new(Tier::ModulePin, Outcome::Abstained).reason("non-exact-source"),
    );
    assert_eq!(
        trail.funnel(),
        vec![
            (
                Tier::StatementTwin,
                vec![(Outcome::Applied, 1), (Outcome::Rejected, 1)]
            ),
            (Tier::ModulePin, vec![(Outcome::Abstained, 1)]),
        ]
    );
}

/// TS: "carries refCount through to the recorded attempt".
#[test]
fn carries_ref_count_through_to_the_recorded_attempt() {
    let target = target_of("let outerDir = 1; outerDir = 2;", "outerDir", 0);
    let mut trail = armed();
    trail.record(
        target,
        "outerDir",
        Attempt::new(Tier::Llm, Outcome::Applied)
            .proposed("dirPath")
            .ref_count(0),
    );
    assert_eq!(
        trail.entries()[0].attempts[0].ref_count,
        Some(0),
        "a zero reference count is the finding, so it must survive to the report"
    );
}

/// TS: "distinguishes zero references from an unrecorded count".
#[test]
fn distinguishes_zero_references_from_an_unrecorded_count() {
    let target = target_of("let a = 1; a = 2;", "a", 0);
    let mut trail = armed();
    trail.record(
        target,
        "a",
        Attempt::new(Tier::ExactMatch, Outcome::Applied).proposed("x"),
    );
    assert_eq!(trail.entries()[0].attempts[0].ref_count, None);
}

/// TS: "keeps ONE entry for one lexical binding, and still counts the
/// clobber" (the scope-epoch test). The Rust key is the declaration span,
/// which no epoch can split: two records for the same lexical binding —
/// from anywhere — are one entry, and the second apply is a clobber.
#[test]
fn keeps_one_entry_for_one_lexical_binding_and_counts_the_clobber() {
    let era_a = target_of("var target = 1; use(target);", "target", 0);
    let era_b = target_of("var target = 1; use(target);", "target", 0);
    assert_eq!(era_a, era_b, "the key is the declaration identifier's span");
    let mut trail = armed();
    trail.record(
        era_a,
        "target",
        Attempt::new(Tier::ExactMatch, Outcome::Applied).proposed("firstName"),
    );
    trail.record(
        era_b,
        "target",
        Attempt::new(Tier::Llm, Outcome::Applied).proposed("secondName"),
    );
    assert_eq!(trail.entries().len(), 1);
    assert_eq!(trail.entries()[0].settled_by, Some(Tier::ExactMatch));
    assert_eq!(trail.entries()[0].post_settle_attempts, 1);
}

/// `writeTransfers`: rows sorted by (text, start, end), ties in first-record
/// order; finalName null when never applied; the attempt fields map 1:1.
#[test]
fn transfer_rows_have_the_dump_shape_and_order() {
    let code = "var b = 1; var a = 2;";
    let tb = target_of(code, "b", 0);
    let ta = target_of(code, "a", 0);
    let mut trail = armed();
    trail.record(
        ta,
        "a",
        Attempt::new(Tier::ExactMatch, Outcome::Rejected)
            .reason("target-in-scope")
            .proposed("b"),
    );
    trail.record(
        tb,
        "b",
        Attempt::new(Tier::BindingCascade, Outcome::Applied).proposed("createObject"),
    );
    let rows = trail.transfer_rows();
    let json = serde_json::to_value(&rows).expect("serializes");
    assert_eq!(
        json,
        serde_json::json!([
            {"target": {"text": "fresh", "start": 4, "end": 5}, "oldName": "b",
             "finalName": "createObject", "settledBy": "binding-cascade",
             "attempts": [{"tier": "binding-cascade", "outcome": "applied", "proposedName": "createObject"}]},
            {"target": {"text": "fresh", "start": 15, "end": 16}, "oldName": "a",
             "finalName": null,
             "attempts": [{"tier": "exact-match", "outcome": "rejected", "reason": "target-in-scope", "proposedName": "b"}]}
        ])
    );
}
