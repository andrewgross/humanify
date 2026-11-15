//! The error a file operation in core reports: which operation, on which
//! path, and the system's reason. `Display` prints `<op> <path>: <reason>`,
//! the words each site used when its errors were strings, so the run's
//! messages are unchanged. Core's remaining disk sites are legacy (listed
//! in the effects census); this type moves with them to the shell.

use std::path::{Path, PathBuf};

/// What the failed operation was, as its message names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOp {
    /// `read <path>`
    Read,
    /// `cannot read <path>`
    CannotRead,
    /// `write <path>`
    Write,
    /// `mkdir <path>`
    Mkdir,
    /// `readdir <path>`
    Readdir,
    /// `stat <path>`
    Stat,
    /// `remove <path>`
    Remove,
}

impl FileOp {
    fn verb(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::CannotRead => "cannot read",
            Self::Write => "write",
            Self::Mkdir => "mkdir",
            Self::Readdir => "readdir",
            Self::Stat => "stat",
            Self::Remove => "remove",
        }
    }
}

/// A file operation that failed.
#[derive(Debug)]
pub struct FileError {
    pub op: FileOp,
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl FileError {
    /// For `map_err`: the error of `op` on `path`.
    pub fn at(op: FileOp, path: &Path) -> impl FnOnce(std::io::Error) -> FileError + '_ {
        move |source| FileError {
            op,
            path: path.to_path_buf(),
            source,
        }
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.op.verb(),
            self.path.display(),
            self.source
        )
    }
}
