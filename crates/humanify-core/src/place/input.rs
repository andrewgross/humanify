//! The split's input: the shipped text's wrapper-body statements — TS
//! `stableSplitFromCode`'s prologue (parse, `findWrapperFunction`, the body
//! block, `body.length < 2 → null`, `body.map(statementHash)`).
//!
//! The statements come from the SAME inventory the twins gate uses
//! ([`crate::twins::statement_inventory_from_json`]) with the wrapper gate
//! REQUIRED: the TS returns null (and the pipeline fails loud) when the
//! code is not one wrapper IIFE, so there is no program-body fallback here.
//!
//! The ≥50-binding half of the gate reads the text at hand by default
//! ([`split_input`]); a split that knows the run's ORIGINAL input bundle
//! reads it there instead ([`split_input_with_original_bundle`]) — being a
//! bundled app is a property of the INPUT, and the vendor extraction
//! shrinking the runtime it hands downstream is expected. The GRAMMAR half
//! always reads the text at hand.

use oxc_allocator::Allocator;
use serde_json::Value;

use crate::ingest::{Ingest, program_estree_json};
use crate::toolchain::BundleLayout;
use crate::twins::statement_inventory_from_json;

/// The wrapper body, ready for placement.
#[derive(Debug, Clone)]
pub struct SplitInput {
    /// Each statement's ESTree JSON subtree, bundle order.
    pub body: Vec<Value>,
    /// Each statement's `[start, end)` in the shipped text (UTF-8 bytes).
    pub spans: Vec<(u32, u32)>,
    /// The rename-invariant statement hash per statement (the Rust bytes;
    /// a gate may substitute the TS's — see `placement_dump`).
    pub hashes: Vec<String>,
}

/// The prior release's top-level statement TEXTS — prior-version.ts's
/// `MatcherCarry.statementTexts` (`topLevelStatements(priorGraph)`): the
/// wrapper body's statements when the wrapper gate passes, else the
/// program's, each sliced from the text. The wrapper is the one the run's
/// bundle `layout` finds.
pub fn top_level_statement_texts(text: &str, layout: BundleLayout) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "prior.js");
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc failed to parse the prior text: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }
    let wrapper = layout.find_wrapper(ingest.program, ingest.semantic());
    let program_json = program_estree_json(ingest.program);
    let (inventory, _) = statement_inventory_from_json(
        &program_json,
        wrapper.map(|w| w.body_span),
        "prior",
        None,
        false,
    )?;
    Ok(inventory
        .statements
        .iter()
        .map(|s| text[s.span.start as usize..s.span.end as usize].to_string())
        .collect())
}

/// Parse `text` and take its wrapper body. `Err` wherever the TS returns
/// null: a parse failure, no wrapper, or fewer than two statements. The
/// ≥50 wrapper-binding threshold is measured on `text` itself — the right
/// default for every text that has no recorded ORIGINAL (prior releases,
/// the standalone owners, the tests). The wrapper is the one the run's
/// bundle `layout` finds.
pub fn split_input(text: &str, layout: BundleLayout) -> Result<SplitInput, String> {
    split_input_with_original_bundle(text, None, layout)
}

/// [`split_input`] with the ≥50 threshold read from the run's ORIGINAL
/// input bundle instead of the text at hand. `original_binding_count` is
/// [`BundleLayout::original_bundle_binding_count`]'s verdict on
/// the ORIGINAL text (what the unpack stage saw): `Some(_)` means the
/// input already cleared the frozen gate, and the text at hand only has
/// to be one wrapper IIFE by GRAMMAR — the vendor extraction removed one
/// wrapper-scope binding per vendored module from it, which is expected,
/// not evidence against being a bundle. `None` falls back to measuring
/// the text at hand (the historical gate).
///
/// The bound check is on the ORIGINAL's count, never the local one: a
/// mid-size app whose vendor half dominates lands under the threshold on
/// its runtime (32 bindings on the esbuild lane's real test app) while
/// its input clears it comfortably.
pub fn split_input_with_original_bundle(
    text: &str,
    original_binding_count: Option<usize>,
    layout: BundleLayout,
) -> Result<SplitInput, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "shipped.js");
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc failed to parse the shipped text: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }
    let wrapper = match original_binding_count {
        // The input settled "is this really a bundled app?"; the text at
        // hand only owes the GRAMMAR (the tight WP1.5 recognition — a
        // non-bundle stays unsplittable however bundled its input was).
        Some(_) => layout.recognize_wrapper(ingest.program, ingest.semantic()),
        None => layout.find_wrapper(ingest.program, ingest.semantic()),
    }
    .ok_or("not stable-splittable: no recognizable bundle wrapper")?;
    let program_json = program_estree_json(ingest.program);
    let (inventory, body) = statement_inventory_from_json(
        &program_json,
        Some(wrapper.body_span),
        "shipped",
        None,
        true,
    )?;
    if body.len() < 2 {
        return Err("not stable-splittable: fewer than two wrapper statements".into());
    }
    Ok(SplitInput {
        body,
        spans: inventory
            .statements
            .iter()
            .map(|s| (s.span.start, s.span.end))
            .collect(),
        hashes: inventory.statements.into_iter().map(|s| s.hash).collect(),
    })
}
