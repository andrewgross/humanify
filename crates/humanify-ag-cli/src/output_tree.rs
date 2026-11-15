//! The shell's side of the run's output tree
//! (`humanify_ag_core::output_tree`): the disk the core's tree reads
//! through for what it did not write, and the ONE writer that puts the
//! tree on disk — once, at the end of the run (also when a stage failed
//! after the tree had content, and on the rename invariant's exit 1).
//!
//! Writing removes the paths the run removed, then writes every file,
//! making its folder. Nothing else in the output folder is touched: a
//! file an earlier run left there stays (the finish has already named the
//! ones under `vendor/`).

use std::path::{Path, PathBuf};

use humanify_ag_core::error::{FileError, FileOp};
use humanify_ag_core::output_tree::{FolderEntry, FolderReader, OutputTree, TreeFile};

/// The disk as it stands.
pub struct Disk;

impl FolderReader for Disk {
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    /// A link to a folder `leads_to_dir` but is not a folder itself: the
    /// finish has walks of both kinds.
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

/// An empty tree for the output folder `root`, over the disk.
pub fn tree_for(root: &Path) -> OutputTree {
    OutputTree::new(root, Box::new(Disk))
}

/// Why the tree could not be written.
#[derive(Debug)]
pub enum WriteError {
    /// Making a folder or writing a file failed.
    File(FileError),
    /// An embedded file could not be copied into the tree.
    EmbeddedCopy {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::File(e) => e.fmt(f),
            Self::EmbeddedCopy { from, to, source } => write!(
                f,
                "copy the embedded file {} to {}: {source}",
                from.display(),
                to.display()
            ),
        }
    }
}

impl From<FileError> for WriteError {
    fn from(e: FileError) -> Self {
        Self::File(e)
    }
}

/// Put `tree` on disk: the output folder (made even when the tree is
/// empty), the removals (a path already gone is fine), then every file.
pub fn write_tree(tree: &OutputTree) -> Result<(), WriteError> {
    let root = tree.root();
    std::fs::create_dir_all(root).map_err(FileError::at(FileOp::Mkdir, root))?;
    for path in tree.removed() {
        // Best-effort, as each removal site was when it removed at once:
        // the path may not exist on disk.
        let _ = std::fs::remove_file(path);
    }
    for (path, file) in tree.files() {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(FileError::at(FileOp::Mkdir, dir))?;
        }
        match file {
            TreeFile::Bytes(bytes) => {
                std::fs::write(path, bytes).map_err(FileError::at(FileOp::Write, path))?;
            }
            TreeFile::CopyOf(from) => {
                std::fs::copy(from, path).map_err(|source| WriteError::EmbeddedCopy {
                    from: from.clone(),
                    to: path.to_path_buf(),
                    source,
                })?;
            }
        }
    }
    Ok(())
}

/// Build the output tree with `build`, then write it ONCE whatever
/// `build` returned: a stage that fails after the tree has content leaves
/// that content on disk, as a run that wrote as it went did. The stage's
/// error is the run's error; the write's comes only after a build that
/// succeeded. The tree is dropped once written.
pub fn build_then_write<T, E: From<WriteError>>(
    mut tree: OutputTree,
    build: impl FnOnce(&mut OutputTree) -> Result<T, E>,
) -> Result<T, E> {
    let built = build(&mut tree);
    let written = {
        let _ph = humanify_ag_core::profiling::phase("write-tree");
        write_tree(&tree)
    };
    drop(tree);
    let built = built?;
    written?;
    Ok(built)
}

#[cfg(test)]
mod output_tree_test;
