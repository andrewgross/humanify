//! The TS spec (src/split/layout.test.ts), case for case.

use super::{
    HUMANIFIED_SOURCE_PATH, SPLIT_LEDGER_PATH, prior_file_in_tree, prior_ledger_in,
    split_tree_root_of,
};
use crate::prior_tree::PriorFiles;
use std::path::{Path, PathBuf};

/// A prior tree in memory: the shell's read of `/t`.
struct Tree(Vec<&'static str>);

impl Tree {
    fn files(&self) -> PriorFiles {
        PriorFiles::new(
            self.0
                .iter()
                .map(|p| (p.to_string(), b"{}".to_vec()))
                .collect(),
        )
    }
}

const ROOT: &str = "/t";

fn at(rel: &str) -> PathBuf {
    Path::new(ROOT).join(rel)
}

#[test]
fn finds_the_ledger_beside_a_prior_humanified_js() {
    let tree = Tree(vec![HUMANIFIED_SOURCE_PATH, SPLIT_LEDGER_PATH]);
    let prior = at(HUMANIFIED_SOURCE_PATH);
    assert_eq!(
        prior_ledger_in(&prior, &tree.files()).as_deref(),
        Some(SPLIT_LEDGER_PATH)
    );
}

#[test]
fn finds_the_ledger_under_humanify_when_the_prior_is_at_the_root() {
    let tree = Tree(vec!["output.js", SPLIT_LEDGER_PATH]);
    assert_eq!(
        prior_ledger_in(&at("output.js"), &tree.files()).as_deref(),
        Some(SPLIT_LEDGER_PATH)
    );
}

#[test]
fn a_flat_underscore_ledger_is_not_a_prior_ledger() {
    let tree = Tree(vec!["output.js", "_split-ledger.json"]);
    assert_eq!(prior_ledger_in(&at("output.js"), &tree.files()), None);
}

#[test]
fn prefers_the_sibling_ledger() {
    let tree = Tree(vec![
        HUMANIFIED_SOURCE_PATH,
        ".humanify/split-ledger.json",
        ".humanify/.humanify/split-ledger.json",
    ]);
    assert_eq!(
        prior_ledger_in(&at(HUMANIFIED_SOURCE_PATH), &tree.files()).as_deref(),
        Some(".humanify/split-ledger.json")
    );
}

#[test]
fn none_when_no_ledger_exists() {
    let tree = Tree(vec!["output.js"]);
    assert_eq!(prior_ledger_in(&at("output.js"), &tree.files()), None);
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

#[test]
fn the_prior_file_is_found_inside_its_own_tree() {
    assert_eq!(
        prior_file_in_tree(Path::new("/t/.humanify/humanified.js")),
        HUMANIFIED_SOURCE_PATH
    );
    assert_eq!(prior_file_in_tree(Path::new("/t/output.js")), "output.js");
    assert_eq!(prior_file_in_tree(Path::new("output.js")), "output.js");
}
