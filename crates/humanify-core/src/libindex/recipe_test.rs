//! Index slices are build recipes: spelled, detected from the bundle, and
//! chosen once.

use humanify_model::detection::{BundlerType, MinifierType};

use super::recipe::{
    DetectedRecipe, Format, Recipe, RecipeBundler, Target, choose_slice, detect_recipe,
    terser_slices,
};

#[test]
fn a_recipe_is_spelled_with_every_setting_and_reads_back() {
    let r = Recipe::parse("bun:target=bun,format=cjs,minify").expect("parses");
    assert_eq!(
        r,
        Recipe {
            bundler: RecipeBundler::Bun,
            target: Target::Bun,
            format: Format::Cjs,
            minify: true,
            production: false,
            static_entry: false
        }
    );
    assert_eq!(r.spelling(), "bun:target=bun,format=cjs,minify");
    assert_eq!(r.slice_name(), "bun-bun-cjs-min");
    let e = Recipe::parse("esbuild:target=node,format=esm").expect("parses");
    assert_eq!(e.slice_name(), "esbuild-node-esm");
    assert_eq!(Recipe::parse(&e.spelling()), Ok(e));
}

#[test]
fn a_production_recipe_reads_back_and_keeps_its_slice() {
    // exp135: `production` = process.env.NODE_ENV defined as "production",
    // as app bundles are built; the SAME slice (a bundle reads it the same
    // way), a different recipe (its shards are new)
    let r = Recipe::parse("bun:target=node,format=cjs,minify,production").expect("parses");
    assert!(r.production && r.minify);
    assert_eq!(r.spelling(), "bun:target=node,format=cjs,minify,production");
    assert_eq!(r.slice_name(), "bun-node-cjs-min");
    assert_eq!(Recipe::parse(&r.spelling()), Ok(r));
    let e = Recipe::parse("esbuild:target=node,format=cjs,minify,production").expect("parses");
    assert_eq!(e.slice_name(), "esbuild-node-cjs-min");
    assert!(
        !Recipe::parse("bun:target=node,format=cjs,minify")
            .unwrap()
            .production
    );
}

#[test]
fn a_static_entry_recipe_reads_back_and_keeps_its_slice() {
    // exp136: `static` = a third app entry, a static `import * as` of every
    // subpath, as an app's own ESM source imports a package (the eager,
    // scope-hoisted form neither `require()` nor `import()` produces); the
    // SAME slice, a different recipe (its shards are new); every bundler
    let r = Recipe::parse("bun:target=node,format=cjs,minify,production,static").expect("parses");
    assert!(r.static_entry && r.production);
    assert_eq!(
        r.spelling(),
        "bun:target=node,format=cjs,minify,production,static"
    );
    assert_eq!(r.slice_name(), "bun-node-cjs-min");
    assert_eq!(Recipe::parse(&r.spelling()), Ok(r));
    for b in ["esbuild", "webpack", "rollup"] {
        let e =
            Recipe::parse(&format!("{b}:target=node,format=cjs,minify,static")).expect("parses");
        assert!(e.static_entry, "{b}");
        assert_eq!(Recipe::parse(&e.spelling()), Ok(e));
    }
    assert!(
        !Recipe::parse("bun:target=node,format=cjs,minify,production")
            .unwrap()
            .static_entry
    );
    // it adds an entry, it does not change how one is built: the build
    // stamp's spelling is the recipe's without it (req/dyn builds reused)
    assert_eq!(
        r.build_spelling(),
        "bun:target=node,format=cjs,minify,production"
    );
    let plain = Recipe::parse("esbuild:target=node,format=cjs,minify").unwrap();
    assert_eq!(plain.build_spelling(), plain.spelling());
}

#[test]
fn production_is_refused_where_the_recipe_script_decides_it() {
    // webpack's script always defines it (production mode), rollup's never:
    // a flag the command line cannot honour is refused, not ignored
    for r in [
        "webpack:target=node,format=cjs,minify,production",
        "rollup:target=node,format=cjs,minify,production",
    ] {
        assert!(Recipe::parse(r).is_err(), "{r}");
    }
}

