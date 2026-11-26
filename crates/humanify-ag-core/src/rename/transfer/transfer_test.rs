//! The transfer stage's fixture tests. TS originals:
//! `src/rename/transfer-pipeline.test.ts` (the registry, the stranded-temp
//! retry case) and the probe-frozen synthetic pairs
//! (test/parity/wp32-transfer-probe.ts → wp32-transfers.json — the REAL TS
//! `applyPriorVersionIfPresent` trail on seven small pairs, one tier each).
//! The four-pair bundle-scale gate is `humanify-ag transfers` +
//! `humanify-parity compare --sections transfers.mechanical,votes`.
// io: test code reads fixtures and writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use humanify_ag_model::dump::TransferRow;
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
            layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
            fresh: case["fresh"].as_str().unwrap(),
            prior: case["prior"].as_str().unwrap(),
            never_rename: crate::rename::eligibility::NeverRename::UNIVERSAL,
            same_program_check: true,
        };
        let rows = match_prior_version(input, |stage| {
            let (outcome, _) = apply_prior_version(
                stage,
                &Default::default(),
                crate::rename::name_profile::NameProfile::Bun,
            )
            .map_err(|e| e.to_string())?;
            Ok::<_, String>(outcome.rename.trail().transfer_rows())
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
        Eligibility::All,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    );
    let rename = RenameState::new(
        semantic,
        Anchor::Fresh,
        crate::rename::name_profile::NameProfile::Bun,
    );
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

#[test]
fn a_nested_binding_sharing_a_module_bindings_name_never_settles_that_module_row() {
    // Names mode restores library source names inside nested scopes
    // (fflate's `var dt` in a function), and the minified bundle may hold a
    // module binding of the same spelling (`var dt = __esm(...)`). A
    // transfer of the NESTED binding settles the nested binding only: the
    // module row keeps its pending state, so its module vote still runs.
    let code = "var dt = 1; function f() { var dt = 2; return dt; } function g() { return dt; }";
    with_run(code, |run| {
        let row = run.rows.module_by_name["dt"];
        let f = run
            .rows
            .fns
            .iter()
            .position(|r| r.session_id.ends_with(":1:12"))
            .expect("f row");
        let nested = run
            .rename
            .binding_in(run.rows.fns[f].scope, "dt")
            .expect("nested dt");
        let nested_scope = run.rename.scope_of_binding(nested);
        assert_ne!(nested_scope, run.rows.modules[row].scope);
        for on_applied in [
            OnApplied::Cascade {
                scope: nested_scope,
                new_name: "distanceTable".to_string(),
                old_name: "dt".to_string(),
            },
            OnApplied::Twin {
                old_name: "dt".to_string(),
                scope: nested_scope,
                new_name: "distanceTable".to_string(),
                declared_fn: None,
            },
        ] {
            run.apply_on_applied(&on_applied);
            assert!(
                run.binding_state[row].is_pending(),
                "a nested binding's transfer settled the module row: {:?}",
                run.binding_state[row]
            );
        }
        // The module binding's own transfer still settles its row.
        let module_scope = run.rows.modules[row].scope;
        run.apply_on_applied(&OnApplied::Cascade {
            scope: module_scope,
            new_name: "initErrorUtils".to_string(),
            old_name: "dt".to_string(),
        });
        assert!(!run.binding_state[row].is_pending());
    });
}

/// The program-scope names, in declaration order, after the prior
/// transfer of `fresh`.
fn names_after_transfer(prior: &str, fresh: &str) -> Vec<String> {
    let input = PriorMatchInput {
        layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
        fresh,
        prior,
        never_rename: crate::rename::eligibility::NeverRename::UNIVERSAL,
        same_program_check: true,
    };
    match_prior_version(input, |stage| {
        let (outcome, _) = apply_prior_version(
            stage,
            &Default::default(),
            crate::rename::name_profile::NameProfile::Bun,
        )
        .map_err(|e| e.to_string())?;
        let scope = outcome.rename.view().program_scope();
        let mut names = outcome.rename.bindings_in(scope);
        names.sort_by_key(|(_, b)| outcome.rename.view().binding(*b).id_span.start);
        Ok::<_, String>(names.into_iter().map(|(n, _)| n).collect())
    })
    .expect("the pair matches")
}

const WRAPPED_PRIOR: &str = r#"function buildHeaders(token) { return { auth: "Bearer " + token, kind: "json" }; }
function fetchOne(token) { return send("one", buildHeaders(token)); }
function fetchTwo(token) { return send("two", buildHeaders(token), 2); }
fetchOne(fetchTwo(1));
"#;

#[test]
fn a_wrapper_the_old_callers_now_call_takes_the_wrapped_functions_name() {
    let names = names_after_transfer(
        WRAPPED_PRIOR,
        r#"function h(t) { return { auth: "Bearer " + t, kind: "json" }; }
function w(t) { return { agent: "cli", ...h(t) }; }
function f1(t) { return send("one", w(t)); }
function f2(t) { return send("two", w(t), 2); }
f1(f2(1));
"#,
    );
    assert_eq!(names, ["h", "buildHeaders", "fetchOne", "fetchTwo"]);
}

#[test]
fn a_function_an_old_caller_still_calls_keeps_its_name() {
    let prior = format!(
        "{WRAPPED_PRIOR}function fetchThree(token) {{ return send(\"three\", buildHeaders(token), 3, 4); }}\n"
    );
    let names = names_after_transfer(
        &prior,
        r#"function h(t) { return { auth: "Bearer " + t, kind: "json" }; }
function w(t) { return { agent: "cli", ...h(t) }; }
function f1(t) { return send("one", w(t)); }
function f2(t) { return send("two", w(t), 2); }
f1(f2(1));
function f3(t) { return send("three", h(t), 3, 4); }
"#,
    );
    assert_eq!(
        names,
        ["buildHeaders", "w", "fetchOne", "fetchTwo", "fetchThree"]
    );
}

#[test]
fn a_new_function_that_does_not_wrap_the_old_one_takes_no_name_from_it() {
    let names = names_after_transfer(
        WRAPPED_PRIOR,
        r#"function h(t) { return { auth: "Bearer " + t, kind: "json" }; }
function w(t) { return { agent: "cli", token: t }; }
function f1(t) { return send("one", w(t)); }
function f2(t) { return send("two", w(t), 2); }
f1(f2(1));
"#,
    );
    assert_eq!(names, ["buildHeaders", "w", "fetchOne", "fetchTwo"]);
}

#[test]
fn a_wrapper_takes_the_name_of_a_wrapped_function_whose_body_changed() {
    let names = names_after_transfer(
        r#"function resolveSpellings(input) {
  let entry = lookupEntry(input);
  if (!entry.unresolved) { return entry.spellings; }
  let resolved = validate(entry.requested);
  if (resolved.ok && !entry.spellings.includes(resolved.path)) { return [...entry.spellings, resolved.path]; }
  return entry.spellings;
}
function checkOne(input) { return report("one", resolveSpellings(input)); }
function checkTwo(input) { return report("two", resolveSpellings(input), 2); }
checkOne(checkTwo(1));
"#,
        r#"function w(i) { return s(lookupEntry(i)); }
function s(e) {
  if (!e.unresolved) { return e.spellings; }
  let r = validate(e.requested);
  if (r.ok && !e.spellings.includes(r.path)) { return [...e.spellings, r.path]; }
  return e.spellings;
}
function c1(i) { return report("one", w(i)); }
function c2(i) { return report("two", w(i), 2); }
c1(c2(1));
"#,
    );
    assert_eq!(names, ["resolveSpellings", "s", "checkOne", "checkTwo"]);
}

