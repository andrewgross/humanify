//! The output tree's layout — TS `src/split/layout.ts`: where humanified
//! code, vendored libraries and generated metadata live in a --split
//! output directory, and where a prior release's split ledger is found.
//! These constants are the ONLY place the folder names are defined.

use std::path::{Path, PathBuf};

/// Folder holding the humanified app code (the nested split tree).
pub const CODE_DIR: &str = "src";
/// Folder holding vendored libraries (Bun CJS factories), one file each.
pub const VENDOR_DIR: &str = "vendor";
/// Folder holding generated metadata and runtime shims.
pub const METADATA_DIR: &str = ".humanify";
/// The split ledger's filename within the metadata folder.
pub const SPLIT_LEDGER_FILENAME: &str = "split-ledger.json";
/// The split ledger's path within the output tree.
pub const SPLIT_LEDGER_PATH: &str = ".humanify/split-ledger.json";
/// The full single-file humanified output (the next release's
/// `--prior-version`).
pub const HUMANIFIED_SOURCE_PATH: &str = ".humanify/humanified.js";
/// Per-tier placement counts beside the ledger.
pub const PLACEMENT_STATS_PATH: &str = ".humanify/placement-stats.json";
/// Stage-boundary content fingerprints beside the ledger.
pub const STAGE_HASHES_PATH: &str = ".humanify/stage-hashes.json";
/// Pre-.humanify ledger filename; still discovered next to --prior-version.
pub const LEGACY_SPLIT_LEDGER_FILENAME: &str = "_split-ledger.json";

/// `findSplitLedgerIn(dir)`: the first existing candidate — `dir` IS the
/// metadata folder, `dir` is the tree root, or the legacy flat filename.
/// This candidate order is the ONLY place the ledger lineage is encoded.
pub fn find_split_ledger_in(dir: &Path) -> Option<PathBuf> {
    [
        dir.join(SPLIT_LEDGER_FILENAME),
        dir.join(SPLIT_LEDGER_PATH),
        dir.join(LEGACY_SPLIT_LEDGER_FILENAME),
    ]
    .into_iter()
    .find(|candidate| candidate.exists())
}

/// `findSplitLedgerPath(priorFile)`: the ledger beside `--prior-version`.
pub fn find_split_ledger_path(prior_file: &Path) -> Option<PathBuf> {
    find_split_ledger_in(prior_file.parent().unwrap_or(Path::new("")))
}

/// `splitTreeRootOf(priorFile)`: the tree root `--prior-version` belongs
/// to — one step out of the metadata folder, or the file's own directory
/// for a flat pre-.humanify layout.
pub fn split_tree_root_of(prior_file: &Path) -> PathBuf {
    let dir = prior_file.parent().unwrap_or(Path::new(""));
    if dir.file_name().and_then(|n| n.to_str()) == Some(METADATA_DIR) {
        dir.parent().unwrap_or(Path::new("")).to_path_buf()
    } else {
        dir.to_path_buf()
    }
}

#[cfg(test)]
mod layout_test;
