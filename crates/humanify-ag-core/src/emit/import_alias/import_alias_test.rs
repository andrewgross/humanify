use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use super::{
    ImportScope, PriorImportAliases, build_import_aliases, parse_require_aliases,
    read_prior_import_aliases,
};

fn files(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|s| s.to_string()).collect()
}

fn scope(imports: &[(usize, &[usize])], names: &[(usize, &[&str])]) -> ImportScope {
    ImportScope {
        always_taken: ["exports", "require", "module"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        names_by_file: names
            .iter()
            .map(|(f, ns)| (*f, ns.iter().map(|s| s.to_string()).collect::<HashSet<_>>()))
            .collect(),
        imports: imports
            .iter()
            .map(|(i, ms)| (*i, ms.iter().copied().collect::<BTreeSet<_>>()))
            .collect(),
    }
}

fn alias(got: &BTreeMap<usize, BTreeMap<usize, String>>, importer: usize, module: usize) -> &str {
    got[&importer][&module].as_str()
}

#[test]
fn both_require_forms_are_read_and_resolved_against_the_importer() {
    let text = "Object.defineProperty(module.exports, \"x\", { get: () => x, enumerable: true, configurable: true });\n\
const __bundle = require(\"../.humanify/_bundle.js\");\n\
const config = require(\"./config.js\");\n\
const netConfig = require(\"../net/config.js\");\n\
const lazyTools = new Proxy({}, { get: (_, k) => require(\"../tools/lazy-tools.js\")[k], set: (_, k, v) => { require(\"../tools/lazy-tools.js\")[k] = v; return true; } });\n\
const fs = require(\"fs\");\n\
\n\
const notHeader = require(\"./x.js\") + 1;\n";
    assert_eq!(
        parse_require_aliases("src/auth/login.js", text),
        vec![
            (
                "src/.humanify/_bundle.js".to_string(),
                "__bundle".to_string()
            ),
            ("src/auth/config.js".to_string(), "config".to_string()),
            ("src/net/config.js".to_string(), "netConfig".to_string()),
            (
                "src/tools/lazy-tools.js".to_string(),
                "lazyTools".to_string()
            ),
        ]
    );
}

#[test]
fn the_prior_is_read_per_importer_from_each_files_own_lines() {
    let tree: HashMap<&str, &str> = [
        ("src/a.js", "const srcFlags = require(\"./flags.js\");\n"),
        ("src/b.js", "const flags = require(\"./flags.js\");\n"),
    ]
    .into_iter()
    .collect();
    let got = read_prior_import_aliases(&files(&["src/a.js", "src/b.js", "src/c.js"]), |f| {
        tree.get(f).map(|s| s.to_string())
    });
    assert_eq!(got["src/a.js"]["src/flags.js"], "srcFlags");
    assert_eq!(got["src/b.js"]["src/flags.js"], "flags");
    assert!(!got.contains_key("src/c.js"));
}

#[test]
fn a_name_in_one_importer_widens_only_that_importer() {
    // 0 = src/validatePathVal.js; 1, 2, 3 import it; 3 uses the name.
    let fs = files(&[
        "src/validatePathVal.js",
        "src/a.js",
        "src/b.js",
        "src/rule.js",
    ]);
    let s = scope(
        &[(1, &[0]), (2, &[0]), (3, &[0])],
        &[(3, &["validatePathVal"])],
    );
    let got = build_import_aliases(&fs, &s, None).unwrap();
    assert_eq!(alias(&got, 1, 0), "validatePathVal");
    assert_eq!(alias(&got, 2, 0), "validatePathVal");
    assert_eq!(alias(&got, 3, 0), "srcValidatePathVal");
}

#[test]
fn a_basename_contest_is_settled_per_importer() {
    // Two config.js files: only the importer that requires BOTH widens.
    let fs = files(&[
        "src/auth/config.js",
        "src/net/config.js",
        "src/x.js",
        "src/y.js",
    ]);
    let s = scope(&[(2, &[0, 1]), (3, &[1])], &[]);
    let got = build_import_aliases(&fs, &s, None).unwrap();
    assert_eq!(alias(&got, 2, 0), "authConfig");
    assert_eq!(alias(&got, 2, 1), "netConfig");
    assert_eq!(alias(&got, 3, 1), "config");
}

#[test]
fn a_prior_alias_is_carried_per_importer() {
    let fs = files(&["src/flags.js", "src/a.js", "src/b.js"]);
    let s = scope(&[(1, &[0]), (2, &[0])], &[]);
    let mut prior = PriorImportAliases::new();
    prior.insert(
        "src/a.js".into(),
        [("src/flags.js".to_string(), "srcFlags".to_string())]
            .into_iter()
            .collect(),
    );
    let got = build_import_aliases(&fs, &s, Some(&prior)).unwrap();
    assert_eq!(
        alias(&got, 1, 0),
        "srcFlags",
        "a.js keeps its own prior alias"
    );
    assert_eq!(alias(&got, 2, 0), "flags", "b.js had none: the ladder");
}

#[test]
fn a_prior_alias_now_shadowed_or_contested_falls_to_the_ladder() {
    let fs = files(&["src/a/flags.js", "src/b/flags.js", "src/x.js", "src/y.js"]);
    let s = scope(&[(2, &[0, 1]), (3, &[0])], &[(3, &["flags"])]);
    let mut prior = PriorImportAliases::new();
    prior.insert(
        "src/x.js".into(),
        [
            ("src/a/flags.js".to_string(), "flags".to_string()),
            ("src/b/flags.js".to_string(), "flags".to_string()),
        ]
        .into_iter()
        .collect(),
    );
    prior.insert(
        "src/y.js".into(),
        [("src/a/flags.js".to_string(), "flags".to_string())]
            .into_iter()
            .collect(),
    );
    let got = build_import_aliases(&fs, &s, Some(&prior)).unwrap();
    assert_eq!(alias(&got, 2, 0), "aFlags");
    assert_eq!(alias(&got, 2, 1), "bFlags");
    assert_eq!(alias(&got, 3, 0), "aFlags", "y.js now uses the name itself");
}