const CLASS_PRIOR: &str = r#"class SleepInhibitor {
  acquire(owner) { this.count = this.count + 1; return this.spawn(owner, "acquire"); }
  release(owner) { this.count = this.count - 1; return this.stop(owner, "release"); }
  spawn(owner, why) { return [owner, why, "caffeinate"]; }
}
use(new SleepInhibitor());
"#;

#[test]
fn a_changed_class_takes_the_name_of_the_class_its_methods_match() {
    let names = names_after_transfer(
        CLASS_PRIOR,
        r#"class q {
  acquire(o) { this.count = this.count + 1; return this.spawn(o, "acquire"); }
  release(o) { this.count = this.count - 1; return this.stop(o, "release"); }
  spawn(o, w, t) { return [o, w, "systemd-inhibit", t]; }
}
use(new q(), 2);
"#,
    );
    assert_eq!(names, ["SleepInhibitor"]);
}

#[test]
fn an_unrelated_class_sharing_one_method_takes_no_name_from_it() {
    let names = names_after_transfer(
        CLASS_PRIOR,
        r#"class q {
  acquire(o) { this.count = this.count + 1; return this.spawn(o, "acquire"); }
  render(p, n) { this.frame = paint(p.rows, n); return this.frame.join("\n"); }
  resize(c, r) { this.columns = c; this.rows = r; return layout(this, "resize"); }
  clear() { this.frame = []; return write("\x1b[2J"); }
}
use(new q(), 2);
"#,
    );
    assert_eq!(names, ["q"]);
}
