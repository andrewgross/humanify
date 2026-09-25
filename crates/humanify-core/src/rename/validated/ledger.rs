//! The rename ledger (WP3.1) — TS original: `src/rename/rename-ledger.ts`.
//!
//! A replayable record of every identifier rename, keyed by position so it
//! reproduces the renamed output without re-running the model. Derived,
//! not instrumented: for every binding whose current name differs from the
//! source text at its declaration, the entry lists the declaration, every
//! identifier reference, and every write target. Because renames change
//! only identifier tokens, a right-to-left splice of `finalName` at every
//! recorded span reproduces the renamed output exactly (the
//! `applyRenameLedger ⇔ generate(ast)` invariant the pipeline self-checks).
//! Post-generate passes (reconcile, the deferred sweep) rename THEIR input
//! texts; each is a `post` stage keyed by its own snapshot hash.
//!
//! Span unit: UTF-8 byte offsets (07 §1's decided unit; the TS ledger's
//! spans are UTF-16 code units and its comment calling them a "byte range"
//! was wrong — the two agree on ASCII text). Entry order follows the TS:
//! scopes in pre-order, each scope's bindings in `Object.keys` order (a
//! renamed name sits at the end), one entry per declaration.
//!
//! Not reproduced: Babel's `getBindingIdentifierPaths` on a DUPLICATE
//! function declaration also walks its params, so a param that ends up with
//! the renamed binding's final name is listed under it too (and the TS
//! replay then throws "overlapping occurrences"). The Rust lists only the
//! binding's own identifiers.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BScopeId, SiteType};

/// One renamed binding.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RenameLedgerEntry {
    /// The identifier's text in the source snapshot (the minified name).
    #[serde(rename = "originalName")]
    pub original_name: String,
    /// The name it was renamed to.
    #[serde(rename = "finalName")]
    pub final_name: String,
    /// Every occurrence: declaration + reads + write targets, `[start, end)`.
    pub occurrences: Vec<[u32; 2]>,
}

/// A post-generate stage: renames of the PREVIOUS stage's output.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct LedgerStage {
    #[serde(rename = "sourceSha256")]
    pub source_sha256: String,
    pub entries: Vec<RenameLedgerEntry>,
}

/// `RenameLedger` (version 1).
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RenameLedger {
    pub version: u32,
    /// sha256 of the source snapshot the spans index into.
    #[serde(rename = "sourceSha256")]
    pub source_sha256: String,
    pub entries: Vec<RenameLedgerEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Vec<LedgerStage>>,
}

/// `renameResult.renameLedger` (`--rename-ledger`): the ledger, its source
/// snapshot (the fresh text), and each post stage's own source (the text
/// that pass renamed — the ledger's spans are bytes into these).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RenameLedgerBundle {
    pub ledger: RenameLedger,
    pub source: String,
    /// One per `ledger.post` stage, in order.
    pub stage_sources: Vec<String>,
}

impl RenameLedgerBundle {
    /// `JSON.stringify(bundle.ledger)` as the TS writes it: the TS's key
    /// order, and every occurrence a JS string index (UTF-16 units) into
    /// its stage's source.
    pub fn to_ts_json(&self) -> String {
        use humanify_model::js::Utf16Offsets;
        let stage = |sha: &str, entries: &[RenameLedgerEntry], source: &str| {
            let offsets = Utf16Offsets::new(source);
            let entries: Vec<RenameLedgerEntry> = entries
                .iter()
                .map(|e| RenameLedgerEntry {
                    original_name: e.original_name.clone(),
                    final_name: e.final_name.clone(),
                    occurrences: e
                        .occurrences
                        .iter()
                        .map(|[s, en]| [offsets.at(*s), offsets.at(*en)])
                        .collect(),
                })
                .collect();
            LedgerStage {
                source_sha256: sha.to_string(),
                entries,
            }
        };
        let base = stage(
            &self.ledger.source_sha256,
            &self.ledger.entries,
            &self.source,
        );
        let post = self.ledger.post.as_ref().map(|stages| {
            stages
                .iter()
                .zip(&self.stage_sources)
                .map(|(s, src)| stage(&s.source_sha256, &s.entries, src))
                .collect()
        });
        serde_json::to_string(&RenameLedger {
            version: self.ledger.version,
            source_sha256: base.source_sha256,
            entries: base.entries,
            post,
        })
        .expect("a ledger serializes")
    }
}

