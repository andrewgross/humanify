//! The package families: explicit families, the same-scope rule and the
//! scoped forks.

use crate::libindex::families::PackageFamilies;

#[test]
fn families_follow_the_shipped_rules() {
    let f = PackageFamilies::shipped();
    // An explicit family, with a prefix member.
    assert_eq!(
        f.family("@aws-sdk/client-s3"),
        f.family("@smithy/node-http-handler")
    );
    assert_eq!(f.family("lodash.merge"), f.family("es-toolkit"));
    assert_eq!(f.family("minimatch"), f.family("glob"));
    // Two packages of one scope are one family.
    assert_eq!(
        f.family("@anthropic-ai/sdk"),
        f.family("@anthropic-ai/sandbox-runtime")
    );
    assert_eq!(
        f.family("@azure/identity"),
        f.family("@azure/core-rest-pipeline")
    );
    // A scoped fork is its original's family.
    assert_eq!(f.family("@colors/colors"), f.family("colors"));
    // Unrelated packages are not.
    assert_ne!(f.family("react"), f.family("react-dom"));
    assert_ne!(f.family("@aws-sdk/client-s3"), f.family("@azure/identity"));
    assert_eq!(f.family("ink"), "ink");
}

#[test]
fn derived_links_join_packages_pairwise_never_through_a_scope() {
    let links = [
        ("mimic-fn", "mimic-function"),
        ("tweetnacl", "@openpgp/tweetnacl"),
        ("jsdoc", "@openpgp/jsdoc"),
        ("a", "b"),
        ("b", "c"),
    ];
    let f = PackageFamilies::shipped()
        .with_links(links.iter().map(|(a, b)| (a.to_string(), b.to_string())));
    assert!(f.same("mimic-fn", "mimic-function"));
    assert!(f.same("tweetnacl", "@openpgp/tweetnacl"));
    // one scope is one family without any link
    assert!(f.same("@openpgp/tweetnacl", "@openpgp/jsdoc"));
    // a chain of links counts; a chain THROUGH a scope family does not
    assert!(f.same("a", "c"));
    assert!(!f.same("tweetnacl", "jsdoc"));
    // the listed families still hold, and unlinked packages stay apart
    assert!(f.same("minimatch", "glob"));
    assert!(!f.same("react", "react-dom"));
    let plain = PackageFamilies::shipped();
    assert!(!plain.same("mimic-fn", "mimic-function"));
    assert!(plain.same("minimatch", "glob"));
}

#[test]
fn the_shipped_derived_links_parse_and_join_a_known_rename() {
    let f = PackageFamilies::shipped().with_shipped_links();
    assert!(f.link_count() > 0);
    assert!(f.same("mimic-fn", "mimic-function"));
}

#[test]
fn the_shipped_derived_links_cover_the_default_index_s_packages() {
    // exp119: the links are derived over EVERY package of the default index
    // (k2-all), so a GitHub-recorded fork the index holds under a scope is its
    // original's family (exp113/exp117 saw @redocly/ajv claim ajv's code).
    let f = PackageFamilies::shipped().with_shipped_links();
    assert!(f.same("ajv", "@redocly/ajv"));
}
