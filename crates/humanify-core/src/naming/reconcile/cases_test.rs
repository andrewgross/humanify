//! The reconcile's decisions against the real TS's on constructed cases
//! (test/parity/wp54-reconcile-cases.json, written by
//! test/parity/wp54-reconcile-cases-probe.ts): renames with kind/votes/
//! declLine/applied, skips with their reasons in order, the hunk stats —
//! under the option presets its two callers use (the pipeline step, the
//! post-split TEXT form) plus the TS defaults and a dry run.

use std::fs;
use std::path::PathBuf;

use oxc_allocator::Allocator;
use serde_json::Value;

use super::hunks::compute_normal_diff;
use super::{ReconcileOptions, collect_word_tokens, reconcile_diff_noise};
use crate::ingest::Ingest;
use crate::rename::eligibility::Eligibility;
use crate::rename::validated::RenameState;
use crate::trail::Anchor;

fn fixture() -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/parity/wp54-reconcile-cases.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn options(preset: &str, prior: &str) -> ReconcileOptions {
    let post = preset == "post";
    ReconcileOptions {
        apply: preset != "dry",
        descriptive_tier: true,
        max_hunk_lines: 10,
        mixed_hunk_tier: post || preset == "mixed",
        prior_line_count: post.then(|| prior.split('\n').count()),
        consumer_tier: post,
        prior_names: post.then(|| collect_word_tokens(prior)),
        last_resort_tier: post,
        skip_import_declarations: post,
        skeleton_vote_tier: post,
        plant: None,
    }
}

fn run_case(preset: &str, prior: &str, next: &str) -> Value {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, next);
    assert!(ingest.errors.is_empty());
    let mut state = RenameState::new(ingest.semantic(), Anchor::Generated);
    let diff = compute_normal_diff(prior, next).unwrap();
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let opts = options(preset, prior);
    let result = reconcile_diff_noise(ingest.semantic(), &mut state, &diff, &eligible, &opts);
    let renames: Vec<Value> = result
        .renames
        .iter()
        .map(|r| {
            serde_json::json!({
                "fromName": r.from_name, "toName": r.to_name, "votes": r.votes,
                "kind": r.kind.as_str(), "declLine": r.decl_line, "applied": r.applied,
            })
        })
        .collect();
    let skipped: Vec<Value> = result
        .skipped
        .iter()
        .map(|s| {
            serde_json::json!({
                "fromName": s.from_name, "toName": s.to_name, "reason": s.reason, "votes": s.votes,
            })
        })
        .collect();
    serde_json::json!({
        "renames": renames,
        "skipped": skipped,
        "priorTooDissimilar": result.prior_too_dissimilar,
        "noise": result.hunks.noise,
        "genuine": result.hunks.genuine,
        "oversized": result.hunks.oversized,
        "mixed": result.hunks.mixed,
        "tainted": result.hunks.tainted,
    })
}

#[test]
fn decisions_match_the_ts() {
    let rows = fixture();
    assert!(rows.len() >= 15);
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let got = run_case(
            row["preset"].as_str().unwrap(),
            row["prior"].as_str().unwrap(),
            row["next"].as_str().unwrap(),
        );
        assert_eq!(got["renames"], row["renames"], "{name}: renames");
        assert_eq!(got["skipped"], row["skipped"], "{name}: skips");
        assert_eq!(
            got["priorTooDissimilar"], row["priorTooDissimilar"],
            "{name}"
        );
        for key in ["noise", "genuine", "oversized", "mixed", "tainted"] {
            assert_eq!(got[key], row["hunks"][key], "{name}: hunks.{key}");
        }
    }
}