/// `BIG_SOURCE_BYTES` (babel-utils.ts): a source at least this long
/// (JS `.length`) is a full bundle, and parsing it through the TS parse
/// funnel clears Babel's module-level path/scope cache first.
pub const BIG_SOURCE_BYTES: usize = 5_000_000;

/// Whether the TS parse funnel clears Babel's path/scope cache before
/// parsing `text` (`maybeClearBabelCache`). A ledger walk over an AST
/// whose scopes were cleared after its renames re-crawls them: its entries
/// come in registration (declaration) order, not rename order.
pub fn parse_clears_scope_cache(text: &str) -> bool {
    humanify_model::js::utf16_len(text) >= BIG_SOURCE_BYTES
}

/// Why a ledger cannot be replayed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LedgerError {
    /// The source is not the snapshot the stage was recorded against.
    SourceMismatch,
    /// Two occurrences overlap (an internal inconsistency).
    Overlap,
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LedgerError::SourceMismatch => {
                write!(
                    f,
                    "rename ledger: source does not match the ledger's sourceSha256"
                )
            }
            LedgerError::Overlap => write!(f, "rename ledger: overlapping occurrences (internal)"),
        }
    }
}

/// sha256 of a text, lowercase hex.
pub fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Derive the ledger from the source snapshot the state was built over and
/// the state's final names.
pub fn build_rename_ledger(source: &str, state: &RenameState) -> RenameLedger {
    let view = state.view();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut entries = Vec::new();
    for scope in 0..view.scopes.len() as u32 {
        for (_, binding) in state.bindings_in(BScopeId(scope)) {
            let b = view.binding(binding);
            if !seen.insert(b.id_span.start) {
                continue;
            }
            let original = &source[b.id_span.start as usize..b.id_span.end as usize];
            let final_name = state.name_of(binding);
            if original == final_name {
                continue;
            }
            let spans = std::iter::once(b.id_span)
                .chain(
                    b.refs
                        .iter()
                        .filter(|r| r.ty == SiteType::Identifier)
                        .map(|r| r.span),
                )
                .chain(b.violation_targets.iter().flatten().copied());
            let mut occurrences: Vec<[u32; 2]> = Vec::new();
            for span in spans {
                if !occurrences.iter().any(|o| o[0] == span.start) {
                    occurrences.push([span.start, span.end]);
                }
            }
            entries.push(RenameLedgerEntry {
                original_name: original.to_string(),
                final_name: final_name.to_string(),
                occurrences,
            });
        }
    }
    RenameLedger {
        version: 1,
        source_sha256: sha256_hex(source),
        entries,
        post: None,
    }
}

/// Apply one stage to `source`, verifying the snapshot hash first: every
/// occurrence spliced to its final name, right to left.
fn apply_stage(
    source: &str,
    sha: &str,
    entries: &[RenameLedgerEntry],
) -> Result<String, LedgerError> {
    if sha256_hex(source) != sha {
        return Err(LedgerError::SourceMismatch);
    }
    let mut edits: Vec<(u32, u32, &str)> = entries
        .iter()
        .flat_map(|e| {
            e.occurrences
                .iter()
                .map(|o| (o[0], o[1], e.final_name.as_str()))
        })
        .collect();
    // Descending start; stable, as the TS `Array.sort`.
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = source.to_string();
    let mut prev_start = u32::MAX;
    for (start, end, text) in edits {
        if end > prev_start {
            return Err(LedgerError::Overlap);
        }
        prev_start = start;
        out.replace_range(start as usize..end as usize, text);
    }
    Ok(out)
}

/// Replay a ledger onto its source snapshot: the base entries, then each
/// post stage over the previous stage's output.
pub fn apply_rename_ledger(source: &str, ledger: &RenameLedger) -> Result<String, LedgerError> {
    let mut out = apply_stage(source, &ledger.source_sha256, &ledger.entries)?;
    for stage in ledger.post.iter().flatten() {
        out = apply_stage(&out, &stage.source_sha256, &stage.entries)?;
    }
    Ok(out)
}

#[cfg(test)]
mod ledger_test;
