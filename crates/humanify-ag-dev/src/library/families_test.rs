//! Verdicts recorded from families_lib.py (the exp095 venv), the oracle.

use super::*;

#[test]
fn scorer_families_match_families_lib() {
    let f = Families::shipped();
    let cases = [
        ("uri", "fast-uri", false, "base:uri"),
        ("fast-uri", "uri-js", true, "fam:uri"),
        ("aws-sdk", "@aws-sdk/client-s3", false, "base:aws-sdk"),
        ("busboy", "@fastify/busboy", true, "fork:busboy"),
        ("@fastify/busboy", "@fastify/cors", false, "fork:busboy"),
        ("@xtuc/long", "long", true, "scope:@xtuc"),
        ("long", "ieee754", false, "base:long"),
        ("lodash.get", "es-toolkit", true, "fam:lodash"),
        ("@openpgp/tweetnacl", "tweetnacl", true, "scope:@openpgp"),
        ("tweetnacl", "jsdoc", false, "base:tweetnacl"),
    ];
    for (a, b, same, fam) in cases {
        assert_eq!(f.same(Some(a), Some(b)), same, "{a} ~ {b}");
        assert_eq!(f.libix.family(a), fam, "{a}");
    }
    assert!(!f.same(None, Some("x")));
}

#[test]
fn the_matcher_spells_families_differently() {
    // The declared difference: the matcher joins the npm package `uri`
    // with the listed family `uri`; the scorer does not.
    let m = humanify_ag_core::libindex::families::PackageFamilies::shipped();
    assert!(m.same("uri", "fast-uri"));
    assert!(!Families::shipped().same(Some("uri"), Some("fast-uri")));
}

#[test]
fn derived_links_identity_matches_the_recorded_summaries() {
    assert_eq!(
        derived_sha(),
        "2254544b66931385b31a7933fff7f2a40f9ecab78134e25229a7047e862fc983"
    );
}
