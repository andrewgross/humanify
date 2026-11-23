//! `alignFileStatements` / `alignEmissionOrder` semantics
//! (src/split/stable-split.test.ts's emission-order cases, in miniature).

use super::{AlignSwitches, align_emission_order, align_file_statements, alignment_key};
use crate::emit::load_order::LoadOrderFacts;
use crate::hash::forms::STATEMENT;
use crate::place::ledger::StableSplitLedger;
use humanify_ag_testkit::strings;

fn pure(n: usize) -> Vec<LoadOrderFacts> {
    vec![LoadOrderFacts::default(); n]
}

#[test]
fn alignment_key_joins_hash_and_name_with_nul() {
    assert_eq!(alignment_key("h", Some("a,b")), "h\u{0}a,b");
    assert_eq!(alignment_key("h", None), "h");
}

#[test]
fn unambiguous_statements_take_their_prior_order() {
    let keys = strings(&["a", "b", "c"]);
    let prior = strings(&["c", "a", "b"]);
    assert_eq!(
        align_file_statements(&[0, 1, 2], &keys, Some(&prior), &pure(3)),
        vec![2, 0, 1]
    );
}

#[test]
fn a_novel_statement_follows_its_predecessor() {
    let keys = strings(&["a", "new", "b"]);
    let prior = strings(&["b", "a"]);
    // `new` anchors after `a` (its predecessor), wherever `a` goes.
    assert_eq!(
        align_file_statements(&[0, 1, 2], &keys, Some(&prior), &pure(3)),
        vec![2, 0, 1]
    );
}

#[test]
fn a_novel_statement_does_not_follow_a_statement_that_moved() {
    // The bundle moved `m` from the top to between `b` and `c`; `new`
    // comes right after it. `m` keeps its prior place; `new` stays with
    // the statements that did not move.
    let keys = strings(&["a", "b", "m", "new", "c"]);
    let prior = strings(&["m", "a", "b", "c"]);
    assert_eq!(
        align_file_statements(&[0, 1, 2, 3, 4], &keys, Some(&prior), &pure(5)),
        vec![2, 0, 1, 3, 4]
    );
}

#[test]
fn a_fingerprint_found_once_on_each_side_keeps_its_place_under_a_new_name() {
    let key = |hash: &str, name: &str| alignment_key(hash, Some(name));
    let keys = vec![key("h1", "renamed"), key("h2", "b"), key("h3", "c")];
    let prior = vec![key("h3", "c"), key("h1", "old"), key("h2", "b")];
    assert_eq!(
        align_file_statements(&[0, 1, 2], &keys, Some(&prior), &pure(3)),
        vec![2, 0, 1]
    );
}

#[test]
fn a_repeated_fingerprint_claims_nothing_by_itself() {
    let key = |hash: &str, name: &str| alignment_key(hash, Some(name));
    let keys = vec![
        key("h1", "x"),
        key("h2", "b"),
        key("h3", "c"),
        key("h1", "y"),
    ];
    let prior = vec![key("h3", "c"), key("h1", "old"), key("h2", "b")];
    assert_eq!(
        align_file_statements(&[0, 1, 2, 3], &keys, Some(&prior), &pure(4)),
        vec![0, 2, 1, 3]
    );
}

#[test]
fn ambiguous_keys_do_not_claim_and_fewer_than_two_claims_is_identity() {
    let keys = strings(&["x", "x", "y"]);
    let prior = strings(&["y", "x", "x"]);
    assert_eq!(
        align_file_statements(&[0, 1, 2], &keys, Some(&prior), &pure(3)),
        vec![0, 1, 2]
    );
    assert_eq!(
        align_file_statements(&[0, 1], &strings(&["a", "b"]), None, &pure(2)),
        vec![0, 1]
    );
}

#[test]
fn emission_order_is_identity_without_a_usable_prior() {
    let assignment = strings(&["f", "f"]);
    let hashes = strings(&["a", "b"]);
    let prior = StableSplitLedger {
        version: 1,
        order: strings(&["f", "f"]),
        emit_hashes: Some(strings(&["b", "a"])),
        hash_version: 99,
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
        hash_version: 1,
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
        hash_version: STATEMENT.version,
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

/// exp227: an EDITED statement (its hash changed) keeps its prior place
/// through a name only it binds, on both sides — it used to follow its
/// bundle predecessor, and moved whenever the prior order and bundle
/// order disagreed around it.
#[test]
fn an_edited_statement_keeps_its_prior_place_by_a_name_only_it_binds() {
    let key = |h: &str, n: &str| alignment_key(h, Some(n));
    // prior: a, edited (old hash), b, c. Bundle: a, b, c, edited (new hash).
    let prior = vec![
        key("ha", "alpha,props"),
        key("old", "Dialog,props"),
        key("hb", "beta,props"),
        key("hc", "gamma"),
    ];
    let keys = vec![
        key("ha", "alpha,props"),
        key("hb", "beta,props"),
        key("hc", "gamma"),
        key("new", "Dialog,props"),
    ];
    assert_eq!(
        align_file_statements(&[0, 1, 2, 3], &keys, Some(&prior), &pure(4)),
        vec![0, 3, 1, 2]
    );
}

/// A name more than one statement binds (a parameter name, say) claims
/// nothing.
#[test]
fn a_shared_name_claims_no_prior_place() {
    let key = |h: &str, n: &str| alignment_key(h, Some(n));
    let prior = vec![
        key("ha", "alpha"),
        key("old", "props,x"),
        key("hb", "beta"),
        key("hd", "props,y"),
    ];
    let keys = vec![
        key("ha", "alpha"),
        key("hb", "beta"),
        key("new", "props,z"),
        key("he", "props,w"),
    ];
    assert_eq!(
        align_file_statements(&[0, 1, 2, 3], &keys, Some(&prior), &pure(4)),
        vec![0, 1, 2, 3]
    );
}
