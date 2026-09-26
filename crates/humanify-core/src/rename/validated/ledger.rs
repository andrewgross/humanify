//! The rename ledger (`--rename-ledger`) — a replayable record of every
//! text edit a naming pass's render made, keyed by position, so it
//! reproduces the renamed output byte for byte without re-running the
//! model.
//!
//! The ledger is DERIVED FROM THE RENDER, not re-derived beside it: a pass
//! renders its text from one edit list
//! ([`crate::naming::waves::render::program_edits`]) and the ledger
//! partitions that same list — every edit written at an identifier
//! occurrence of a binding goes to that binding's entry (its printed text
//! recorded only when it is not the final name: a shorthand property
//! expanded to `key: name`, a specifier form), every other edit (the
//! `{ key: v = d }` collapse, an aliased specifier collapsing onto its
//! other side, the statement twins' private names, babel's `export const`
//! split) is a stage-level text edit. So the replay reproduces the render
//! exactly (finding #49: the old ledger spliced names only and never
//! reproduced a shorthand collapse), and `outputSha256` pins the shipped
//! text the whole chain must reach. Post-generate passes (reconcile, the
//! deferred sweep) render THEIR input texts; each is a `post` stage keyed
//! by its own snapshot hash.
//!
//! The replay is linear (finding #48): the edits sorted ascending, the
//! output assembled from the source slices between them — never a
//! whole-text copy per edit.
//!
//! Span unit: UTF-8 byte offsets (07 §1). The written JSON
//! ([`RenameLedgerBundle::to_json`]) converts them to JS string indexes
//! (UTF-16 units), the unit the standalone `apply.mjs` slices by.

use std::collections::{BTreeSet, HashMap};

use sha2::{Digest, Sha256};

use crate::naming::waves::generate::Replacement;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BScopeId, SiteType};

/// The ledger format version (2: occurrences may carry their printed text,
/// stages carry non-name text edits, the ledger pins its output hash).
pub const RENAME_LEDGER_VERSION: u32 = 2;

/// One written occurrence of a binding: `[start, end)` and, when the
/// render prints something other than the entry's final name there, that
/// text. Serialized `[start, end]` or `[start, end, text]`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Occurrence {
    pub start: u32,
    pub end: u32,
    pub text: Option<String>,
}

