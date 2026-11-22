//! The persisted split ledger — TS `StableSplitLedger` / `FossilLedgerModule`
//! (src/split/stable-split.ts) — the cross-release memory the placement
//! strategies READ. A field this binary always writes is required: a ledger
//! without one was written by another pipeline and is refused at the read.
//! The optional fields are the ones a run legitimately leaves out (a regime
//! without fossil modules, `--disable emit-align`).
//!
//! Two shapes: [`SplitLedger`] is what this run writes and patches (key
//! order fixed, `nameToFiles` in declaration order); [`StableSplitLedger`]
//! is the prior's, read only — its `nameToFiles` is only looked up, not
//! iterated, so a hash map is safe for it.

use std::collections::HashMap;

use crate::hash::forms::STATEMENT;

/// One fossil module as the ledger records it (`FossilLedgerModule`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct FossilLedgerModule {
    pub file: String,
    pub hashes: Vec<String>,
    pub imports: Vec<usize>,
    pub declared: Vec<String>,
    pub tokens: Vec<String>,
    /// The module's original source path, when the bundler kept it
    /// (esbuild's unminified form — exp075). Recorded metadata ONLY:
    /// no placement or matching tier reads it.
    #[serde(
        rename = "sourcePath",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub source_path: Option<String>,
    /// The module's MECHANICAL stem (`module_stem`: its first function,
    /// else its first declaration) — what the next hop's same-file-name
    /// match tier compares, since the file itself carries the module's
    /// name (docs/design/module-naming.md).
    #[serde(rename = "mechanicalStem")]
    pub mechanical_stem: String,
}

/// The ledger this run writes to `.humanify/split-ledger.json` (compact,
/// no trailing newline) and the post-split reconcile patches. Field order
/// is the key order written; `nameToFiles` keeps the order names were
/// first declared in the bundle. Every field but the last two is always
/// written.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitLedger {
    pub version: u64,
    pub files: Vec<String>,
    pub name_to_files: humanify_ag_model::llm::JsRecord<Vec<String>>,
    pub order: Vec<String>,
    pub hashes: Vec<String>,
    pub emit_hashes: Vec<String>,
    pub emit_names: Vec<Option<String>>,
    pub hash_version: u64,
    /// Only the module regime records fossil modules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fossil_modules: Option<Vec<FossilLedgerModule>>,
    /// The runnable emit's statement layout; absent for a review tree and
    /// under `--disable emit-align`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emit_indexes: Option<Vec<usize>>,
}

impl SplitLedger {
    /// The written bytes.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a split ledger serializes")
    }
}

/// `StableSplitLedger`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Deserialize)]
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
    pub hash_version: u64,
    #[serde(default)]
    pub fossil_modules: Option<Vec<FossilLedgerModule>>,
}

impl StableSplitLedger {
    /// Were this ledger's statement hashes (`hashes`, `emitHashes`,
    /// `fossilModules[].hashes`) written by THIS hash function? The ONE
    /// owner of the version question: the hash tier, the emission
    /// alignment and the fossil matcher all refuse a ledger for which this
    /// is false — a stale-era ledger (the TS's `hashVersion: 1`, or an
    /// older Rust hash era) is never joined against this run's own hashes
    /// (WP5.6e; every era since, re-keyed via
    /// [`rederive_stale_era_hashes`] or refused loudly).
    pub fn hashes_current(&self) -> bool {
        self.hash_version == STATEMENT.version
    }
}

/// What [`rederive_stale_era_hashes`] proved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rederived {
    /// The era the ledger carried before the re-derivation (`1` = the TS
    /// bytes; `2` = the pre-exp094b Rust era).
    pub recorded_version: u64,
    pub statements: usize,
    /// Distinct hash classes — the same count on both sides (a bijection).
    pub classes: usize,
}

/// Why a stale-era ledger's hashes could not be re-derived.
#[derive(Debug, thiserror::Error)]
pub enum RederiveError {
    /// The ledger is already on this hash function.
    #[error("hashVersion {version} is current — nothing to re-derive")]
    Current { version: u64 },
    /// The ledger records no hash per statement.
    #[error("the ledger records no per-statement hashes")]
    NoHashes,
    /// The prior text gives no split input.
    #[error(transparent)]
    Input(#[from] super::input::SplitInputError),
    /// The prior text and the ledger hold different statement counts.
    #[error("the prior text has {prior} wrapper statements, the ledger {ledger}")]
    CountMismatch { prior: usize, ledger: usize },
    /// The two eras' hash partitions differ at `statement`.
    #[error(
        "statement {statement}: the recorded (hashVersion {recorded_version}) and current (hashVersion {}) hash partitions differ (not a bijection)",
        STATEMENT.version
    )]
    NotBijection {
        statement: usize,
        recorded_version: u64,
    },
    /// A hash the ledger lists is none of its statements'.
    #[error("hash {hash} is not among the ledger's statements")]
    UnknownHash { hash: String },
}

