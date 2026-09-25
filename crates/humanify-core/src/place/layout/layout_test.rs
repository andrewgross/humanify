//! The TS spec (src/split/layout.test.ts), case for case.

use super::{
    HUMANIFIED_SOURCE_PATH, LEGACY_SPLIT_LEDGER_FILENAME, SPLIT_LEDGER_PATH,
    find_split_ledger_path, split_tree_root_of,
};
use std::path::{Path, PathBuf};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("humanify-layout-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        TempDir(dir)
    }
    fn write(&self, rel: &str) -> PathBuf {
        let abs = self.0.join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).expect("mkdir");
        std::fs::write(&abs, "{}").expect("write");
        abs
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn finds_the_ledger_beside_a_prior_humanified_js() {
    let d = TempDir::new("sibling");
    let prior = d.write(HUMANIFIED_SOURCE_PATH);
    let ledger = d.write(SPLIT_LEDGER_PATH);
    assert_eq!(find_split_ledger_path(&prior), Some(ledger));
}

#[test]
fn finds_the_ledger_under_humanify_when_the_prior_is_at_the_root() {
    let d = TempDir::new("root");
    let prior = d.write("output.js");
    let ledger = d.write(SPLIT_LEDGER_PATH);
    assert_eq!(find_split_ledger_path(&prior), Some(ledger));
}

#[test]
fn falls_back_to_the_legacy_flat_ledger() {
    let d = TempDir::new("legacy");
    let prior = d.write("output.js");
    let legacy = d.write(LEGACY_SPLIT_LEDGER_FILENAME);
    assert_eq!(find_split_ledger_path(&prior), Some(legacy));
}

#[test]
fn prefers_the_sibling_ledger() {
    let d = TempDir::new("prefer");
    let prior = d.write(HUMANIFIED_SOURCE_PATH);
    let sibling = d.write(".humanify/split-ledger.json");
    d.write(LEGACY_SPLIT_LEDGER_FILENAME);
    assert_eq!(find_split_ledger_path(&prior), Some(sibling));
}

#[test]
fn none_when_no_ledger_exists() {
    let d = TempDir::new("none");
    let prior = d.write("output.js");
    assert_eq!(find_split_ledger_path(&prior), None);
}

#[test]
fn the_tree_root_steps_out_of_the_metadata_folder() {
    assert_eq!(
        split_tree_root_of(Path::new("/t/.humanify/humanified.js")),
        PathBuf::from("/t")
    );
    assert_eq!(
        split_tree_root_of(Path::new("/t/output.js")),
        PathBuf::from("/t")
    );
}
