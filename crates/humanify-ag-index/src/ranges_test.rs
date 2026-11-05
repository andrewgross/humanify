//! npm range matching over stable versions (the spec planner's dependency
//! resolution): node-semver's grammar for the shapes package.json ranges
//! use, and the dependency specs that are not ranges at all.

use super::ranges::{DepSpec, Range, dep_spec};
use humanify_ag_core::libindex::format::Semver;

fn sat(range: &str, version: &str) -> bool {
    let r = Range::parse(range).unwrap_or_else(|| panic!("{range} parses"));
    r.satisfies(&Semver::parse(version).expect("a version"))
}

#[test]
fn caret_tilde_and_x_ranges_desugar_as_node_semver_does() {
    let cases: &[(&str, &str, bool)] = &[
        ("^1.2.3", "1.9.9", true),
        ("^1.2.3", "2.0.0", false),
        ("^1.2.3", "1.2.2", false),
        ("^0.2.3", "0.2.9", true),
        ("^0.2.3", "0.3.0", false),
        ("^0.0.3", "0.0.3", true),
        ("^0.0.3", "0.0.4", false),
        ("^1.x", "1.0.0", true),
        ("^0.x", "0.9.0", true),
        ("^0.0.x", "0.0.9", true),
        ("^0.0.x", "0.1.0", false),
        ("~1.2", "1.2.9", true),
        ("~1.2", "1.3.0", false),
        ("~1", "1.9.0", true),
        ("~0.2.3", "0.3.0", false),
        ("~>1.2.3", "1.2.9", true),
        ("1.x", "1.4.0", true),
        ("1.2.x", "1.3.0", false),
        ("2", "2.9.9", true),
        ("2", "3.0.0", false),
        ("*", "5.0.0", true),
        ("", "0.0.1", true),
        ("x", "0.0.1", true),
        ("latest", "9.9.9", true),
    ];
    for (r, v, want) in cases {
        assert_eq!(sat(r, v), *want, "{r} vs {v}");
    }
}

#[test]
fn comparators_hyphens_and_unions() {
    let cases: &[(&str, &str, bool)] = &[
        (">=1.0.0 <2", "1.5.0", true),
        (">=1.0.0 <2", "2.0.0", false),
        (">= 1.2.3", "1.2.3", true),
        (">1.2", "1.2.9", false),
        (">1.2", "1.3.0", true),
        (">1", "2.0.0", true),
        ("<=1.2", "1.2.5", true),
        ("<=1.2", "1.3.0", false),
        ("<1.2", "1.2.0", false),
        ("<1.2", "1.1.9", true),
        ("=1.2.3", "1.2.3", true),
        ("v1.2.3", "1.2.4", false),
        ("1.2.3 - 2.3", "2.3.9", true),
        ("1.2.3 - 2.3", "2.4.0", false),
        ("1.2.3 - 2.3.4", "2.3.4", true),
        ("1.2 - 2", "2.9.0", true),
        ("1.2 - 2", "1.1.0", false),
        ("1.x || >=3", "3.1.0", true),
        ("1.x || >=3", "2.1.0", false),
        ("^1.0.0 || ^2.0.0", "2.4.1", true),
    ];
    for (r, v, want) in cases {
        assert_eq!(sat(r, v), *want, "{r} vs {v}");
    }
}

#[test]
fn a_prerelease_bound_still_admits_the_stable_releases_above_it() {
    assert!(sat("^1.0.0-beta.1", "1.0.0"));
    assert!(sat("^1.0.0-beta.1", "1.4.0"));
    assert!(!sat("^1.0.0-beta.1", "2.0.0"));
    // `<2.0.0-0` is below 2.0.0 itself
    assert!(!sat("<2.0.0-0", "2.0.0"));
    assert!(sat("<=1.2.3-rc.1", "1.2.2"));
    assert!(!sat("<=1.2.3-rc.1", "1.2.3"));
}

#[test]
fn what_is_not_a_range_does_not_parse() {
    for r in [
        "github:a/b",
        "git+https://x/y.git",
        "file:../x",
        "http://x/y.tgz",
        "1.2.3.4",
        "^a",
    ] {
        assert!(Range::parse(r).is_none(), "{r}");
    }
}

#[test]
fn a_dependency_spec_is_a_range_an_alias_or_nothing_resolvable() {
    assert_eq!(
        dep_spec("left-pad", "^1.0.0"),
        Some(DepSpec {
            package: "left-pad".into(),
            range: Range::parse("^1.0.0").expect("range")
        })
    );
    assert_eq!(
        dep_spec("strip-ansi-cjs", "npm:strip-ansi@^6.0.1"),
        Some(DepSpec {
            package: "strip-ansi".into(),
            range: Range::parse("^6.0.1").expect("range")
        })
    );
    assert_eq!(
        dep_spec("x", "npm:@scope/pkg@~2.1"),
        Some(DepSpec {
            package: "@scope/pkg".into(),
            range: Range::parse("~2.1").expect("range")
        })
    );
    assert_eq!(
        dep_spec("x", "npm:other"),
        Some(DepSpec {
            package: "other".into(),
            range: Range::parse("*").expect("range")
        })
    );
    assert_eq!(dep_spec("x", "github:a/b"), None);
    assert_eq!(dep_spec("x", "workspace:*"), None);
}