/// A prior ledger's statement hashes, as this run will read them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PriorHashes {
    /// Written by this hash function: read as-is.
    Current,
    /// Stale era, re-keyed onto this hash function
    /// ([`rederive_stale_era_hashes`]).
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
                format!(
                    "Split ledger hashes: hashVersion {} (current)",
                    STATEMENT.version
                )
            }
            PriorHashes::Rederived(r) => format!(
                "Split ledger hashes: stale era (hashVersion {}) re-derived from \
the prior text — {} statements, {} classes (bijection proven)",
                r.recorded_version, r.statements, r.classes
            ),
            PriorHashes::Refused(why) => format!(
                "WARNING: split ledger hashes REFUSED ({why}) — the hash tier, emission-order \
alignment and fossil matching run WITHOUT prior hashes this hop"
            ),
        }
    }
}

/// Settle which hashes a freshly-read prior ledger offers this run: its own
/// when current, re-derived from `prior_text` when a STALE era's (the TS's
/// `hashVersion: 1`, or an older Rust hash era — exp094b bumped the Rust
/// to 3), else refused.
pub fn settle_prior_hashes(
    ledger: &mut StableSplitLedger,
    prior_text: Option<&str>,
    layout: crate::toolchain::BundleLayout,
) -> PriorHashes {
    if ledger.hashes_current() {
        return PriorHashes::Current;
    }
    let Some(text) = prior_text else {
        return PriorHashes::Refused(format!(
            "hashVersion {} is not {} and no prior text to re-derive from",
            ledger.hash_version, STATEMENT.version
        ));
    };
    match rederive_stale_era_hashes(ledger, text, layout) {
        Ok(r) => PriorHashes::Rederived(r),
        Err(e) => PriorHashes::Refused(e.to_string()),
    }
}

/// Bring a STALE-era ledger onto THIS hash function (WP5.6e brought the
/// TS's `hashVersion: 1` across; exp094b widened the same machinery to
/// every older era, because a version bump makes the previous Rust era a
/// stale one too) by re-deriving its statement hashes from the prior
/// release's own text — R6's promise.
///
/// `prior_text` is the prior's `humanified.js` (the shipped text the ledger
/// was written from: one wrapper statement per `order` entry). Its
/// statements are hashed by the Rust, and the ledger's recorded bytes are
/// re-keyed through the per-statement correspondence ONLY when that
/// correspondence is a BIJECTION between the two partitions — the same
/// proof the migration's hash injection required, run the other way. A
/// class MERGE across the era (the wrapper-spelling unification merges an
/// arrow-spelled statement into its function twin's class) fails this
/// proof and refuses the ledger, never half-carries it. Then `hashes` is
/// the Rust's, and `emitHashes` / `fossilModules[].hashes` are translated
/// class for class (each module's list re-sorted, as the extraction sorts
/// it), so every reader decides exactly as it would have on the recorded
/// bytes.
///
/// Any failure leaves the ledger UNTOUCHED — still its stale era's version,
/// which every reader refuses ([`StableSplitLedger::hashes_current`]) —
/// and says why, for the caller to log. Never a partial re-key.
pub fn rederive_stale_era_hashes(
    ledger: &mut StableSplitLedger,
    prior_text: &str,
    layout: crate::toolchain::BundleLayout,
) -> Result<Rederived, RederiveError> {
    let recorded_version = ledger.hash_version;
    if recorded_version == STATEMENT.version {
        return Err(RederiveError::Current {
            version: recorded_version,
        });
    }
    let recorded = ledger
        .hashes
        .as_ref()
        .filter(|h| h.len() == ledger.order.len())
        .ok_or(RederiveError::NoHashes)?;
    let current = super::input::split_input(prior_text, layout)?.hashes;
    if current.len() != recorded.len() {
        return Err(RederiveError::CountMismatch {
            prior: current.len(),
            ledger: recorded.len(),
        });
    }
    let mut recorded_to_current: HashMap<&str, &str> = HashMap::new();
    let mut current_to_recorded: HashMap<&str, &str> = HashMap::new();
    for (i, (t, r)) in recorded.iter().zip(&current).enumerate() {
        if *recorded_to_current.entry(t).or_insert(r) != r
            || *current_to_recorded.entry(r).or_insert(t) != t
        {
            return Err(RederiveError::NotBijection {
                statement: i,
                recorded_version,
            });
        }
    }
    let translate = |list: &[String]| -> Result<Vec<String>, RederiveError> {
        list.iter()
            .map(|h| {
                recorded_to_current
                    .get(h.as_str())
                    .map(|r| r.to_string())
                    .ok_or_else(|| RederiveError::UnknownHash { hash: h.clone() })
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
                .collect::<Result<Vec<_>, RederiveError>>()?,
        ),
        None => None,
    };
    let classes = recorded_to_current.len();
    ledger.hashes = Some(current);
    ledger.emit_hashes = emit_hashes;
    ledger.fossil_modules = fossil_modules;
    ledger.hash_version = STATEMENT.version;
    Ok(Rederived {
        recorded_version,
        statements: ledger.order.len(),
        classes,
    })
}

#[cfg(test)]
mod ledger_test;
