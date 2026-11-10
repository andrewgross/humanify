use super::*;
use crate::library::families::Libix;

fn claim<'a>(claimed: &'a str, truth: &'a str, file: &'a str, bytes: i64) -> WrongClaim<'a> {
    WrongClaim {
        claimed,
        truth,
        file: Some(file),
        rule: Some("seed"),
        bytes,
    }
}

fn get<'a>(v: &'a PyValue, path: &[&str]) -> &'a PyValue {
    path.iter()
        .fold(v, |v, k| v.get(k).unwrap_or_else(|| panic!("no key {k}")))
}

fn list(v: &PyValue) -> &[PyValue] {
    match v {
        PyValue::List(xs) => xs,
        other => panic!("not a list: {other:?}"),
    }
}

fn int_of(v: &PyValue, k: &str) -> i64 {
    match v.get(k) {
        Some(PyValue::Int(d)) => d.parse().unwrap(),
        other => panic!("{k}: {other:?}"),
    }
}

fn str_of<'a>(v: &'a PyValue, k: &str) -> &'a str {
    match v.get(k) {
        Some(PyValue::Str(x)) => x,
        other => panic!("{k}: {other:?}"),
    }
}

#[test]
fn an_entry_groups_wrong_claims_by_claimed_and_true_package() {
    let mut e = EntryWrongs::default();
    e.add(claim("lodash", "underscore", "a.js", 100));
    e.add(claim("lodash", "underscore", "b.js", 50));
    e.add(claim("lodash", "underscore", "a.js", 10));
    e.add(claim("ms", "debug", "x.js", 7));
    assert_eq!(e.bytes(), 167);
    assert_eq!(e.units(), 4);
    let pairs = e.pairs();
    assert_eq!(pairs.len(), 2);
    let (key, p) = &pairs[0];
    assert_eq!(*key, &("lodash".to_string(), "underscore".to_string()));
    assert_eq!((p.bytes, p.units, p.files.len()), (160, 3, 2));
    assert_eq!(p.files["a.js"], 110);
}

#[test]
fn the_doc_folds_packages_into_families_and_sorts_by_bytes() {
    let fam = Libix::shipped();
    let mut a = EntryWrongs::default();
    // lodash.get and es-toolkit are one family (fam:lodash) for the scorer.
    a.add(claim("lodash.get", "underscore", "a.js", 30));
    a.add(claim("es-toolkit", "underscore", "b.js", 40));
    a.add(claim("ms", "debug", "x.js", 100));
    let mut b = EntryWrongs::default();
    b.add(claim("ms", "debug", "y.js", 5));
    let empty = EntryWrongs::default();
    let mut sets = std::collections::BTreeMap::new();
    sets.insert(
        "s".to_string(),
        vec![
            ("app-a".to_string(), a),
            ("app-b".to_string(), b),
            ("app-c".to_string(), empty),
        ],
    );
    let doc = doc("lbl", &sets, &[], &fam);
    check_two_families(&doc, &fam);
}

fn check_two_families(doc: &PyValue, fam: &Libix) {
    let set = get(doc, &["sets", "s"]);
    assert_eq!(int_of(set, "wrongFamilyBytes"), 175);
    assert_eq!(int_of(set, "units"), 4);
    let by_family = list(get(set, &["byFamily"]));
    assert_eq!(by_family.len(), 2);
    assert_eq!(str_of(&by_family[0], "claimedFamily"), fam.family("ms"));
    assert_eq!(int_of(&by_family[0], "bytes"), 105);
    assert_eq!(int_of(&by_family[0], "entries"), 2);
    assert_eq!(
        str_of(&by_family[1], "claimedFamily"),
        fam.family("lodash.get")
    );
    assert_eq!(int_of(&by_family[1], "bytes"), 70);
    assert_eq!(int_of(&by_family[1], "entries"), 1);
    check_entries_and_packages(set);
}

fn check_entries_and_packages(set: &PyValue) {
    let by_entry = list(get(set, &["byEntry"]));
    // an entry with no wrong bytes is left out
    assert_eq!(by_entry.len(), 2);
    assert_eq!(str_of(&by_entry[0], "entry"), "app-a");
    assert_eq!(int_of(&by_entry[0], "bytes"), 170);
    let by_package = list(get(set, &["byPackage"]));
    assert_eq!(str_of(&by_package[0], "claimed"), "ms");
    assert_eq!(int_of(&by_package[0], "bytes"), 105);
    assert_eq!(int_of(&by_package[0], "files"), 2);
}

#[test]
fn a_set_scored_on_sample_truth_says_it_is_not_broken_down() {
    let fam = Libix::shipped();
    let sets = std::collections::BTreeMap::new();
    let doc = doc("lbl", &sets, &["cc".to_string()], &fam);
    assert!(matches!(
        get(&doc, &["sets", "cc", "notBrokenDown"]),
        PyValue::Str(_)
    ));
}

#[test]
fn the_breakdown_must_add_up_to_the_cards_wrong_family_bytes() {
    let mut e = EntryWrongs::default();
    e.add(claim("ms", "debug", "x.js", 7));
    assert!(check_total("s", &[("x".to_string(), e)], 7).is_ok());
    let mut e = EntryWrongs::default();
    e.add(claim("ms", "debug", "x.js", 7));
    let err = check_total("s", &[("x".to_string(), e)], 8).unwrap_err();
    assert!(err.contains("s"), "{err}");
}
