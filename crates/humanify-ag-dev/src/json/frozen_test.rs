use super::*;
use crate::committed::library::{CORPORA, load_corpus};
use crate::paths::repo_root;

/// The corpus identities every recorded layer manifest, label and
/// `bands.json` carries, computed before the scorer stopped writing records
/// in this spelling. A change to the identity spelling moves them.
const RECORDED_CORPUS_IDS: &[(&str, &str)] = &[
    (
        "dev",
        "749abc00ec0c228e63b9cf9fc5b43e8f452bd80a609bf76bbce8f42f890e7db5",
    ),
    (
        "test",
        "806a10156ef07435e0c2eba1e8de01d021b3f49be7b859fe11c64c87dafd708e",
    ),
    (
        "test2",
        "d497ddcf6166f05e5a8262e3c6ad30e6dfea0e901e2d953110b6824c6f5fa34a",
    ),
    (
        "bundlers",
        "93a335a04b194076036454b80a01225261653397b251ecc8a4d8f70597377dde",
    ),
    (
        "apps",
        "98b06b6770a0a73eb0fea4afd294511d4d4fa5e60cfaafe0a55b35ff3ef23e6f",
    ),
    (
        "pop",
        "626b032b7d7d11fa1aa18e9d08e789cdc83ba5a899c50e03f5a1f93641c02fea",
    ),
    (
        "remin",
        "2140b7be1c132d909d6e4bd1d3d6baa7b5cbd23f012a439f755e3bae68861f45",
    ),
    (
        "remin2",
        "ad8e97d642f9a21f445cae02bd972018118c8183cf43a422e9184aa19239e598",
    ),
    (
        "remin3",
        "d1848381676a1d79a46c18a65f743135b8a56440dbb5f299185c78cbdd4b8981",
    ),
    (
        "remin4",
        "d84ebdf5b15dcdd0f6bd3ddf400a597afa72dd5ba481dad42bdeb1dc799abefd",
    ),
    (
        "bun-dev",
        "20424b85e5c234567ec6fde11b299aacd68c92f11e64424e21d6428a25f53a7c",
    ),
    (
        "bun-held",
        "db5f016057b975a8a53934160e44b6755ea4d6ac198eddf1648610e5b5140a3f",
    ),
    (
        "kind-esbuild",
        "b41462dd52d73522267eaea472f9b986beeb55b99dade166f2903fcf4f5695d5",
    ),
    (
        "kind-webpack",
        "2a0c31dd454dc0d3b09ab93e1cf1d74148d558496c0428f4e784995773b84fc2",
    ),
    (
        "kind-rollup",
        "163ad2253b6685b39d4d850ec5f0bf729f27b805cb064edb7dd3b48c6a963a42",
    ),
    (
        "cc",
        "cd753f375938d83b7769ef1c4545764def662d01faa4a5fd805f8e31abec3f90",
    ),
];

/// Every corpus's identity is the recorded one, and every committed layer
/// of it names that identity; three layer ids are pinned as well.
#[test]
fn recorded_identities_are_unchanged() {
    assert_eq!(RECORDED_CORPUS_IDS.len(), CORPORA.len());
    for (name, want) in RECORDED_CORPUS_IDS {
        let c = load_corpus(&repo_root(), name).unwrap();
        assert_eq!(c.manifest_id, *want, "{name}");
        for l in &c.layers {
            assert_eq!(l.value.corpus_sha256, *want, "{}", l.path.display());
        }
    }
    let layer =
        |corpus: &str, key: &str| load_corpus(&repo_root(), corpus).unwrap().layer_ids[key].clone();
    assert_eq!(
        layer("apps", "originsSha256"),
        "8872eb35102a3cff989b46b2d288fbdc500e955eb6ae3b25993aed2a03e1556b"
    );
    assert_eq!(
        layer("bun-dev", "mapdefect14Sha256"),
        "ce962f23a070a782ee38da281f5a094ad31f4d218f1bea28a2c7b26c136bc240"
    );
    assert_eq!(
        layer("cc", "chainedSha256"),
        "b0f847ac78c7c7db91615ea5612ca2765b8897d53237a7e65c5d7ad140e4c4e1"
    );
}

/// `bands.json` records the dev corpus's identity in its provenance.
#[test]
fn bands_provenance_names_the_dev_corpus_identity() {
    let text = std::fs::read_to_string(
        repo_root()
            .join(crate::paths::LIBRARY_DIR)
            .join("bands.json"),
    )
    .unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["provenance"]["corpusSha256"],
        load_corpus(&repo_root(), "dev").unwrap().manifest_id
    );
}

#[test]
fn float_text_is_the_shortest_round_trip_digits() {
    let cases: &[(f64, &str)] = &[
        (0.0, "0.0"),
        (-0.0, "-0.0"),
        (1.0, "1.0"),
        (100.0, "100.0"),
        (1.5, "1.5"),
        (0.1, "0.1"),
        (0.0001, "0.0001"),
        (0.00001, "1e-05"),
        (1e16, "1e+16"),
        (1e15, "1000000000000000.0"),
        (123456789012345678.0, "1.2345678901234568e+17"),
        (1.2345e-7, "1.2345e-07"),
        (-2.5e-10, "-2.5e-10"),
        (2.71, "2.71"),
        (1.0 / 3.0, "0.3333333333333333"),
        (9007199254740992.0, "9007199254740992.0"),
        (1e22, "1e+22"),
        (5e-324, "5e-324"),
        (1.7976931348623157e308, "1.7976931348623157e+308"),
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
    ];
    for (x, want) in cases {
        assert_eq!(float_text(*x), *want, "{x:e}");
    }
}

#[test]
fn identity_text_sorts_keys_escapes_and_indents_by_one() {
    let want = "{\n \"a\": \"\\u00e9\\ud83d\\ude00\\u007f\\n\\\"\",\n \"b\": [\n  1,\n  2.0,\n  {}\n ],\n \"c\": [],\n \"d\": {\n  \"y\": true,\n  \"z\": null\n }\n}\n";
    let v: Value = serde_json::from_str(want).unwrap();
    assert_eq!(identity_text(&v), want);
    // Keys arrive unsorted; ints and floats stay apart; the last duplicate wins.
    let v: Value = serde_json::from_str(r#"{"b": 1.0, "a": 1, "b": 2.50}"#).unwrap();
    assert_eq!(identity_text(&v), "{\n \"a\": 1,\n \"b\": 2.5\n}\n");
    assert_eq!(compact(&v), "{\"a\":1,\"b\":2.5}");
}
