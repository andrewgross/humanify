//! The init-identity carry's pure halves: which modules pair by content,
//! and what the prior ledger lends.

use super::{PriorModuleInit, pair_modules_by_content, prior_module_inits};
use crate::place::ledger::{FossilLedgerModule, StableSplitLedger};

fn hs(list: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = list.iter().map(|s| s.to_string()).collect();
    v.sort();
    v
}

fn pairs(fresh: &[Vec<String>], prior: &[Vec<String>]) -> Vec<(usize, usize)> {
    let f: Vec<&[String]> = fresh.iter().map(Vec::as_slice).collect();
    let p: Vec<&[String]> = prior.iter().map(Vec::as_slice).collect();
    pair_modules_by_content(&f, &p)
}

#[test]
fn modules_pair_with_their_own_content_whatever_the_order() {
    let prior = vec![
        hs(&["a", "b", "i"]),
        hs(&["c", "d", "i"]),
        hs(&["e", "f", "i"]),
    ];
    let fresh = vec![hs(&["e", "f", "i"]), hs(&["n", "i"]), hs(&["a", "b", "x"])];
    assert_eq!(
        pairs(&fresh, &prior),
        vec![(0, 2), (2, 0)],
        "the shared setup shape `i` alone pairs nothing; an edited module \
         (2/4 = 0.5) still pairs"
    );
}

#[test]
fn twins_by_content_pair_with_nothing() {
    let prior = vec![hs(&["a", "b"]), hs(&["a", "b"])];
    let fresh = vec![hs(&["a", "b"])];
    assert!(
        pairs(&fresh, &prior).is_empty(),
        "two prior modules with the same statements: the content cannot say which"
    );
}

#[test]
fn a_pair_must_be_the_best_both_ways() {
    // fresh 0 likes prior 0 best (2/3), but prior 0 likes fresh 1 more
    // (3/3): only (1, 0) pairs.
    let prior = vec![hs(&["a", "b", "c"])];
    let fresh = vec![hs(&["a", "b"]), hs(&["a", "b", "c"])];
    assert_eq!(pairs(&fresh, &prior), vec![(1, 0)]);
}

#[test]
fn below_the_floor_nothing_pairs() {
    let prior = vec![hs(&["a", "b", "c"])];
    let fresh = vec![hs(&["a", "x", "y"])];
    assert!(
        pairs(&fresh, &prior).is_empty(),
        "1/5 is not the same module"
    );
}

fn ledger_module(file: &str, hashes: &[&str], declared: &[&str]) -> FossilLedgerModule {
    FossilLedgerModule {
        file: file.to_string(),
        hashes: hashes.iter().map(|h| h.to_string()).collect(),
        declared: Some(declared.iter().map(|d| d.to_string()).collect()),
        ..Default::default()
    }
}

#[test]
fn the_ledger_lends_each_modules_last_declared_name_as_its_init() {
    let ledger = StableSplitLedger {
        hash_version: Some(crate::hash::forms::STATEMENT.version),
        fossil_modules: Some(vec![
            ledger_module("src/color.js", &["h2", "h1"], &["round", "initColorUtils"]),
            ledger_module("vendor/ms/index.js", &["h3"], &["parse", "initMs"]),
            FossilLedgerModule {
                file: "src/old.js".to_string(),
                hashes: vec!["h4".to_string()],
                ..Default::default()
            },
        ]),
        ..Default::default()
    };
    assert_eq!(
        prior_module_inits(&ledger),
        vec![
            PriorModuleInit {
                hashes: hs(&["h1", "h2"]),
                init_name: "initColorUtils".to_string(),
                app: true,
            },
            PriorModuleInit {
                hashes: hs(&["h3"]),
                init_name: "initMs".to_string(),
                app: false,
            },
        ],
        "a module recorded without its declared names lends nothing"
    );
}

#[test]
fn a_stale_era_ledger_lends_nothing() {
    let ledger = StableSplitLedger {
        hash_version: Some(1),
        fossil_modules: Some(vec![ledger_module("src/a.js", &["h1"], &["initA"])]),
        ..Default::default()
    };
    assert!(prior_module_inits(&ledger).is_empty());
}
