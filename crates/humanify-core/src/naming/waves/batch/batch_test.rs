//! The lane loop's decisions (processor.ts runBatchRenameLoop and its
//! helpers; the processor.test.ts batch-loop cases in miniature).

use crate::rename::validated::RejectionReason;
use humanify_model::llm::Renames;

use super::{Lane, LaneEffect, LaneEnv, compute_lane_count, split_by_position};

fn renames(pairs: &[(&str, &str)]) -> Renames {
    Renames::from_entries(
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), Some(b.to_string()))),
    )
}

fn names(ns: &[&str]) -> Vec<String> {
    ns.iter().map(|s| s.to_string()).collect()
}

fn env<'e>(
    used: &'e dyn Fn(&str) -> bool,
    reject: &'e dyn Fn(&str, &str) -> Option<RejectionReason>,
) -> LaneEnv<'e> {
    LaneEnv {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        used,
        would_reject: reject,
        transform: None,
    }
}

#[test]
fn lanes_split_by_count_and_position() {
    assert_eq!(compute_lane_count(25, 25), 0);
    assert_eq!(compute_lane_count(26, 25), 4);
    assert_eq!(compute_lane_count(201, 25), 8);
    assert_eq!(compute_lane_count(1001, 25), 16);
    let ids = names(&["a", "b", "c", "d", "e"]);
    assert_eq!(
        split_by_position(&ids, 4),
        vec![names(&["a", "b"]), names(&["c", "d"]), names(&["e"])]
    );
}

#[test]
fn a_clean_answer_claims_every_name_in_one_call() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a", "b"]), true);
    let call = lane.next_call().expect("a call");
    assert_eq!(call.round, 1);
    lane.feed(Ok((renames(&[("a", "alpha"), ("b", "beta")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![
            LaneEffect::Rename {
                old: "a".into(),
                new: "alpha".into()
            },
            LaneEffect::Rename {
                old: "b".into(),
                new: "beta".into()
            },
        ]
    );
}

#[test]
fn a_window_with_no_success_is_exhausted_without_a_retry() {
    // Both answer `x`: both evicted as duplicates. Nothing applied and
    // nothing left the batch — `validThisCall === 0 && nextRetry.length ===
    // batchSizeBefore` exhausts the window on the spot. The tail resolves
    // both from their suggestions over a SNAPSHOT that never sees `a`'s
    // claim — so `b` claims `x` too (the barrier sorts it out).
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a", "b"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "x"), ("b", "x")]), None)), &e);
    assert!(
        lane.next_call().is_none(),
        "suggestions exist: no stragglers"
    );
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![
            LaneEffect::Rename {
                old: "a".into(),
                new: "x".into()
            },
            LaneEffect::Rename {
                old: "b".into(),
                new: "x".into()
            },
        ]
    );
}

#[test]
fn a_partial_success_retries_the_rest_in_round_two() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a", "b"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "alpha"), ("b", "b")]), None)), &e);
    let retry = lane.next_call().expect("a retry");
    assert_eq!(retry.batch, names(&["b"]));
    assert_eq!(retry.round, 2, "b has a suggestion (itself): retry-shaped");
    assert_eq!(retry.failures.unchanged, names(&["b"]));
    lane.feed(Ok((renames(&[("b", "beta")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(lane.effects.len(), 2);
}

#[test]
fn a_missing_answer_goes_to_the_straggler_pass_then_identity() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[]), None)), &e);
    let straggler = lane.next_call().expect("straggler");
    assert_eq!(straggler.round, 2, "the straggler pass always asks round 2");
    lane.feed(Ok((renames(&[]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Identity { name: "a".into() }]
    );
}

/// `--rename-retries 0`: no disclosed re-ask exists, so a lone window's
/// colliding answer is settled by the tail's ladder on the spot (the
/// pre-2026-10-04 behavior, kept as the disabled-budget path).
fn no_reasks() -> super::WaveTunables {
    super::WaveTunables {
        reask_limit: 0,
        ..super::WaveTunables::default()
    }
}

/// Finding #74's open item (2026-10-04): a ONE-id window whose answer
/// collides with a name already in use dies under the all-failed rule
/// before any disclosed round-2 — and the tail used to decorate it on the
/// spot (`w` → `error`, held by a sibling → `errorVal`). With a re-ask
/// budget the lane hands the model's OWN answer to the barrier instead,
/// undecorated: the barrier sees the collision and gives it the same
/// disclosed re-ask every other conflict gets; the ladder is the last
/// resort once that budget is spent.
#[test]
fn a_lone_colliding_answer_goes_to_the_barrier_undecorated() {
    let used = |n: &str| n == "error";
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["w"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("w", "error")]), None)), &e);
    assert!(
        lane.next_call().is_none(),
        "the all-failed rule still burns the window"
    );
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Rename {
            old: "w".into(),
            new: "error".into()
        }],
        "the colliding answer itself, for the barrier to re-ask"
    );
    assert_eq!(lane.report.collision_handoffs, 1);
    assert!(
        lane.report.contention.is_empty(),
        "no decoration happened in the lane"
    );
}

