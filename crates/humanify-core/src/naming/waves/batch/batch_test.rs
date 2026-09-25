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
    assert_eq!(WaveTunables::default().batch_size, 10);
    assert_eq!(WaveTunables::default().lane_threshold, 25);
}
