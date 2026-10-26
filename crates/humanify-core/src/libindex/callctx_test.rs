//! Call context: containment of a bundle unit's callees in the index's,
//! the index side read through the store, and the call-tree hash.

use std::collections::HashMap;

use oxc_span::Span;

use crate::libindex::callctx::{Callee, CalleeFloor, Fit, tree_hashes};
use crate::libindex::format::{Callee as RowCallee, EdgeRow, UnitRow};
use crate::libindex::lookup::LibraryIndex;
use crate::libindex::table::{literal_key, unit_key};
use crate::libindex::units::{StatementPrints, UnitPrint};

fn hx(name: &str) -> String {
    format!("{:016x}", literal_key(name))
}

fn key(name: &str) -> u64 {
    unit_key(&hx(name)).unwrap()
}

fn row(hash: &str, owner: &str) -> UnitRow {
    UnitRow {
        hash: hx(hash),
        owner: owner.into(),
        file: "index.js".into(),
        nodes: 40,
        content: 5,
        module_bytes: 1000,
        versions: vec![("1.0.0".into(), "1.0.0".into())],
    }
}

fn edge(hash: &str, owner: &str, inside: &[&str], out_of: &[(&str, &str)]) -> EdgeRow {
    EdgeRow {
        hash: hx(hash),
        owner: owner.into(),
        file: "index.js".into(),
        inside: inside.iter().map(|h| hx(h)).collect(),
        out_of: out_of
            .iter()
            .map(|(o, h)| RowCallee {
                owner: (*o).into(),
                hash: hx(h),
            })
            .collect(),
    }
}

fn whole(hash: &str) -> StatementPrints {
    StatementPrints {
        whole: UnitPrint {
            span: Span::new(0, 40),
            hash: hx(hash),
            nodes: 40,
            content: 5,
        },
        functions: Vec::new(),
    }
}

fn callees(names: &[(&str, u32)]) -> Vec<Callee> {
    let mut v: Vec<Callee> = names
        .iter()
        .map(|(n, nodes)| Callee {
            key: key(n),
            nodes: *nodes,
        })
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn containment_counts_only_the_query_callees_the_reference_lacks() {
    let mut reference = vec![1, 2, 3, 4];
    reference.sort_unstable();
    let fit = Fit::of([1, 3], &reference);
    assert_eq!(
        fit,
        Fit {
            callees: 2,
            held: 2
        }
    );
    assert!(fit.confirms() && !fit.refutes());
    // an index callee the bundle lacks (tree-shaken) counts for nothing;
    // a bundle callee the index lacks counts against
    let fit = Fit::of([1, 9], &reference);
    assert!(!fit.confirms() && fit.refutes());
    // no callee: neither
    let fit = Fit::of([], &reference);
    assert!(!fit.confirms() && !fit.refutes());
}

#[test]
fn the_rowed_floor_skips_small_callees() {
    let c = callees(&[("big", 40), ("tiny", 6)]);
    assert_eq!(CalleeFloor::All.keys(&c).count(), 2);
    assert_eq!(
        CalleeFloor::Rowed.keys(&c).collect::<Vec<_>>(),
        vec![key("big")]
    );
}

#[test]
fn twin_bodies_with_different_callees_fit_only_their_own_owner() {
    let ix = LibraryIndex::from_rows_with_edges(
        vec![row("twin", "alpha"), row("twin", "beta")],
        vec![
            edge("twin", "alpha", &["a1", "a2"], &[]),
            edge("twin", "beta", &["b1"], &[("tslib", "t1")]),
        ],
        vec![],
    );
    let alpha = ix
        .package_names()
        .iter()
        .position(|p| p == "alpha")
        .unwrap() as u32;
    let beta = ix.package_names().iter().position(|p| p == "beta").unwrap() as u32;
    let prints = whole("twin");
    let bundle = callees(&[("a1", 30)]);
    let fit = |owner: u32| {
        ix.call_fit(&prints, &bundle, CalleeFloor::All, &|o| o == owner)
            .unwrap()
    };
    assert!(fit(alpha).confirms());
    assert!(fit(beta).refutes());
    // OUT-OF callees count as the row's callees too
    let bundle = callees(&[("b1", 30), ("t1", 30)]);
    assert!(
        ix.call_fit(&prints, &bundle, CalleeFloor::All, &|o| o == beta)
            .unwrap()
            .confirms()
    );
    // no accepted row: no verdict
    assert_eq!(
        ix.call_fit(&prints, &bundle, CalleeFloor::All, &|_| false),
        None
    );
    // a /3 index records no edges: no verdict, never evidence against
    let old = LibraryIndex::from_rows(vec![row("twin", "alpha")], vec![]);
    assert_eq!(
        old.call_fit(&prints, &bundle, CalleeFloor::All, &|_| true),
        None
    );
}

#[test]
fn equal_call_trees_hash_equal_whatever_the_node_ids() {
    // side A: 1 -> 2 -> 3 -> 2 (a cycle), 1 -> 4
    // side B: the same shape on other ids, values equal by position
    let a: HashMap<u32, Vec<u32>> = [(1, vec![2, 4]), (2, vec![3]), (3, vec![2]), (4, vec![])]
        .into_iter()
        .collect();
    let b: HashMap<u32, Vec<u32>> = [
        (10, vec![40, 20]),
        (20, vec![30]),
        (30, vec![20]),
        (40, vec![]),
    ]
    .into_iter()
    .collect();
    let val = |n: u32| u64::from(if n >= 10 { n / 10 } else { n }) * 1000;
    let mut ma = HashMap::new();
    tree_hashes(&[1], &mut ma, |n| a[&n].clone(), val);
    let mut mb = HashMap::new();
    tree_hashes(&[10], &mut mb, |n| b[&n].clone(), val);
    assert_eq!(ma[&1], mb[&10]);
    assert_eq!(ma[&2], mb[&20]);
    assert_ne!(ma[&2], ma[&3], "a component's members keep their own value");
    // a different leaf under the same root changes the root's tree
    let c: HashMap<u32, Vec<u32>> = [(1, vec![2, 5]), (2, vec![3]), (3, vec![2]), (5, vec![])]
        .into_iter()
        .collect();
    let mut mc = HashMap::new();
    tree_hashes(&[1], &mut mc, |n| c[&n].clone(), |n| u64::from(n) * 1000);
    assert_ne!(ma[&1], mc[&1]);
    assert_eq!(ma[&4], mb[&40]);
}
