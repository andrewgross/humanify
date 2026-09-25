//! The transfer stage's fixture tests. TS originals:
//! `src/rename/transfer-pipeline.test.ts` (the registry, the stranded-temp
//! retry case) and the probe-frozen synthetic pairs
//! (test/parity/wp32-transfer-probe.ts → wp32-transfers.json — the REAL TS
//! `applyPriorVersionIfPresent` trail on seven small pairs, one tier each).
//! The four-pair bundle-scale gate is `humanify transfers` +
//! `humanify-parity compare --sections transfers.mechanical,votes`.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use humanify_model::dump::TransferRow;
use oxc_allocator::Allocator;
use serde_json::Value;

use super::retry::{OnApplied, RejectedTransfer, retry_rejected_transfers};
use super::rows::Rows;
use super::{TRANSFER_PIPELINE, TransferRun, apply_prior_version};
use crate::graph::{Eligibility, build_unified_graph_with_eligibility};
use crate::ingest::Ingest;
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::rename::validated::{RejectionReason, RenameState};
use crate::trail::Anchor;

fn repo_file(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

#[test]
fn the_registry_declares_the_phase_one_strategies_in_evidence_order() {
    let names: Vec<&str> = TRANSFER_PIPELINE.iter().map(|s| s.name).collect();
    assert_eq!(
        names,
        [
            "statement-twin",
            "exact-match",
            "close-match",
            "binding-cascade",
            "vote-propagation",
            "close-match-suggestions",
            "retry"
        ]
    );
    let mut seen = HashSet::new();
    for step in &TRANSFER_PIPELINE {
        assert!(
            step.description.len() > 20,
            "{} needs a real description",
            step.name
        );
        assert!(seen.insert(step.name), "duplicate step {}", step.name);
    }
}

#[test]
fn the_naming_pipeline_doc_mentions_every_step() {
    let doc = fs::read_to_string(repo_file("docs/naming-pipeline.md")).expect("doc exists");
    for step in &TRANSFER_PIPELINE {
        assert!(
            doc.contains(step.name),
            "docs/naming-pipeline.md is missing {}",
            step.name
        );
    }
}

#[test]
fn synthetic_pairs_match_the_ts_probe() {
    let frozen: Value = serde_json::from_str(
        &fs::read_to_string(repo_file("test/parity/wp32-transfers.json")).expect("probe JSON"),
    )
    .expect("probe JSON parses");
    let cases = frozen.as_array().expect("case list");
    assert!(cases.len() >= 7);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let expected: Vec<TransferRow> =
            serde_json::from_value(case["transfers"].clone()).expect("rows");
        let input = PriorMatchInput {
            fresh: case["fresh"].as_str().unwrap(),
            prior: case["prior"].as_str().unwrap(),
            bundler: None,
            minifier: None,
            visit_optional_calls: false,
        };
        let rows = match_prior_version(input, |stage| {
            let (outcome, _) = apply_prior_version(stage, &Default::default())?;
            Ok(outcome.rename.trail().transfer_rows())
        })
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(rows, expected, "case {name:?}");
    }
}

/// A single-text run (no prior): the retry pass's own fixture.
fn with_run<R>(code: &str, f: impl FnOnce(&mut TransferRun<'_, '_>) -> R) -> R {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty());
    let semantic = ingest.semantic();
    let graph = build_unified_graph_with_eligibility(
        semantic,
        ingest.program,
        "input.js",
        &[],
        Eligibility::All,
    );
    let rename = RenameState::new(semantic, Anchor::Fresh);
    let rows = Rows::build(&graph, semantic, rename.view());
    let (n_fns, n_bindings) = (graph.functions.len(), graph.module_bindings.len());
    let mut run = TransferRun {
        semantic,
        graph: &graph,
        rows,
        rename,
        fn_state: vec![super::lifecycle::Lifecycle::Pending; n_fns],
        fn_transferred: vec![HashSet::new(); n_fns],
        fn_transferred_pairs: vec![None; n_fns],
        fn_prior_context: vec![false; n_fns],
        binding_state: vec![super::lifecycle::Lifecycle::Pending; n_bindings],
        binding_suggested: vec![None; n_bindings],
        retry_queue: Vec::new(),
        refs_twin: Vec::new(),
        refs_exact: Vec::new(),
        refs_close: Vec::new(),
        private_renames: Vec::new(),
        applied_binding_renames: Vec::new(),
        applied_module_votes: Vec::new(),
        suggestions_applied: 0,
        stats_twin: Default::default(),
        stats_exact: Default::default(),
        stats_close: Default::default(),
        stats_retry: Default::default(),
        votes_dump: Vec::new(),
    };
    f(&mut run)
}

#[test]
fn a_retry_temp_never_survives_a_positional_landing_failure() {
    // Swap cycle a<->b: the cycle break temps `a`, the mate lands on `a`,
    // and the temped entry's landing on `b` then fails for good — inner()
    // binds `b` around a reference to the temped binding (shadows-child
    // exists only on the wanted name). Restoration must produce a real
    // name: the original is taken by the mate, so the wanted name
    // decorated through the owner ladder.
    let code = "var a = 1; var b = 2; function inner() { var b = 9; return a + b; }";
    with_run(code, |run| {
        let scope = run.rename.view().program_scope();
        let entry = |run: &TransferRun<'_, '_>, old: &str, new: &str| RejectedTransfer {
            scope,
            old_name: old.to_string(),
            new_name: new.to_string(),
            binding: run.rename.binding_in(scope, old).expect("bound"),
            last_reason: RejectionReason::TargetInScope,
            on_applied: OnApplied::Propagated { row: 0 },
        };
        let queue = vec![entry(run, "a", "b"), entry(run, "b", "a")];
        let stats = retry_rejected_transfers(run, queue);
        let names: Vec<String> = run
            .rename
            .bindings_in(scope)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert!(
            !names.iter().any(|n| n.starts_with("__hf_retry_")),
            "a retry temp survived: {names:?}"
        );
        assert!(
            names.contains(&"a".to_string()),
            "the mate lands on the freed name: {names:?}"
        );
        assert!(stats.applied >= 1);
    });
}

#[test]
fn the_owned_binding_map_keeps_first_names_and_skips_nested_functions() {
    let code = "function f(p) { var v; { let blk; } function g() { let inG; } \
                for (let it of p) {} try {} catch (err) {} }";
    with_run(code, |run| {
        let f = run
            .rows
            .fns
            .iter()
            .position(|r| r.session_id.ends_with(":1:0"))
            .expect("f row");
        let names: Vec<String> =
            super::owned::build_owned_binding_map(&run.rename, &run.rows.fns[f])
                .into_iter()
                .map(|(n, _)| n)
                .collect();
        assert_eq!(names, ["p", "v", "g", "blk", "it", "err", "f"]);
    });
}
