//! `alignFileStatements` / `alignEmissionOrder` semantics
//! (src/split/stable-split.test.ts's emission-order cases, in miniature).

use super::{AlignSwitches, align_emission_order, align_file_statements, alignment_key};
use crate::emit::load_order::LoadOrderFacts;
use crate::hash::statement_hash::STATEMENT_HASH_VERSION;
use crate::place::ledger::StableSplitLedger;

fn pure(n: usize) -> Vec<LoadOrderFacts> {
    vec![LoadOrderFacts::default(); n]
}

fn s(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|x| x.to_string()).collect()
}

#[test]
fn alignment_key_joins_hash_and_name_with_nul() {
    assert_eq!(alignment_key("h", Some("a,b")), "h\u{0}a,b");
    assert_eq!(alignment_key("h", None), "h");
}

#[test]
fn unambiguous_statements_take_their_prior_order() {
    let keys = s(&["a", "b", "c"]);
    let prior = s(&["c", "a", "b"]);
    assert_eq!(
        align_file_statements(&[0, 1, 2], &keys, Some(&prior), &pure(3)),
        vec![2, 0, 1]
    );
}

#[test]
fn a_novel_statement_follows_its_predecessor() {
    let keys = s(&["a", "new", "b"]);
    let prior = s(&["b", "a"]);
    // `new` anchors after `a` (its predecessor), wherever `a` goes.
    assert_eq!(
        align_file_statements(&[0, 1, 2], &keys, Some(&prior), &pure(3)),
        vec![2, 0, 1]
    );
}

#[test]
fn ambiguous_keys_do_not_claim_and_fewer_than_two_claims_is_identity() {
    let keys = s(&["x", "x", "y"]);
    let prior = s(&["y", "x", "x"]);
    assert_eq!(
        align_file_statements(&[0, 1, 2], &keys, Some(&prior), &pure(3)),
        vec![0, 1, 2]
    );
    assert_eq!(
        align_file_statements(&[0, 1], &s(&["a", "b"]), None, &pure(2)),
        vec![0, 1]
    );
}

#[test]
fn emission_order_is_identity_without_a_usable_prior() {
    let assignment = s(&["f", "f"]);
    let hashes = s(&["a", "b"]);
    let prior = StableSplitLedger {
        version: 1,
        order: s(&["f", "f"]),
        emit_hashes: Some(s(&["b", "a"])),
        hash_version: Some(99),
        ..StableSplitLedger::default()
    };
    let perm = align_emission_order(
        &assignment,
        &hashes,
        &pure(2),
        Some(&prior),
        None,
        AlignSwitches::default(),
    );
    assert_eq!(perm, vec![0, 1]);
    // A TS-era ledger (hashVersion 1: the TS statement-hash bytes) is not
    // a usable prior for the Rust's own hashes (WP5.6e).
    let ts_era = StableSplitLedger {
        hash_version: Some(1),
        ..prior.clone()
    };
    let perm = align_emission_order(
        &assignment,
        &hashes,
        &pure(2),
        Some(&ts_era),
        None,
        AlignSwitches::default(),
    );
    assert_eq!(perm, vec![0, 1], "a v1 ledger must not align the emission");
    let good = StableSplitLedger {
        hash_version: Some(STATEMENT_HASH_VERSION),
        ..prior
    };
    let perm = align_emission_order(
        &assignment,
        &hashes,
        &pure(2),
        Some(&good),
        None,
        AlignSwitches::default(),
    );
    assert_eq!(perm, vec![1, 0]);
}
