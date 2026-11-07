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
    #[serde(rename = "$comment", default)]
    pub comment: String,
    pub entries: Vec<ConcurrencySite>,
}

/// One confirmed site: file + scope + construct, its pattern and why. A
/// missing field reads as empty, so the census can name the malformed
/// entry (`missing "reason"`) instead of failing the whole load.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConcurrencySite {
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub construct: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub reason: String,
    /// A known deviation from the rules, naming the open finding.
    pub exception: Option<String>,
}

/// Load the concurrency allowlist.
pub fn concurrency_allowlist(repo: &Path) -> Result<Loaded<ConcurrencyAllowlist>, String> {
    concurrency_allowlist_at(&repo.join(CONCURRENCY_ALLOWLIST))
}

/// Load a concurrency allowlist from an explicit path (`--allowlist`).
pub fn concurrency_allowlist_at(file: &Path) -> Result<Loaded<ConcurrencyAllowlist>, String> {
    load_json(file)
}
