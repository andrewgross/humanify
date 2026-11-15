//! The byte-exact REVIEW tree and the fresh ledger's layout — the
//! post-placement half of TS `stableSplitFromCode` (stable-split.ts):
//! `statementAlignName`, `alignEmissionOrder` over the bundle,
//! `emitFiles`, `buildLedger`'s layout fields.
//!
//! The review tree is what `--split-pure` writes and what the runnable
//! emit falls back to when it declines; its emitted layout (`emitHashes`,
//! `emitNames`) is also what the runnable emit re-aligns to.

use humanify_ag_model::js::cmp_utf16;

use crate::place::ledger::StableSplitLedger;

use super::align::{AlignSwitches, align_emission_order};
use super::load_order::LoadOrderFacts;

/// `statementAlignName`: every name the statement binds (babel
/// `getBindingIdentifiers`, sorted by UTF-16 code units) joined by `,`, or
/// None when it binds nothing.
pub fn statement_align_name(mut declared: Vec<String>) -> Option<String> {
    if declared.is_empty() {
        return None;
    }
    declared.sort_by(|a, b| cmp_utf16(a, b));
    Some(declared.join(","))
}

/// What the review split decided.
pub struct ReviewSplit {
    /// `perm[slot]` = the bundle statement emitted at that slot.
    pub perm: Vec<usize>,
    /// `ledger.emitHashes` / `emitNames`, per slot.
    pub emit_hashes: Vec<String>,
    pub emit_names: Vec<Option<String>>,
    /// `ledger.files`: the distinct files, sorted by UTF-16 code units.
    pub files: Vec<String>,
    /// The review tree: path → content, in first-slot order.
    pub contents: Vec<(String, String)>,
    /// Per file (first-slot order), the bundle indexes in slot order —
    /// the review emit's layout capture.
    pub layout: Vec<(String, Vec<usize>)>,
}

/// The review split over the placed bundle.
#[allow(clippy::too_many_arguments)]
pub fn review_split(
    code: &str,
    spans: &[(u32, u32)],
    assignment: &[String],
    hashes: &[String],
    names: &[Option<String>],
    facts: &[LoadOrderFacts],
    prior: Option<&StableSplitLedger>,
    switches: AlignSwitches,
) -> ReviewSplit {
    let perm = align_emission_order(assignment, hashes, facts, prior, Some(names), switches);
    let emit_hashes: Vec<String> = perm.iter().map(|&i| hashes[i].clone()).collect();
    let emit_names: Vec<Option<String>> = perm.iter().map(|&i| names[i].clone()).collect();
    // `emitFiles`: slot i holds body[perm[i]] and belongs to assignment[i].
    let mut layout: Vec<(String, Vec<usize>)> = Vec::new();
    let mut at: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (slot, file) in assignment.iter().enumerate() {
        let k = *at.entry(file.as_str()).or_insert_with(|| {
            layout.push((file.clone(), Vec::new()));
            layout.len() - 1
        });
        layout[k].1.push(perm[slot]);
    }
    let contents = layout
        .iter()
        .map(|(file, idxs)| {
            let parts: Vec<&str> = idxs
                .iter()
                .map(|&i| &code[spans[i].0 as usize..spans[i].1 as usize])
                .collect();
            (file.clone(), format!("{}\n", parts.join("\n")))
        })
        .collect();
    let mut files: Vec<String> = layout.iter().map(|(f, _)| f.clone()).collect();
    files.sort_by(|a, b| cmp_utf16(a, b));
    ReviewSplit {
        perm,
        emit_hashes,
        emit_names,
        files,
        contents,
        layout,
    }
}

/// `fileStatementSlices`: one emitted file re-parsed and byte-sliced back
/// into its statement texts — the program's directives and body merged in
/// source order (a leading bare string re-parses as a directive).
fn file_statement_slices(content: &str) -> Vec<&str> {
    use oxc_span::GetSpan;
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, content, "file.js");
    let mut spans: Vec<(u32, u32)> = ingest
        .program
        .directives
        .iter()
        .map(|d| (d.span.start, d.span.end))
        .chain(ingest.program.body.iter().map(|s| {
            let sp = s.span();
            (sp.start, sp.end)
        }))
        .collect();
    spans.sort_by_key(|s| s.0);
    spans
        .into_iter()
        .map(|(s, e)| &content[s as usize..e as usize])
        .collect()
}

/// Why the review tree does not reconstruct the bundle's statements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReconstructError {
    /// The ledger asks `file` for a statement it does not hold.
    Short { file: String, at: usize },
    /// `file` holds statements the ledger does not ask for.
    Beyond { file: String, extra: usize },
    /// The statements differ from the bundle's.
    Mismatch,
}

impl std::fmt::Display for ReconstructError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Short { file, at } => write!(f, "reconstruct: {file} is short of statement {at}"),
            Self::Beyond { file, extra } => write!(
                f,
                "reconstruct: {file} has {extra} statement(s) beyond the ledger"
            ),
            Self::Mismatch => f.write_str(
                "stable split: emitted tree does not reconstruct the source statements (tree/ledger invariant violated)",
            ),
        }
    }
}

/// `assertConcatEquivalence` (+ `reconstructBodyParts`): the review tree,
/// re-sliced per file and replayed through the ledger's `order` (per-file
/// FIFO cursors), must hold EXACTLY the bundle's statements — as a
/// multiset, byte for byte. The Rust cuts the tree from the same spans, so
/// this holds by construction; it is kept because a split that violates it
/// must fail before anything is written, with the TS's message.
pub fn assert_concat_equivalence(
    contents: &[(String, String)],
    order: &[String],
    spans: &[(u32, u32)],
    code: &str,
) -> Result<(), ReconstructError> {
    let parts: Vec<(&str, Vec<&str>)> = contents
        .iter()
        .map(|(file, content)| (file.as_str(), file_statement_slices(content)))
        .collect();
    let index: std::collections::HashMap<&str, usize> = parts
        .iter()
        .enumerate()
        .map(|(i, (f, _))| (*f, i))
        .collect();
    let mut cursor = vec![0usize; parts.len()];
    let mut rebuilt: Vec<&str> = Vec::with_capacity(order.len());
    for file in order {
        let at = index.get(file.as_str()).map_or(0, |&i| cursor[i]);
        let Some(&i) = index.get(file.as_str()).filter(|&&i| at < parts[i].1.len()) else {
            return Err(ReconstructError::Short {
                file: file.clone(),
                at,
            });
        };
        rebuilt.push(parts[i].1[at]);
        cursor[i] += 1;
    }
    for (i, (file, slices)) in parts.iter().enumerate() {
        if cursor[i] != slices.len() {
            return Err(ReconstructError::Beyond {
                file: file.to_string(),
                extra: slices.len() - cursor[i],
            });
        }
    }
    let mut expected: Vec<&str> = spans
        .iter()
        .map(|&(s, e)| &code[s as usize..e as usize])
        .collect();
    // `[...x].sort()`: UTF-16 code-unit order (equality is all that is
    // read, so any total order would do).
    rebuilt.sort_by(|a, b| cmp_utf16(a, b));
    expected.sort_by(|a, b| cmp_utf16(a, b));
    if rebuilt != expected {
        return Err(ReconstructError::Mismatch);
    }
    Ok(())
}

#[cfg(test)]
mod review_test;
