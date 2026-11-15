//! The post-split reconcile + bundle carry against the real TS on one
//! constructed tree (test/parity/wp54-postsplit.json, written by
//! test/parity/wp54-postsplit-probe.ts): the shipped file text, the rename
//! trail with top-level flags and locators, the patched ledger's exact
//! bytes, and the carried bundle. The real-scale regimes are the gate
//! (/work/wp54/regimes.sh); this pins the mechanics at unit scale.
// io: test code reads fixtures and writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use humanify_ag_io::diff::compute_normal_diff as gnu_diff;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::{PostSplitInput, post_split_reconcile};
use crate::finish::carry::carry_renames_into_bundle;
use crate::place::ledger::SplitLedger;
use crate::rename::eligibility::Eligibility;

fn fixture() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-postsplit.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// The ledger the run writes, from its text.
fn ledger(text: &str) -> SplitLedger {
    serde_json::from_str(text).unwrap()
}

/// The ledger's written bytes.
fn written(ledger: &SplitLedger) -> String {
    ledger.to_json()
}

/// A ledger with every always-written field, naming `files` and no
/// statements.
fn bare_ledger(files: &[&str]) -> SplitLedger {
    let quoted: Vec<String> = files.iter().map(|f| format!("\"{f}\"")).collect();
    ledger(&format!(
        "{{\"version\":1,\"files\":[{}],\"nameToFiles\":{{}},\"order\":[],\"hashes\":[],\"emitHashes\":[],\"emitNames\":[],\"hashVersion\":3}}",
        quoted.join(",")
    ))
}

/// The probe file's `ledgerIn`: the ledger the TS wrote, which carries no
/// `hashVersion` (this binary always writes one).
fn probe_ledger_in(raw: &str) -> SplitLedger {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct TsLedger {
        version: u64,
        files: Vec<String>,
        name_to_files: humanify_ag_model::llm::JsRecord<Vec<String>>,
        order: Vec<String>,
        hashes: Vec<String>,
        emit_hashes: Vec<String>,
        emit_names: Vec<Option<String>>,
        emit_indexes: Option<Vec<usize>>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Probe {
        ledger_in: TsLedger,
    }
    let ts = serde_json::from_str::<Probe>(raw).unwrap().ledger_in;
    SplitLedger {
        version: ts.version,
        files: ts.files,
        name_to_files: ts.name_to_files,
        order: ts.order,
        hashes: ts.hashes,
        emit_hashes: ts.emit_hashes,
        emit_names: ts.emit_names,
        hash_version: 3,
        fossil_modules: None,
        emit_indexes: ts.emit_indexes,
    }
}

/// The TS probe's ledgers carry no `hashVersion`; this binary always
/// writes one, after `emitNames`.
fn with_hash_version(ts_ledger: &str) -> String {
    ts_ledger.replacen(
        ",\"emitIndexes\":",
        ",\"hashVersion\":3,\"emitIndexes\":",
        1,
    )
}

fn files(v: &Value) -> HashMap<String, String> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect()
}

#[test]
fn reconcile_and_carry_match_the_ts() {
    let fx = fixture();
    let fresh = files(&fx["fresh"]);
    let prior = files(&fx["prior"]);
    let raw = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-postsplit.json"),
    )
    .unwrap();
    let mut ledger = probe_ledger_in(&raw);
    let read_fresh = |f: &str| fresh.get(f).cloned();
    let read_prior = |f: &str| prior.get(f).cloned();
    let eligible = Eligibility::new(crate::rename::eligibility::NeverRename::UNIVERSAL);
    let result = post_split_reconcile(PostSplitInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        ledger: &mut ledger,
        read_fresh: &read_fresh,
        read_prior: &read_prior,
        eligible: &eligible,
        disabled: false,
        differ: &gnu_diff,
    });
    let changed: HashMap<String, String> = result.changed.iter().cloned().collect();
    assert_eq!(changed, files(&fx["changed"]));
    let renames: Vec<Value> = result
        .renames
        .iter()
        .map(|r| {
            let mut o = serde_json::json!({
                "file": r.file, "fromName": r.from_name, "toName": r.to_name,
                "kind": r.kind, "votes": r.votes, "topLevel": r.top_level,
            });
            if let Some((b, n)) = r.locator {
                o["locator"] = serde_json::json!({"bodyOrdinal": b, "nameOrdinal": n});
            }
            o
        })
        .collect();
    assert_eq!(Value::Array(renames), fx["renames"]);
    assert_eq!(
        written(&ledger),
        with_hash_version(fx["ledgerOut"].as_str().unwrap())
    );
    assert_eq!(
        result.stats.considered as u64,
        fx["stats"]["considered"].as_u64().unwrap()
    );

    let bundle = fx["bundle"].as_str().unwrap();
    let carry = carry_renames_into_bundle(
        bundle,
        &ledger,
        &result.renames,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )
    .unwrap();
    assert_eq!(carry.code.as_deref(), fx["carry"]["code"].as_str());
    assert_eq!(
        carry.carried as u64,
        fx["carry"]["carried"].as_u64().unwrap()
    );
    let abstained: Vec<Value> = carry
        .abstained
        .iter()
        .map(|(r, n)| serde_json::json!([r, n]))
        .collect();
    assert_eq!(Value::Array(abstained), fx["carry"]["abstained"]);
}