/// The hand-off is for answers the all-failed rule cut off BEFORE their
/// round-2: an id that already had its lane round-2 and collided again is
/// settled by the tail's ladder as before.
#[test]
fn a_collision_exhausted_through_round_two_still_ladders_in_the_tail() {
    let used = |n: &str| n == "taken";
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a", "b"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "alpha"), ("b", "taken")]), None)), &e);
    let retry = lane.next_call().expect("b's round-2");
    assert_eq!(retry.batch, names(&["b"]));
    lane.feed(Ok((renames(&[("b", "taken")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects[1],
        LaneEffect::Rename {
            old: "b".into(),
            new: "takenVal".into()
        }
    );
    assert_eq!(lane.report.collision_handoffs, 0);
}

/// An unescapable rejection is never handed off: no re-ask can fix it.
#[test]
fn an_unescapable_lone_rejection_is_not_handed_off() {
    let used = |n: &str| n == "taken";
    let reject = |_: &str, _: &str| Some(RejectionReason::ExportedName);
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "taken")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Identity { name: "a".into() }]
    );
    assert_eq!(lane.report.collision_handoffs, 0);
}

#[test]
fn a_used_suggestion_resolves_through_the_conflict_ladder() {
    let used = |n: &str| n == "taken";
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a"]), false).tuned(&no_reasks());
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "taken")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Rename {
            old: "a".into(),
            new: "takenVal".into()
        }]
    );
}

/// Finding #74: the ladder's first decoration is SCOPE-unsafe (a child
/// scope that reads `a` already binds `takenVal`). The tail used to give
/// up and record identity — the answer dropped, never re-asked. It steps
/// past the unsafe decoration like a taken one and lands the next.
#[test]
fn the_ladder_steps_past_a_scope_unsafe_decoration() {
    let used = |n: &str| n == "taken";
    let reject = |_: &str, n: &str| (n == "takenVal").then_some(RejectionReason::ShadowsChild);
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a"]), true).tuned(&no_reasks());
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "taken")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Rename {
            old: "a".into(),
            new: "takenVar".into()
        }]
    );
}

/// The ladder stops at the id's OWN name: a binding already wearing the
/// decoration (`isReplBridgeActiveVal`, answered `isReplBridgeActive`,
/// r3 2.1.198 of the census) keeps it rather than being re-decorated
/// past itself to `...Var`.
#[test]
fn the_ladder_stops_at_the_ids_own_name() {
    let used = |n: &str| n == "taken";
    let reject = |old: &str, n: &str| (old == n).then_some(RejectionReason::TargetInScope);
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["takenVal"]), true).tuned(&no_reasks());
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("takenVal", "taken")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Identity {
            name: "takenVal".into()
        }]
    );
}

