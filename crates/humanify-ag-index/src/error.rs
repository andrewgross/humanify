//! The error a file operation in the index builder reports: the words
//! before the path (`mkdir `, `journal `, or none), the path, and the
//! system's reason, printed `<op><path>: <reason>` as the sites printed
//! it when their errors were strings.

use std::path::{Path, PathBuf};

/// A file operation that failed.
#[derive(Debug, thiserror::Error)]
#[error("{op}{}: {source}", .path.display())]
pub struct FileError {
    pub op: &'static str,
    pub path: PathBuf,
    pub source: std::io::Error,
}

/// For `map_err`: the error of an operation on `path`, printed with no
/// words before the path.
pub fn at(path: &Path) -> impl FnOnce(std::io::Error) -> FileError + '_ {
    op_at("", path)
}

/// For `map_err`: the error of operation `op` (its words and a space, as
/// the message prints them) on `path`.
pub fn op_at<'p>(
    op: &'static str,
    path: &'p Path,
) -> impl FnOnce(std::io::Error) -> FileError + 'p {
    move |source| FileError {
        op,
        path: path.to_path_buf(),
        source,
    }
}