#[test]
fn the_kill_switch_does_nothing() {
    let mut ledger = bare_ledger(&["a.js"]);
    let read = |_: &str| Some("var a = 1;\n".to_string());
    let eligible = Eligibility::new(crate::rename::eligibility::NeverRename::UNIVERSAL);
    let result = post_split_reconcile(PostSplitInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        ledger: &mut ledger,
        read_fresh: &read,
        read_prior: &read,
        eligible: &eligible,
        disabled: true,
        differ: &gnu_diff,
    });
    assert_eq!(result.stats.considered, 0);
    assert!(result.changed.is_empty());
}

/// Finding #28, fixed TS-first 2026-09-25: the old-token read at each
/// identifier's loc was ASCII-only, so in a file with ANY reconcile rename
/// `café` read as `caf` and was "substituted" to `caféé`. The read is now
/// the ECMAScript IdentifierName grammar (`identifierTokenAt`).
#[test]
fn a_non_ascii_identifier_is_left_intact() {
    let fresh =
        "function f(a) {\n  const Xq = a + 1;\n  return Xq;\n}\nvar café = 1;\nuse(café);\n";
    let prior =
        "function f(a) {\n  const total = a + 1;\n  return total;\n}\nvar café = 1;\nuse(café);\n";
    let mut ledger = ledger(
        "{\"version\":1,\"files\":[\"a.js\"],\"nameToFiles\":{},\"order\":[\"a.js\",\"a.js\",\"a.js\"],\"hashes\":[],\"emitHashes\":[],\"emitNames\":[null,null,null],\"hashVersion\":3}",
    );
    let read_fresh = |_: &str| Some(fresh.to_string());
    let read_prior = |_: &str| Some(prior.to_string());
    let eligible = Eligibility::new(crate::rename::eligibility::NeverRename::UNIVERSAL);
    let result = post_split_reconcile(PostSplitInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        ledger: &mut ledger,
        read_fresh: &read_fresh,
        read_prior: &read_prior,
        eligible: &eligible,
        disabled: false,
        differ: &gnu_diff,
    });
    assert_eq!(
        result.changed,
        vec![(
            "a.js".to_string(),
            "function f(a) {\n  const total = a + 1;\n  return total;\n}\nvar café = 1;\nuse(café);\n"
                .to_string()
        )]
    );
}

