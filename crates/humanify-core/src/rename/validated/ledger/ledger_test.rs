//! The rename ledger's tests, ported from `src/rename/rename-ledger.test.ts`
//! against the TS probe (`test/parity/wp31-ledger-probe.mjs`, frozen at
//! `test/parity/wp31-ledger.json`). The TS renames with Babel's
//! `scope.rename` (`renameAll`) and emits with Babel's generator; the Rust
//! applies the same renames through the validated overlay, derives its own
//! ledger, and must produce the TS ledger's entries AND replay onto the
//! source to the TS GENERATED output (there is no Rust generator before
//! WP5.3 — the TS output is the oracle the replay must hit).

use serde_json::Value;

use crate::rename::validated::ledger::{
    LedgerError, LedgerStage, RenameLedger, RenameLedgerBundle, apply_rename_ledger,
    build_rename_ledger, sha256_hex,
};
use crate::rename::validated::scopes::BScopeId;
use crate::rename::validated::test_support::with_semantic;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::Anchor;

/// `renameAll`: collect (scope, name → to) over scopes in pre-order and
/// each scope's bindings in map order, then rename in that order.
fn rename_all(source: &str, renames: &[(String, String)]) -> (RenameLedger, String) {
    with_semantic(source, false, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let mut pending: Vec<(BScopeId, String, String)> = Vec::new();
        for s in 0..state.view().scopes.len() as u32 {
            for (name, _) in state.bindings_in(BScopeId(s)) {
                if let Some((_, to)) = renames.iter().find(|(from, _)| *from == name) {
                    pending.push((BScopeId(s), name, to.clone()));
                }
            }
        }
        for (scope, from, to) in &pending {
            let attempt = state.attempt_validated_rename(
                RenameRequest {
                    scope: *scope,
                    old_name: from,
                    new_name: to,
                    expected: None,
                },
                TrailSpec::Untrailed {
                    why: "ledger fixture",
                },
            );
            assert!(attempt.applied, "{from}→{to}: {:?}", attempt.reason);
        }
        let ledger = build_rename_ledger(source, &state);
        let replayed = apply_rename_ledger(source, &ledger).expect("replays");
        (ledger, replayed)
    })
}

fn renames_of(stage: &Value) -> Vec<(String, String)> {
    stage["renames"]
        .as_array()
        .expect("renames")
        .iter()
        .map(|pair| {
            (
                pair[0].as_str().expect("from").to_string(),
                pair[1].as_str().expect("to").to_string(),
            )
        })
        .collect()
}

/// Every rename-ledger.test.ts case (records one entry per renamed binding;
/// captures every occurrence; skips unchanged bindings; pins the source
/// hash; replays to the generated output exactly; chains an output-space
/// stage) plus a destructuring/redeclaration/for-of probe: the Rust ledger
/// equals the TS ledger entry for entry, and each stage (and the chained
/// ledger) replays to the TS generated output.
#[test]
fn ledgers_match_the_ts_probe_and_replay_to_its_output() {
    let raw = include_str!("../../../../../../test/parity/wp31-ledger.json");
    let probe: Value = serde_json::from_str(raw).expect("fixture parses");
    for case in probe["cases"].as_array().expect("cases") {
        let label = &case["label"];
        let stages = case["stages"].as_array().expect("stages");
        let mut chained: Option<RenameLedger> = None;
        let first_source = stages[0]["source"].as_str().expect("source");
        for stage in stages {
            let source = stage["source"].as_str().expect("source");
            let (ledger, replayed) = rename_all(source, &renames_of(stage));
            let ts_ledger: RenameLedger =
                serde_json::from_value(stage["ledger"].clone()).expect("ts ledger shape");
            assert_eq!(ledger, ts_ledger, "{label}: ledger differs");
            if stage["fixedPoint"] == true {
                assert_eq!(
                    replayed,
                    stage["output"].as_str().expect("output"),
                    "{label}: replay differs from the TS generated output"
                );
            }
            chained = Some(match chained {
                None => ledger,
                Some(mut base) => {
                    base.post.get_or_insert_with(Vec::new).push(LedgerStage {
                        source_sha256: ledger.source_sha256,
                        entries: ledger.entries,
                    });
                    base
                }
            });
        }
        let last_output = stages.last().expect("a stage")["output"]
            .as_str()
            .expect("output");
        let chained = chained.expect("a stage");
        if stages[0]["fixedPoint"] != true {
            continue;
        }
        assert_eq!(
            apply_rename_ledger(first_source, &chained).expect("chain replays"),
            last_output,
            "{label}: the chained ledger must reproduce the FINAL output"
        );
    }
}

