//! The e2e stage's KNOWN-GAP mechanism and fixture expectations (ported
//! from test/e2e-known-gaps.test.ts): a fixture that records a gap the
//! pipeline has today stays green only while it fails EXACTLY the declared
//! way — a different failure, or a pass, fails the stage.

use std::collections::BTreeMap;

use super::*;
use crate::committed::e2e::Expect;

const GAP: KnownGap = KnownGap {
    fixture: "some-esm",
    spec: "plugin-spec I25",
    reason: "no wrapper",
    error: "no recognizable bundle wrapper",
};

fn files(entries: &[(&str, &str)]) -> Files {
    entries
        .iter()
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .collect()
}

#[test]
fn a_fixture_with_no_gap_and_no_failure_passes() {
    assert_eq!(judge_known_gap(None, None).verdict, Verdict::Pass);
}

#[test]
fn a_fixture_with_no_gap_that_failed_fails_carrying_the_failure() {
    let j = judge_known_gap(None, Some("boom"));
    assert_eq!(j.verdict, Verdict::Fail);
    assert!(j.message.contains("boom"));
}

#[test]
fn the_declared_failure_is_recorded_as_a_known_gap_quoting_it() {
    let failure = "fresh: the binary exited 1\nError: ... has no recognizable bundle wrapper (...)";
    let j = judge_known_gap(Some(&GAP), Some(failure));
    assert_eq!(j.verdict, Verdict::KnownGap);
    assert!(j.message.contains("plugin-spec I25"));
    assert!(
        j.message
            .contains("fails as declared: Error: ... has no recognizable bundle wrapper (...)")
    );
}

#[test]
fn a_known_gap_that_now_fails_differently_fails() {
    let j = judge_known_gap(Some(&GAP), Some("oxc failed to parse the input bundle"));
    assert_eq!(j.verdict, Verdict::Fail);
    assert!(j.message.contains("changed"));
    assert!(j.message.contains("oxc failed to parse"));
}

#[test]
fn a_known_gap_that_now_passes_fails_asking_for_promotion() {
    let j = judge_known_gap(Some(&GAP), None);
    assert_eq!(j.verdict, Verdict::Fail);
    assert!(j.message.contains("now PASSES"));
    assert!(j.message.contains("KNOWN_GAPS"));
}

#[test]
fn stale_known_gaps_names_an_entry_whose_fixture_does_not_exist() {
    assert_eq!(
        stale_known_gaps(&[GAP], &["other".to_string()]),
        vec!["KNOWN_GAPS entry \"some-esm\" names no fixture"]
    );
}

#[test]
fn stale_known_gaps_names_a_fixture_declared_twice() {
    assert_eq!(
        stale_known_gaps(&[GAP, GAP], &["some-esm".to_string()]),
        vec!["KNOWN_GAPS declares \"some-esm\" twice"]
    );
}

#[test]
fn the_committed_known_gaps_name_committed_fixtures() {
    let dir = crate::paths::repo_root().join(crate::paths::E2E_FIXTURES);
    let fixtures: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert!(stale_known_gaps(KNOWN_GAPS, &fixtures).is_empty());
}

fn detection(bundler: &str, adapter: &str) -> Detection {
    Detection {
        bundler: bundler.into(),
        unpack_adapter: adapter.into(),
        ..Detection::default()
    }
}

fn expect() -> Expect {
    Expect {
        bundler: None,
        unpack_adapter: None,
        vendor: None,
        split_method: None,
        apart: None,
        assets: None,
        stays: None,
        placement: None,
    }
}

#[test]
fn no_expectation_no_failure() {
    let observed = detection("unknown", "passthrough");
    let run = ObservedRun {
        vendor_files: 0,
        split_method: None,
    };
    assert!(expectation_failures(None, &observed, &run).is_empty());
}

#[test]
fn each_expectation_the_run_missed_is_named() {
    let e = Expect {
        bundler: Some("esbuild".into()),
        unpack_adapter: Some("esbuild".into()),
        vendor: Some(true),
        split_method: Some("not-split".into()),
        ..expect()
    };
    let run = ObservedRun {
        vendor_files: 0,
        split_method: Some("fresh-grouping".into()),
    };
    assert_eq!(
        expectation_failures(Some(&e), &detection("unknown", "passthrough"), &run),
        vec![
            "detection: bundler is unknown, expected esbuild",
            "toolchain: unpack adapter is passthrough, expected esbuild",
            "unpack: the split tree's vendor/ is empty, expected the extracted dependencies",
            "split: the fresh run's split method is fresh-grouping, expected not-split",
        ]
    );
}