impl From<[u32; 2]> for Occurrence {
    fn from([start, end]: [u32; 2]) -> Self {
        Occurrence {
            start,
            end,
            text: None,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum OccurrenceRepr {
    Plain([u32; 2]),
    Printed(u32, u32, String),
}

impl serde::Serialize for Occurrence {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match &self.text {
            None => OccurrenceRepr::Plain([self.start, self.end]).serialize(s),
            Some(t) => OccurrenceRepr::Printed(self.start, self.end, t.clone()).serialize(s),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Occurrence {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match OccurrenceRepr::deserialize(d)? {
            OccurrenceRepr::Plain([start, end]) => Occurrence {
                start,
                end,
                text: None,
            },
            OccurrenceRepr::Printed(start, end, text) => Occurrence {
                start,
                end,
                text: Some(text),
            },
        })
    }
}

/// A text edit that is not an occurrence of a binding: `[start, end, text]`.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct LedgerEdit(pub u32, pub u32, pub String);

/// One renamed binding.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RenameLedgerEntry {
    /// The identifier's text in the source snapshot (the minified name).
    #[serde(rename = "originalName")]
    pub original_name: String,
    /// The name it was renamed to.
    #[serde(rename = "finalName")]
    pub final_name: String,
    /// Every occurrence the render rewrote: declaration, reads, write
    /// targets.
    pub occurrences: Vec<Occurrence>,
}

/// A post-generate stage: the edits of the PREVIOUS stage's output.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct LedgerStage {
    #[serde(rename = "sourceSha256")]
    pub source_sha256: String,
    pub entries: Vec<RenameLedgerEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<LedgerEdit>,
}

/// `RenameLedger`.
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct RenameLedger {
    pub version: u32,
    /// sha256 of the source snapshot the spans index into.
    #[serde(rename = "sourceSha256")]
    pub source_sha256: String,
    pub entries: Vec<RenameLedgerEntry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<LedgerEdit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<Vec<LedgerStage>>,
    /// sha256 of the text the whole chain must reproduce (the naming
    /// stage's shipped output); None when not pinned.
    #[serde(
        rename = "outputSha256",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub output_sha256: Option<String>,
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
    /// `rename-ledger.json`: compact JSON, every offset a JS string index
    /// (UTF-16 units) into its stage's source — what `apply.mjs` slices.
    pub fn to_json(&self) -> String {
        use humanify_model::js::Utf16Offsets;
        let units = |source: &str, entries: &[RenameLedgerEntry], edits: &[LedgerEdit]| {
            let offsets = Utf16Offsets::new(source);
            let entries = entries
                .iter()
                .map(|e| RenameLedgerEntry {
                    original_name: e.original_name.clone(),
                    final_name: e.final_name.clone(),
                    occurrences: e
                        .occurrences
                        .iter()
                        .map(|o| Occurrence {
                            start: offsets.at(o.start),
                            end: offsets.at(o.end),
                            text: o.text.clone(),
                        })
                        .collect(),
                })
                .collect();
            let edits = edits
                .iter()
                .map(|LedgerEdit(s, e, t)| LedgerEdit(offsets.at(*s), offsets.at(*e), t.clone()))
                .collect();
            (entries, edits)
        };
        let (entries, edits) = units(&self.source, &self.ledger.entries, &self.ledger.edits);
        let post = self.ledger.post.as_ref().map(|stages| {
            stages
                .iter()
                .zip(&self.stage_sources)
                .map(|(s, src)| {
                    let (entries, edits) = units(src, &s.entries, &s.edits);
                    LedgerStage {
                        source_sha256: s.source_sha256.clone(),
                        entries,
                        edits,
                    }
                })
                .collect()
        });
        serde_json::to_string(&RenameLedger {
            version: self.ledger.version,
            source_sha256: self.ledger.source_sha256.clone(),
            entries,
            edits,
            post,
            output_sha256: self.ledger.output_sha256.clone(),
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
    /// Two edits overlap (an internal inconsistency).
    Overlap,
    /// An edit leaves the text or cuts a character.
    OutOfRange,
    /// The chain ran but did not reach the pinned output.
    OutputMismatch,
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LedgerError::SourceMismatch => {
                "rename ledger: source does not match the ledger's sourceSha256"
            }
            LedgerError::Overlap => "rename ledger: overlapping edits (internal)",
            LedgerError::OutOfRange => "rename ledger: an edit leaves its text (internal)",
            LedgerError::OutputMismatch => {
                "rename ledger: the replay does not reach the ledger's outputSha256"
            }
        })
    }
}

/// sha256 of a text, lowercase hex.
pub fn sha256_hex(text: &str) -> String {
    let digest = Sha256::digest(text.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Derive one stage from the source snapshot the state was built over,
/// the state's final names, and the edit list the render applied to that
/// snapshot. Entries: bindings in scope pre-order (each scope's bindings
/// in table order), one per declaration, listing the occurrences it
/// rewrote; `edits`: every rendered edit no binding occurrence owns.
pub fn build_rename_ledger(
    source: &str,
    state: &RenameState,
    rendered: &[Replacement],
) -> RenameLedger {
    let mut by_span: HashMap<(u32, u32), usize> = HashMap::with_capacity(rendered.len());
    for (i, r) in rendered.iter().enumerate() {
        by_span.entry((r.span.start, r.span.end)).or_insert(i);
    }
    let mut owned = vec![false; rendered.len()];
    let view = state.view();
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut entries = Vec::new();
    for scope in 0..view.scopes.len() as u32 {
        for (_, binding) in state.bindings_in(BScopeId(scope)) {
            let b = view.binding(binding);
            if !seen.insert(b.id_span.start) {
                continue;
            }
            let final_name = state.name_of(binding);
            let spans = std::iter::once(b.id_span)
                .chain(
                    b.refs
                        .iter()
                        .filter(|r| r.ty == SiteType::Identifier)
                        .map(|r| r.span),
                )
                .chain(b.violation_targets.iter().flatten().copied());
            let mut occurrences = Vec::new();
            for span in spans {
                let Some(&i) = by_span.get(&(span.start, span.end)) else {
                    continue;
                };
                if std::mem::replace(&mut owned[i], true) {
                    continue;
                }
                let text = &rendered[i].text;
                occurrences.push(Occurrence {
                    start: span.start,
                    end: span.end,
                    text: (text != final_name).then(|| text.clone()),
                });
            }
            if occurrences.is_empty() {
                continue;
            }
            entries.push(RenameLedgerEntry {
                original_name: source[b.id_span.start as usize..b.id_span.end as usize].to_string(),
                final_name: final_name.to_string(),
                occurrences,
            });
        }
    }
    let edits = rendered
        .iter()
        .zip(&owned)
        .filter(|(_, owned)| !**owned)
        .map(|(r, _)| LedgerEdit(r.span.start, r.span.end, r.text.clone()))
        .collect();
    RenameLedger {
        version: RENAME_LEDGER_VERSION,
        source_sha256: sha256_hex(source),
        entries,
        edits,
        post: None,
        output_sha256: None,
    }
}

/// Apply one stage to `source`, verifying the snapshot hash first: every
/// edit in ascending order, the output assembled from the source slices
/// between them (linear in text + edits).
fn apply_stage(
    source: &str,
    sha: &str,
    entries: &[RenameLedgerEntry],
    edits: &[LedgerEdit],
) -> Result<String, LedgerError> {
    if sha256_hex(source) != sha {
        return Err(LedgerError::SourceMismatch);
    }
    let mut all: Vec<(u32, u32, &str)> = entries
        .iter()
        .flat_map(|e| {
            e.occurrences.iter().map(|o| {
                (
                    o.start,
                    o.end,
                    o.text.as_deref().unwrap_or(e.final_name.as_str()),
                )
            })
        })
        .chain(edits.iter().map(|LedgerEdit(s, e, t)| (*s, *e, t.as_str())))
        .collect();
    all.sort_by_key(|&(s, e, _)| (s, e));
    let mut out = String::with_capacity(source.len() + source.len() / 4);
    let mut pos = 0usize;
    for (start, end, text) in all {
        let (start, end) = (start as usize, end as usize);
        if start < pos {
            return Err(LedgerError::Overlap);
        }
        if end < start || source.get(start..end).is_none() {
            return Err(LedgerError::OutOfRange);
        }
        out.push_str(&source[pos..start]);
        out.push_str(text);
        pos = end;
    }
    out.push_str(&source[pos..]);
    Ok(out)
}

/// Replay a ledger onto its source snapshot: the base stage, then each
/// post stage over the previous stage's output, then the pinned output
/// hash (when present).
pub fn apply_rename_ledger(source: &str, ledger: &RenameLedger) -> Result<String, LedgerError> {
    let mut out = apply_stage(
        source,
        &ledger.source_sha256,
        &ledger.entries,
        &ledger.edits,
    )?;
    for stage in ledger.post.iter().flatten() {
        out = apply_stage(&out, &stage.source_sha256, &stage.entries, &stage.edits)?;
    }
    match &ledger.output_sha256 {
        Some(sha) if sha256_hex(&out) != *sha => Err(LedgerError::OutputMismatch),
        _ => Ok(out),
    }
}

#[cfg(test)]
mod ledger_test;
