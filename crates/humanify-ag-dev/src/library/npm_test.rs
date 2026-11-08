use super::*;

#[test]
fn hashed_chunk_names() {
    assert!(hashed("index-AbC_1234"));
    assert!(hashed("chunk-ABCDEFGH-ab-c_d-e"));
    assert!(!hashed("-ABCDEFGH"));
    assert!(!hashed("index-ABC.1234"));
    assert!(chunk_dir("dist/_chunks/x.js"));
    assert!(chunk_dir("libs/x.js"));
    assert!(!chunk_dir("dist/chunks.js"));
}

#[test]
fn license_collections() {
    assert_eq!(
        lic_dir("package/dist/licenses/@s/p/LICENSE").as_deref(),
        Some("@s/p")
    );
    assert_eq!(
        lic_dir("package/Third-Party_Licenses/foo/LICENSE.txt").as_deref(),
        Some("foo")
    );
    assert_eq!(lic_dir("package/licenses/LICENSE"), None);
    assert!(lic_md("package/LICENSE.md"));
    assert!(lic_md("package/third_party_licenses"));
    assert!(lic_md("package/THIRD-PARTY-LICENSE.md"));
    assert!(!lic_md("package/dist/LICENSE"));
    let md = "# Licenses\n## lodash\n### @scope/pkg  \n## not a name\n#### deep\n##nospace\n";
    assert_eq!(
        headings(md),
        vec!["lodash".to_string(), "@scope/pkg".to_string()]
    );
}

#[test]
fn quote_is_urllib_s() {
    assert_eq!(quote("@scope/pkg", "@"), "@scope%2Fpkg");
    assert_eq!(quote("1.0.0-beta/x", "/"), "1.0.0-beta/x");
}

#[test]
fn json_dump_sorted() {
    let v = PyValue::parse(r#"{"b": [1, "x"], "a": {"d": null, "c": true}}"#).expect("parse");
    let mut s = String::new();
    dumps_sorted(&mut s, &v);
    assert_eq!(s, r#"{"a": {"c": true, "d": null}, "b": [1, "x"]}"#);
}
