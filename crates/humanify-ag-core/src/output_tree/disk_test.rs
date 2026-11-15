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

    fn has_file(&self, path: &Path) -> bool {
        path.is_file()
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

/// A dump folder on the real disk (the product's is the shell's, cli
/// `writers::DumpDir`).
pub(crate) struct DumpDisk(std::path::PathBuf);

impl DumpDisk {
    /// The folder `dir`, made when missing.
    pub(crate) fn create(dir: &Path) -> DumpDisk {
        std::fs::create_dir_all(dir).unwrap();
        DumpDisk(dir.to_path_buf())
    }
}

impl crate::artifact_dump::DumpFolder for DumpDisk {
    fn path(&self) -> &Path {
        &self.0
    }

    fn create(&self, name: &str) -> std::io::Result<Box<dyn std::io::Write + Send>> {
        Ok(Box::new(std::fs::File::create(self.0.join(name))?))
    }

    fn open(&self, name: &str) -> std::io::Result<Box<dyn std::io::Read>> {
        Ok(Box::new(std::fs::File::open(self.0.join(name))?))
    }

    fn remove(&self, name: &str) -> std::io::Result<()> {
        std::fs::remove_file(self.0.join(name))
    }
}
