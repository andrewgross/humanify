//! The ONE reader of the prior release (`--prior-version`, and the split
//! ledger `--split-ledger` names or the prior tree holds). It reads every
//! prior file the run uses once, at the start, before anything is written
//! — so a prior folder that is also the output folder is read whole before
//! the run changes it — and hands the core values: a
//! [`humanify_ag_core::prior_tree::PriorTree`] and the parsed ledger.
//!
//! No `--prior-version` is the only "no prior". A prior that is there but
//! cannot be read, or that holds a vendor manifest or a ledger this binary
//! does not read, fails the run here with its path.

use std::path::{Path, PathBuf};

use humanify_ag_core::place::layout::{
    METADATA_DIR, prior_file_in_tree, prior_ledger_in, split_tree_root_of,
};
use humanify_ag_core::place::ledger::{LedgerParseError, StableSplitLedger, parse_ledger};
use humanify_ag_core::prior_tree::{
    PriorFiles, PriorTree, PriorTreeError, enters_folder, vendor_manifest_path,
};
use humanify_ag_io::tree::{TreeError, read_tree};

/// The prior split ledger the run inherits from, with the path it was
/// read from (the run log names it).
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
    /// `--split-ledger` when given, else the ledger beside
    /// `--prior-version`; None when neither exists.
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
    /// The ledger file.
    #[error("{}: {source}", .path.display())]
    LedgerRead {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The ledger's text.
    #[error("{}", ledger_text(.path, .error))]
    Ledger {
        path: PathBuf,
        error: LedgerParseError,
    },
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

fn ledger_text(path: &Path, error: &LedgerParseError) -> String {
    match error {
        LedgerParseError::UnsupportedVersion => {
            format!("Unsupported split ledger version in {}", path.display())
        }
        error => format!("{}: {error}", path.display()),
    }
}

/// A ledger file as read: its path and bytes, parsed by the caller.
pub type RawLedger = (PathBuf, Vec<u8>);

/// Read the prior release: the `--prior-version` file, its tree and the
/// split ledger in force (`split_ledger` wins over the one beside the
/// prior file).
pub fn load_prior(
    prior_file: Option<&Path>,
    split_ledger: Option<&Path>,
) -> Result<PriorInputs, PriorLoadError> {
    let (tree, beside) = match prior_file {
        Some(p) => {
            let code = std::fs::read(p)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .map_err(|source| PriorLoadError::PriorFile {
                    path: p.to_path_buf(),
                    source,
                })?;
            let (tree, beside) = load_prior_tree(p, code)?;
            (Some(tree), beside)
        }
        None => (None, None),
    };
    let ledger = match (split_ledger, beside) {
        (Some(path), _) => {
            let bytes = std::fs::read(path).map_err(|source| PriorLoadError::LedgerRead {
                path: path.to_path_buf(),
                source,
            })?;
            Some(parse_prior_ledger(path.to_path_buf(), &bytes)?)
        }
        (None, Some((path, bytes))) => Some(parse_prior_ledger(path, &bytes)?),
        (None, None) => None,
    };
    Ok(PriorInputs { tree, ledger })
}

/// The tree `prior_file` belongs to, with `code` as the prior's text (the
/// `unpack` verb reads only the vendor record and passes none), and the
/// raw ledger beside the prior file (path, bytes) when the tree holds one.
/// The prior file and the ledger are taken out of the file set, which
/// would otherwise hold them a second time.
pub fn load_prior_tree(
    prior_file: &Path,
    code: String,
) -> Result<(PriorTree, Option<RawLedger>), PriorLoadError> {
    let root = split_tree_root_of(prior_file);
    let mut files = PriorFiles::new(read_tree(&root, &enters_folder)?);
    // The prior file is held once, as the code — unless it sits among the
    // tree's own files (outside the metadata folder), where a reader may
    // ask for it by path.
    let in_tree = prior_file_in_tree(prior_file);
    if in_tree.starts_with(&format!("{METADATA_DIR}/")) {
        files.take(&in_tree);
    }
    let beside = prior_ledger_in(prior_file, &files)
        .and_then(|rel| files.take(&rel).map(|bytes| (root.join(rel), bytes)));
    let tree =
        PriorTree::parse(code, files).map_err(|error| PriorLoadError::Refused { root, error })?;
    Ok((tree, beside))
}

fn parse_prior_ledger(path: PathBuf, bytes: &[u8]) -> Result<PriorLedger, PriorLoadError> {
    let text = std::str::from_utf8(bytes).map_err(|e| PriorLoadError::LedgerRead {
        path: path.clone(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
    })?;
    match parse_ledger(text) {
        Ok(ledger) => Ok(PriorLedger { path, ledger }),
        Err(error) => Err(PriorLoadError::Ledger { path, error }),
    }
}

#[cfg(test)]
mod prior_tree_test;