/// A rejection NO name escapes (`exported-name`) still settles identity —
/// and the ladder never spins looking for a name that cannot exist.
#[test]
fn an_unescapable_rejection_stays_identity_without_laddering() {
    let used = |n: &str| n == "taken";
    let reject = |_: &str, _: &str| Some(RejectionReason::ExportedName);
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a"]), true).tuned(&no_reasks());
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "taken")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Identity { name: "a".into() }]
    );
}

/// `--batch-size` / `--max-retries` (processor.ts runBatchRenameLoop's
/// `options.batchSize ?? DEFAULT_BATCH_SIZE` and
/// `options.maxRetriesPerIdentifier ?? DEFAULT_MAX_RETRIES_PER_ID`): the
/// window is the configured size, and an identifier gets exactly the
/// configured number of calls.
#[test]
fn the_configured_batch_size_and_retry_limit_shape_the_loop() {
    use super::WaveTunables;
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let tunables = WaveTunables {
        batch_size: 1,
        max_retries: 1,
        ..WaveTunables::default()
    };
    let mut lane = Lane::new(names(&["a", "b"]), true).tuned(&tunables);
    let first = lane.next_call().expect("a call");
    assert_eq!(first.batch, names(&["a"]), "a window of one");
    // `a` echoes its own name: with one call allowed there is no retry.
    lane.feed(Ok((renames(&[("a", "a")]), None)), &e);
    let second = lane.next_call().expect("b's window");
    assert_eq!(second.batch, names(&["b"]));
    // The default batch size is its own test (the_default_batch_size_is_25).
    assert_eq!(WaveTunables::default().lane_threshold, 25);
    // The name-conflict re-ask budget defaults to reask.rs's TWO
    // (`--rename-retries` sizes it; the lane reads only whether it is
    // nonzero — whether a lone collision may be handed to the barrier).
    assert_eq!(
        WaveTunables::default().reask_limit,
        crate::naming::reask::REASK_LIMIT
    );
    assert_eq!(WaveTunables::default().reask_limit, 2);
}

/// The default window is 25 identifiers (the 2026-09-28 relaxed-default
/// flip, docs/rust-port/20-fast-mode.md §defaults; was the TS's 10 — the
/// conservative schedule recovers it explicitly with `--batch-size 10`,
/// which the e2e gate pins against the committed legacy goldens).
#[test]
fn the_default_batch_size_is_25() {
    assert_eq!(super::DEFAULT_BATCH_SIZE, 25);
    assert_eq!(super::WaveTunables::default().batch_size, 25);
}

