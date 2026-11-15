//! The prior release's tree as one value: what `PriorTree::parse` finds in
//! the files the shell read, what it refuses, and how a file is read.

use std::collections::BTreeMap;

use super::{PriorFiles, PriorTree, PriorTreeError, enters_folder};
use crate::naming::vendor_bodies::VENDOR_NAMING_SOURCE;

fn files(entries: &[(&str, &[u8])]) -> PriorFiles {
    PriorFiles::new(
        entries
            .iter()
            .map(|(p, b)| (p.to_string(), b.to_vec()))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// A one-entry vendor manifest this binary writes (current hash era).
fn manifest(version: u64) -> String {
    serde_json::json!({
        "adapter": "bun",
        "hashVersion": version,
        "factories": [{
            "factoryVar": "a",
            "fileName": "vendor/pkg.js",
            "name": "pkg",
            "nameSource": "llm",
            "structuralHash": "h1",
            "runtimeIdentifier": "pkg_x",
            "identity": null
        }]
    })
    .to_string()
}

#[test]
fn a_tree_without_a_vendor_manifest_has_no_vendor_record() {
    let tree = PriorTree::parse("prior".into(), files(&[("src/a.js", b"x")])).unwrap();
    assert!(tree.vendor.is_none());
    assert!(!tree.has_vendor_manifest());
    assert!(tree.vendor_naming.is_none());
    assert_eq!(tree.code, "prior");
}

#[test]
fn the_vendor_manifest_is_read_with_its_module_files() {
    let current = crate::hash::forms::MATCH_KEY.version;
    let tree = PriorTree::parse(
        String::new(),
        files(&[
            ("vendor/_bun-modules.json", manifest(current).as_bytes()),
            ("vendor/pkg.js", b"module text"),
            (VENDOR_NAMING_SOURCE, b"joined"),
        ]),
    )
    .unwrap();
    assert!(tree.has_vendor_manifest());
    let vendor = tree.vendor.expect("a vendor record");
    assert!(
        vendor.names.is_some(),
        "a current-era manifest joins by hash"
    );
    let files = vendor.tree.expect("the prior's module files");
    assert_eq!(files.texts, vec![Some("module text".to_string())]);
    assert!(files.bodies_named, "the named-bodies source is there");
    assert_eq!(tree.vendor_naming.as_deref(), Some("joined"));
}

#[test]
fn a_manifest_this_binary_cannot_read_is_an_error_not_absent() {
    let err = PriorTree::parse(
        String::new(),
        files(&[("vendor/_bun-modules.json", b"{\"factories\": []}")]),
    )
    .expect_err("an unstamped manifest is refused");
    let PriorTreeError::VendorManifest { why } = &err else {
        panic!("{err}");
    };
    assert!(why.contains("hashVersion"), "{why}");
}

#[test]
fn the_named_bodies_source_belongs_to_a_tree_with_a_vendor_manifest() {
    let tree =
        PriorTree::parse(String::new(), files(&[(VENDOR_NAMING_SOURCE, b"joined")])).unwrap();
    assert!(tree.vendor_naming.is_none(), "no manifest, no vendor tree");
}

#[test]
fn a_named_bodies_source_that_is_not_text_is_an_error() {
    let current = crate::hash::forms::MATCH_KEY.version;
    let err = PriorTree::parse(
        String::new(),
        files(&[
            ("vendor/_bun-modules.json", manifest(current).as_bytes()),
            (VENDOR_NAMING_SOURCE, b"\xff"),
        ]),
    )
    .expect_err("not UTF-8");
    assert!(
        matches!(err, PriorTreeError::VendorNamingNotText { .. }),
        "{err}"
    );
}

#[test]
fn a_file_reads_strictly_or_lossily_as_its_reader_asks() {
    let f = files(&[("a.js", b"ok"), ("b.js", b"\xffx")]);
    assert_eq!(f.text("a.js"), Some("ok"));
    assert_eq!(f.text("b.js"), None, "strict: not UTF-8 reads as absent");
    assert_eq!(f.text_lossy("b.js").as_deref(), Some("\u{fffd}x"));
    assert_eq!(f.text("missing.js"), None);
    assert!(f.contains("a.js") && !f.contains("missing.js"));
}

#[test]
fn the_shell_reads_the_metadata_folder_and_skips_other_hidden_and_installed_folders() {
    assert!(enters_folder(".humanify"));
    assert!(enters_folder("src"));
    assert!(enters_folder("vendor"));
    assert!(!enters_folder(".git"));
    assert!(!enters_folder("node_modules"));
}
