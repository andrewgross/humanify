//! The committed slice of the WP4.1 replay gate: 45 real TS dispatches (the
//! smallest row of every request shape across the four oracle pairs — naming
//! with callee snippets, retries with failures/previousAttempt/promptBody,
//! prior-version fields, contextVars, alreadyRenamed/priorNameHints, the
//! vendor and split namers' explicit prompts), their standing-cache entries
//! and the TS CachedLLMProvider's answers. Picked by
//! test/parity/wp41-pick-fixture.mjs from the full-material capture
//! (test/parity/wp41-capture-pair.sh). The full four-pair run is the CLI's
//! `llm-replay-gate` (test/parity/wp41-run-gate.sh).

use std::path::PathBuf;

use crate::replay_gate::run;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/parity/wp41-replay")
        .join(name)
}

#[test]
fn every_committed_ts_dispatch_replays_byte_identical() {
    let report = run(
        &fixture("requests.jsonl"),
        None,
        &fixture("ts-replay.jsonl"),
        &fixture("cache"),
    )
    .unwrap();
    assert!(report.identical(), "{}", report.summary());
    assert_eq!(report.rows, 45);
    assert_eq!(report.keys_identical, 45);
    assert_eq!(report.hits, 45);
    assert_eq!(report.entries_roundtrip_identical, 45);
    assert_eq!(report.cache_writes, 0);
}

/// The fixture carries callee snippets — the key material the oracle dump
/// dropped. Stripping them must break the keys (the gate's red).
#[test]
fn dropping_the_callee_snippet_breaks_the_key() {
    use humanify_model::llm::cache_key_of;
    let text = std::fs::read_to_string(fixture("requests.jsonl")).unwrap();
    let mut with_callees = 0;
    for line in text.lines() {
        let mut row: crate::replay_gate::KeyRow = serde_json::from_str(line).unwrap();
        if row.request.callee_signatures.is_empty() {
            continue;
        }
        with_callees += 1;
        assert_eq!(cache_key_of(&row.request, &row.params), row.cache_key);
        for callee in &mut row.request.callee_signatures {
            callee.snippet = None;
        }
        assert_ne!(cache_key_of(&row.request, &row.params), row.cache_key);
    }
    assert!(
        with_callees >= 5,
        "the fixture must exercise callee snippets ({with_callees})"
    );
}
