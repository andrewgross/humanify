//! The registry snapshot: the one retry layer a fetch wave gets, and the
//! registry document → packument conversion the fetch reads from.

use std::collections::BTreeMap;

use super::registry::{registry_url, with_retry};
use crate::asof::Packument;

/// A transient failure is retried ONCE: two attempts, then success.
#[test]
fn a_transient_fetch_failure_is_retried_once() {
    let mut calls = 0;
    let got = with_retry(
        || {
            calls += 1;
            if calls == 1 {
                Err("npm view x: network error".to_string())
            } else {
                Ok(7)
            }
        },
        "npm view x",
    )
    .expect("retried");
    assert_eq!(got, 7);
    assert_eq!(calls, 2, "exactly one retry");
}

/// A persistent failure fails after the second attempt, naming the site.
#[test]
fn a_persistent_fetch_failure_fails_after_two_attempts() {
    let mut calls = 0;
    let err = with_retry(
        || {
            calls += 1;
            Err::<(), String>("registry timeout".to_string())
        },
        "npm view stub-pkg",
    )
    .expect_err("fails loud");
    assert_eq!(calls, 2, "no third attempt");
    assert!(
        err.contains("stub-pkg") && err.contains("registry timeout"),
        "{err}"
    );
}

/// The fetcher reads ONE registry document (what the registry itself
/// serves at /<name>, `npm install`'s source of truth), not npm view:
/// npm view's multi-version output OMITS `dist.unpackedSize` and `os`/
/// `cpu` and drops versions that define none of the requested fields
/// (found on the first default-spec fetch: the platform and size
/// refusals were dead on the npm-view data, fsevents' darwin restriction
/// unread). Every version the doc holds must land in the packument with
/// its metadata — even a version defining none of it.
#[test]
fn a_registry_doc_parses_into_a_complete_packument() {
    let doc: serde_json::Value = serde_json::from_str(
        r#"{
        "_id": "ms", "name": "ms",
        "time": {
            "created": "2016-01-05T20:09:20.123Z",
            "modified": "2025-09-01T10:00:00.000Z",
            "0.1.0": "2016-01-05T20:09:20.123Z",
            "2.1.3": "2023-04-02T01:02:03.004Z",
            "4.0.0": "2025-09-01T10:00:00.000Z"
        },
        "versions": {
            "4.0.0": {
                "name": "ms", "version": "4.0.0",
                "dist": {"unpackedSize": 14671}
            },
            "2.1.3": {
                "name": "ms", "version": "2.1.3",
                "dependencies": {},
                "peerDependencies": {"chalk": "^4"},
                "peerDependenciesMeta": {"chalk": {"optional": true}},
                "os": ["darwin"]
            },
            "0.1.0": {"name": "ms", "version": "0.1.0"}
        }
    }"#,
    )
    .expect("doc");
    let p = Packument::from_doc(&doc, Some("2026-11-02T00:00:00.000Z".into())).expect("packument");
    assert_eq!(p.name, "ms");
    assert_eq!(p.fetched_at.as_deref(), Some("2026-11-02T00:00:00.000Z"));
    assert_eq!(
        p.time.get("created").map(String::as_str),
        Some("2016-01-05T20:09:20.123Z")
    );
    assert_eq!(p.versions, ["0.1.0", "2.1.3", "4.0.0"]);
    assert_eq!(p.deps.len(), 3, "every version gets an entry");
    let four = &p.deps["4.0.0"];
    assert_eq!(four.unpacked_size, Some(14_671), "the size reads");
    assert!(!four.platform);
    let two = &p.deps["2.1.3"];
    assert!(two.platform, "an os restriction marks the platform");
    assert!(
        two.peer_optional.contains("chalk"),
        "an optional peer is marked"
    );
    let zero = &p.deps["0.1.0"];
    assert!(
        !zero.platform && zero.unpacked_size.is_none(),
        "empty meta, kept"
    );
}

/// A scoped name's `/` must be escaped for the registry URL (the
/// registry routes `/@scope/name` as ONE path segment).
#[test]
fn a_scoped_name_encodes_for_the_registry_url() {
    assert_eq!(
        registry_url("@angular/flex-layout"),
        "https://registry.npmjs.org/@angular%2Fflex-layout"
    );
    assert_eq!(registry_url("ms"), "https://registry.npmjs.org/ms");
}

/// A 404/tombstone fetch WRITES an absent packument into the snapshot —
/// the recorded absence replays offline like any other packument.
#[test]
fn an_absent_packument_round_trips_through_its_snapshot_format() {
    let p = Packument {
        name: "gone".into(),
        fetched_at: Some("2026-11-02T00:00:00.000Z".into()),
        time: BTreeMap::new(),
        versions: Vec::new(),
        deps: BTreeMap::new(),
        deps_fetched: true,
        absent: true,
    };
    let text = serde_json::to_string(&p).expect("serialize");
    assert!(text.contains("\"absent\":true"), "{text}");
    let again = Packument::from_json(&text).expect("re-read");
    assert!(again.absent);
}

/// The whole doc round-trips through the snapshot format it is WRITTEN
/// in (the fetch stores the packument, and a later run re-reads it).
#[test]
fn the_parsed_packument_round_trips_through_its_snapshot_format() {
    let doc: serde_json::Value = serde_json::from_str(
        r#"{
        "name": "ms",
        "time": {"0.1.0": "2016-01-05T20:09:20.123Z"},
        "versions": {"0.1.0": {"name": "ms", "version": "0.1.0"}}
    }"#,
    )
    .expect("doc");
    let p = Packument::from_doc(&doc, None).expect("packument");
    let text = serde_json::to_string(&p).expect("serialize");
    let again = Packument::from_json(&text).expect("re-read");
    assert_eq!(p, again);
}

/// An UNPUBLISHED package's tombstone: the registry answers 200 with a
/// doc that has a `time` (holding an `unpublished` entry) and NO
/// `versions` map. It is absence, not a broken fetch — the planner
/// skips and records it like any other 404 (208 of these surfaced on
/// the first default-spec fetch, exp184).
#[test]
fn an_unpublished_tombstone_doc_reads_as_absent() {
    let doc: serde_json::Value = serde_json::from_str(
        r#"{
        "_id": "ability-aloud", "name": "ability-aloud",
        "time": {
            "created": "2024-01-31T19:40:31.753Z",
            "modified": "2024-04-10T06:56:41.554Z",
            "unpublished": {"time": "2024-04-10T06:56:41.554Z", "versions": []}
        }
    }"#,
    )
    .expect("doc");
    let err = Packument::from_doc(&doc, None).expect_err("tombstone");
    assert!(
        err.contains(crate::registry::NOT_FOUND),
        "the error carries the not-found marker: {err}"
    );
}
