use std::sync::Arc;

use super::*;

fn layer(names: &[&str]) -> Arc<NameLayer> {
    Arc::new(NameLayer::new(names.iter().map(|s| s.to_string())))
}

#[test]
fn the_order_is_the_first_occurrence_union_inner_to_outer() {
    // context_test's union case, as layers.
    let set = UsedSet::new(vec![
        layer(&["b", "a"]),
        layer(&["c", "a"]),
        layer(&["z", "b", "y"]),
        layer(&["g", "z"]),
    ]);
    assert_eq!(
        set.order().collect::<Vec<_>>(),
        ["b", "a", "c", "z", "y", "g"]
    );
}

#[test]
fn a_big_outer_layer_shadowed_by_a_small_inner_one_skips_the_right_positions() {
    // The probe walks the smaller side in both directions.
    let big: Vec<String> = (0..50).map(|i| format!("n{i}")).collect();
    let big_layer = Arc::new(NameLayer::new(big.clone()));
    let set = UsedSet::new(vec![
        layer(&["n7", "x", "n3"]),
        big_layer.clone(),
        layer(&["x", "n9", "g"]),
    ]);
    let mut want = vec!["n7".to_string(), "x".into(), "n3".into()];
    want.extend(big.iter().filter(|n| *n != "n7" && *n != "n3").cloned());
    want.push("g".into());
    assert_eq!(set.order().collect::<Vec<_>>(), want);
    // A layer that is itself deduplicated keeps its first occurrence.
    assert_eq!(NameLayer::new(["a", "b", "a"].map(String::from)).len(), 2);
}

#[test]
fn membership_sees_the_barrier_edits_and_the_order_does_not() {
    let mut set = UsedSet::new(vec![layer(&["a", "b"]), layer(&["c"])]);
    assert!(set.contains("a") && set.contains("c") && !set.contains("q"));
    // delete(old); add(new) — the barrier's pair.
    set.remove("a");
    set.insert("alpha");
    assert!(!set.contains("a") && set.contains("alpha"));
    // add after delete, delete after add.
    set.insert("a");
    assert!(set.contains("a"));
    set.remove("alpha");
    assert!(!set.contains("alpha"));
    // An outer layer's name deleted through this context is gone for it.
    set.remove("c");
    assert!(!set.contains("c"));
    assert_eq!(set.order().collect::<Vec<_>>(), ["a", "b", "c"]);
}

#[test]
fn contexts_share_their_layers_and_own_only_their_edits() {
    let module: Vec<String> = (0..1000).map(|i| format!("m{i}")).collect();
    let shared = Arc::new(NameLayer::new(module));
    let mut sets: Vec<UsedSet> = (0..100)
        .map(|i| UsedSet::new(vec![layer(&[&format!("p{i}")]), shared.clone()]))
        .collect();
    sets[0].remove("p0");
    sets[0].insert("param");
    assert_eq!(Arc::strong_count(&shared), 101);
    assert_eq!(sets.iter().map(UsedSet::owned_names).sum::<usize>(), 2);
}
