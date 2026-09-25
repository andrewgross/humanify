//! The naming stage's post-wave passes (WP4.5), in plugin order (TS
//! `src/rename/plugin.ts` after the waves; WP4.6's driver wires them):
//!
//! 1. the naming FLOOR over the naming-era state — [`floor_passes`]:
//!    class/function-expression inner-id derivation, then decoration retry
//!    (the LLM sweep defers when a prior version is present);
//! 2. `generate` — [`crate::naming::waves::render::render_program`];
//! 3. the prior-diff reconcile — [`crate::naming::reconcile`] (WP4.4);
//! 4. the deferred, prior-aware coverage sweep — [`sweep`];
//! 5. the family permutation over the final text — [`family_permute`];
//! 6. the minted census of the shipped text — [`census`].
//!
//! Each post-generate pass parses ITS input text afresh (a new Babel scope
//! epoch in the TS — every `parseSourceAst` of a bundle clears the traverse
//! cache) and hands on a re-rendered text plus the continued strategy
//! trail. [`dump`] is the gate's verb (migration scaffolding).
//!
//! ## What the driver (`naming::driver`) runs, and when (plugin.ts)
//!
//! - after the waves + library prefix, when `namingFloor`:
//!   [`floor_passes::derive_expression_inner_names`] then
//!   [`floor_passes::retry_decorated_names`] on the naming-era state (one
//!   `collect_eval_with_taint` for both), then — only when the sweep is NOT
//!   deferred — [`sweep::sweep_minted_names`] (anchor `fresh`). The sweep
//!   DEFERS (`isSweepDeferred`) when `namingFloor && namingFloorSweep &&
//!   reconcilePriorDiff && priorVersionCode && !sourceMap`;
//! - `generate`: [`crate::naming::waves::render::render_program_with`] with
//!   the statement twins' [`crate::naming::waves::render::private_rename_edits`];
//! - when `reconcilePriorDiff && prior && !sourceMap && outputValid`:
//!   [`crate::naming::reconcile::step::run_prior_diff_reconciliation`]
//!   (an Err is "did not run": ship the generated text);
//! - when deferred and `outputValid`: [`sweep::run_deferred_sweep`] over the
//!   reconciled text (anchor `reconciled`) else the generated one;
//! - unless `--disable family-permute`, when `reconcilePriorDiff && prior &&
//!   !sourceMap && !emitRenameLedger && outputValid`:
//!   [`family_permute::run_family_permute`] over the last text produced;
//! - always: [`census_of_text`] of the shipped text (`coverage.mintedCensus`).
//!
//! `namingFloor` stats: derived = class-id applies; undecorated = decoration
//! applies; swept = pre-generate + deferred sweep applies; skipped =
//! class-id skips + decoration skips + both sweeps' skips.

pub mod census;
pub mod dump;
pub mod family_permute;
pub mod floor_passes;
pub mod sweep;

use oxc_allocator::Allocator;

use crate::ingest::Ingest;
use crate::rename::eligibility::Eligibility;
use crate::rename::validated::RenameState;
use crate::trail::{Anchor, StrategyTrail};
use census::{MintedCensus, collect_free_references, collect_minted_bindings, summarize_census};

/// The end-of-run census of the shipped text (plugin.ts: after the
/// permute's traverse-cache clear, so the walk sees a FRESH crawl — scope
/// maps in registration order under the final names, as a new parse).
pub fn census_of_text(text: &str, eligible: &Eligibility) -> Result<MintedCensus, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    if !ingest.errors.is_empty() {
        return Err(format!("shipped text does not parse: {}", ingest.errors[0]));
    }
    let semantic = ingest.semantic();
    let state = RenameState::with_trail(semantic, Anchor::Shipped, StrategyTrail::default());
    let walk = collect_minted_bindings(semantic, &state, eligible);
    Ok(summarize_census(
        &walk.entries,
        walk.total_bindings,
        collect_free_references(&state),
    ))
}

#[cfg(test)]
mod passes_test;
