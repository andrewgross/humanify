//! The post-split reconcile + bundle carry against the real TS on one
//! constructed tree (test/parity/wp54-postsplit.json, written by
//! test/parity/wp54-postsplit-probe.ts): the shipped file text, the rename
//! trail with top-level flags and locators, the patched ledger's exact
//! bytes, and the carried bundle. The real-scale regimes are the gate
//! (/work/wp54/regimes.sh); this pins the mechanics at unit scale.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use humanify_model::js::{JsValue, stringify};
use serde_json::Value;

use super::{PostSplitInput, post_split_reconcile};
use crate::finish::carry::carry_renames_into_bundle;
use crate::rename::eligibility::Eligibility;

fn fixture() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-postsplit.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
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
    // serde_json alphabetizes keys; the ledger's key order is the output's,
    // so it comes through the order-preserving JS value.
    let raw = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-postsplit.json"),
    )
    .unwrap();
    let mut ledger = match JsValue::parse(&raw).unwrap() {
        JsValue::Object(o) => o.get("ledgerIn").unwrap().clone(),
        _ => unreachable!(),
    };
    let read_fresh = |f: &str| fresh.get(f).cloned();
    let read_prior = |f: &str| prior.get(f).cloned();
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let result = post_split_reconcile(PostSplitInput {
        ledger: &mut ledger,
        read_fresh: &read_fresh,
        read_prior: &read_prior,
        eligible: &eligible,
        disabled: false,
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
    assert_eq!(stringify(&ledger), fx["ledgerOut"].as_str().unwrap());
    assert_eq!(
        result.stats.considered as u64,
        fx["stats"]["considered"].as_u64().unwrap()
    );

    let bundle = fx["bundle"].as_str().unwrap();
    let carry = carry_renames_into_bundle(bundle, &ledger, &result.renames).unwrap();
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
    let mut ledger = JsValue::parse("{\"files\":[\"a.js\"],\"order\":[]}").unwrap();
    let read = |_: &str| Some("var a = 1;\n".to_string());
    let eligible = Eligibility::new(None, None);
    let result = post_split_reconcile(PostSplitInput {
        ledger: &mut ledger,
        read_fresh: &read,
        read_prior: &read,
        eligible: &eligible,
        disabled: true,
    });
    assert_eq!(result.stats.considered, 0);
    assert!(result.changed.is_empty());
}

/// A TS quirk reproduced for parity (reported to the structure owner):
/// `collectSubstitutions` reads the token at each identifier's `loc` with
/// the ASCII-only `IDENT_AT`, so in a file with ANY reconcile rename a
/// non-ASCII identifier `café` reads as `caf` and is "substituted" to
/// `café` — shipping `caféé`. The rewrite is still a consistent rename of
/// one binding, so the re-parse guard accepts it. Real output of the TS
/// (`postSplitReconcile`, 2026-09-25).
#[test]
fn a_non_ascii_identifier_is_mangled_exactly_like_the_ts() {
    let fresh =
        "function f(a) {\n  const Xq = a + 1;\n  return Xq;\n}\nvar café = 1;\nuse(café);\n";
    let prior =
        "function f(a) {\n  const total = a + 1;\n  return total;\n}\nvar café = 1;\nuse(café);\n";
    let mut ledger = JsValue::parse(
        "{\"version\":1,\"files\":[\"a.js\"],\"nameToFiles\":{},\"order\":[\"a.js\",\"a.js\",\"a.js\"],\"hashes\":[],\"emitHashes\":[],\"emitNames\":[null,null,null]}",
    )
    .unwrap();
    let read_fresh = |_: &str| Some(fresh.to_string());
    let read_prior = |_: &str| Some(prior.to_string());
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let result = post_split_reconcile(PostSplitInput {
        ledger: &mut ledger,
        read_fresh: &read_fresh,
        read_prior: &read_prior,
        eligible: &eligible,
        disabled: false,
    });
    assert_eq!(
        result.changed,
        vec![(
            "a.js".to_string(),
            "function f(a) {\n  const total = a + 1;\n  return total;\n}\nvar caféé = 1;\nuse(caféé);\n"
                .to_string()
        )]
    );
}
