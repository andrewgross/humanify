//! The names-mode module-answer reader's fixtures (library-names plan
//! step 4, exp156): the `modules.<slice>.jsonl` rows the index writes,
//! keyed by whole-module vendor content key, become the planner's
//! per-module answers.

use std::collections::{BTreeMap, BTreeSet};

use super::files::MemoryIndexFiles;
use super::module_answers::read_module_answers;

/// An index holding only `slice`'s module rows.
fn index_with_rows(slice: &str, rows: &[String]) -> MemoryIndexFiles {
    MemoryIndexFiles(BTreeMap::from([(
        format!("modules.{slice}.jsonl"),
        (rows.join("\n") + "\n").into_bytes(),
    )]))
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
    let d = index_with_rows(
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
                owners: BTreeMap::from([("ms".to_string(), vec!["index.js".to_string()])]),
            }
        )]),
        "only the keys asked for are read back, with their files"
    );
}

#[test]
fn a_key_two_packages_hold_answers_every_owner_with_its_own_files() {
    let d = index_with_rows(
        "s",
        &[
            row("k1", "aa", &["lib/a.js"]),
            row("k1", "bb", &["dist/b.js"]),
        ],
    );
    let out = read_module_answers(&d, "s", &keys(&["k1"])).unwrap();
    assert_eq!(
        out["k1"].owners,
        BTreeMap::from([
            ("aa".to_string(), vec!["lib/a.js".to_string()]),
            ("bb".to_string(), vec!["dist/b.js".to_string()]),
        ]),
        "a key held by two packages keeps each owner's files (exp211): \
         whether the claim is one of them is the planner's question"
    );
}

#[test]
fn a_multi_file_row_answers_all_its_files_sorted() {
    let d = index_with_rows("s", &[row("k1", "ms", &["b.js", "a.js"])]);
    let out = read_module_answers(&d, "s", &keys(&["k1"])).unwrap();
    assert_eq!(
        out["k1"].owners["ms"],
        vec!["a.js".to_string(), "b.js".to_string()]
    );
}

#[test]
fn a_key_the_index_does_not_hold_is_simply_absent() {
    let d = index_with_rows("s", &[row("k1", "ms", &["index.js"])]);
    let out = read_module_answers(&d, "s", &keys(&["k1", "nope"])).unwrap();
    assert!(out.contains_key("k1"));
    assert!(!out.contains_key("nope"));
}

#[test]
fn a_missing_modules_file_refuses_loudly() {
    let d = MemoryIndexFiles::default();
    let err = read_module_answers(&d, "s", &keys(&["k1"])).unwrap_err();
    assert!(
        err.to_string().contains("modules.s.jsonl"),
        "the refusal names the file it wanted: {err}"
    );
}
