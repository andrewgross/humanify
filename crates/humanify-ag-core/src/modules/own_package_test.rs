//! exp183: the bundle's own package (pure decision over module evidence).

use std::collections::BTreeSet;

use super::{ModuleEvidence, innermost_bytes, majority, own_packages, own_packages_from_entries};

fn module(packages: &[&str], requires: &[usize]) -> ModuleEvidence {
    ModuleEvidence {
        packages: packages
            .iter()
            .map(|p| p.to_string())
            .collect::<BTreeSet<_>>(),
        requires: requires.to_vec(),
        ..ModuleEvidence::default()
    }
}

fn claimed_mostly(package: &str, requires: &[usize]) -> ModuleEvidence {
    ModuleEvidence {
        majority: Some(package.to_string()),
        ..module(&[package], requires)
    }
}

fn own(e: &[ModuleEvidence], entries: &[usize]) -> Vec<String> {
    own_packages(e, entries).into_iter().collect()
}

#[test]
fn a_package_claiming_the_entry_is_the_bundles_own() {
    // The bundle runs 0 and 1, both mostly claimed as `cli` (a published
    // CLI whose own code the index knows as a dependency of another
    // package); 2 is lodash.
    let e = vec![
        claimed_mostly("cli", &[2]),
        claimed_mostly("cli", &[2]),
        module(&["lodash"], &[]),
    ];
    assert_eq!(own(&e, &[0, 1]), vec!["cli".to_string()]);
}

#[test]
fn the_entry_rule_holds_even_when_a_library_requires_the_package() {
    // Another package's module requiring one of the entry's package is no
    // refusal here: the entry IS the app, whatever else reaches its code.
    let e = vec![
        claimed_mostly("cli", &[1]),
        module(&["cli"], &[]),
        module(&["run"], &[1]),
    ];
    assert_eq!(own(&e, &[0]), vec!["cli".to_string()]);
}

#[test]
fn entries_claimed_by_different_packages_name_no_own_package() {
    // An app whose last statements run a library's register module and
    // its own (unclaimed) main: the library is not the app.
    let e = vec![claimed_mostly("register-lib", &[]), module(&[], &[])];
    assert!(own(&e, &[0, 1]).is_empty());
    let e = vec![claimed_mostly("a", &[]), claimed_mostly("b", &[])];
    assert!(own(&e, &[0, 1]).is_empty());
}

#[test]
fn a_package_touching_the_entry_without_most_of_it_is_no_own_package() {
    // A small claim inside the entry (a shared helper print) names nothing.
    let e = vec![module(&["tslib"], &[1]), module(&["tslib"], &[])];
    assert!(own(&e, &[0]).is_empty());
}

#[test]
fn no_entry_no_own_package() {
    let e = vec![claimed_mostly("cli", &[])];
    assert!(own(&e, &[]).is_empty());
}

#[test]
fn the_mutual_and_manifest_rules_still_name_the_own_package() {
    // exp172's rules, now asked here: the entry's unattributed module 1
    // requires app's module 2, which requires 1 back (mutual)...
    let e = vec![module(&[], &[1]), module(&[], &[2]), module(&["app"], &[1])];
    assert_eq!(own(&e, &[0]), vec!["app".to_string()]);
    // ...and the entry's module requires cli's inlined package.json.
    let mut manifest = module(&["cli"], &[]);
    manifest.manifest_of = Some("cli".to_string());
    let e = vec![module(&[], &[1]), manifest];
    assert_eq!(own(&e, &[0]), vec!["cli".to_string()]);
}

/// The fence's asker: the entry rule ALONE — the front's manifest and
/// mutual candidates name nothing here (at discovery scope an unclaimed
/// module is no proof of app code; workflow_nest's node build).
#[test]
fn the_fence_asks_only_the_entry_rule() {
    let mut manifest = module(&["cli"], &[]);
    manifest.manifest_of = Some("cli".to_string());
    let e = vec![module(&[], &[1]), manifest];
    assert_eq!(
        own_packages_from_entries(&e, &[0])
            .into_iter()
            .collect::<Vec<_>>(),
        Vec::<String>::new()
    );
    // ...but the entry rule still holds for it.
    let e = vec![claimed_mostly("cli", &[1]), module(&["cli"], &[])];
    assert_eq!(
        own_packages_from_entries(&e, &[0])
            .into_iter()
            .collect::<Vec<_>>(),
        vec!["cli".to_string()]
    );
}

#[test]
fn most_of_a_module_is_more_than_half_of_its_owned_bytes() {
    assert_eq!(majority([("p", 60), ("q", 30)], 100), Some("p".to_string()));
    assert_eq!(majority([("p", 30), ("p", 21)], 100), Some("p".to_string()));
    assert_eq!(
        majority([("p", 50), ("q", 50)], 100),
        None,
        "half is not most"
    );
}

#[test]
fn nested_claim_spans_count_for_the_innermost_claim() {
    // Wrapper 0..100 claimed p, its body 10..70 claimed q.
    let owned = innermost_bytes([("p", 0, 100), ("q", 10, 70)]);
    assert_eq!(majority(owned.clone(), 100), Some("q".to_string()));
    let total = |pkg: &str| {
        owned
            .iter()
            .filter(|(p, _)| *p == pkg)
            .map(|(_, b)| b)
            .sum::<u64>()
    };
    assert_eq!((total("p"), total("q")), (40, 60));
    // Disjoint spans and a nested run of the same package.
    let owned = innermost_bytes([("p", 0, 10), ("p", 20, 100), ("q", 30, 40)]);
    let total = |pkg: &str| {
        owned
            .iter()
            .filter(|(p, _)| *p == pkg)
            .map(|(_, b)| b)
            .sum::<u64>()
    };
    assert_eq!((total("p"), total("q")), (80, 10));
}