/// The `late` drop (2026-09-28, log-proven 4/18 in the walk survivors): a
/// suggestion the scope-safety check rejects at claim time used to stay in
/// `valid`, so `classify`'s success short-circuit skipped it — no retry,
/// no exhaustion, no last suggestion — and the id settled as identity,
/// minified forever. It must flow into the retry lane with the duplicate
/// failure preamble (the disclosed round-2 ask), like any other rejected
/// suggestion.
#[test]
fn a_scope_rejected_suggestion_is_retried_not_dropped() {
    let used = |_: &str| false;
    // The scope check alone rejects `a → taken` (a `wouldReject` class the
    // used-set check cannot see — e.g. a child-scope shadow).
    let reject = |old: &str, new: &str| {
        (old == "a" && new == "taken").then_some(RejectionReason::ShadowsChild)
    };
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "taken")]), None)), &e);
    let retry = lane.next_call().expect("a disclosed round-2 retry");
    assert_eq!(retry.batch, names(&["a"]));
    assert_eq!(retry.round, 2);
    assert_eq!(retry.failures.duplicates, names(&["a"]));
    assert_eq!(retry.prev.0, vec![("a".to_string(), "taken".to_string())]);
    lane.feed(Ok((renames(&[("a", "fresh")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Rename {
            old: "a".into(),
            new: "fresh".into()
        }]
    );
}

/// The reason thread (2026-09-29, the 2026-09-28 audit's bool-only
/// `would_reject`): the scope check's WHY reaches the retry CALL — the
/// reason is recorded per id at claim time and carried on the round-2
/// LaneCall for the ask trace — while the retry FLOW is byte-unchanged:
/// the id still rides the duplicate failure preamble, so no prompt or ask
/// sequence moves (output-neutral; the lane's wording fix awaits its cold
/// eval).
#[test]
fn a_scope_rejected_suggestion_carries_its_reason_into_the_retry_call() {
    let used = |_: &str| false;
    // Two scope classes: `a → taken` shadows a child scope, `b → self` is
    // an export name (unrecoverable — no fresh suggestion can fix it).
    let reject = |old: &str, new: &str| match (old, new) {
        ("a", "taken") => Some(RejectionReason::ShadowsChild),
        ("b", "exported") => Some(RejectionReason::ExportedName),
        _ => None,
    };
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a", "b"]), true);
    lane.next_call().unwrap();
    lane.feed(
        Ok((renames(&[("a", "taken"), ("b", "exported")]), None)),
        &e,
    );
    let retry = lane.next_call().expect("a disclosed round-2 retry");
    assert_eq!(
        retry.failures.duplicates,
        names(&["a", "b"]),
        "the retry flow is unchanged: the duplicate preamble"
    );
    assert_eq!(
        retry.rejections,
        vec![
            ("a".to_string(), RejectionReason::ShadowsChild),
            ("b".to_string(), RejectionReason::ExportedName),
        ],
        "the scope check's reasons travel with the retry, in batch order"
    );
    lane.feed(Ok((renames(&[("a", "freshA"), ("b", "freshB")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![
            LaneEffect::Rename {
                old: "a".into(),
                new: "freshA".into()
            },
            LaneEffect::Rename {
                old: "b".into(),
                new: "freshB".into()
            },
        ]
    );
}

/// The invalid-dead-end (log-proven 4/18): the resolution tail used to
/// send a last suggestion that is a reserved word / global builtin
/// (`self`) straight to identity, though `sanitizeIdentifier` exists for
/// exactly that shape. Sanitize first, then the ladder decides.
#[test]
fn an_invalid_last_suggestion_is_sanitized_in_the_tail() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    // One call allowed (`--max-retries 1`): the invalid answer exhausts
    // the id without a last_suggestion retry, so only the tail can name it.
    let tunables = super::WaveTunables {
        batch_size: 1,
        max_retries: 1,
        ..super::WaveTunables::default()
    };
    let mut lane = Lane::new(names(&["a"]), true).tuned(&tunables);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("a", "self")]), None)), &e);
    assert!(
        lane.next_call().is_none(),
        "an id with a suggestion is no straggler"
    );
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Rename {
            old: "a".into(),
            new: "self_".into()
        }],
        "`self` sanitizes to `self_`, not identity"
    );
}

/// The singleton all-failed window (log-proven: three `t → self` ask
/// survivors): when a window's every id fails, the window is exhausted on
/// the spot — nothing gets its round-2 — and the straggler pass excluded
/// ids holding a suggestion, so an INVALID-failed id got no feedback ask
/// at all. Such an id must be admitted to the straggler pass once: the
/// model is told "self is not allowed" and answers again.
#[test]
fn an_all_failed_window_with_invalid_answers_gets_one_feedback_straggler() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["t"]), true);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("t", "self")]), None)), &e);
    let straggler = lane.next_call().expect("one feedback straggler ask");
    assert_eq!(straggler.batch, names(&["t"]));
    assert_eq!(straggler.failures.invalid, names(&["t"]));
    assert_eq!(
        straggler.prev.0,
        vec![("t".to_string(), "self".to_string())]
    );
    lane.feed(Ok((renames(&[("t", "totalValue")]), None)), &e);
    assert!(
        lane.next_call().is_none(),
        "the feedback ask is bounded to one"
    );
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![LaneEffect::Rename {
            old: "t".into(),
            new: "totalValue".into()
        }]
    );
}