/// TS: "pins the source hash so a mismatched snapshot is detectable".
#[test]
fn pins_the_source_hash() {
    let (ledger, _) = rename_all("var q = 1;\n", &[("q".into(), "quantity".into())]);
    assert_eq!(ledger.version, 1);
    assert_eq!(ledger.source_sha256.len(), 64);
    assert!(ledger.source_sha256.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(ledger.source_sha256, sha256_hex("var q = 1;\n"));
}

/// TS: "throws when applied to a source that does not match the ledger hash".
#[test]
fn refuses_a_source_that_does_not_match_the_hash() {
    let (ledger, _) = rename_all("var x = 1;\n", &[("x".into(), "count".into())]);
    assert_eq!(
        apply_rename_ledger("var y = 1;\n", &ledger),
        Err(LedgerError::SourceMismatch)
    );
}

/// TS: "is a no-op for an empty ledger".
#[test]
fn an_empty_ledger_is_a_no_op() {
    let source = "var unchanged = 1;\n";
    let ledger = RenameLedger {
        version: 1,
        source_sha256: sha256_hex(source),
        entries: Vec::new(),
        post: None,
    };
    assert_eq!(apply_rename_ledger(source, &ledger).as_deref(), Ok(source));
}

/// TS: "verifies each stage's hash — a tampered intermediate throws".
#[test]
fn a_tampered_intermediate_stage_is_refused() {
    let source = "var x = 1;\nx;\n";
    let (mut ledger, _) = rename_all(source, &[("x".into(), "count".into())]);
    ledger.post = Some(vec![LedgerStage {
        source_sha256: "deadbeef".repeat(8),
        entries: Vec::new(),
    }]);
    assert_eq!(
        apply_rename_ledger(source, &ledger),
        Err(LedgerError::SourceMismatch)
    );
}

/// Spans are UTF-8 byte offsets (07 §1): a non-ASCII string before the
/// renamed binding shifts byte offsets, and the replay still lands.
#[test]
fn replay_uses_utf8_byte_offsets() {
    let source = "var s = \"héllo 𝄞\";\nvar a = s;\nconsole.log(a);\n";
    let (ledger, replayed) = rename_all(source, &[("a".into(), "greeting".into())]);
    assert_eq!(
        replayed,
        "var s = \"héllo 𝄞\";\nvar greeting = s;\nconsole.log(greeting);\n"
    );
    let entry = &ledger.entries[0];
    // `é` is 2 bytes / 1 UTF-16 unit, `𝄞` 4 bytes / 2 units: the binding
    // sits at byte 27 (UTF-16 would say 24).
    assert_eq!(
        entry.occurrences[0],
        [27, 28],
        "UTF-8 bytes, not UTF-16 units"
    );
}

/// `--rename-ledger`'s `rename-ledger.json` is `JSON.stringify(ledger)` of
/// the TS's ledger: its offsets are JS string indexes (UTF-16 units) into
/// each stage's own source — the base stage's the fresh text, a post
/// stage's the text that pass renamed — and its keys in the TS order.
#[test]
fn the_written_ledger_is_the_ts_json_in_utf16_units() {
    let source = "var s = \"héllo 𝄞\";\nvar a = s;\nconsole.log(a);\n";
    let (base, stage1) = rename_all(source, &[("a".into(), "greeting".into())]);
    let (post, _) = rename_all(&stage1, &[("s".into(), "text".into())]);
    let mut ledger = base;
    ledger.post = Some(vec![LedgerStage {
        source_sha256: post.source_sha256.clone(),
        entries: post.entries,
    }]);
    let bundle = RenameLedgerBundle {
        ledger,
        source: source.to_string(),
        stage_sources: vec![stage1.clone()],
    };
    let expected = format!(
        concat!(
            r#"{{"version":1,"sourceSha256":"{}","entries":[{{"originalName":"a","finalName":"greeting","occurrences":[[24,25],[43,44]]}}],"#,
            r#""post":[{{"sourceSha256":"{}","entries":[{{"originalName":"s","finalName":"text","occurrences":[[4,5],[35,36]]}}]}}]}}"#
        ),
        sha256_hex(source),
        sha256_hex(&stage1)
    );
    assert_eq!(bundle.to_ts_json(), expected);
}

/// The entry order is the ledger walk's `Object.keys(scope.bindings)`: on a
/// LIVE scope a rename moves the name to the end (renamed in application
/// order), but once a big parse cleared Babel's path/scope cache (the TS
/// `parseSourceAst` funnel, sources >= `BIG_SOURCE_BYTES`) the walk
/// re-crawls every scope — registration (declaration) order. Found on the
/// four pairs: the TS base stage lists `Go9, Ro9, _4H` (declaration order).
#[test]
fn a_recrawled_walk_lists_entries_in_declaration_order() {
    use crate::rename::validated::ledger::{BIG_SOURCE_BYTES, parse_clears_scope_cache};
    let source = "var a = 1;\nvar b = 2;\nuse(a, b);\n";
    with_semantic(source, false, |semantic| {
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        let program = state.view().program_scope();
        for (from, to) in [("b", "second"), ("a", "first")] {
            let attempt = state.attempt_validated_rename(
                RenameRequest {
                    scope: program,
                    old_name: from,
                    new_name: to,
                    expected: None,
                },
                TrailSpec::Untrailed { why: "order" },
            );
            assert!(attempt.applied);
        }
        let live: Vec<String> = build_rename_ledger(source, &state)
            .entries
            .iter()
            .map(|e| e.final_name.clone())
            .collect();
        assert_eq!(live, ["second", "first"], "rename order on a live scope");
        state.recrawl_order(|_| true);
        let crawled: Vec<String> = build_rename_ledger(source, &state)
            .entries
            .iter()
            .map(|e| e.final_name.clone())
            .collect();
        assert_eq!(
            crawled,
            ["first", "second"],
            "declaration order after a re-crawl"
        );
    });
    assert!(!parse_clears_scope_cache("x"));
    assert!(parse_clears_scope_cache(&"x".repeat(BIG_SOURCE_BYTES)));
    // JS `.length`: UTF-16 units, not bytes.
    assert!(!parse_clears_scope_cache(&"é".repeat(BIG_SOURCE_BYTES - 1)));
}
