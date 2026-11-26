//! The import alias a file's previous copy used comes back when the
//! file's final text has room for it.

use super::restore_aliases_in_file;
use crate::rename::eligibility::{Eligibility, NeverRename};
use crate::rename::name_profile::NameProfile;

fn file(alias: &str, rest: &str) -> String {
    format!(
        "const {alias} = require(\"./plugin-cache.js\");\n\
         function load(key) {{\n  return (0, {alias}.read)(key);\n}}\n{rest}"
    )
}

fn run(text: &str, prior: &str) -> Option<String> {
    let eligible = Eligibility::new(NeverRename::default());
    restore_aliases_in_file("src/a.js", text, prior, &eligible, NameProfile::Bun)
}

#[test]
fn the_previous_alias_comes_back_beside_a_local_that_cannot_see_the_module() {
    let local = "function size(pluginCache) {\n  return pluginCache.length;\n}\n";
    let out = run(&file("srcPluginCache", local), &file("pluginCache", ""));
    assert_eq!(out, Some(file("pluginCache", local)));
}

#[test]
fn a_local_that_can_see_the_module_keeps_the_alias_away() {
    let local =
        "function size(pluginCache) {\n  return (0, srcPluginCache.read)(pluginCache);\n}\n";
    assert_eq!(
        run(&file("srcPluginCache", local), &file("pluginCache", "")),
        None
    );
}

#[test]
fn a_new_top_level_binding_of_that_name_makes_way_for_the_alias() {
    let top = |name: &str| {
        format!("var {name} = new Map();\nfunction has(k) {{\n  return {name}.has(k);\n}}\n")
    };
    let out = run(
        &file("srcPluginCache", &top("pluginCache")),
        &file("pluginCache", ""),
    );
    assert_eq!(out, Some(file("pluginCache", &top("pluginCacheVal"))));
}

#[test]
fn an_exported_binding_of_that_name_keeps_the_alias_away() {
    let top = "var pluginCache = new Map();\n\
        Object.defineProperty(module.exports, \"pluginCache\", { get: () => pluginCache, enumerable: true, configurable: true });\n";
    assert_eq!(
        run(&file("srcPluginCache", top), &file("pluginCache", "")),
        None
    );
}

#[test]
fn an_alias_the_previous_copy_also_used_is_left_alone() {
    assert_eq!(
        run(&file("pluginCache", ""), &file("pluginCache", "")),
        None
    );
}
