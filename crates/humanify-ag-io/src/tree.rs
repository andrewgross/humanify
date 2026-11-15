//! A folder read into memory in one pass: every file under a root, by its
//! `/`-separated path relative to the root, as bytes. The shell reads the
//! prior release's tree this way once, before the run writes anything, so
//! the core works on values and a prior folder that is also the output
//! folder is read whole before it changes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A folder or file under the root could not be read.
#[derive(Debug)]
pub struct TreeError {
    pub path: PathBuf,
    pub source: std::io::Error,
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "read {}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for TreeError {}

/// Every file under `root` (an empty path is the working directory).
/// `enter(name)` decides, per sub-folder name, whether the folder is read.
/// A symbolic link to a file is read as the file; a link to a folder is
/// not followed (a link loop would not end).
pub fn read_tree(
    root: &Path,
    enter: &dyn Fn(&str) -> bool,
) -> Result<BTreeMap<String, Vec<u8>>, TreeError> {
    let root = if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    };
    let mut files = BTreeMap::new();
    read_into(root, "", enter, &mut files)?;
    Ok(files)
}

fn read_into(
    dir: &Path,
    prefix: &str,
    enter: &dyn Fn(&str) -> bool,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), TreeError> {
    let error = |path: &Path| {
        let path = path.to_path_buf();
        move |source| TreeError { path, source }
    };
    for entry in std::fs::read_dir(dir).map_err(error(dir))? {
        let entry = entry.map_err(error(dir))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type().map_err(error(&path))?;
        let rel = format!("{prefix}{name}");
        if kind.is_dir() {
            if enter(&name) {
                read_into(&path, &format!("{rel}/"), enter, files)?;
            }
        } else if kind.is_file()
            || (kind.is_symlink() && std::fs::metadata(&path).is_ok_and(|m| m.is_file()))
        {
            files.insert(rel, std::fs::read(&path).map_err(error(&path))?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tree_test;
