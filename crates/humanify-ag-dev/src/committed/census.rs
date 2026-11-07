//! `scripts/concurrency-allowlist.json`: every confirmed concurrency site
//! with its documented pattern (docs/concurrency.md). The clone census's
//! allowlist is still an array inside `scripts/clone-census.ts`; it
//! becomes a file and a loader here when lane L4 ports that census.

use std::path::Path;

use serde::Deserialize;

use super::{Loaded, load_json};
use crate::paths::CONCURRENCY_ALLOWLIST;

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
