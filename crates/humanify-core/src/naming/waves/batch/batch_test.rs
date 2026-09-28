//! The lane loop's decisions (processor.ts runBatchRenameLoop and its
//! helpers; the processor.test.ts batch-loop cases in miniature).

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

fn env<'e>(used: &'e dyn Fn(&str) -> bool, reject: &'e dyn Fn(&str, &str) -> bool) -> LaneEnv<'e> {
    LaneEnv {
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
    let reject = |_: &str, _: &str| false;
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
    let reject = |_: &str, _: &str| false;
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
    let reject = |_: &str, _: &str| false;
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
    let reject = |_: &str, _: &str| false;
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

#[test]
fn a_used_suggestion_resolves_through_the_conflict_ladder() {
    let used = |n: &str| n == "taken";
    let reject = |_: &str, _: &str| false;
    let e = env(&used, &reject);
    let mut lane = Lane::new(names(&["a"]), false);
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

/// `--batch-size` / `--max-retries` (processor.ts runBatchRenameLoop's
/// `options.batchSize ?? DEFAULT_BATCH_SIZE` and
/// `options.maxRetriesPerIdentifier ?? DEFAULT_MAX_RETRIES_PER_ID`): the
/// window is the configured size, and an identifier gets exactly the
/// configured number of calls.
#[test]
fn the_configured_batch_size_and_retry_limit_shape_the_loop() {
    use super::WaveTunables;
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| false;
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
    let reject = |old: &str, new: &str| old == "a" && new == "taken";
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

/// The invalid-dead-end (log-proven 4/18): the resolution tail used to
/// send a last suggestion that is a reserved word / global builtin
/// (`self`) straight to identity, though `sanitizeIdentifier` exists for
/// exactly that shape. Sanitize first, then the ladder decides.
#[test]
fn an_invalid_last_suggestion_is_sanitized_in_the_tail() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| false;
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
    let reject = |_: &str, _: &str| false;
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

/// The exhausted-through-round-2 id is NOT re-admitted: it had its
/// disclosed retry already, so only the tail (sanitize + ladder) remains.
#[test]
fn an_invalid_id_exhausted_through_round_two_gets_no_straggler() {
    let used = |_: &str| false;
    let reject = |_: &str, _: &str| false;
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
