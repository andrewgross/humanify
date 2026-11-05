//! The bundler tools' tests (exp181): every recipe bundler's tool lives
//! in the store (`<root>/tools/<bundler>-<version>/`), found there by one
//! rule, and a recipe resolves to the build that runs it.
//!
//! WHAT IS STUBBED: the installs. A stub "installed" tool is the files
//! [`find_tool`] checks for (a `node_modules/.bin/<bin>` script, or the
//! bundler's package.json beside its recipe script); nothing is fetched or
//! run. Fetching is proven by the lane's real builds (exp181 README).

use std::path::{Path, PathBuf};

use humanify_ag_core::libindex::recipe::{Recipe, RecipeBundler};

use crate::cache::TOOLS_DIR;
use crate::tools::{
    ALL_RECIPE_BUNDLERS, TOOLS_BEFORE, fetch_specs, find_tool, pin, recipe_build, recipe_script,
    tool_dir,
};

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("humanify-ix-tools-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// Lay down what an install of `bundler` at `version` leaves under the
/// store, as far as [`find_tool`] looks.
fn stub_install(root: &Path, bundler: RecipeBundler, version: &str) -> PathBuf {
    let dir = tool_dir(root, bundler, version);
    match recipe_script(bundler) {
        None => {
            let bin = dir.join("node_modules/.bin");
            std::fs::create_dir_all(&bin).expect("mkdir");
            std::fs::write(bin.join(bundler.name()), "#!/bin/sh\necho stub\n").expect("stub");
        }
        Some(_) => {
            let pkg = dir.join("node_modules").join(bundler.name());
            std::fs::create_dir_all(&pkg).expect("mkdir");
            std::fs::write(pkg.join("package.json"), "{}").expect("stub");
        }
    }
    dir
}

/// Every recipe bundler has ONE pinned tool, and it lives in the store:
/// `<root>/tools/<bundler>-<version>/`. Bun and esbuild run as their
/// `node_modules/.bin` binary; webpack and rollup as the builder's recipe
/// script, written beside their install (the script resolves the bundler
/// from its own directory) with exactly the bytes the builder ships.
#[test]
fn every_recipe_bundler_s_tool_lives_in_the_store() {
    let root = temp("each");
    for bundler in ALL_RECIPE_BUNDLERS {
        let p = pin(bundler);
        assert_eq!(p.bundler, bundler);
        let dir = tool_dir(&root, bundler, p.version);
        assert_eq!(
            dir,
            root.join(TOOLS_DIR)
                .join(format!("{}-{}", bundler.name(), p.version))
        );
        assert!(find_tool(&root, bundler).is_err(), "nothing installed yet");
        stub_install(&root, bundler, p.version);
        let tool = find_tool(&root, bundler).expect("installed");
        assert_eq!(tool.version, p.version, "{}", bundler.name());
        assert!(tool.bin.starts_with(&dir), "{}", tool.bin.display());
        match recipe_script(bundler) {
            None => assert!(
                tool.bin
                    .ends_with(format!("node_modules/.bin/{}", bundler.name()))
            ),
            Some((name, bytes)) => {
                assert_eq!(tool.bin, dir.join(name));
                assert_eq!(std::fs::read_to_string(&tool.bin).expect("written"), bytes);
            }
        }
    }
}

/// A recipe script left in the store by an older binary is rewritten with
/// this binary's: the script IS the recipe (in every shard key through
/// RECIPE_VERSION), so the store never runs a stale one.
#[test]
fn a_stale_recipe_script_in_the_store_is_replaced() {
    let root = temp("stale");
    let dir = stub_install(
        &root,
        RecipeBundler::Webpack,
        pin(RecipeBundler::Webpack).version,
    );
    let (name, bytes) = recipe_script(RecipeBundler::Webpack).expect("webpack has a script");
    std::fs::write(dir.join(name), "// old\n").expect("stale");
    let tool = find_tool(&root, RecipeBundler::Webpack).expect("installed");
    assert_eq!(std::fs::read_to_string(tool.bin).expect("rewritten"), bytes);
}

/// More than one version installed: the NEWEST wins (semver order, not
/// text: 0.25.10 > 0.25.9), and none installed says exactly how to fetch
/// the pinned one by hand, under the store.
#[test]
fn the_newest_installed_tool_wins_and_none_names_the_fetch() {
    let root = temp("newest");
    stub_install(&root, RecipeBundler::Esbuild, "0.25.9");
    stub_install(&root, RecipeBundler::Esbuild, "0.25.10");
    let tool = find_tool(&root, RecipeBundler::Esbuild).expect("installed");
    assert_eq!(tool.version, "0.25.10");
    let bare = temp("none");
    for bundler in ALL_RECIPE_BUNDLERS {
        let err = find_tool(&bare, bundler).expect_err("nothing installed");
        let specs = fetch_specs(bundler).join(" ");
        assert!(err.contains(&specs), "{}: {err}", bundler.name());
        assert!(
            err.contains(&format!("--before={TOOLS_BEFORE}")),
            "dated, as the fetch is: {err}"
        );
        assert!(
            err.contains(&bare.join(TOOLS_DIR).display().to_string()),
            "{err}"
        );
    }
}

/// What each tool's fetch installs: the bundler at its pin, and for rollup
/// the plugins its recipe script loads (each pinned too: a plugin's
/// version changes the build as much as rollup's).
#[test]
fn the_fetch_installs_the_bundler_and_its_recipe_s_plugins_pinned() {
    for bundler in ALL_RECIPE_BUNDLERS {
        let specs = fetch_specs(bundler);
        assert_eq!(
            specs[0],
            format!("{}@{}", bundler.name(), pin(bundler).version)
        );
        assert!(
            specs.iter().all(|s| s[1..].contains('@')),
            "every spec pinned: {specs:?}"
        );
    }
    let rollup = fetch_specs(RecipeBundler::Rollup).join(" ");
    for plugin in [
        "@rollup/plugin-node-resolve@",
        "@rollup/plugin-commonjs@",
        "@rollup/plugin-json@",
        "@rollup/plugin-terser@",
    ] {
        assert!(rollup.contains(plugin), "{rollup}");
    }
}

/// A recipe resolves to the build that runs it with the store's tool.
#[test]
fn a_recipe_resolves_to_the_store_s_tool() {
    let root = temp("recipe");
    for text in [
        "bun:target=bun,format=cjs,minify,production",
        "esbuild:target=node,format=cjs,minify,production",
        "webpack:target=node,format=cjs,minify",
        "rollup:target=node,format=cjs,minify",
    ] {
        let recipe = Recipe::parse(text).expect("a recipe");
        assert!(recipe_build(&root, recipe).is_err(), "{text}: none yet");
        stub_install(&root, recipe.bundler, pin(recipe.bundler).version);
        let (build, tool) = recipe_build(&root, recipe).expect("installed");
        assert_eq!(build.recipe, recipe);
        assert_eq!(build.bin, tool.bin);
    }
}

/// The real fetch, every recipe bundler (network: `#[ignore]`d in the
/// gate; run by hand with `cargo test -p humanify-ag-index
/// fetches_every_tool -- --ignored`). Each tool lands in
/// the store, answers `--version` with its pin, and a second lookup finds
/// it without fetching.
#[test]
#[ignore = "network: fetches every pinned tool from the npm registry"]
fn fetches_every_tool_for_real() {
    let root = temp("fetch");
    for bundler in ALL_RECIPE_BUNDLERS {
        let tool = match find_tool(&root, bundler) {
            Ok(t) => t,
            Err(_) => crate::tools::fetch_tool(&root, bundler).expect("fetched"),
        };
        assert_eq!(tool.version, pin(bundler).version);
        let said = crate::recipe::bundler_version_of(
            bundler,
            &tool.bin,
            std::time::Duration::from_secs(120),
        )
        .expect("it runs");
        assert_eq!(said, pin(bundler).version, "{}", bundler.name());
        assert_eq!(find_tool(&root, bundler).expect("found"), tool);
    }
}
