//! The runnable scaffold against the real TS's outputs
//! (test/parity/wp54-scaffold.json, written by
//! test/parity/wp54-scaffold-probe.ts), plus the ported unit cases of
//! src/split/runnable-scaffold.test.ts.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::{
    external_packages_from, package_json_source, readme_source, resolve_external_versions,
    runner_source, write_runnable_scaffold,
};

fn fixture() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-scaffold.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn scratch(tag: &str) -> Scratch {
    let p = std::env::temp_dir().join(format!("humanify-wp54-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

/// The probe's scratch tree: node_modules at the root, lookups two down.
fn installed_tree(fx: &Value, tag: &str) -> (Scratch, PathBuf) {
    let root = scratch(tag);
    for pair in fx["installed"].as_array().unwrap() {
        let (name, version) = (pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
        let dir = root.0.join("node_modules").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("package.json"),
            format!("{{\"version\":\"{version}\"}}"),
        )
        .unwrap();
    }
    let deep = root.0.join("a").join("b");
    fs::create_dir_all(&deep).unwrap();
    (root, deep)
}

#[test]
fn external_packages_match_the_ts() {
    let fx = fixture();
    let texts = strings(&fx["specifierTexts"]);
    assert_eq!(
        external_packages_from(texts.iter().map(String::as_str)),
        strings(&fx["externals"])
    );
}

#[test]
fn resolved_versions_match_the_ts() {
    let fx = fixture();
    let (_root, deep) = installed_tree(&fx, "resolve");
    let got = resolve_external_versions(
        &["@scope/pkg".into(), "ajv".into(), "ws".into()],
        Some(&deep),
    );
    let want: Vec<(String, String)> = fx["resolved"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    let mut got_sorted = got.clone();
    got_sorted.sort();
    assert_eq!(got_sorted, want);
    // Without a directory everything is "*".
    assert!(
        resolve_external_versions(&["ajv".into()], None)
            .iter()
            .all(|(_, v)| v == "*")
    );
}

#[test]
fn the_walk_up_starts_from_the_resolved_directory() {
    // `path.resolve` normalizes `..` LEXICALLY before the walk: from
    // `<root>/x/../a/b` the walk visits a/b, a, root — never `x`. The
    // oracle runs pass `$REPO/../claude-code-versions/...`, and a raw
    // parent walk from there visits `$REPO` (which HAS node_modules).
    let fx = fixture();
    let (root, _deep) = installed_tree(&fx, "dotdot");
    let decoy = root.0.join("x");
    fs::create_dir_all(decoy.join("node_modules").join("ws")).unwrap();
    fs::write(
        decoy.join("node_modules").join("ws").join("package.json"),
        "{\"version\":\"9.9.9\"}",
    )
    .unwrap();
    let from = root.0.join("x").join("..").join("a").join("b");
    assert_eq!(
        resolve_external_versions(&["ws".into(), "ajv".into()], Some(&from)),
        vec![
            ("ws".to_string(), "*".to_string()),
            ("ajv".to_string(), "8.1.0".to_string())
        ]
    );
}

#[test]
fn scaffold_files_are_the_ts_bytes() {
    let fx = fixture();
    let (_root, deep) = installed_tree(&fx, "scaffold");
    for case in fx["scaffolds"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let entry = case["entry"].as_str().unwrap();
        let externals = strings(&case["externals"]);
        let from = case["resolve"].as_bool().unwrap().then_some(deep.as_path());
        let out = scratch(&format!("out-{name}"));
        write_runnable_scaffold(&out.0, entry, &externals, from).unwrap();
        let read = |f: &str| fs::read_to_string(out.0.join(f)).unwrap();
        assert_eq!(
            read("package.json"),
            case["packageJson"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(
            read("RUNNABLE.md"),
            case["readme"].as_str().unwrap(),
            "{name}"
        );
        assert_eq!(read("run.cjs"), case["runner"].as_str().unwrap(), "{name}");
    }
}

#[test]
fn pure_sources_compose() {
    let deps = vec![("ws".to_string(), "*".to_string())];
    assert!(package_json_source(&deps).ends_with("\"ws\": \"*\"\n  }\n}\n"));
    assert!(readme_source("index.js", &deps).contains("best-effort"));
    assert!(runner_source("x/\"y\".js").contains("path.join(__dirname, \"x/\\\"y\\\".js\")"));
}
