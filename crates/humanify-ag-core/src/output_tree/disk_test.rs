//! Test support: the disk under an output tree, and writing a tree to
//! disk, for the unit tests that run a stage and read its files back (the
//! product's writer is the shell's, cli `output_tree`).
// io: test code writes scratch trees; it is not the pipeline
#![allow(clippy::disallowed_methods)]

use std::path::Path;

use super::{FolderEntry, FolderReader, OutputTree, TreeFile};

/// The real disk.
pub(crate) struct Disk;

impl FolderReader for Disk {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn list(&self, dir: &Path) -> std::io::Result<Vec<FolderEntry>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            out.push(FolderEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: entry.file_type()?.is_dir(),
                leads_to_dir: entry.path().is_dir(),
            });
        }
        Ok(out)
    }
}

/// A tree for the folder `root`, over the real disk.
pub(crate) fn tree_on_disk(root: &Path) -> OutputTree {
    OutputTree::new(root, Box::new(Disk))
}

/// Write `tree` to disk: its removals, then its files.
pub(crate) fn write(tree: &OutputTree) {
    std::fs::create_dir_all(tree.root()).unwrap();
    for path in tree.removed() {
        std::fs::remove_file(path).ok();
    }
    for (path, file) in tree.files() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        match file {
            TreeFile::Bytes(b) => std::fs::write(path, b).unwrap(),
            TreeFile::CopyOf(from) => {
                std::fs::copy(from, path).unwrap();
            }
        }
    }
}
