//! Ported from src/split/emitter.test.ts.

use super::compute_relative_import_path as rel;

#[test]
fn same_directory_uses_dot_slash() {
    assert_eq!(rel("app.js", "utils.js"), "./utils.js");
    assert_eq!(rel("src/helpers/a.js", "src/helpers/b.js"), "./b.js");
}

#[test]
fn sibling_and_parent_directories_use_dot_dot() {
    assert_eq!(rel("src/app.js", "lib/utils.js"), "../lib/utils.js");
    assert_eq!(rel("helpers/utils.js", "shared.js"), "../shared.js");
    assert_eq!(
        rel("src/components/app.js", "src/helpers/util.js"),
        "../helpers/util.js"
    );
}

#[test]
fn child_directory_uses_dot_slash_child() {
    assert_eq!(rel("app.js", "helpers/utils.js"), "./helpers/utils.js");
}

#[test]
fn dot_folder_target_still_gets_the_dot_slash_prefix() {
    assert_eq!(
        rel("index.js", ".humanify/_bundle.js"),
        "./.humanify/_bundle.js"
    );
    assert_eq!(
        rel("src/a/b.js", ".humanify/_bundle.js"),
        "../../.humanify/_bundle.js"
    );
}
