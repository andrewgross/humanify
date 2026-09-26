//! Emit (WP5.3): the split's per-file EMISSION ORDER and the runnable
//! CommonJS tree — TS `src/split/load-order.ts`, `cjs-emit.ts`,
//! `emitter.ts`, `substitutions.ts`, `shared/bun-helpers.ts`
//! (`identifyBunLazyInit`), plus the emission-order half of
//! `stable-split.ts` (`alignEmissionOrder` and friends, the review tree,
//! `buildLedger`).
//!
//! The technique is the TS's (02 §3, 12 §1): every emitted file is SLICES
//! of the one rendered text, cut by statement span, with span-local edits
//! for the runnable form (cross-file references, redeclarations, the
//! wrapper's module context). Nothing is re-generated, so the slices carry
//! their formatting from the single canonical render.
//!
//! Decision code is sequential in the TS's order; every JS Map/Set whose
//! order a read can observe is a `Vec` in insertion order here (lesson 4).
//!
//! - [`load_order`] — what each top-level statement does while the module
//!   loads, and the constrained greedy topological order (exp038/049);
//! - [`align`] — each file's emission order aligned to the prior layout
//!   (exp037 Lever B, the exp050 (hash, name) key);
//! - [`review`] — the byte-exact review tree + the fresh ledger's layout;
//! - [`cjs`] — the runnable live-binding CommonJS module graph;
//! - [`bun_helpers`] — the Bun lazy-init helper's structural detection;
//! - [`paths`] / [`substitutions`] — the relative-import and positional
//!   text-splice owners the post-split passes share;
//! - [`emit_dump`] — the emit section of the `--dump-artifacts` catalog.

pub mod align;
pub mod bun_helpers;
pub mod cjs;
pub mod emit_dump;
pub mod load_order;
pub mod paths;
pub mod review;
pub mod stable_split;
pub mod substitutions;