/// docs/perf-inventory.md item 3's determinism pin: the per-file
/// read+compute runs on the rayon pool (`crate::par::map_ordered`) and
/// the rejoin — the ledger patches, the stale counts, the trail, the
/// claim totals, the changed list, the renames — is serial in LEDGER
/// FILE order. The cross-file state the rejoin touches is the ledger's
/// `nameToFiles`: several files here rename the same `Rb`→`value`, so
/// each file's patch APPENDS the file to the shared `value` list — a
/// completion-order rejoin would permute that list (and every other
/// order-sensitive accumulator) while a ledger-order rejoin reproduces
/// the serial bytes exactly.
///
/// The pin runs the same scenario repeatedly with per-file read DELAYS
/// that scramble the workers' completion order differently each run, and
/// requires every run — including an undelayed one — to agree on every
/// byte: the changed files, the renames WITH locators, the trail, the
/// claim counters, the stats, and the patched ledger's exact `stringify`
/// bytes. The final `nameToFiles["value"]` list is additionally asserted
/// to be the LEDGER order of the renaming files — the canonical rejoin
/// order, so the pin cannot pass vacuously.
/// The scenario for the determinism pin below: six files rename
/// `Rb`→`value` (top-level: patches emitNames AND nameToFiles), three
/// rename `Xq`→`total` inside a function (locator paths, no patch), one
/// is a finding-#31 chain, one is identical to its prior (empty diff),
/// one is missing its prior (skipped) and one is missing its fresh text
/// (skipped). Returns (fresh, prior, the ledger's JSON text).
fn scramble_scenario() -> (HashMap<String, String>, HashMap<String, String>, String) {
    let mut fresh = HashMap::new();
    let mut prior = HashMap::new();
    for i in 0..6 {
        let name = format!("src/a{i}.js");
        fresh.insert(name.clone(), format!("var Rb = {i};\nuse(Rb);\n"));
        prior.insert(name.clone(), format!("var value = {i};\nuse(value);\n"));
    }
    for i in 0..3 {
        let name = format!("src/b{i}.js");
        fresh.insert(
            name.clone(),
            format!("function f(a) {{\n  const Xq = a + {i};\n  return Xq;\n}}\n"),
        );
        prior.insert(
            name.clone(),
            format!("function f(a) {{\n  const total = a + {i};\n  return total;\n}}\n"),
        );
    }
    let chain = |first: &str, second: &str| {
        format!(
            "function f(p) {{\n  {{\n    let {first} = p + 1;\n    g({first});\n  }}\n  {{\n    let {second} = p + 2;\n    h({second});\n  }}\n  {{\n    let value = p + 3;\n    k(value);\n  }}\n}}\n"
        )
    };
    fresh.insert("src/e.js".to_string(), chain("Rb", "value"));
    prior.insert("src/e.js".to_string(), chain("value", "total"));
    for (map, text) in [
        (&mut fresh, "var same = 1;\nuse(same);\n"),
        (&mut prior, "var same = 1;\nuse(same);\n"),
    ] {
        map.insert("src/c.js".to_string(), text.to_string());
    }
    fresh.insert("src/d.js".to_string(), "var only = 1;\n".to_string()); // no prior
    prior.insert(
        "src/g.js".to_string(),
        "var ghost = 1;\nuse(ghost);\n".to_string(),
    ); // no fresh

    // The ledger's file order is SORTED — a stable order that is not the
    // scenario-construction order, so the rejoin is pinned to the
    // LEDGER's order, not something incidental.
    let mut sorted: Vec<&str> = [
        "src/a0.js",
        "src/a1.js",
        "src/a2.js",
        "src/a3.js",
        "src/a4.js",
        "src/a5.js",
        "src/b0.js",
        "src/b1.js",
        "src/b2.js",
        "src/c.js",
        "src/d.js",
        "src/e.js",
        "src/g.js",
    ]
    .to_vec();
    sorted.sort_unstable();
    let quoted: Vec<String> = sorted.iter().map(|f| format!("\"{f}\"")).collect();
    let emit_names: Vec<String> = sorted
        .iter()
        .map(|f| match *f {
            f if f.starts_with("src/a") => "\"Rb\"".to_string(),
            f if f.starts_with("src/b") => "\"f,Xq\"".to_string(),
            "src/c.js" | "src/d.js" | "src/g.js" => "null".to_string(),
            _ => "\"f\"".to_string(),
        })
        .collect();
    let emit_indexes: Vec<String> = (0..sorted.len()).map(|i| i.to_string()).collect();
    let ledger_text = format!(
        "{{\"version\":1,\"files\":[{}],\"nameToFiles\":{{\"Rb\":[\"src/a0.js\",\"src/a1.js\",\"src/a2.js\",\"src/a3.js\",\"src/a4.js\",\"src/a5.js\"],\"Xq\":[\"src/b0.js\",\"src/b1.js\",\"src/b2.js\"]}},\"order\":[{}],\"hashes\":[],\"emitHashes\":[],\"emitNames\":[{}],\"hashVersion\":3,\"emitIndexes\":[{}]}}",
        quoted.join(","),
        quoted.join(","),
        emit_names.join(","),
        emit_indexes.join(",")
    );
    (fresh, prior, ledger_text)
}

