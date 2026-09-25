//! The split stage end to end on a regime the four oracle pairs never
//! reach (lesson 17): a runnable-emit DECLINE.

use std::collections::HashMap;

use humanify_model::js::JsValue;
use serde_json::Value;

use super::{SplitOptions, stable_split};
use crate::place::ledger::StableSplitLedger;
use crate::place::placement_dump::Regime;

/// Finding #40: a prior ledger whose names place `xa` in a.js and `yb`,
/// `zb` in b.js makes a load-time reference cycle (a.js reads b.js's `yb`
/// at load, b.js reads a.js's `xa`) — the runnable emit declines, the
/// byte-exact review tree is written, and the PERSISTED ledger keeps the
/// aliases the TS emit had assigned before it threw (the wp53 vector of
/// the same fixture records the TS's).
#[test]
fn a_declined_emit_persists_the_ts_aliases() {
    let vectors: Vec<Value> =
        serde_json::from_str(include_str!("../../../../../test/parity/wp53-cjs.json"))
            .expect("vectors");
    let v = vectors
        .iter()
        .find(|v| v["name"] == "load-time cycle declines")
        .expect("the cycle vector");
    let code = v["code"].as_str().expect("code");
    let order: Vec<String> = v["order"]
        .as_array()
        .expect("order")
        .iter()
        .map(|s| s.as_str().expect("file").to_string())
        .collect();
    let mut name_to_files: HashMap<String, Vec<String>> = HashMap::new();
    name_to_files.insert("xa".into(), vec!["a.js".into()]);
    name_to_files.insert("yb".into(), vec!["b.js".into()]);
    name_to_files.insert("zb".into(), vec!["b.js".into()]);
    for i in 0..60 {
        name_to_files.insert(format!("padFiller{i}"), vec!["pad/fill.js".into()]);
    }
    let prior = StableSplitLedger {
        version: 1,
        files: vec!["a.js".into(), "b.js".into(), "pad/fill.js".into()],
        name_to_files,
        order,
        hashes: None,
        emit_hashes: None,
        emit_names: None,
        emit_indexes: None,
        hash_version: None,
        aliases: None,
        fossil_modules: None,
    };
    let outcome = stable_split(
        code,
        SplitOptions {
            regime: Regime::Tiers,
            prior: Some(&prior),
            carry: None,
            namer: None,
            reviser: None,
            placement: Default::default(),
            align: Default::default(),
            registrar_exemption_disabled: false,
            split_pure: false,
            trail: None,
        },
    )
    .expect("the split runs");
    assert_eq!(
        outcome.declined.as_deref(),
        v["declined"].as_str(),
        "the runnable emit declines with the TS's reason"
    );
    assert!(outcome.runnable.is_none(), "the review tree is written");
    let ledger = outcome.ledger.as_object().expect("a ledger object");
    let aliases: Vec<(String, String)> = match ledger.get("aliases") {
        Some(JsValue::Object(o)) => o
            .entries()
            .iter()
            .map(|(f, a)| (f.clone(), a.as_str().unwrap_or_default().to_string()))
            .collect(),
        other => panic!("the declined ledger has no aliases: {other:?}"),
    };
    let ts: Vec<(String, String)> = v["declinedLedger"]["aliases"]
        .as_array()
        .expect("ts aliases")
        .iter()
        .map(|e| {
            (
                e[0].as_str().unwrap().to_string(),
                e[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(aliases, ts);
    assert!(
        ledger.get("emitIndexes").is_none(),
        "the layout was never recorded"
    );
}