/// Finding #73's module-lane leftover (rep73.py, 17/26/24 per run): the
/// lane's round-2 disclosed the PRIOR-VERSION name the module transform
/// snapped the model's word to — the model said `setupApplication`, the
/// snap made it the prior `setupApplication12` (taken), and the re-ask
/// showed `setupApplication12`. It must disclose the model's own word.
#[test]
fn a_lane_round_two_discloses_the_models_word_not_the_prior_snap() {
    let used = |n: &str| n == "setupApplication12";
    let reject = |_: &str, _: &str| None;
    let snap = |old: &str, s: &str| {
        if old == "z88" && s == "setupApplication" {
            "setupApplication12".to_string()
        } else {
            s.to_string()
        }
    };
    let e = LaneEnv {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        used: &used,
        would_reject: &reject,
        transform: Some(&snap),
    };
    let mut lane = Lane::new(names(&["T88", "z88"]), false);
    lane.next_call().unwrap();
    lane.feed(
        Ok((
            renames(&[("T88", "chalkInstance"), ("z88", "setupApplication")]),
            None,
        )),
        &e,
    );
    let retry = lane.next_call().expect("z88's round-2");
    assert_eq!(retry.batch, names(&["z88"]));
    assert_eq!(retry.failures.duplicates, names(&["z88"]));
    assert_eq!(
        retry.prev.0,
        vec![("z88".to_string(), "setupApplication".to_string())],
        "the model's own word is disclosed, not the snapped prior name"
    );
}

/// Round 2, the IDENTITY ECHO: a window that answers every multi-letter
/// minified name with itself (`{"yl":"yl","zf":"zf"}`, ref r2 2.1.216)
/// used to settle them as identity — a KEEP the sweep never revisits.
/// The lane hands such an echo to the barrier as a refused answer (the
/// echo effect); a single letter keeps its identity record.
#[test]
fn an_echoed_minified_name_leaves_the_lane_as_an_echo_not_identity() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["yl", "zf", "i"]), true);
    lane.next_call().unwrap();
    lane.feed(
        Ok((renames(&[("yl", "yl"), ("zf", "zf"), ("i", "i")]), None)),
        &e,
    );
    assert!(lane.next_call().is_none(), "an all-failed window");
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![
            LaneEffect::Echo { name: "yl".into() },
            LaneEffect::Echo { name: "zf".into() },
            LaneEffect::Identity { name: "i".into() },
        ]
    );
    // A module lane (no identity records) hands its echo on too.
    let mut lane = Lane::new(names(&["yl"]), false);
    lane.next_call().unwrap();
    lane.feed(Ok((renames(&[("yl", "yl")]), None)), &e);
    assert!(lane.next_call().is_none());
    lane.finish(&e);
    assert_eq!(lane.effects, vec![LaneEffect::Echo { name: "yl".into() }]);
}

/// The exhausted-through-round-2 id is NOT re-admitted: it had its
/// disclosed retry already, so only the tail (sanitize + ladder) remains.
#[test]
fn an_invalid_id_exhausted_through_round_two_gets_no_straggler() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| None;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a", "t"]), true);
    lane.next_call().unwrap();
    // `a` succeeds; `t`'s invalid answer gets its round-2 in-window.
    lane.feed(Ok((renames(&[("a", "alpha"), ("t", "self")]), None)), &e);
    let retry = lane.next_call().expect("t's round-2");
    assert_eq!(retry.batch, names(&["t"]));
    lane.feed(Ok((renames(&[("t", "self")]), None)), &e);
    assert!(lane.next_call().is_none(), "no straggler: t had its retry");
    lane.finish(&e);
    assert_eq!(
        lane.effects,
        vec![
            LaneEffect::Rename {
                old: "a".into(),
                new: "alpha".into()
            },
            LaneEffect::Rename {
                old: "t".into(),
                new: "self_".into()
            },
        ]
    );
}
