//! The TS spec (src/split/layout.test.ts), case for case.

use super::{HUMANIFIED_SOURCE_PATH, prior_file_in_tree, split_tree_root_of};
use std::path::{Path, PathBuf};

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