#[test]
fn a_recipe_with_a_missing_or_unknown_setting_is_refused() {
    for bad in [
        "bun",
        "bun:format=cjs,minify",
        "bun:target=bun,minify",
        "bun:target=deno,format=cjs",
        "bun:target=bun,format=cjs,sourcemap",
        "parcel:target=node,format=cjs",
        "esbuild:target=bun,format=cjs",
        "webpack:target=bun,format=cjs",
        "rollup:target=bun,format=cjs",
    ] {
        assert!(Recipe::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn the_bun_banner_says_target_bun_and_its_format() {
    let cc = "// @bun @bytecode @bun-cjs\n(function(exports, require, module) {})";
    let d = detect_recipe(cc, BundlerType::Bun, MinifierType::Bun);
    assert_eq!(
        (d.bundler, d.target, d.format, d.minified),
        (
            Some(RecipeBundler::Bun),
            Some(Target::Bun),
            Some(Format::Cjs),
            true
        )
    );
    let esm = detect_recipe("// @bun\nvar a=1;", BundlerType::Bun, MinifierType::None);
    assert_eq!(
        (esm.target, esm.format, esm.minified),
        (Some(Target::Bun), Some(Format::Esm), false)
    );
    // --target node writes no banner; a later `// @bun` is not one
    let node = detect_recipe("var a=1;\n// @bun\n", BundlerType::Bun, MinifierType::Bun);
    assert_eq!((node.target, node.format), (Some(Target::Node), None));
    // esbuild records no platform
    let esb = detect_recipe("var a=1;", BundlerType::Esbuild, MinifierType::Esbuild);
    assert_eq!(
        (esb.bundler, esb.target),
        (Some(RecipeBundler::Esbuild), None)
    );
}

fn slices() -> Vec<(Recipe, bool)> {
    [
        "bun:target=bun,format=cjs,minify",
        "bun:target=node,format=cjs,minify",
        "esbuild:target=node,format=cjs,minify",
    ]
    .iter()
    .enumerate()
    .map(|(i, s)| (Recipe::parse(s).unwrap(), i != 0))
    .collect()
}

fn detected(bundler: Option<RecipeBundler>, target: Option<Target>) -> DetectedRecipe {
    DetectedRecipe {
        bundler,
        target,
        target_reason: "test",
        format: None,
        minified: true,
    }
}

#[test]
fn the_slice_is_chosen_by_bundler_and_target() {
    let s = slices();
    let pick = |b, t| choose_slice(&s, &detected(b, t)).map(|(r, _)| r.slice_name());
    assert_eq!(
        pick(Some(RecipeBundler::Bun), Some(Target::Bun)),
        Ok("bun-bun-cjs-min".into())
    );
    assert_eq!(
        pick(Some(RecipeBundler::Bun), Some(Target::Node)),
        Ok("bun-node-cjs-min".into())
    );
    // an unrecorded target takes the bundler's default slice
    assert_eq!(
        pick(Some(RecipeBundler::Esbuild), None),
        Ok("esbuild-node-cjs-min".into())
    );
}

#[test]
fn a_recipe_the_index_does_not_hold_is_an_error_naming_what_it_has() {
    let only_node = vec![(
        Recipe::parse("bun:target=node,format=cjs,minify").unwrap(),
        true,
    )];
    let err = choose_slice(
        &only_node,
        &detected(Some(RecipeBundler::Bun), Some(Target::Bun)),
    )
    .expect_err("no target-bun slice");
    assert!(
        err.contains("bun:target=bun") && err.contains("has: bun:target=node,format=cjs,minify"),
        "{err}"
    );
    assert!(
        choose_slice(&only_node, &detected(None, None)).is_err(),
        "an unknown bundler has no slice"
    );
}

#[test]
fn webpack_and_rollup_are_recipe_bundlers_with_node_slices() {
    for (text, slice, bundler, kind) in [
        (
            "webpack:target=node,format=cjs,minify",
            "webpack-node-cjs-min",
            RecipeBundler::Webpack,
            BundlerType::Webpack,
        ),
        (
            "rollup:target=node,format=cjs,minify",
            "rollup-node-cjs-min",
            RecipeBundler::Rollup,
            BundlerType::Rollup,
        ),
    ] {
        let r = Recipe::parse(text).expect("parses");
        assert_eq!((r.bundler, r.slice_name()), (bundler, slice.to_string()));
        assert_eq!(r.bundler.bundler_type(), kind);
        assert_eq!(RecipeBundler::for_bundler(kind), Some(bundler));
        // neither bundle records its platform: the bundler's default slice
        let d = detect_recipe("var a=1;", kind, MinifierType::Terser);
        assert_eq!((d.bundler, d.target), (Some(bundler), None));
        let s = vec![
            (
                Recipe::parse("esbuild:target=node,format=cjs,minify").unwrap(),
                true,
            ),
            (r, true),
        ];
        assert_eq!(
            choose_slice(&s, &d).map(|(r, _)| r.slice_name()),
            Ok(slice.to_string())
        );
    }
}

/// exp127: the minifier each recipe bundler's slice is built with, and the
/// terser-built slices a run may read beside the one it chose.
#[test]
fn the_terser_built_slices_are_every_minified_webpack_or_rollup_slice_but_the_chosen() {
    assert_eq!(RecipeBundler::Bun.minifier(), MinifierType::Bun);
    assert_eq!(RecipeBundler::Esbuild.minifier(), MinifierType::Esbuild);
    assert_eq!(RecipeBundler::Webpack.minifier(), MinifierType::Terser);
    assert_eq!(RecipeBundler::Rollup.minifier(), MinifierType::Terser);
    let s: Vec<(Recipe, bool)> = [
        "bun:target=bun,format=cjs,minify",
        "esbuild:target=node,format=cjs,minify",
        "rollup:target=node,format=cjs,minify",
        "webpack:target=node,format=cjs,minify",
        "rollup:target=node,format=esm",
    ]
    .iter()
    .map(|t| (Recipe::parse(t).unwrap(), true))
    .collect();
    let names = |chosen: &str| -> Vec<String> {
        let c = Recipe::parse(chosen).unwrap();
        terser_slices(&s, &c)
            .iter()
            .map(|r| r.slice_name())
            .collect()
    };
    assert_eq!(
        names("esbuild:target=node,format=cjs,minify"),
        ["rollup-node-cjs-min", "webpack-node-cjs-min"],
        "manifest order; an unminified slice is no terser build"
    );
    assert_eq!(
        names("bun:target=bun,format=cjs,minify"),
        ["rollup-node-cjs-min", "webpack-node-cjs-min"]
    );
    assert_eq!(
        names("rollup:target=node,format=cjs,minify"),
        ["webpack-node-cjs-min"],
        "the chosen slice is never its own companion"
    );
}
