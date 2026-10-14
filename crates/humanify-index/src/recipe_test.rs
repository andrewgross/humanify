//! The builder's recipe rules: which npm files feed the literal index.

use humanify_core::libindex::recipe::RecipeBundler;

use std::path::{Path, PathBuf};

use humanify_core::libindex::recipe::Recipe;

use super::recipe::{RecipeBuild, bundler_package, is_source_file, is_sponge_file};

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
    assert_eq!(bundler_package(RecipeBundler::Bun), "bun");
    assert_eq!(bundler_package(RecipeBundler::Esbuild), "esbuild");
    assert_eq!(bundler_package(RecipeBundler::Webpack), "webpack");
    assert_eq!(bundler_package(RecipeBundler::Rollup), "rollup");
}

fn args_of(cmd: &std::process::Command) -> Vec<String> {
    std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn webpack_and_rollup_run_as_node_scripts_writing_out_js() {
    for (recipe, script) in [
        (
            "webpack:target=node,format=cjs,minify",
            "/t/webpack-build.cjs",
        ),
        (
            "rollup:target=node,format=cjs,minify",
            "/t/rollup-build.mjs",
        ),
    ] {
        let build = RecipeBuild {
            recipe: Recipe::parse(recipe).unwrap(),
            bin: PathBuf::from(script),
        };
        let cmd = build.command(
            Path::new("/w"),
            "entries/req.cjs",
            Path::new("/w/build/x/req"),
            &["fsevents".to_string()],
            false,
        );
        assert_eq!(
            args_of(&cmd),
            [
                "node",
                script,
                "entries/req.cjs",
                "/w/build/x/req",
                "cjs",
                "fsevents"
            ]
        );
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/w")));
        let esm = build.command(Path::new("/w"), "e.mjs", Path::new("/o"), &[], true);
        assert_eq!(args_of(&esm)[4], "esm");
        assert_eq!(
            args_of(&build.version_command()),
            ["node", script, "--version"]
        );
    }
}

#[test]
fn the_entries_reach_every_subpath_export_a_node_app_can_load() {
    use super::recipe::entry_specifiers;
    // @aws-sdk/nested-clients: no "." export, only subpaths (plus types and
    // the package.json).
    let nested = serde_json::json!({
        "main": "./dist-cjs/index.js",
        "exports": {
            "./package.json": "./package.json",
            "./sts": {"types": "./dist-types/sts.d.ts", "node": "./dist-cjs/sts.js", "import": "./dist-es/sts.js"},
            "./sso": {"require": "./dist-cjs/sso.js"},
            "./browser-only": {"browser": "./dist-es/b.js", "types": "./b.d.ts"},
            "./gone": null,
        }
    });
    let nested_files = files_of(&[
        "dist-cjs/sts.js",
        "dist-es/sts.js",
        "dist-cjs/sso.js",
        "dist-es/b.js",
    ]);
    assert_eq!(
        entry_specifiers("@aws-sdk/nested-clients", &nested, &nested_files),
        ["@aws-sdk/nested-clients/sso", "@aws-sdk/nested-clients/sts"],
        "an exports map without \".\" has no main entry; browser- and types-only subpaths are not Node's"
    );
    // @modelcontextprotocol/sdk 1.9.0: one wildcard.
    let mcp = serde_json::json!({
        "exports": {"./*": {"import": "./dist/esm/*", "require": "./dist/cjs/*"}}
    });
    let files: Vec<String> = [
        "dist/cjs/client/index.js",
        "dist/cjs/client/index.d.ts",
        "dist/cjs/types.js",
        "dist/esm/types.js",
        "package.json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(
        entry_specifiers("@modelcontextprotocol/sdk", &mcp, &files),
        [
            "@modelcontextprotocol/sdk/client/index.js",
            "@modelcontextprotocol/sdk/types.js"
        ]
    );
    // A main export AND subpaths: both.
    let both = serde_json::json!({
        "exports": {".": {"require": "./index.js"}, "./extra": "./extra.js"}
    });
    assert_eq!(
        entry_specifiers("p", &both, &files_of(&["index.js", "extra.js"])),
        ["p", "p/extra"]
    );
    // An export whose file the package does not ship (@modelcontextprotocol/
    // sdk 1.32's "." → dist/cjs/index.js) is not entered: the bundler would
    // retry it as an external, and an external package name takes every
    // subpath with it.
    assert_eq!(
        entry_specifiers("p", &both, &files_of(&["extra.js"])),
        ["p/extra"]
    );
    // No exports map: the package itself.
    let plain = serde_json::json!({"main": "lib/index.js"});
    assert_eq!(entry_specifiers("p", &plain, &[]), ["p"]);
    // A conditions object at the top is the "." export.
    let top = serde_json::json!({"exports": {"node": "./n.js", "default": "./d.js"}});
    assert_eq!(entry_specifiers("p", &top, &[]), ["p"]);
}

fn files_of(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}
