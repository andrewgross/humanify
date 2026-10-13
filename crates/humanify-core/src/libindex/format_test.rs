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
    assert_eq!(f.module, crate::hash::forms::VENDOR_CONTENT.stamp());
    assert_eq!(
        f.literals,
        format!("literals/{}", crate::hash::forms::LIBRARY_LITERALS_VERSION)
    );
    assert_eq!(f.units, crate::hash::forms::LIBRARY_UNITS.stamp());
}

fn manifest_for(recipes: &[&str]) -> super::format::Manifest {
    use super::format::{INDEX_FORMAT, Manifest, RECIPE_VERSION, SliceRecord};
    use super::recipe::Recipe;
    Manifest {
        format: INDEX_FORMAT.into(),
        as_of: "2026-10-11T23:59:59.999Z".into(),
        fingerprints: fingerprint_versions(),
        registry: Default::default(),
        slices: recipes
            .iter()
            .map(|s| SliceRecord {
                slice: Recipe::parse(s).unwrap().slice_name(),
                recipe: (*s).into(),
                recipe_version: RECIPE_VERSION.into(),
                default_for_bundler: true,
                bundler: "bun".into(),
                version: "1.0.0".into(),
                published: None,
                newest_at_as_of: None,
                bundler_after_as_of: false,
            })
            .collect(),
        packages: vec![],
        refused: vec![],
        shards: Default::default(),
        tables: Default::default(),
        data: Default::default(),
    }
}

const BUN: &str = "bun:target=bun,format=cjs,minify";

#[test]
fn a_manifest_built_by_this_binary_is_compatible() {
    assert_eq!(
        manifest_for(&[BUN, "esbuild:target=node,format=cjs,minify"]).check_compatible(),
        Ok(())
    );
}

#[test]
fn every_compatibility_version_is_checked_and_named() {
    // Each stamp alone: the refusal names the field, both spellings, and
    // how to rebuild (fail early, never half-match).
    type Edit = fn(&mut super::format::Manifest);
    let cases: [(&str, Edit); 7] = [
        ("format", |m| m.format = "humanify-library-index/0".into()),
        ("module", |m| {
            m.fingerprints.module = "vendor-content-key/3".into()
        }),
        ("literals", |m| {
            m.fingerprints.literals = "literals/0".into()
        }),
        ("units", |m| m.fingerprints.units = "units/2".into()),
        ("recipe version", |m| {
            m.slices[0].recipe_version = "recipe/3".into()
        }),
        ("recipe", |m| m.slices[0].recipe = "bun:target=bun".into()),
        ("slice", |m| m.slices[0].slice = "bun".into()),
    ];
    for (field, edit) in cases {
        let mut m = manifest_for(&[BUN]);
        edit(&mut m);
        let err = m.check_compatible().expect_err(field);
        assert!(err.contains(field), "{field}: {err}");
        assert!(err.contains("humanify-index build"), "{field}: {err}");
    }
}

#[test]
fn an_unknown_or_missing_stamp_does_not_deserialize() {
    // A stamp this binary does not know (a newer index) or one it expects
    // and the manifest lacks (an older one) is a refusal, not a default.
    let m = manifest_for(&[BUN]);
    let mut v = serde_json::to_value(&m).unwrap();
    v["fingerprints"]["serializer"] = "x/1".into();
    assert!(serde_json::from_value::<super::format::Manifest>(v).is_err());
    let mut v = serde_json::to_value(&m).unwrap();
    v["fingerprints"].as_object_mut().unwrap().remove("units");
    assert!(serde_json::from_value::<super::format::Manifest>(v).is_err());
    let mut v = serde_json::to_value(&m).unwrap();
    v["slices"][0]
        .as_object_mut()
        .unwrap()
        .remove("recipeVersion");
    assert!(serde_json::from_value::<super::format::Manifest>(v).is_err());
}

#[test]
fn every_form_the_index_manifest_stamps_is_a_checked_field() {
    // The form registry is the list of persisted stamps: each one stamped
    // in the library index manifest must be a FingerprintVersions field
    // that check_compatible compares (a new one fails here until it is).
    let stamped: Vec<String> = crate::hash::forms::FORMS
        .iter()
        .filter(|f| {
            f.stamped_in
                .is_some_and(|s| s.starts_with("library index manifest"))
        })
        .map(|f| f.stamp())
        .collect();
    let f = fingerprint_versions();
    assert_eq!(stamped, vec![f.units.clone(), f.module.clone()]);
}

#[test]
fn an_older_manifest_is_refused_by_its_stamps_not_a_parse_error() {
    // index v2's manifest shape: format /1, units/2, slices without recipes
    let old = r#"{"format":"humanify-library-index/1","fingerprints":{"module":"vendor-content-key/4","literals":"literals/1","units":"units/2"},"slices":[{"slice":"bun"}]}"#;
    let err = super::format::check_manifest_stamps(old).expect_err("older era");
    assert!(
        err.contains("humanify-library-index/1") && err.contains("units/2"),
        "{err}"
    );
    assert!(err.contains("humanify-index build"), "{err}");
    let mut ours = serde_json::to_value(manifest_for(&[BUN])).unwrap();
    assert_eq!(
        super::format::check_manifest_stamps(&ours.to_string()),
        Ok(())
    );
    ours["fingerprints"]["serializer"] = "x/1".into();
    let err = super::format::check_manifest_stamps(&ours.to_string()).expect_err("unknown");
    assert!(err.contains("serializer"), "{err}");
}
