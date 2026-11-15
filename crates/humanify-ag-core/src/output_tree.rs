//! The run's output tree as ONE value: every file the run writes, by its
//! path, built in memory by the stages (unpack, the per-file naming, the
//! vendor-body naming, the split, the finish) and written to disk once by
//! the shell at the end — also when a stage fails after the tree has
//! content, and when the rename invariant fails the run (exit 1 with a
//! tree).
//!
//! The output folder may already hold files (an earlier run into the same
//! folder, or the prior release when `--prior-version` points at the
//! output folder). The run deletes such a file only when it removes the
//! same path itself, and the finish still sees them: a read or a folder
//! listing that the tree does not answer goes to what is on disk, through
//! the [`FolderReader`] the shell hands in. So the finish sees exactly
//! the folder a write-as-you-go run would have left on disk at that point:
//! the files already there, overlaid by what this run wrote, less what it
//! removed. Core opens no file (enforced: census:effects).

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::error::{FileError, FileOp};

/// One entry of a folder as the disk lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderEntry {
    pub name: String,
    /// The entry is a folder itself (a link to a folder is not).
    pub is_dir: bool,
    /// The entry is a folder or a link to one.
    pub leads_to_dir: bool,
}

/// What is on disk, read by the shell when the core asks: the output
/// folder's files that this run did not write, and the files beside the
/// input (installed package manifests, embedded files).
pub trait FolderReader: Send + Sync {
    /// The file's bytes.
    fn read(&self, path: &Path) -> std::io::Result<Vec<u8>>;
    /// The folder's entries, in any order.
    fn list(&self, dir: &Path) -> std::io::Result<Vec<FolderEntry>>;
    /// Is there a file (or a link to one) at `path`? Without reading it,
    /// for a reader that can ask the disk.
    fn has_file(&self, path: &Path) -> bool {
        self.read(path).is_ok()
    }
}

/// Nothing on disk: an output folder that does not exist yet (the unit
/// tests, and the verbs that unpack into a fresh scratch folder).
pub struct NothingOnDisk;

impl FolderReader for NothingOnDisk {
    fn read(&self, _path: &Path) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::NotFound.into())
    }

    fn list(&self, _dir: &Path) -> std::io::Result<Vec<FolderEntry>> {
        Err(std::io::ErrorKind::NotFound.into())
    }
}

/// One file of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeFile {
    /// The file's bytes.
    Bytes(Vec<u8>),
    /// A copy of a file on disk (an embedded file a linked Bun app reads by
    /// path), copied by the shell when it writes the tree; the tree holds only
    /// the source path.
    CopyOf(PathBuf),
}

/// What the tree says about one path.
pub enum Lookup<'t> {
    /// This run wrote the file.
    Written(&'t TreeFile),
    /// This run removed the file.
    Removed,
    /// This run did not touch the path: what is on disk stands.
    Untouched,
}

/// The run's output tree.
pub struct OutputTree {
    root: PathBuf,
    files: BTreeMap<PathBuf, TreeFile>,
    removed: BTreeSet<PathBuf>,
    disk: Box<dyn FolderReader>,
}

impl std::fmt::Debug for OutputTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputTree")
            .field("root", &self.root)
            .field("files", &self.files.len())
            .field("removed", &self.removed)
            .finish()
    }
}

impl OutputTree {
    /// An empty tree for the output folder `root`, over what `disk` reads.
    pub fn new(root: &Path, disk: Box<dyn FolderReader>) -> OutputTree {
        OutputTree {
            root: root.to_path_buf(),
            files: BTreeMap::new(),
            removed: BTreeSet::new(),
            disk,
        }
    }

    /// The output folder, as the run named it.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The reader of what is on disk.
    pub fn disk(&self) -> &dyn FolderReader {
        self.disk.as_ref()
    }

    /// Write `bytes` at `path` (a later write of the same path replaces
    /// it; a removed path is written again).
    pub fn put(&mut self, path: PathBuf, bytes: impl Into<Vec<u8>>) {
        self.removed.remove(&path);
        self.files.insert(path, TreeFile::Bytes(bytes.into()));
    }

    /// Write `bytes` at `rel` under the root.
    pub fn put_rel(&mut self, rel: &str, bytes: impl Into<Vec<u8>>) -> PathBuf {
        let path = self.root.join(rel);
        self.put(path.clone(), bytes);
        path
    }

