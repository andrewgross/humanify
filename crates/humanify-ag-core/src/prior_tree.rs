//! The prior release's tree as ONE value (`--prior-version`): the prior
//! humanified text, every file of the tree it belongs to, and what the
//! core reads out of them — the vendor manifest (parsed, with its module
//! files) and the named vendor bodies' joined text.
//!
//! The shell reads the folder once, before the run writes anything
//! (`humanify_ag_io::tree::read_tree` with [`enters_folder`]), and hands
//! the bytes to [`PriorTree::parse`]; the core opens no prior file
//! (enforced: census:effects).
//! A run without `--prior-version` has no `PriorTree`; that is the only
//! "no prior". A file the tree does not hold is absent (data: the prior
//! release did not write it); a manifest or named-bodies source that is
//! there but cannot be read is an error, not "absent".

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::naming::vendor_bodies::VENDOR_NAMING_SOURCE;
use crate::place::layout::METADATA_DIR;
use crate::unpack::bun::{BUN_MODULES_MANIFEST, PriorVendor, VENDOR_DIR};

/// Whether the shell reads a sub-folder of the prior tree: the metadata
/// folder yes, any other hidden folder (`.git`) and installed packages
/// (`node_modules`) no — the pipeline writes neither, so nothing the core
/// reads can be there.
pub fn enters_folder(name: &str) -> bool {
    name == METADATA_DIR || !(name.starts_with('.') || name == "node_modules")
}

/// The prior tree's files by their `/`-separated path relative to the tree
/// root, as the shell read them.
#[derive(Clone, Debug, Default)]
pub struct PriorFiles {
    by_path: BTreeMap<String, Vec<u8>>,
}

impl PriorFiles {
    pub fn new(by_path: BTreeMap<String, Vec<u8>>) -> PriorFiles {
        PriorFiles { by_path }
    }

    /// Whether the tree holds a file at `rel`.
    pub fn contains(&self, rel: &str) -> bool {
        self.by_path.contains_key(rel)
    }

    /// The file's bytes, when the tree holds it.
    pub fn bytes(&self, rel: &str) -> Option<&[u8]> {
        self.by_path.get(rel).map(Vec::as_slice)
    }

    /// The file as UTF-8 text; a file that is not UTF-8 reads as absent
    /// (the readers that ask for text this way skip such a file).
    // default: a file that is not UTF-8 is data here, not a failed read:
    // the strict-text readers (the split's prior entry order and import
    // aliases, the vendor record's module texts) skip it
    #[allow(clippy::disallowed_methods)]
    pub fn text(&self, rel: &str) -> Option<&str> {
        std::str::from_utf8(self.bytes(rel)?).ok()
    }

    /// The file as text with invalid UTF-8 replaced (the readers that
    /// compare a prior file with a fresh one).
    pub fn text_lossy(&self, rel: &str) -> Option<Cow<'_, str>> {
        self.bytes(rel).map(String::from_utf8_lossy)
    }

    /// Remove a file from the set (the shell drops the files it parsed
    /// separately, so they are not held twice).
    pub fn take(&mut self, rel: &str) -> Option<Vec<u8>> {
        self.by_path.remove(rel)
    }
}

/// Why a prior tree was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PriorTreeError {
    /// The vendor manifest is there but is not one this binary reads
    /// (not UTF-8, not JSON, no `hashVersion`, an entry missing a field
    /// it always writes).
    #[error(
        "prior vendor manifest {} is not one this binary reads ({why}); rebuild the prior release with this binary",
        vendor_manifest_path()
    )]
    VendorManifest { why: String },
    /// The named vendor bodies' joined source is there but is not UTF-8.
    #[error("prior {VENDOR_NAMING_SOURCE} is not UTF-8 text ({why})")]
    VendorNamingNotText { why: String },
}

/// The vendor manifest's path within a tree.
pub fn vendor_manifest_path() -> String {
    format!("{VENDOR_DIR}/{BUN_MODULES_MANIFEST}")
}

/// The prior release as the run reads it.
#[derive(Clone, Debug, Default)]
pub struct PriorTree {
    /// The prior humanified text (`--prior-version` itself).
    pub code: String,
    /// Every file of the tree (the split's prior entry order and import
    /// aliases, the vendor-body inherit, the post-split reconcile).
    pub files: PriorFiles,
    /// The vendor manifest's record, when the tree has a manifest with
    /// entries.
    pub vendor: Option<PriorVendor>,
    /// The named vendor bodies' joined text ([`VENDOR_NAMING_SOURCE`]),
    /// when the prior ran the vendor-body naming (in a tree with a vendor
    /// manifest).
    pub vendor_naming: Option<String>,
}

impl PriorTree {
    /// Read what the core needs out of the prior's text and files.
    pub fn parse(code: String, files: PriorFiles) -> Result<PriorTree, PriorTreeError> {
        // The vendor side of a prior is a tree holding a vendor manifest:
        // the named-bodies source counts only there.
        let manifest = files.bytes(&vendor_manifest_path());
        let vendor_naming = match manifest.and(files.bytes(VENDOR_NAMING_SOURCE)) {
            None => None,
            Some(bytes) => Some(
                String::from_utf8(bytes.to_vec())
                    .map_err(|e| PriorTreeError::VendorNamingNotText { why: e.to_string() })?,
            ),
        };
        let vendor = match manifest {
            None => None,
            Some(bytes) => PriorVendor::parse(bytes, &files, vendor_naming.is_some())?,
        };
        Ok(PriorTree {
            code,
            files,
            vendor,
            vendor_naming,
        })
    }

    /// Whether the tree holds a vendor manifest (the vendor-body inherit
    /// reads only a tree that does).
    pub fn has_vendor_manifest(&self) -> bool {
        self.files.contains(&vendor_manifest_path())
    }
}

#[cfg(test)]
mod prior_tree_test;
