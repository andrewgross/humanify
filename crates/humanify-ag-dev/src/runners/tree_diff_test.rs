use std::path::PathBuf;

use super::tree_diff::*;
use crate::eval::test_support::overcharged_pair;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dev-tree-diff-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn file_and_tree_counts_are_the_minimal_edit() {
    // The walk report's per-directory columns and the self-hop's cold
    // count read these: the same minimal count as the scorer's
    // `eval::difftool` (exp222).
    let d = dir("minimal");
    let (a, b, minimal) = overcharged_pair(16_000);
    for (side, text) in [("x", &a), ("y", &b)] {
        std::fs::create_dir_all(d.join(side).join("src")).unwrap();
        std::fs::write(d.join(side).join("src").join("big.js"), text).unwrap();
    }
    std::fs::write(d.join("y").join("src").join("new.js"), "one\ntwo\n").unwrap();
    let file = |s: &str| d.join(s).join("src").join("big.js");
    assert_eq!(
        changed_lines(&file("x"), &file("y")).unwrap(),
        minimal as u64
    );
    assert_eq!(
        changed_lines_in_trees(&d.join("x"), &d.join("y")).unwrap(),
        minimal as u64 + 2
    );
    assert_eq!(
        changed_lines_in_trees(&d.join("x"), &d.join("x")).unwrap(),
        0
    );
}
