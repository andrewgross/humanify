//! The TS truth table (test/parity/wp51-declared.json, from
//! test/parity/wp51-declared-probe.ts — the exact `getBindingIdentifiers`
//! / `getOuterBindingIdentifiers` calls): names AND breadth-first order.

use super::{declared_names, outer_declared_names};
use crate::twins::statement_inventory_with_values;
use serde_json::Value;

const VECTORS: &str = include_str!("../../../../../test/parity/wp51-declared.json");

#[test]
fn every_statement_declares_what_babel_says_in_babels_order() {
    let doc: Value = serde_json::from_str(VECTORS).expect("vectors");
    let (_, values) =
        statement_inventory_with_values(doc["code"].as_str().unwrap(), "shipped", None)
            .expect("inventory");
    let rows = doc["rows"].as_array().unwrap();
    assert_eq!(values.len(), rows.len());
    let strs = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    for (k, (stmt, row)) in values.iter().zip(rows).enumerate() {
        assert_eq!(declared_names(stmt), strs(&row["names"]), "statement {k}");
        assert_eq!(
            outer_declared_names(stmt),
            strs(&row["outer"]),
            "statement {k}"
        );
    }
}
