//! The split's input: the shipped text's wrapper-body statements — TS
//! `stableSplitFromCode`'s prologue (parse, `findWrapperFunction`, the body
//! block, `body.length < 2 → null`, `body.map(statementHash)`).
//!
//! The statements come from the SAME inventory the twins gate uses
//! ([`crate::twins::statement_inventory_from_json`]) with the wrapper gate
//! REQUIRED: the TS returns null (and the pipeline fails loud) when the
//! code is not one wrapper IIFE, so there is no program-body fallback here.

use oxc_allocator::Allocator;
use serde_json::Value;

use crate::ingest::{Ingest, program_estree_json};
use crate::modules::wrapper::find_wrapper_function;
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
/// program's, each sliced from the text.
pub fn top_level_statement_texts(text: &str) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "prior.js");
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc failed to parse the prior text: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic());
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
/// null: a parse failure, no wrapper, or fewer than two statements.
pub fn split_input(text: &str) -> Result<SplitInput, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "shipped.js");
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc failed to parse the shipped text: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic())
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
