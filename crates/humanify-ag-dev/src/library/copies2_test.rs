use super::*;

/// copies2.py `named` on these inputs (recorded from Python 3.11).
#[test]
fn named_is_python_s() {
    let cases: &[(&str, &str, bool)] = &[
        ("import x from \"lodash\"; lodash.map()", "lodash", true),
        ("import x from \"lodash\";", "lodash", false),
        ("require(\"lodash/fp\")", "lodash", false),
        ("require( `lodash` )", "lodash", false),
        ("my-lodash is here", "lodash", false),
        ("lodash_x lodash", "lodash", true),
        ("xlodash", "lodash", false),
        ("import(\"@scope/pkg/sub\") @scope/pkg", "@scope/pkg", true),
        ("from\"a.b\" a.b", "a.b", true),
        ("importlodash \"lodash\"", "lodash", true),
        ("\u{e9}from \"lodash\"", "lodash", true),
        ("from \"lodash'", "lodash", true),
        ("", "x", false),
        ("ab ab-c abab", "ab", true),
    ];
    for (text, name, want) in cases {
        assert_eq!(named(text, name), *want, "{text:?} {name:?}");
    }
}

#[test]
fn install_root_is_the_last_segment() {
    assert_eq!(
        install_root("/a/node_modules/x/node_modules/y/lib.js", "y").as_deref(),
        Some("/a/node_modules/x/node_modules/y")
    );
    assert_eq!(install_root("/a/src/y.js", "y"), None);
}
