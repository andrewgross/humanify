//! The one writer of the output tree: what an earlier run left in the
//! output folder stays (unless this run removed that path), the run's
//! removals go, its files are written with their folders, an embedded
//! file is copied, and a copy that cannot be made fails the write.
// io: test code writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use std::fs;
use std::path::PathBuf;

use humanify_ag_testkit::ScratchDir;

use super::{WriteError, tree_for, write_tree};

#[test]
fn files_an_earlier_run_left_stay_unless_this_run_removed_them() {
    let out = ScratchDir::new("tree-leftovers");
    let root = out.path();
    fs::create_dir_all(root.join("vendor")).unwrap();
    fs::write(root.join("vendor/stale.js"), "stale").unwrap();
    fs::write(root.join("runtime.js"), "old runtime").unwrap();
    fs::write(root.join("index.js"), "old index").unwrap();
    let mut tree = tree_for(root);
    tree.put_rel("index.js", "new index");
    tree.put_rel("src/deep/a.js", "a");
    tree.remove(&root.join("runtime.js"));
    // A removal of a path that is not on disk is fine.
    tree.remove(&root.join("never.js"));
    write_tree(&tree).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("vendor/stale.js")).unwrap(),
        "stale"
    );
    assert_eq!(
        fs::read_to_string(root.join("index.js")).unwrap(),
        "new index"
    );
    assert_eq!(fs::read_to_string(root.join("src/deep/a.js")).unwrap(), "a");
    assert!(!root.join("runtime.js").exists());
}

#[test]
fn the_output_folder_is_made_even_for_an_empty_tree() {
    let out = ScratchDir::new("tree-empty");
    let root = out.path().join("not/yet");
    write_tree(&tree_for(&root)).unwrap();
    assert!(root.is_dir());
}

#[test]
fn the_tree_reads_what_is_on_disk_beneath_it() {
    let out = ScratchDir::new("tree-beneath");
    let root = out.path();
    fs::write(root.join("left.js"), "left").unwrap();
    let tree = tree_for(root);
    assert_eq!(tree.read_lossy(&root.join("left.js")).unwrap(), "left");
    let names: Vec<String> = tree
        .list(root)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["left.js"]);
}

#[test]
fn an_embedded_file_is_copied_and_a_missing_one_fails_the_write() {
    let out = ScratchDir::new("tree-embedded");
    let root = out.path().join("out");
    fs::write(out.path().join("a.bin"), b"\x00\xff").unwrap();
    let mut tree = tree_for(&root);
    tree.copy(root.join("embedded/a.bin"), out.path().join("a.bin"));
    write_tree(&tree).unwrap();
    assert_eq!(fs::read(root.join("embedded/a.bin")).unwrap(), b"\x00\xff");

    let gone = out.path().join("nowhere/gone.bin");
    tree.copy(root.join("embedded/gone.bin"), gone.clone());
    let err = write_tree(&tree).unwrap_err();
    assert!(matches!(err, WriteError::EmbeddedCopy { .. }), "{err}");
    assert!(err.to_string().contains("gone.bin"), "{err}");
}

#[test]
fn an_embedded_copy_error_prints_the_finish_stages_words() {
    let err = WriteError::EmbeddedCopy {
        from: PathBuf::from("in/a.bin"),
        to: PathBuf::from("out/embedded/a.bin"),
        source: std::io::Error::other("gone"),
    };
    assert_eq!(
        err.to_string(),
        "copy the embedded file in/a.bin to out/embedded/a.bin: gone"
    );
}

/// A stage that fails after the tree has content: the content is written
/// (as a run that wrote as it went left it), and the stage's error is the
/// run's error.
#[test]
fn a_failed_build_still_writes_what_it_built_and_returns_its_error() {
    let out = ScratchDir::new("tree-failed-build");
    let root = out.path().join("out");
    let got: Result<(), String> = super::build_then_write(tree_for(&root), |tree| {
        tree.put_rel("vendor/a.js", "a");
        Err("the split failed".to_string())
    });
    assert_eq!(got, Err("the split failed".to_string()));
    assert_eq!(fs::read_to_string(root.join("vendor/a.js")).unwrap(), "a");
}

/// A build that succeeds (also one whose files the rename invariant
/// rejected: the exit code is decided after) is written before the run
/// goes on.
#[test]
fn a_built_tree_is_written_before_the_build_value_is_handed_on() {
    let out = ScratchDir::new("tree-built");
    let root = out.path().join("out");
    let got: Result<usize, String> = super::build_then_write(tree_for(&root), |tree| {
        tree.put_rel("index.js", "x");
        Ok(1)
    });
    assert_eq!(got, Ok(1));
    assert!(root.join("index.js").is_file());
}

impl From<WriteError> for String {
    fn from(e: WriteError) -> String {
        e.to_string()
    }
}
