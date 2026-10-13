//! The content key is SYMMETRIC between a fresh bundle's factory body and
//! the prior tree's vendor file (relinked or not), whatever the release-
//! specific spellings; the re-key carries only exact group matches.

use std::collections::HashSet;

use super::{
    NameSource, StaleEraEntry, UNJOINED, prior_file_content_key, rekey_prior_by_content,
    vendor_content_key,
};

/// A fresh factory body: `REQ` is the bundle's require var, `qA`/`kC` are
/// other factories, `hZ` a bundle-level helper the unpack leaves free.
const FRESH: &str =
    "(exports,module)=>{ var p=REQ(\"node:path\"); module.exports=qA(p.sep)+kC+hZ(process); }";

/// A content key with no program-declared names.
fn key(text: &str) -> Option<String> {
    vendor_content_key(text, &HashSet::new())
}

fn fresh_key() -> String {
    let body = crate::unpack::bun::rewrite_require_calls(FRESH, "REQ");
    key(&body).expect("parses")
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
    assert_ne!(key(&other_global), Some(fresh_key()));
    // So is a literal's class and the tree shape.
    assert_ne!(
        key("(e,m)=>{ m.exports=qA(1); }"),
        key("(e,m)=>{ m.exports=qA(\"1\"); }")
    );
    // Two references to ONE outside binding are not two outside bindings.
    assert_ne!(
        key("(e,m)=>{ m.exports=qA(qA); }"),
        key("(e,m)=>{ m.exports=qA(kC); }")
    );
    // An unparseable text keys to nothing.
    assert_eq!(key("(a,b)=>{"), None);
    assert_eq!(prior_file_content_key("var x = 1;"), None);
}

#[test]
fn a_program_declared_name_that_spells_a_known_global_is_a_slot() {
    // 2.1.85: Bun spelled the protobufjs factory `gc` (also Node's
    // `--expose-gc` hook), and its dependents read it as `gc()`. The prior
    // vendor file reads the same factory as `lib_f3ea04ef.f()`.
    let fresh = "(FIO,ru7)=>{ru7.exports=LJ;var KsH=gc();function LJ(H){this.lo=H}}";
    let prior = concat!(
        "const lib_f3ea04ef = require(\"./protobufjs-minimal/lib_f3ea04ef.js\");\n",
        "const { __commonJS } = require(\"../.humanify/__bun-runtime.js\");\n",
        "exports.f = __commonJS((E,M)=>{M.exports=LJ;var K=lib_f3ea04ef.f();function LJ(H){this.lo=H}});\n",
    );
    let bundle_declared: HashSet<String> = ["gc".to_string()].into();
    assert_eq!(
        vendor_content_key(fresh, &bundle_declared),
        prior_file_content_key(prior)
    );
    // Undeclared, `gc` IS the host's global: content, verbatim.
    assert_ne!(key(fresh), prior_file_content_key(prior));
}

fn entry(name: &str, ts: &str, ordinal: usize, key: Option<&str>) -> StaleEraEntry {
    StaleEraEntry {
        name: name.into(),
        origin: NameSource::Llm,
        era_hash: ts.into(),
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
    let names_of = |h: &str| {
        r.names
            .get(h)
            .map(|v| v.iter().map(|c| c.name.as_str()).collect::<Vec<_>>())
    };
    assert_eq!(names_of("h1"), Some(vec!["dep"]));
    assert_eq!(names_of("h2"), Some(vec!["shim-a", "shim-b"]));
    // A carry keeps each name's prior label (finding #71).
    assert!(
        r.names
            .values()
            .flatten()
            .all(|c| c.origin == NameSource::Llm)
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

// ---- a whole bundle's module keys (the library index's module fingerprint)

/// The same two CommonJS modules, minified twice with different spellings.
const BUNDLE_A: &str = r#"var m=(e,r)=>()=>(r||e((r={exports:{}}).exports,r),r.exports);var B=m((ge,q)=>{q.exports=function(e){return e+"abc"}});var C=m((x,y)=>{var t=B();y.exports=function(n){return t(n)*1024}});module.exports=C();"#;
const BUNDLE_B: &str = r#"var k=(a,b)=>()=>(b||a((b={exports:{}}).exports,b),b.exports);var Z=k((Q,W)=>{W.exports=function(n){return n+"abc"}});var Y=k((P,O)=>{var s=Z();O.exports=function(v){return s(v)*1024}});module.exports=Y();"#;

fn bundle_keys(code: &str) -> Vec<super::BundleModuleKey> {
    super::bundle_content_keys(
        code,
        crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )
}

#[test]
fn every_bundled_module_gets_the_pipelines_content_key_in_bundle_order() {
    let a = bundle_keys(BUNDLE_A);
    assert_eq!(a.len(), 2);
    let first = &BUNDLE_A[a[0].body_span.start as usize..a[0].body_span.end as usize];
    assert_eq!(first, r#"(ge,q)=>{q.exports=function(e){return e+"abc"}}"#);
    // The key IS the vendor content key of that factory's text.
    assert_eq!(a[0].key, key(first));
    assert!(a.iter().all(|m| m.key.is_some()));
    assert_ne!(a[0].key, a[1].key);
}

#[test]
fn a_bundled_module_keys_the_same_whatever_the_minifier_named_things() {
    let a: Vec<Option<String>> = bundle_keys(BUNDLE_A).into_iter().map(|m| m.key).collect();
    let b: Vec<Option<String>> = bundle_keys(BUNDLE_B).into_iter().map(|m| m.key).collect();
    assert_eq!(a, b);
}

#[test]
fn a_bundle_without_modules_has_no_module_keys() {
    assert!(bundle_keys("function f(){return 1}module.exports=f;").is_empty());
}
