//! The names-mode module-answer reader's fixtures (library-names plan
//! step 4, exp156): the `modules.<slice>.jsonl` rows the index writes,
//! keyed by whole-module vendor content key, become the planner's
//! per-module answers.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::module_answers::read_module_answers;

fn dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("humanify-module-answers-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_rows(dir: &Path, slice: &str, rows: &[String]) {
    let file = dir.join(format!("modules.{slice}.jsonl"));
    std::fs::write(file, rows.join("\n") + "\n").unwrap();
}

fn row(key: &str, owner: &str, files: &[&str]) -> String {
    format!(
        "{{\"key\":\"{key}\",\"owner\":\"{owner}\",\"files\":{},\"versions\":[]}}",
        serde_json::json!(files)
    )
}

fn keys(k: &[&str]) -> BTreeSet<String> {
    k.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_single_owner_row_answers_its_files() {
    let d = dir("single");
    write_rows(
        &d,
        "bun-node-cjs-min",
        &[
            row("k1", "ms", &["index.js"]),
            row("unwanted", "core-js", &["index.js"]),
        ],
    );
    let out = read_module_answers(&d, "bun-node-cjs-min", &keys(&["k1"])).unwrap();
    assert_eq!(
        out,
        BTreeMap::from([(
            "k1".to_string(),
            crate::place::library_layout::ModuleAnswer {
                owners: vec!["ms".to_string()],
                files: vec!["index.js".to_string()],
            }
        )]),
        "only the keys asked for are read back, with their files"
    );
}

#[test]
fn a_key_two_packages_hold_answers_foreign() {
    let d = dir("collide");
    write_rows(
        &d,
        "s",
        &[
            row("k1", "aa", &["index.js"]),
            row("k1", "bb", &["index.js"]),
        ],
    );
    let out = read_module_answers(&d, "s", &keys(&["k1"])).unwrap();
    let answer = &out["k1"];
    assert_eq!(answer.owners, vec!["aa".to_string(), "bb".to_string()]);
    assert!(
        answer.files.is_empty(),
        "a key held by two packages is the planner's disagreement case — \
         no file is offered"
    );
}

#[test]
fn a_multi_file_row_answers_all_its_files_sorted() {
    let d = dir("multifile");
    write_rows(&d, "s", &[row("k1", "ms", &["b.js", "a.js"])]);
    let out = read_module_answers(&d, "s", &keys(&["k1"])).unwrap();
    assert_eq!(
        out["k1"].files,
        vec!["a.js".to_string(), "b.js".to_string()]
    );
}

#[test]
fn a_key_the_index_does_not_hold_is_simply_absent() {
    let d = dir("absent");
    write_rows(&d, "s", &[row("k1", "ms", &["index.js"])]);
    let out = read_module_answers(&d, "s", &keys(&["k1", "nope"])).unwrap();
    assert!(out.contains_key("k1"));
    assert!(!out.contains_key("nope"));
}

#[test]
fn a_missing_modules_file_refuses_loudly() {
    let d = dir("missing");
    std::fs::create_dir_all(&d).unwrap();
    let err = read_module_answers(&d, "s", &keys(&["k1"])).unwrap_err();
    assert!(
        err.contains("modules.s.jsonl"),
        "the refusal names the file it wanted: {err}"
    );
}
