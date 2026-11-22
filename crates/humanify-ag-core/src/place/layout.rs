//! The output tree's layout — TS `src/split/layout.ts`: where humanified
//! code, vendored libraries and generated metadata live in a --split
//! output directory, and where a prior release's split ledger is found.
//! These constants are the ONLY place the folder names are defined.

use std::path::{Path, PathBuf};

/// Folder holding the humanified app code (the nested split tree).
pub const CODE_DIR: &str = "src";
/// Folder holding vendored libraries (Bun CJS factories), one file each.
pub const VENDOR_DIR: &str = "vendor";
/// Folder holding the app's own TEXT assets — bundled modules that are
/// nothing but a piece of app text (`modules::text_assets`, finding #90).
/// Inside the app tree, so the app's text is reviewed and scored as app
/// code; in a folder the split's own names can never produce (every split
/// folder and file name is kebab-case, `[a-z0-9-]`; this one starts with
/// `_`); and fixed, not next to the file that reads it, so an asset never
/// moves because its reader did.
pub const ASSETS_DIR: &str = "src/_assets";
/// Folder holding generated metadata and runtime shims.
pub const METADATA_DIR: &str = ".humanify";
/// The split ledger's path within the output tree.
pub const SPLIT_LEDGER_PATH: &str = ".humanify/split-ledger.json";
/// The full single-file humanified output (the next release's
/// `--prior-version`).
pub const HUMANIFIED_SOURCE_PATH: &str = ".humanify/humanified.js";
/// Per-tier placement counts beside the ledger.
pub const PLACEMENT_STATS_PATH: &str = ".humanify/placement-stats.json";
/// Stage-boundary content fingerprints beside the ledger.
pub const STAGE_HASHES_PATH: &str = ".humanify/stage-hashes.json";
/// The folder `--prior-version` sits in, as a path prefix inside its tree
/// ([`split_tree_root_of`]): `.humanify/` when the file is in the metadata
/// folder, else empty.
fn prior_dir_in_tree(prior_file: &Path) -> String {
    let dir = prior_file.parent().unwrap_or(Path::new(""));
    if dir.file_name().and_then(|n| n.to_str()) == Some(METADATA_DIR) {
        format!("{METADATA_DIR}/")
    } else {
        String::new()
    }
}

/// `--prior-version`'s own path inside its tree.
pub fn prior_file_in_tree(prior_file: &Path) -> String {
    let name = prior_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!("{}{name}", prior_dir_in_tree(prior_file))
}

/// `splitTreeRootOf(priorFile)`: the tree root `--prior-version` belongs
/// to — one step out of the metadata folder, or the file's own directory.
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
