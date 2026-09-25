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

use crate::hash::statement_hash::STATEMENT_HASH_VERSION;

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

impl StableSplitLedger {
    /// Were this ledger's statement hashes (`hashes`, `emitHashes`,
    /// `fossilModules[].hashes`) written by THIS hash function? The ONE
    /// owner of the version question: the hash tier, the emission
    /// alignment and the fossil matcher all refuse a ledger for which this
    /// is false — a TS-era ledger (`hashVersion: 1`, the TS bytes) is
    /// never joined against the Rust's own hashes (WP5.6e).
    pub fn hashes_current(&self) -> bool {
        self.hash_version == Some(STATEMENT_HASH_VERSION)
    }
}

/// What [`rederive_ts_era_hashes`] proved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rederived {
    pub statements: usize,
    /// Distinct hash classes — the same count on both sides (a bijection).
    pub classes: usize,
}

/// The TS-era hash version (`src/split/statement-hash.ts`).
const TS_ERA_HASH_VERSION: u64 = 1;

/// A prior ledger's statement hashes, as this run will read them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PriorHashes {
    /// Written by this hash function: read as-is.
    Current,
    /// TS-era, re-keyed onto this hash function ([`rederive_ts_era_hashes`]).
    Rederived(Rederived),
    /// Not usable: every hash reader refuses the ledger (the hash tier
    /// reads `no-prior-hashes`, emission alignment and fossil matching run
    /// as with no prior); the layout falls back to what the regime does
    /// without prior hashes. The reason, for the log.
    Refused(String),
}

impl PriorHashes {
    /// The run-log line — LOUD for a refusal (never a silent mis-join).
    pub fn describe(&self) -> String {
        match self {
            PriorHashes::Current => {
                format!("Split ledger hashes: hashVersion {STATEMENT_HASH_VERSION} (current)")
            }
            PriorHashes::Rederived(r) => format!(
                "Split ledger hashes: TS-era (hashVersion {TS_ERA_HASH_VERSION}) re-derived from \
the prior text — {} statements, {} classes (bijection proven)",
                r.statements, r.classes
            ),
            PriorHashes::Refused(why) => format!(
                "WARNING: split ledger hashes REFUSED ({why}) — the hash tier, emission-order \
alignment and fossil matching run WITHOUT prior hashes this hop"
            ),
        }
    }
}

/// Settle which hashes a freshly-read prior ledger offers this run: its own
/// when current, re-derived from `prior_text` when TS-era, else refused.
pub fn settle_prior_hashes(
    ledger: &mut StableSplitLedger,
    prior_text: Option<&str>,
) -> PriorHashes {
    if ledger.hashes_current() {
        return PriorHashes::Current;
    }
    let Some(text) = prior_text else {
        return PriorHashes::Refused(format!(
            "hashVersion {} is not {STATEMENT_HASH_VERSION} and no prior text to re-derive from",
            ledger
                .hash_version
                .map_or("(absent)".to_string(), |v| v.to_string())
        ));
    };
    match rederive_ts_era_hashes(ledger, text) {
        Ok(r) => PriorHashes::Rederived(r),
        Err(e) => PriorHashes::Refused(e),
    }
}

/// Bring a TS-era ledger (`hashVersion: 1`) onto THIS hash function by
/// re-deriving its statement hashes from the prior release's own text —
/// R6's promise, WP5.6e.
///
/// `prior_text` is the prior's `humanified.js` (the shipped text the ledger
/// was written from: one wrapper statement per `order` entry). Its
/// statements are hashed by the Rust, and the ledger's TS bytes are
/// re-keyed through the per-statement correspondence ONLY when that
/// correspondence is a BIJECTION between the two partitions — the same
/// proof the migration's hash injection required, run the other way. Then
/// `hashes` is the Rust's, and `emitHashes` / `fossilModules[].hashes` are
/// translated class for class (each module's list re-sorted, as the
/// extraction sorts it), so every reader decides exactly as it would have
/// on the TS bytes.
///
/// Any failure leaves the ledger UNTOUCHED — still `hashVersion: 1`, which
/// every reader refuses ([`StableSplitLedger::hashes_current`]) — and says
/// why, for the caller to log. Never a partial re-key.
pub fn rederive_ts_era_hashes(
    ledger: &mut StableSplitLedger,
    prior_text: &str,
) -> Result<Rederived, String> {
    if ledger.hash_version != Some(TS_ERA_HASH_VERSION) {
        return Err(format!(
            "hashVersion {} is not the TS era ({TS_ERA_HASH_VERSION})",
            ledger
                .hash_version
                .map_or("(absent)".to_string(), |v| v.to_string())
        ));
    }
    let ts = ledger
        .hashes
        .as_ref()
        .filter(|h| h.len() == ledger.order.len())
        .ok_or("the ledger records no per-statement hashes")?;
    let rust = super::input::split_input(prior_text)?.hashes;
    if rust.len() != ts.len() {
        return Err(format!(
            "the prior text has {} wrapper statements, the ledger {}",
            rust.len(),
            ts.len()
        ));
    }
    let mut ts_to_rust: HashMap<&str, &str> = HashMap::new();
    let mut rust_to_ts: HashMap<&str, &str> = HashMap::new();
    for (i, (t, r)) in ts.iter().zip(&rust).enumerate() {
        if *ts_to_rust.entry(t).or_insert(r) != r || *rust_to_ts.entry(r).or_insert(t) != t {
            return Err(format!(
                "statement {i}: the TS and Rust hash partitions differ (not a bijection)"
            ));
        }
    }
    let translate = |list: &[String]| -> Result<Vec<String>, String> {
        list.iter()
            .map(|h| {
                ts_to_rust
                    .get(h.as_str())
                    .map(|r| r.to_string())
                    .ok_or_else(|| format!("hash {h} is not among the ledger's statements"))
            })
            .collect()
    };
    let emit_hashes = ledger.emit_hashes.as_deref().map(translate).transpose()?;
    let fossil_modules = match &ledger.fossil_modules {
        Some(modules) => Some(
            modules
                .iter()
                .map(|m| {
                    let mut hashes = translate(&m.hashes)?;
                    hashes.sort();
                    Ok(FossilLedgerModule {
                        hashes,
                        ..m.clone()
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        ),
        None => None,
    };
    let classes = ts_to_rust.len();
    ledger.hashes = Some(rust);
    ledger.emit_hashes = emit_hashes;
    ledger.fossil_modules = fossil_modules;
    ledger.hash_version = Some(STATEMENT_HASH_VERSION);
    Ok(Rederived {
        statements: ledger.order.len(),
        classes,
    })
}

/// Read a ledger file (`loadPriorSplitLedger`'s parse).
pub fn read_ledger(path: &Path) -> Result<StableSplitLedger, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ledger: StableSplitLedger =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if ledger.version != 1 {
        return Err(format!(
            "Unsupported split ledger version in {}",
            path.display()
        ));
    }
    Ok(ledger)
}

#[cfg(test)]
mod ledger_test;
