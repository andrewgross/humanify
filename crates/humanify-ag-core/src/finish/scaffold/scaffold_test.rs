//! The runnable scaffold against the real TS's outputs
//! (test/parity/wp54-scaffold.json, written by
//! test/parity/wp54-scaffold-probe.ts), plus the ported unit cases of
//! src/split/runnable-scaffold.test.ts.
// io: test code reads fixtures and writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use humanify_ag_testkit::ScratchDir;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::output_tree::disk_test::Disk;

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

/// The probe's scratch tree: node_modules at the root, lookups two down.
fn installed_tree(fx: &Value, tag: &str) -> (ScratchDir, PathBuf) {
    let root = ScratchDir::new(tag);
    for pair in fx["installed"].as_array().unwrap() {
        let (name, version) = (pair[0].as_str().unwrap(), pair[1].as_str().unwrap());
        let dir = root.path().join("node_modules").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("package.json"),
            format!("{{\"version\":\"{version}\"}}"),
        )
        .unwrap();
    }
    let deep = root.path().join("a").join("b");
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
        Path::new("/"),
        &Disk,
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
        resolve_external_versions(&["ajv".into()], None, Path::new("/"), &Disk)
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
    let decoy = root.path().join("x");
    fs::create_dir_all(decoy.join("node_modules").join("ws")).unwrap();
    fs::write(
        decoy.join("node_modules").join("ws").join("package.json"),
        "{\"version\":\"9.9.9\"}",
    )
    .unwrap();
    let from = root.path().join("x").join("..").join("a").join("b");
    assert_eq!(
        resolve_external_versions(
            &["ws".into(), "ajv".into()],
            Some(&from),
            Path::new("/"),
            &Disk
        ),
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
        let out = ScratchDir::new(&format!("out-{name}"));
        let mut tree = crate::output_tree::disk_test::tree_on_disk(out.path());
        write_runnable_scaffold(&mut tree, entry, &externals, from, Path::new("/"));
        crate::output_tree::disk_test::write(&tree);
        let read = |f: &str| fs::read_to_string(out.path().join(f)).unwrap();
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

/// The in-tree package.json and run.cjs, byte for byte: indent 2, a
/// trailing newline, dependencies in JS record order (an array-index name
/// first), non-ASCII written raw, control characters escaped.
#[test]
fn package_json_and_runner_bytes_are_pinned() {
    let deps: Vec<(String, String)> = [("ws", "*"), ("@a/b", "^1.2.3"), ("10", "1"), ("é", "~2")]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    assert_eq!(
        package_json_source(&deps),
        "{\n  \"name\": \"humanified-runnable\",\n  \"private\": true,\n  \"description\": \
         \"Unpacked, humanified, split bundle — runnable under Node.\",\n  \"scripts\": {\n    \
         \"start\": \"node run.cjs\"\n  },\n  \"dependencies\": {\n    \"10\": \"1\",\n    \
         \"ws\": \"*\",\n    \"@a/b\": \"^1.2.3\",\n    \"é\": \"~2\"\n  }\n}\n"
    );
    assert!(package_json_source(&[]).ends_with("\"dependencies\": {}\n}\n"));
    assert!(runner_source("a\u{1}é\\b.js").contains("\"a\\u0001é\\\\b.js\""));
}
