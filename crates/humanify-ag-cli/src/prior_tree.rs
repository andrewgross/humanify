//! The ONE reader of the prior release (`--prior-version`, with the split's
//! memory its layout record holds). It reads every
//! prior file the run uses once, at the start, before anything is written
//! — so a prior folder that is also the output folder is read whole before
//! the run changes it — and hands the core values: a
//! [`humanify_ag_core::prior_tree::PriorTree`] and the parsed ledger.
//!
//! No `--prior-version` is the only "no prior". A prior that is there but
//! cannot be read, or that holds a vendor manifest or a ledger this binary
//! does not read, fails the run here with its path.

use std::path::{Path, PathBuf};

use humanify_ag_core::layout_record::ledger_view::ledger_of;
use humanify_ag_core::layout_record::{LAYOUT_PATH, Layout};
use humanify_ag_core::place::layout::{
    METADATA_DIR, SPLIT_LEDGER_PATH, prior_file_in_tree, split_tree_root_of,
};
use humanify_ag_core::place::ledger::StableSplitLedger;
use humanify_ag_core::prior_tree::{
    PriorFiles, PriorTree, PriorTreeError, enters_folder, vendor_manifest_path,
};
use humanify_ag_io::tree::{TreeError, read_tree};

/// The prior split ledger the run inherits from, with the layout record
/// it was read from (the run log names it).
#[derive(Clone, Debug)]
pub struct PriorLedger {
    pub path: PathBuf,
    pub ledger: StableSplitLedger,
}

/// Everything the run reads from the prior release.
#[derive(Debug, Default)]
pub struct PriorInputs {
    /// The prior tree; None without `--prior-version`.
    pub tree: Option<PriorTree>,
    /// The split's memory the prior tree's layout record holds.
    pub ledger: Option<PriorLedger>,
}

/// Why the prior could not be read.
#[derive(Debug, thiserror::Error)]
pub enum PriorLoadError {
    /// The `--prior-version` file itself.
    #[error("{}", crate::util::node_fs_error(.source, "open", &.path.display().to_string()))]
    PriorFile {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A folder or file of the prior tree.
    #[error("cannot read the prior tree: {0}")]
    Tree(#[from] TreeError),
    /// What the tree holds is not what this binary reads.
    #[error("{}", refused_text(.root, .error))]
    Refused {
        root: PathBuf,
        error: PriorTreeError,
    },
    /// The tree's layout record.
    #[error("prior layout record {} was not read: {why}", .path.display())]
    Layout { path: PathBuf, why: String },
}

fn refused_text(root: &Path, error: &PriorTreeError) -> String {
    match error {
        PriorTreeError::VendorManifest { why } => format!(
            "prior vendor manifest {} is not one this binary reads ({why}); \
             rebuild the prior release with this binary",
            root.join(vendor_manifest_path()).display()
        ),
        error => format!("{}: {error}", root.display()),
    }
}

/// Read the prior release: the `--prior-version` file and its tree, with
/// the split's memory its layout record holds.
pub fn load_prior(prior_file: Option<&Path>) -> Result<PriorInputs, PriorLoadError> {
    let Some(p) = prior_file else {
        return Ok(PriorInputs::default());
    };
    let code = std::fs::read(p)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|source| PriorLoadError::PriorFile {
            path: p.to_path_buf(),
            source,
        })?;
    let tree = load_prior_tree(p, code)?;
    let ledger = prior_ledger(&split_tree_root_of(p), &tree)?;
    Ok(PriorInputs {
        tree: Some(tree),
        ledger,
    })
}

/// The ledger the prior tree's layout record holds; `None`: the tree has
/// no layout record or was not split.
fn prior_ledger(root: &Path, tree: &PriorTree) -> Result<Option<PriorLedger>, PriorLoadError> {
    let path = root.join(LAYOUT_PATH);
    let refused = |why: String| PriorLoadError::Layout {
        path: path.clone(),
        why,
    };
    let Some(text) = tree.files.text(LAYOUT_PATH) else {
        return match tree.files.contains(SPLIT_LEDGER_PATH) {
            true => Err(refused(
                "the tree holds a split ledger and no layout record; \
                 rebuild the prior release with this binary"
                    .into(),
            )),
            false => Ok(None),
        };
    };
    let layout = Layout::parse(text).map_err(|e| refused(e.to_string()))?;
    let ledger = ledger_of(&layout).map_err(|e| refused(e.to_string()))?;
    Ok(ledger.map(|ledger| PriorLedger {
        path: path.clone(),
        ledger,
    }))
}

/// The tree `prior_file` belongs to, with `code` as the prior's text (the
/// `unpack` verb reads only the vendor record and passes none). The prior
/// file is taken out of the file set, which would otherwise hold it a
/// second time.
pub fn load_prior_tree(prior_file: &Path, code: String) -> Result<PriorTree, PriorLoadError> {
    let root = split_tree_root_of(prior_file);
    let mut files = PriorFiles::new(read_tree(&root, &enters_folder)?);
    // The prior file is held once, as the code — unless it sits among the
    // tree's own files (outside the metadata folder), where a reader may
    // ask for it by path.
    let in_tree = prior_file_in_tree(prior_file);
    if in_tree.starts_with(&format!("{METADATA_DIR}/")) {
        files.take(&in_tree);
    }
    PriorTree::parse(code, files).map_err(|error| PriorLoadError::Refused { root, error })
}

#[cfg(test)]
mod prior_tree_test;
