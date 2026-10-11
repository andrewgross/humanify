//! The builder's recipe rules: which npm files feed the literal index.

use humanify_core::libindex::IndexSlice;

use super::recipe::{bundler_package, is_source_file, is_sponge_file};

#[test]
fn pre_bundled_files_are_sponges() {
    for f in [
        "dist/vue.global.js",
        "dist/axios.min.js",
        "lib/bundled.js",
        "dist/index.bundle.js",
        "umd/react.umd.js",
        "dist/browser.js",
        "cdn.js",
        "dist/cdn.min.js",
    ] {
        assert!(is_sponge_file(f), "{f}");
    }
    for f in [
        "index.js",
        "lib/parse.js",
        "dist/cjs/index.cjs",
        "src/bundle/index.js",
        "minimist.js",
    ] {
        assert!(!is_sponge_file(f), "{f}");
    }
}

#[test]
fn source_files_are_javascript_and_json_but_not_type_declarations() {
    assert!(is_source_file("index.js"));
    assert!(is_source_file("lib/a.mjs"));
    assert!(is_source_file("lib/a.cjs"));
    assert!(is_source_file("package.json"));
    assert!(!is_source_file("index.d.ts"));
    assert!(!is_source_file("index.ts"));
    assert!(!is_source_file("README.md"));
}

#[test]
fn each_slice_names_its_bundlers_npm_package() {
    assert_eq!(bundler_package(IndexSlice::Bun), "bun");
    assert_eq!(bundler_package(IndexSlice::Esbuild), "esbuild");
}
