//! `rederive_ts_era_hashes`: a TS-era ledger re-keyed onto the Rust's own
//! statement hashes through a PROVEN bijection, or left untouched.

use std::collections::HashMap;

use super::{FossilLedgerModule, Rederived, StableSplitLedger, rederive_ts_era_hashes};
use crate::hash::statement_hash::STATEMENT_HASH_VERSION;
use crate::place::input::split_input;

/// Five wrapper statements (the first only pads the wrapper past its
/// 50-binding threshold), two of them structurally identical (one class).
fn prior() -> String {
    let pad: Vec<String> = (0..50).map(|i| format!("p{i}")).collect();
    format!(
        "(function () {{\n  var {};\n  var a = 1;\n  var b = 1;\n  function c(x) {{ return x + a; }}\n  c(b);\n}})();",
        pad.join(", ")
    )
}

/// The "TS bytes": a relabelling of the Rust classes (same partition).
fn ts_of(rust: &str) -> String {
    format!("ts-{}", &rust[..6])
}

fn ts_ledger() -> (StableSplitLedger, Vec<String>) {
    let rust = split_input(&prior()).expect("wrapper").hashes;
    assert_eq!(rust.len(), 5);
    assert_eq!(rust[1], rust[2], "the fixture needs one shared class");
    let ts: Vec<String> = rust.iter().map(|h| ts_of(h)).collect();
    let order = vec![
        "src/a.js".to_string(),
        "src/a.js".to_string(),
        "src/a.js".to_string(),
        "src/c.js".to_string(),
        "src/c.js".to_string(),
    ];
    let ledger = StableSplitLedger {
        version: 1,
        files: vec!["src/a.js".into(), "src/c.js".into()],
        name_to_files: HashMap::new(),
        order,
        hashes: Some(ts.clone()),
        emit_hashes: Some(vec![
            ts[0].clone(),
            ts[2].clone(),
            ts[1].clone(),
            ts[4].clone(),
            ts[3].clone(),
        ]),
        hash_version: Some(1),
        fossil_modules: Some(vec![FossilLedgerModule {
            file: "src/c.js".into(),
            hashes: {
                let mut h = vec![ts[4].clone(), ts[3].clone()];
                h.sort();
                h
            },
            ..FossilLedgerModule::default()
        }]),
        ..StableSplitLedger::default()
    };
    (ledger, rust)
}

#[test]
fn a_ts_era_ledger_is_rekeyed_class_for_class() {
    let (mut ledger, rust) = ts_ledger();
    let report = rederive_ts_era_hashes(&mut ledger, &prior()).expect("bijection");
    assert_eq!(
        report,
        Rederived {
            statements: 5,
            classes: 4
        }
    );
    assert!(ledger.hashes_current());
    assert_eq!(ledger.hash_version, Some(STATEMENT_HASH_VERSION));
    assert_eq!(ledger.hashes.as_ref(), Some(&rust));
    assert_eq!(
        ledger.emit_hashes,
        Some(vec![
            rust[0].clone(),
            rust[2].clone(),
            rust[1].clone(),
            rust[4].clone(),
            rust[3].clone()
        ])
    );
    let mut module = vec![rust[3].clone(), rust[4].clone()];
    module.sort();
    assert_eq!(ledger.fossil_modules.unwrap()[0].hashes, module);
}

#[test]
fn a_failed_rederivation_leaves_the_ledger_untouched_and_refused() {
    // A statement-count mismatch (the text is not this ledger's).
    let (mut ledger, _) = ts_ledger();
    let before = format!("{ledger:?}");
    let short = prior().replace("  c(b);\n", "");
    assert!(rederive_ts_era_hashes(&mut ledger, &short).is_err());
    assert_eq!(format!("{ledger:?}"), before);
    assert!(!ledger.hashes_current());

    // The partitions differ: the TS split one Rust class in two.
    let (mut ledger, _) = ts_ledger();
    ledger.hashes.as_mut().unwrap()[2] = "ts-split".into();
    let before = format!("{ledger:?}");
    let err = rederive_ts_era_hashes(&mut ledger, &prior()).unwrap_err();
    assert!(err.contains("not a bijection"), "{err}");
    assert_eq!(format!("{ledger:?}"), before);

    // An emitted hash that no statement carries.
    let (mut ledger, _) = ts_ledger();
    ledger.emit_hashes.as_mut().unwrap()[0] = "ts-ghost".into();
    assert!(rederive_ts_era_hashes(&mut ledger, &prior()).is_err());
    assert!(!ledger.hashes_current());

    // Not the TS era: nothing to re-derive.
    let (mut ledger, _) = ts_ledger();
    ledger.hash_version = None;
    assert!(rederive_ts_era_hashes(&mut ledger, &prior()).is_err());
}
