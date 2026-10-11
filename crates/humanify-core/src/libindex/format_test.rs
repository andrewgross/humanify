//! The index format: semver order, version ranges, the fingerprint stamps.

use std::cmp::Ordering;

use super::format::{ModuleRow, Semver, fingerprint_versions, sort_versions, version_ranges};

#[test]
fn versions_order_by_semver_precedence_not_by_text() {
    let mut v: Vec<String> = [
        "1.10.0",
        "1.9.0",
        "1.0.0",
        "1.0.0-beta.2",
        "1.0.0-beta.11",
        "1.0.0-alpha",
        "0.9.12",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    sort_versions(&mut v);
    assert_eq!(
        v,
        [
            "0.9.12",
            "1.0.0-alpha",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0",
            "1.9.0",
            "1.10.0"
        ]
    );
}

#[test]
fn a_version_parses_into_its_parts_and_knows_if_it_is_a_prerelease() {
    let v = Semver::parse("2.1.3").expect("semver");
    assert_eq!((v.major, v.minor, v.patch), (2, 1, 3));
    assert!(!v.is_prerelease());
    assert!(
        Semver::parse("3.0.0-canary.1")
            .expect("semver")
            .is_prerelease()
    );
    // build metadata does not order
    assert_eq!(
        Semver::parse("1.0.0+abc")
            .expect("semver")
            .cmp(&Semver::parse("1.0.0").expect("semver")),
        Ordering::Equal
    );
    assert_eq!(Semver::parse("latest"), None);
    assert_eq!(Semver::parse("1.2"), None);
}

#[test]
fn contiguous_indexed_versions_collapse_into_one_range() {
    let indexed = ["1.0.0", "1.1.0", "1.2.0", "2.0.0", "2.1.0"].map(String::from);
    let present = ["1.0.0", "1.1.0", "1.2.0", "2.1.0"].map(String::from);
    assert_eq!(
        version_ranges(&indexed, &present),
        vec![
            ("1.0.0".to_string(), "1.2.0".to_string()),
            ("2.1.0".to_string(), "2.1.0".to_string()),
        ]
    );
}

#[test]
fn a_version_that_is_not_indexed_never_appears_in_a_range() {
    let indexed = ["1.0.0", "2.0.0"].map(String::from);
    let present = ["1.0.0", "1.5.0", "2.0.0"].map(String::from);
    assert_eq!(
        version_ranges(&indexed, &present),
        vec![("1.0.0".to_string(), "2.0.0".to_string())]
    );
}

#[test]
fn a_module_row_serializes_with_a_fixed_key_order() {
    let row = ModuleRow {
        key: "abc".into(),
        owner: "ms".into(),
        files: vec!["index.js".into()],
        versions: vec![("2.1.2".into(), "2.1.3".into())],
    };
    assert_eq!(
        serde_json::to_string(&row).expect("json"),
        r#"{"key":"abc","owner":"ms","files":["index.js"],"versions":[["2.1.2","2.1.3"]]}"#
    );
}

#[test]
fn the_fingerprint_stamp_names_the_pipelines_own_versions() {
    let f = fingerprint_versions();
    assert_eq!(
        f.module,
        format!(
            "vendor-content-key/{}",
            crate::modules::FACTORY_HASH_VERSION
        )
    );
    assert_eq!(
        f.literals,
        format!("literals/{}", super::literals::LITERALS_VERSION)
    );
}
