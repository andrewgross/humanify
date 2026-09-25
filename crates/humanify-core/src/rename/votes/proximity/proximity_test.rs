//! TS original: `src/rename/proximity.test.ts` (its five cases), plus the
//! inherited-record case the TS carries by accident.

use std::collections::HashMap;

use super::{ProximityBinding, get_proximate_used_names};

fn binding(line: u32, refs: &[u32]) -> ProximityBinding {
    ProximityBinding {
        decl_line: Some(line),
        ref_lines: refs.to_vec(),
    }
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn run(
    all: &[&str],
    lines: &[u32],
    bindings: &HashMap<&str, ProximityBinding>,
    total: usize,
    eligible: impl Fn(&str) -> bool,
) -> Vec<String> {
    get_proximate_used_names(
        &names(all),
        lines,
        |n| bindings.get(n).cloned(),
        total,
        eligible,
    )
}

#[test]
fn always_includes_well_known_names() {
    let b = HashMap::from([
        ("exports", binding(1, &[])),
        ("require", binding(2, &[])),
        ("console", binding(3, &[])),
        ("a", binding(1000, &[])),
        ("b", binding(1001, &[])),
    ]);
    let out = run(
        &["exports", "require", "console", "a", "b"],
        &[50],
        &b,
        200,
        |_| true,
    );
    for n in ["exports", "require", "console"] {
        assert!(out.iter().any(|x| x == n), "{n}");
    }
}

#[test]
fn excludes_eligible_names() {
    let b = HashMap::from([
        ("a", binding(50, &[])),
        ("b", binding(50, &[])),
        ("c", binding(50, &[])),
        ("myVar", binding(50, &[])),
    ]);
    let out = run(&["a", "b", "c", "myVar"], &[50], &b, 200, |n| n.len() == 1);
    assert_eq!(out, names(&["myVar"]));
}

#[test]
fn includes_names_within_the_radius_only() {
    let b = HashMap::from([("nearVar", binding(55, &[])), ("farVar", binding(500, &[]))]);
    let out = run(&["nearVar", "farVar"], &[50], &b, 200, |n| n.len() == 1);
    assert_eq!(out, names(&["nearVar"]));
}

#[test]
fn includes_a_name_whose_reference_is_near() {
    let b = HashMap::from([("refVar", binding(500, &[45]))]);
    let out = run(&["refVar"], &[50], &b, 200, |n| n.len() == 1);
    assert_eq!(out, names(&["refVar"]));
}

#[test]
fn returns_every_preserved_name_below_the_threshold() {
    let b = HashMap::from([
        ("nearVar", binding(50, &[])),
        ("farVar", binding(500, &[])),
        ("a", binding(50, &[])),
    ]);
    let out = run(&["nearVar", "farVar", "a"], &[50], &b, 50, |n| n.len() == 1);
    assert_eq!(out, names(&["nearVar", "farVar"]));
}

#[test]
fn an_absent_binding_is_included_but_an_inherited_record_name_is_not() {
    // `scopeBindings[name]` on a plain object: a truly absent name reads
    // undefined (included "to be safe"); `toString` reads the inherited
    // Object.prototype function — truthy, no loc — and is excluded.
    let b: HashMap<&str, ProximityBinding> = HashMap::new();
    let out = run(&["missingVar", "toString"], &[50], &b, 200, |_| false);
    assert_eq!(out, names(&["missingVar"]));
}
