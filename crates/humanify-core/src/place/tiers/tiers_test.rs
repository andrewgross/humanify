//! The hash tier's prior-ledger version gate (WP5.6e): a ledger whose
//! statement hashes were written by a DIFFERENT hash function (the TS era,
//! `hashVersion: 1`) must be refused — every statement reads
//! `no-prior-hashes` — never joined, even when its bytes happen to collide
//! with the fresh ones.

use std::collections::HashMap;

use super::{PlacementSwitches, TierInput, assign_with_prior};
use crate::hash::statement_hash::STATEMENT_HASH_VERSION;
use crate::place::ledger::StableSplitLedger;
use crate::place::trail::PlacementTrail;
use crate::twins::statement_inventory_with_values;

const CODE: &str = "function alpha(x) { return x + 1; }\nvar beta = alpha(2);\nconsole.log(beta);";

/// (statements placed by the hash tier, each row's `hashMiss`).
fn run(hash_version: Option<u64>) -> (usize, Vec<Option<String>>) {
    let (inv, body) = statement_inventory_with_values(CODE, "shipped", None).expect("inventory");
    let spans: Vec<(u32, u32)> = inv
        .statements
        .iter()
        .map(|s| (s.span.start, s.span.end))
        .collect();
    let hashes: Vec<String> = inv.statements.iter().map(|s| s.hash.clone()).collect();
    // The prior placed every statement in its own file, under the SAME
    // hash bytes the fresh side computes.
    let order: Vec<String> = (0..hashes.len()).map(|i| format!("src/f{i}.js")).collect();
    let prior = StableSplitLedger {
        version: 1,
        files: order.clone(),
        name_to_files: HashMap::new(),
        order,
        hashes: Some(hashes.clone()),
        hash_version,
        ..StableSplitLedger::default()
    };
    let mut trail = PlacementTrail::default();
    let (_, stats) = assign_with_prior(
        &TierInput {
            body: &body,
            spans: &spans,
            hashes: &hashes,
            code: CODE,
            prior: &prior,
            carry: None,
            switches: PlacementSwitches::default(),
        },
        Some(&mut trail),
    )
    .expect("assign");
    let misses = trail.rows.iter().map(|r| r.hash_miss.clone()).collect();
    (stats.by_tier[0], misses)
}

#[test]
fn a_current_version_ledger_joins_by_hash() {
    let (via_hash, misses) = run(Some(STATEMENT_HASH_VERSION));
    assert_eq!(via_hash, 3);
    assert!(misses.iter().all(Option::is_none));
}

#[test]
fn a_ts_era_ledger_is_refused_not_joined() {
    // hashVersion 1 = the TS statement-hash bytes (src/split/statement-hash.ts).
    let (via_hash, misses) = run(Some(1));
    assert_eq!(via_hash, 0, "a v1 ledger must never feed the hash tier");
    assert_eq!(misses, vec![Some("no-prior-hashes".to_string()); 3]);
    let (via_hash, _) = run(None);
    assert_eq!(via_hash, 0);
}
