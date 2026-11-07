//! Every committed config file the harness reads, parsed and validated
//! ONCE by its own typed loader (docs/dev-harness.md §2.3): serde with
//! `deny_unknown_fields`, so a field nobody reads — or a typo — fails the
//! load instead of being silently ignored (the `jq '... // empty'` class,
//! finding §6 row 1). Each loader returns the value with the file's path
//! and sha256; the sha goes into the run identity. Files are read where
//! they are TODAY (`crate::paths`); the `dev/` move is lane L5.

pub mod census;
pub mod e2e;
pub mod eval;
pub mod library;
pub mod local;

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

/// A loaded committed file: its typed value, where it came from, its sha.
#[derive(Clone, Debug)]
pub struct Loaded<T> {
    pub value: T,
    pub path: PathBuf,
    pub sha256: String,
}

/// sha256 of some bytes, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Read and parse one JSON file into `T`. A missing file, bad JSON or a
/// field `T` does not declare is an error naming the file.
pub fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Loaded<T>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value = serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Loaded {
        value,
        path: path.to_path_buf(),
        sha256: sha256_hex(&bytes),
    })
}

/// Read one text file (a launch golden).
pub fn load_text(path: &Path) -> Result<Loaded<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sha256 = sha256_hex(&bytes);
    let value = String::from_utf8(bytes).map_err(|_| format!("{}: not UTF-8", path.display()))?;
    Ok(Loaded {
        value,
        path: path.to_path_buf(),
        sha256,
    })
}

#[cfg(test)]
#[path = "committed_test.rs"]
mod committed_test;
