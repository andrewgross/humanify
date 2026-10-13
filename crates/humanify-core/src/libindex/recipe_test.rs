//! Index slices are build recipes: spelled, detected from the bundle, and
//! chosen once.

use humanify_model::detection::{BundlerType, MinifierType};

use super::recipe::{
    DetectedRecipe, Format, Recipe, RecipeBundler, Target, choose_slice, detect_recipe,
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
            minify: true
        }
    );
    assert_eq!(r.spelling(), "bun:target=bun,format=cjs,minify");
    assert_eq!(r.slice_name(), "bun-bun-cjs-min");
    let e = Recipe::parse("esbuild:target=node,format=esm").expect("parses");
    assert_eq!(e.slice_name(), "esbuild-node-esm");
    assert_eq!(Recipe::parse(&e.spelling()), Ok(e));
}

#[test]
fn a_recipe_with_a_missing_or_unknown_setting_is_refused() {
    for bad in [
        "bun",
        "bun:format=cjs,minify",
        "bun:target=bun,minify",
        "bun:target=deno,format=cjs",
        "bun:target=bun,format=cjs,sourcemap",
        "rollup:target=node,format=cjs",
        "esbuild:target=bun,format=cjs",
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