/// One full scenario run, with per-file read delays derived from `run` —
/// each run scrambles the workers' completion order differently. `run <
/// 0` is the undelayed baseline. Returns the result and the patched
/// ledger's exact bytes.
// concurrency: test: per-file sleeps scramble completion order on the rayon pool to prove the result is order-independent
#[allow(clippy::disallowed_methods)]
fn scramble_run(
    fresh: &HashMap<String, String>,
    prior: &HashMap<String, String>,
    ledger_text: &str,
    run: i64,
) -> (super::PostSplitResult, String) {
    let sleep_for = move |f: &str| -> u64 {
        if run < 0 {
            return 0;
        }
        let h = f
            .bytes()
            .fold(7u64, |a, b| a.wrapping_mul(31).wrapping_add(u64::from(b)));
        (h.wrapping_add((run as u64).wrapping_mul(0x9E3779B9))) % 17
    };
    let read_fresh = |f: &str| {
        std::thread::sleep(std::time::Duration::from_millis(sleep_for(f)));
        fresh.get(f).cloned()
    };
    let read_prior = |f: &str| {
        std::thread::sleep(std::time::Duration::from_millis(sleep_for(f)));
        prior.get(f).cloned()
    };
    let mut ledger = ledger(ledger_text);
    let eligible = Eligibility::new(crate::rename::eligibility::NeverRename::UNIVERSAL);
    let result = post_split_reconcile(PostSplitInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        ledger: &mut ledger,
        read_fresh: &read_fresh,
        read_prior: &read_prior,
        eligible: &eligible,
        disabled: false,
        differ: &gnu_diff,
    });
    (result, written(&ledger))
}

#[test]
fn results_are_byte_identical_under_scrambled_completion_order() {
    let (fresh, prior, ledger_text) = scramble_scenario();
    let one_run = |run: i64| -> (super::PostSplitResult, String) {
        scramble_run(&fresh, &prior, &ledger_text, run)
    };

    let canonical = one_run(-1);
    for run in 0..5 {
        let scrambled = one_run(run);
        assert_eq!(
            scrambled.0.changed, canonical.0.changed,
            "run {run}: the changed files moved"
        );
        assert_eq!(
            scrambled.0.renames, canonical.0.renames,
            "run {run}: the rename list (with locators) moved"
        );
        assert_eq!(
            scrambled.0.stats, canonical.0.stats,
            "run {run}: the stats moved"
        );
        assert_eq!(
            scrambled.0.claims, canonical.0.claims,
            "run {run}: the claim totals moved"
        );
        assert_eq!(
            format!("{:?}", scrambled.0.trail),
            format!("{:?}", canonical.0.trail),
            "run {run}: the trail moved"
        );
        assert_eq!(
            scrambled.1, canonical.1,
            "run {run}: the patched ledger's bytes moved"
        );
    }

    // The pin must not pass vacuously: the scenario really reconciled.
    let (result, ledger_bytes) = canonical;
    assert_eq!(
        sha2::Digest::finalize(sha2::Digest::chain_update(
            <sha2::Sha256 as sha2::Digest>::new(),
            ledger_bytes.as_bytes()
        ))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>(),
        "2aed7c5501302f66fddae4c7a1a7d19c49e6a7d6926acd03b0fc49f2612b3958",
        "the patched ledger's bytes"
    );
    assert_eq!(result.stats.considered, 11, "one per file with both texts");
    assert!(
        result.changed.len() >= 9,
        "the six a-files and three b-files changed: {:?}",
        result.changed.iter().map(|(f, _)| f).collect::<Vec<_>>()
    );
    assert!(result.renames.iter().any(|r| r.top_level));
    assert!(!result.trail.is_empty(), "the trail recorded rows");
    // THE cross-file state check: `value`'s home list is the renaming
    // files in LEDGER order (the rejoin order), not completion order.
    let ledger: serde_json::Value = serde_json::from_str(&ledger_bytes).unwrap();
    let value_homes: Vec<&str> = ledger["nameToFiles"]["value"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        value_homes,
        vec![
            "src/a0.js",
            "src/a1.js",
            "src/a2.js",
            "src/a3.js",
            "src/a4.js",
            "src/a5.js"
        ],
        "nameToFiles[\"value\"] is the ledger order"
    );
    let rb_homes: Vec<&str> = ledger["nameToFiles"]["Rb"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap()).collect())
        .unwrap_or_default();
    assert!(rb_homes.is_empty(), "every Rb home moved: {rb_homes:?}");
}

