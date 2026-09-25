//! The persisted split ledger — TS `StableSplitLedger` / `FossilLedgerModule`
//! (src/split/stable-split.ts) — the cross-release memory the placement
//! strategies READ. Every optional field is optional for the TS's reason:
//! a ledger written before the field existed simply turns the tier that
//! reads it off for that hop, never a wrong answer.
//!
//! Read side only: `nameToFiles` is looked up, never iterated, so a hash
//! map is safe for it (the writer — WP5.3's `buildLedger` — owns the key
//! order of what it writes).

use std::collections::HashMap;
use std::path::Path;

/// One fossil module as the ledger records it (`FossilLedgerModule`).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct FossilLedgerModule {
    pub file: String,
    pub hashes: Vec<String>,
    pub imports: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<String>>,
}

/// `StableSplitLedger`.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StableSplitLedger {
    pub version: u64,
    pub files: Vec<String>,
    pub name_to_files: HashMap<String, Vec<String>>,
    pub order: Vec<String>,
    #[serde(default)]
    pub hashes: Option<Vec<String>>,
    #[serde(default)]
    pub emit_hashes: Option<Vec<String>>,
    #[serde(default)]
    pub emit_names: Option<Vec<Option<String>>>,
    #[serde(default)]
    pub emit_indexes: Option<Vec<u64>>,
    #[serde(default)]
    pub hash_version: Option<u64>,
    #[serde(default)]
    pub aliases: Option<HashMap<String, String>>,
    #[serde(default)]
    pub fossil_modules: Option<Vec<FossilLedgerModule>>,
}

/// Read a ledger file (`loadPriorSplitLedger`'s parse).
pub fn read_ledger(path: &Path) -> Result<StableSplitLedger, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}
