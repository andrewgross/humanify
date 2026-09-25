//! The byte-exact REVIEW tree and the fresh ledger's layout — the
//! post-placement half of TS `stableSplitFromCode` (stable-split.ts):
//! `statementAlignName`, `alignEmissionOrder` over the bundle,
//! `emitFiles`, `buildLedger`'s layout fields.
//!
//! The review tree is what `--split-pure` writes and what the runnable
//! emit falls back to when it declines; its emitted layout (`emitHashes`,
//! `emitNames`) is also what the runnable emit re-aligns to.

use humanify_model::js::cmp_utf16;

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
