//! `scripts/concurrency-allowlist.json`: every confirmed concurrency site
//! with its documented pattern (docs/concurrency.md); and
//! `scripts/clone-allowlist.json`: every reviewed twin group of the clone
//! census (`cargo dev census clones`) with its justification.

use std::path::Path;

use serde::Deserialize;

use super::{Loaded, load_json};
use crate::paths::{CLONE_ALLOWLIST, CONCURRENCY_ALLOWLIST};

/// The allowlist file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencyAllowlist {
    #[serde(rename = "$comment")]
    pub comment: String,
    pub entries: Vec<ConcurrencySite>,
}

/// One confirmed site: file + scope + construct, its pattern and why.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencySite {
    pub file: String,
    pub scope: String,
    pub construct: String,
    pub pattern: String,
    pub reason: String,
    /// A known deviation from the rules, naming the open finding.
    pub exception: Option<String>,
}

/// Load the concurrency allowlist.
pub fn concurrency_allowlist(repo: &Path) -> Result<Loaded<ConcurrencyAllowlist>, String> {
    load_json(&repo.join(CONCURRENCY_ALLOWLIST))
}

/// The clone census's allowlist file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CloneAllowlist {
    #[serde(rename = "$comment")]
    pub comment: String,
    pub entries: Vec<CloneGroup>,
}

/// One reviewed group: its SORTED `path:functionName` members (no line
/// numbers, so ordinary edits do not churn it; moving or renaming a
/// member invalidates the entry, which is the point) and why it stays.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CloneGroup {
    pub members: Vec<String>,
    pub justification: String,
}

/// Load the clone allowlist of the tree at `root`.
pub fn clone_allowlist(root: &Path) -> Result<Loaded<CloneAllowlist>, String> {
    load_json(&root.join(CLONE_ALLOWLIST))
}
