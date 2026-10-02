//! The shared-snapshot taken set (finding #66's fix): membership over
//! the chain's layers is membership over the union the private clone
//! used to hold; the snapshot-at-build freeze is the Arc's immutability
//! (a refreshed map entry REPLACES the Arc, it never mutates it); the
//! own-bytes arithmetic is the per-context Arc list alone.

use std::collections::HashSet;
use std::sync::Arc;

use super::TakenNames;

fn set(names: &[&str]) -> Arc<HashSet<String>> {
    Arc::new(names.iter().map(|n| n.to_string()).collect::<HashSet<_>>())
}

#[test]
fn membership_over_layers_is_membership_over_the_union() {
    let taken = TakenNames::new(vec![set(&["eventHooks", "qBase"]), set(&["qBase", "run"])]);
    for name in ["eventHooks", "qBase", "run"] {
        assert!(taken.contains(name), "the union holds {name}");
    }
    assert!(!taken.contains("zNamed"));
    assert!(
        !TakenNames::default().contains("eventHooks"),
        "the empty view"
    );
}

#[test]
fn a_shared_snapshot_answers_two_viewholders_identically() {
    let shared = set(&["eventHooks"]);
    let a = TakenNames::new(vec![shared.clone(), set(&["qBase"])]);
    let b = TakenNames::new(vec![shared, set(&["run"])]);
    // The SAME Arc: one snapshot, however many contexts were built over
    // the unchanged scope — the #56 sharing, observable by pointer.
    assert_eq!(Arc::as_ptr(&a.layers()[0]), Arc::as_ptr(&b.layers()[0]));
    assert!(a.contains("eventHooks") && b.contains("eventHooks"));
    assert!(a.contains("qBase") && !b.contains("qBase"));
    assert!(b.contains("run") && !a.contains("run"));
}

#[test]
fn own_bytes_is_the_arc_list_not_the_shared_contents() {
    let taken = TakenNames::new(vec![set(&["eventHooks", "qBase"]), set(&["run"])]);
    assert_eq!(
        taken.own_bytes(),
        2 * std::mem::size_of::<Arc<HashSet<String>>>() as u64
    );
    assert_eq!(TakenNames::default().own_bytes(), 0);
}