#[test]
fn an_unwritten_split_method_reads_undefined_as_e2e_ts_printed_it() {
    let e = Expect {
        split_method: Some("module-markers".into()),
        ..expect()
    };
    let run = ObservedRun {
        vendor_files: 0,
        split_method: None,
    };
    assert_eq!(
        expectation_failures(Some(&e), &detection("bun", "bun"), &run),
        vec!["split: the fresh run's split method is undefined, expected module-markers"]
    );
}

#[test]
fn every_expectation_holding_is_empty() {
    let e = Expect {
        bundler: Some("bun".into()),
        unpack_adapter: Some("bun".into()),
        vendor: Some(true),
        split_method: Some("module-markers".into()),
        ..expect()
    };
    let run = ObservedRun {
        vendor_files: 26,
        split_method: Some("module-markers".into()),
    };
    assert!(expectation_failures(Some(&e), &detection("bun", "bun"), &run).is_empty());
}

fn pair(a: &str, b: &str) -> Vec<String> {
    vec![a.to_string(), b.to_string()]
}

#[test]
fn apart_names_a_pair_that_shares_a_file() {
    let f = files(&[
        ("src/index.js", "LEDGER# PALETTE#"),
        ("src/archive.js", "ARCHIVE#"),
    ]);
    assert_eq!(
        apart_failures(&[pair("PALETTE#", "LEDGER#")], &f),
        vec!["split: \"PALETTE#\" and \"LEDGER#\" share src/index.js, expected separate files"]
    );
    assert_eq!(
        apart_failures(&[pair("ARCHIVE#", "CODEC#")], &f),
        vec!["split: no file holds \"CODEC#\""]
    );
    assert!(apart_failures(&[pair("ARCHIVE#", "PALETTE#")], &f).is_empty());
}

#[test]
fn asset_failures_name_an_expected_asset_the_tree_lacks() {
    assert_eq!(
        asset_failures(
            &["src/_assets/a.js".into(), "src/_assets/b.js".into()],
            &["src/_assets/a.js".into(), "src/index.js".into()]
        ),
        vec!["unpack: no app text asset src/_assets/b.js"]
    );
}

fn placement(src: &[&str], vendor: &[&str]) -> BTreeMap<String, Vec<String>> {
    let mut m = BTreeMap::new();
    m.insert("src".into(), src.iter().map(|s| s.to_string()).collect());
    m.insert(
        "vendor".into(),
        vendor.iter().map(|s| s.to_string()).collect(),
    );
    m
}

#[test]
fn placement_passes_app_markers_in_src_and_library_markers_in_vendor() {
    let src = files(&[("src/cli.js", "APP-MARK")]);
    let vendor = files(&[("vendor/tinylib/index.js", "LIB-MARK")]);
    assert!(placement_failures(&placement(&["APP-MARK"], &["LIB-MARK"]), &src, &vendor).is_empty());
    assert_eq!(
        placement_failures(
            &placement(&["LIB-MARK", "GONE"], &["APP-MARK"]),
            &src,
            &vendor
        ),
        vec![
            "placement: \"LIB-MARK\" is in vendor/tinylib/index.js, expected src/",
            "placement: no file holds \"GONE\"",
            "placement: \"APP-MARK\" is in src/cli.js, expected vendor/",
        ]
    );
}

#[test]
fn stays_passes_a_vendor_file_that_kept_its_path_and_names_one_that_moved() {
    let fresh = files(&[("vendor/lib_aaaaaaaa.js", "YAML-MARK")]);
    let kept = files(&[("vendor/lib_aaaaaaaa.js", "YAML-MARK v2")]);
    let moved = files(&[("vendor/lib_bbbbbbbb.js", "YAML-MARK v2")]);
    let marks = ["YAML-MARK".to_string()];
    assert!(stays_failures(&marks, &fresh, &kept).is_empty());
    assert_eq!(
        stays_failures(&marks, &fresh, &moved),
        vec![
            "unpack: \"YAML-MARK\" moved from vendor/lib_aaaaaaaa.js to vendor/lib_bbbbbbbb.js across the release"
        ]
    );
    assert_eq!(
        stays_failures(&marks, &fresh, &Files::new()),
        vec!["unpack: no vendor file holds \"YAML-MARK\" in both releases"]
    );
}
