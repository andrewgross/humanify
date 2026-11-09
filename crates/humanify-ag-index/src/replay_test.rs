//! The replay plan: an existing index's exact package@versions, as-of,
//! recipes and bundler pins, read from its manifest.

use super::replay::replay_plan;
use humanify_ag_core::libindex::recipe::{Recipe, RecipeBundler};

/// A built index's manifest, cut down to what a replay reads (the
/// cb64c1f5… shape: a pinned build, two Bun slices, names stamped).
fn manifest(recipe_version: &str, names: bool) -> String {
    let names = if names { r#","names":"names/1""# } else { "" };
    format!(
        r#"{{"format":"humanify-library-index/4","asOf":"2026-10-25T23:59:59.999Z",
"fingerprints":{{"module":"vendor-content-key/4","literals":"literals/1","units":"units/3","edges":"edges/2"{names}}},
"registry":{{"earliestFetch":null,"latestFetch":null,"packuments":{{}}}},
"slices":[
 {{"slice":"bun-bun-cjs-min","recipe":"bun:target=bun,format=cjs,minify,production","recipeVersion":"{recipe_version}","defaultForBundler":true,"bundler":"bun","version":"1.4.0","published":null,"newestAtAsOf":null,"bundlerAfterAsOf":false}},
 {{"slice":"bun-node-cjs-min","recipe":"bun:target=node,format=cjs,minify,production","recipeVersion":"{recipe_version}","defaultForBundler":false,"bundler":"bun","version":"1.4.0","published":null,"newestAtAsOf":null,"bundlerAfterAsOf":false}}],
"packages":[
 {{"name":"@scope/b","version":"2.0.0","role":"pinned","published":"2020-01-01T00:00:00.000Z"}},
 {{"name":"a","version":"1.0.0","role":"pinned","published":"2020-01-01T00:00:00.000Z"}},
 {{"name":"a","version":"1.1.0","role":"latest","published":"2020-02-01T00:00:00.000Z"}}],
"refused":[{{"name":"a","version":"0.9.0","reason":"unpublished"}}],
"shards":{{}},"tables":{{}},"data":{{}}}}"#
    )
}

#[test]
fn a_replay_pins_exactly_the_indexed_versions_under_the_same_recipes() {
    let r = replay_plan(&manifest(crate::recipe::RECIPE_VERSION, true)).unwrap();
    assert_eq!(r.as_of, "2026-10-25T23:59:59.999Z");
    // every indexed version, scoped names included; never a refused one
    assert_eq!(r.pinned, vec!["@scope/b@2.0.0", "a@1.0.0", "a@1.1.0"]);
    assert_eq!(
        r.recipes,
        vec![
            (
                Recipe::parse("bun:target=bun,format=cjs,minify,production").unwrap(),
                "1.4.0".to_string()
            ),
            (
                Recipe::parse("bun:target=node,format=cjs,minify,production").unwrap(),
                "1.4.0".to_string()
            ),
        ]
    );
    assert_eq!(r.recipes[0].0.bundler, RecipeBundler::Bun);
    assert!(r.names, "a names-stamped index replays with the names pass");
    let plain = replay_plan(&manifest(crate::recipe::RECIPE_VERSION, false)).unwrap();
    assert!(!plain.names);
}

#[test]
fn a_replay_refuses_an_index_built_under_another_recipe_version() {
    // a different recipe version means different bundles: re-fingerprinting
    // the stored builds would not reproduce that index's plan
    let err = replay_plan(&manifest("recipe/1", true)).expect_err("old recipes");
    assert!(err.contains("recipe/1"), "{err}");
}