/// Finding #31: a rename CHAIN inside one statement — X `a`→`b`, then
/// Y `b`→`c`, with a third binding W still named `b` after Y. The locator's
/// nameOrdinal was counted in the REWRITTEN file, where X already holds
/// `b`: Y read as the 2nd `b` there, but in the bundle (which still has the
/// fresh names) the 2nd `b` is W. The carry then renamed W, not Y — the
/// bundle (the next release's prior) disagreed with the tree while still
/// passing the structural-signature check. The carried bundle must hold
/// exactly the tree's names.
#[test]
fn a_rename_chain_carries_the_binding_the_tree_renamed() {
    let fresh = "function f(p) {\n  {\n    let Rb = p + 1;\n    g(Rb);\n  }\n  {\n    let value = p + 2;\n    h(value);\n  }\n  {\n    let value = p + 3;\n    k(value);\n  }\n}\n";
    let prior = "function f(p) {\n  {\n    let value = p + 1;\n    g(value);\n  }\n  {\n    let total = p + 2;\n    h(total);\n  }\n  {\n    let value = p + 3;\n    k(value);\n  }\n}\n";
    let mut ledger = ledger(
        "{\"version\":1,\"files\":[\"a.js\"],\"nameToFiles\":{},\"order\":[\"a.js\"],\"hashes\":[\"h0\"],\"emitHashes\":[\"h0\"],\"emitNames\":[\"f\"],\"hashVersion\":3,\"emitIndexes\":[0]}",
    );
    let read_fresh = |_: &str| Some(fresh.to_string());
    let read_prior = |_: &str| Some(prior.to_string());
    let eligible = Eligibility::new(crate::rename::eligibility::NeverRename::UNIVERSAL);
    let result = post_split_reconcile(PostSplitInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        ledger: &mut ledger,
        read_fresh: &read_fresh,
        read_prior: &read_prior,
        eligible: &eligible,
        disabled: false,
        differ: &gnu_diff,
    });
    // The tree took the chain: the file now reads exactly as the prior.
    assert_eq!(
        result.changed,
        vec![("a.js".to_string(), prior.to_string())]
    );
    let indent = |s: &str| {
        s.lines()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let bundle = format!("(function () {{\n{}\n}})();\n", indent(fresh));
    let locators: Vec<_> = result.renames.iter().map(|r| r.locator).collect();
    // X is the only `Rb`; Y is the FIRST of the fresh `value`s.
    assert_eq!(locators, vec![Some((0, 0)), Some((0, 0))]);
    let carry = carry_renames_into_bundle(
        &bundle,
        &ledger,
        &result.renames,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )
    .unwrap();
    assert_eq!(carry.carried, 2);
    assert_eq!(
        carry.code.as_deref(),
        Some(format!("(function () {{\n{}\n}})();\n", indent(prior)).as_str())
    );
}

/// Audit R3: a `diff` that fails (cannot start under EAGAIN/ENOMEM, exits
/// 2, or runs past its deadline) used to be counted as a parse discard —
/// a silent hole in the reconcile. It is counted on its own, its first
/// error kept, and the run prints a WARNING naming it.
#[test]
fn a_failed_diff_is_counted_apart_from_discards_and_warned() {
    let mut ledger = bare_ledger(&["a.js", "b.js"]);
    let fresh = |_: &str| Some("var a = 1;\n".to_string());
    let prior = |_: &str| Some("var b = 1;\n".to_string());
    let eligible = Eligibility::new(crate::rename::eligibility::NeverRename::UNIVERSAL);
    let differ = |p: &str, n: &str| {
        let _ = (p, n);
        Err("diff timed out after 600s (killed)".to_string())
    };
    let result = post_split_reconcile(PostSplitInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        ledger: &mut ledger,
        read_fresh: &fresh,
        read_prior: &prior,
        eligible: &eligible,
        disabled: false,
        differ: &differ,
    });
    assert_eq!(result.stats.considered, 2);
    assert_eq!(result.stats.diff_failed, 2);
    assert_eq!(
        result.stats.discarded, 0,
        "a failed diff is not a parse discard"
    );
    let warning = super::diff_failure_warning(&result).expect("a WARNING");
    assert!(warning.starts_with("WARNING:"), "{warning}");
    assert!(warning.contains("2 file(s)"), "{warning}");
    assert!(warning.contains("diff timed out after 600s"), "{warning}");
    assert!(warning.contains("a.js"), "names the first file: {warning}");
}

#[test]
fn a_clean_reconcile_has_no_diff_warning() {
    let mut ledger = bare_ledger(&["a.js"]);
    let read = |_: &str| Some("var a = 1;\n".to_string());
    let eligible = Eligibility::new(crate::rename::eligibility::NeverRename::UNIVERSAL);
    let result = post_split_reconcile(PostSplitInput {
        name_profile: crate::rename::name_profile::NameProfile::Bun,
        ledger: &mut ledger,
        read_fresh: &read,
        read_prior: &read,
        eligible: &eligible,
        disabled: false,
        differ: &gnu_diff,
    });
    assert_eq!(result.stats.diff_failed, 0);
    assert_eq!(super::diff_failure_warning(&result), None);
}