    /// Place a copy of the file on disk at `from` at `path`.
    pub fn copy(&mut self, path: PathBuf, from: PathBuf) {
        self.removed.remove(&path);
        self.files.insert(path, TreeFile::CopyOf(from));
    }

    /// Remove the file at `path`: it is not part of the tree, and the shell
    /// deletes it from disk when it writes the tree.
    pub fn remove(&mut self, path: &Path) {
        self.files.remove(path);
        self.removed.insert(path.to_path_buf());
    }

    /// What the tree says about `path`.
    pub fn lookup(&self, path: &Path) -> Lookup<'_> {
        if let Some(file) = self.files.get(path) {
            Lookup::Written(file)
        } else if self.removed.contains(path) {
            Lookup::Removed
        } else {
            Lookup::Untouched
        }
    }

    /// Every file this run wrote, by path.
    pub fn files(&self) -> impl Iterator<Item = (&Path, &TreeFile)> {
        self.files.iter().map(|(p, f)| (p.as_path(), f))
    }

    /// Every path this run removed (and did not write again).
    pub fn removed(&self) -> impl Iterator<Item = &Path> {
        self.removed.iter().map(PathBuf::as_path)
    }

    /// The file at `path` as the folder would hold it now: this run's
    /// bytes, else what is on disk. A removed path reads as not found.
    pub fn read(&self, path: &Path) -> std::io::Result<Cow<'_, [u8]>> {
        match self.lookup(path) {
            Lookup::Written(TreeFile::Bytes(b)) => Ok(Cow::Borrowed(b)),
            Lookup::Written(TreeFile::CopyOf(from)) => self.disk.read(from).map(Cow::Owned),
            Lookup::Removed => Err(std::io::ErrorKind::NotFound.into()),
            Lookup::Untouched => self.disk.read(path).map(Cow::Owned),
        }
    }

    /// [`Self::read`] as text, invalid UTF-8 replaced (`readFile(…,
    /// "utf-8")`); an error names the path as a `read`.
    pub fn read_lossy(&self, path: &Path) -> Result<String, FileError> {
        let bytes = self.read(path).map_err(FileError::at(FileOp::Read, path))?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// [`Self::read`] as strict UTF-8 text (`read_to_string`): a file that
    /// is not UTF-8 is an error, worded as the standard library words it.
    pub fn read_text(&self, path: &Path) -> Result<String, FileError> {
        let bytes = self.read(path).map_err(FileError::at(FileOp::Read, path))?;
        String::from_utf8(bytes.into_owned()).map_err(|e| FileError {
            op: FileOp::Read,
            path: path.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("stream did not contain valid UTF-8: {}", e.utf8_error()),
            ),
        })
    }

    /// Whether this run's tree places a folder at `dir`: the root, or a
    /// folder some written file sits under.
    fn holds_dir(&self, dir: &Path) -> bool {
        dir == self.root.as_path()
            || self
                .files
                .range::<Path, _>((std::ops::Bound::Excluded(dir), std::ops::Bound::Unbounded))
                .next()
                .is_some_and(|(p, _)| p.starts_with(dir) && p.as_path() != dir)
    }

    /// The folder's entries as the folder would hold them now: what is on
    /// disk (less the files this run removed), with this run's files and
    /// the folders they sit in, sorted by name. A folder only this run's
    /// tree places (nothing on disk yet) lists the tree's entries; a folder
    /// neither holds is the disk's error.
    pub fn list(&self, dir: &Path) -> std::io::Result<Vec<FolderEntry>> {
        let on_disk = match self.disk.list(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && self.holds_dir(dir) => Vec::new(),
            Err(e) => return Err(e),
        };
        let mut by_name: BTreeMap<String, FolderEntry> = BTreeMap::new();
        for entry in on_disk {
            if !entry.is_dir && self.removed.contains(&dir.join(&entry.name)) {
                continue;
            }
            by_name.insert(entry.name.clone(), entry);
        }
        for path in self.files.keys() {
            let Ok(rest) = path.strip_prefix(dir) else {
                continue;
            };
            let mut parts = rest.components();
            let Some(first) = parts.next() else {
                continue;
            };
            let name = first.as_os_str().to_string_lossy().into_owned();
            let is_dir = parts.next().is_some();
            by_name.entry(name.clone()).or_insert(FolderEntry {
                name,
                is_dir,
                leads_to_dir: is_dir,
            });
        }
        Ok(by_name.into_values().collect())
    }
}

#[cfg(test)]
mod output_tree_test;

#[cfg(test)]
pub(crate) mod disk_test;
