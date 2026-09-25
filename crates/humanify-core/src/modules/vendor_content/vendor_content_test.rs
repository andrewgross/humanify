//! The content key is SYMMETRIC between a fresh bundle's factory body and
//! the prior tree's vendor file (relinked or not), whatever the release-
//! specific spellings; the re-key carries only exact group matches.

use super::{
    TsEraEntry, UNJOINED, prior_file_content_key, rekey_prior_by_content, vendor_content_key,
};

/// A fresh factory body: `REQ` is the bundle's require var, `qA`/`kC` are
/// other factories, `hZ` a bundle-level helper the unpack leaves free.
const FRESH: &str =
    "(exports,module)=>{ var p=REQ(\"node:path\"); module.exports=qA(p.sep)+kC+hZ(process); }";

fn fresh_key() -> String {
    let body = crate::unpack::bun::rewrite_require_calls(FRESH, "REQ");
    vendor_content_key(&body).expect("parses")
}

#[test]
fn a_relinked_prior_vendor_file_keys_like_the_fresh_body() {
    let relinked = concat!(
        "const lib_0a1b2c3d = require(\"./a/lib_0a1b2c3d.js\");\n",
        "const lib_99999999_2 = require(\"./lib_99999999-2.js\");\n",
        "const { __commonJS } = require(\"../.humanify/__bun-runtime.js\");\n",
        "exports.f = __commonJS((E,M)=>{ var s=require(\"node:path\"); M.exports=lib_0a1b2c3d.f(s.sep)+lib_99999999_2.f+Wq(process); });\n",
    );
    assert_eq!(prior_file_content_key(relinked), Some(fresh_key()));
}

#[test]
fn an_unlinked_prior_vendor_file_keys_like_the_fresh_body() {
    let raw = "(E,M)=>{ var s=require(\"node:path\"); M.exports=lib_0a1b2c3d(s.sep)+lib_99999999_2+Wq(process); }";
    assert_eq!(prior_file_content_key(raw), Some(fresh_key()));
}

#[test]
fn content_and_known_globals_are_in_the_key() {
    // A known global is content (verbatim): `Buffer` is not `process`.
    let other_global = crate::unpack::bun::rewrite_require_calls(
        "(exports,module)=>{ var p=REQ(\"node:path\"); module.exports=qA(p.sep)+kC+hZ(Buffer); }",
        "REQ",
    );
    assert_ne!(vendor_content_key(&other_global), Some(fresh_key()));
    // So is a literal's class and the tree shape.
    assert_ne!(
        vendor_content_key("(e,m)=>{ m.exports=qA(1); }"),
        vendor_content_key("(e,m)=>{ m.exports=qA(\"1\"); }")
    );
    // Two references to ONE outside binding are not two outside bindings.
    assert_ne!(
        vendor_content_key("(e,m)=>{ m.exports=qA(qA); }"),
        vendor_content_key("(e,m)=>{ m.exports=qA(kC); }")
    );
    // An unparseable text keys to nothing.
    assert_eq!(vendor_content_key("(a,b)=>{"), None);
    assert_eq!(prior_file_content_key("var x = 1;"), None);
}

fn entry(name: &str, ts: &str, ordinal: usize, key: Option<&str>) -> TsEraEntry {
    TsEraEntry {
        name: name.into(),
        ts_hash: ts.into(),
        ordinal,
        key: key.map(String::from),
    }
}

#[test]
fn the_rekey_carries_exact_groups_in_prior_bundle_order() {
    // Prior (manifest order): the shim pair was emitted in REVERSE bundle
    // order; hashOrdinal restores it.
    let prior = vec![
        entry("shim-b", "t2", 1, Some("K2")),
        entry("dep", "t1", usize::MAX, Some("K1")),
        entry("shim-a", "t2", 0, Some("K2")),
        entry("gone", "t9", usize::MAX, Some("K9")),
        entry("unread", "t8", usize::MAX, None),
    ];
    let fresh = vec![
        ("h1".to_string(), Some("K1".to_string())),
        ("h2".to_string(), Some("K2".to_string())),
        ("h2".to_string(), Some("K2".to_string())),
        ("h3".to_string(), Some("K3".to_string())),
    ];
    let r = rekey_prior_by_content(&fresh, &prior);
    assert_eq!(r.names.get("h1"), Some(&vec!["dep".to_string()]));
    assert_eq!(
        r.names.get("h2"),
        Some(&vec!["shim-a".to_string(), "shim-b".to_string()])
    );
    assert_eq!(r.names.get("h3"), None);
    let hashes: Vec<&str> = r
        .factories
        .iter()
        .map(|e| e.structural_hash.as_str())
        .collect();
    assert_eq!(
        hashes,
        vec![
            "h2",
            "h1",
            "h2",
            &format!("{UNJOINED}t9"),
            &format!("{UNJOINED}t8")
        ]
    );
    assert_eq!(
        (
            r.stats.prior_entries,
            r.stats.prior_keyed,
            r.stats.groups_joined,
            r.stats.factories_joined
        ),
        (5, 4, 2, 3)
    );
}

#[test]
fn the_rekey_refuses_anything_inexact() {
    // One content key over two TS groups with different names: the bundle
    // order between them is unknown — refused.
    let prior = vec![
        entry("a", "t1", usize::MAX, Some("K")),
        entry("b", "t2", usize::MAX, Some("K")),
    ];
    let fresh = vec![
        ("h".to_string(), Some("K".to_string())),
        ("h".to_string(), Some("K".to_string())),
    ];
    let r = rekey_prior_by_content(&fresh, &prior);
    assert!(r.names.is_empty());
    assert_eq!(r.stats.prior_groups_ambiguous, 1);
    // ...but the class correspondence still anchors the manifest ORDER, as
    // the TS's hash pass did whether or not a name carried.
    assert!(r.factories.iter().all(|e| e.structural_hash == "h"));
    // The same names in both: order is immaterial — carried.
    let same = vec![
        entry("a", "t1", usize::MAX, Some("K")),
        entry("a", "t2", usize::MAX, Some("K")),
    ];
    assert_eq!(rekey_prior_by_content(&fresh, &same).names.len(), 1);
    // A fresh hash group whose members key differently, or a key spread
    // over two fresh hash groups: never carried.
    let prior = vec![entry("a", "t1", usize::MAX, Some("K"))];
    let split = vec![
        ("h".to_string(), Some("K".to_string())),
        ("h".to_string(), Some("L".to_string())),
    ];
    assert!(rekey_prior_by_content(&split, &prior).names.is_empty());
    let spread = vec![
        ("h".to_string(), Some("K".to_string())),
        ("g".to_string(), Some("K".to_string())),
    ];
    assert!(rekey_prior_by_content(&spread, &prior).names.is_empty());
}
