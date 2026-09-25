//! The LLM naming waves (WP4.3) — TS originals: `src/rename/processor.ts`
//! (processUnified, the wave loop, batching, per-node and wave retries,
//! the straggler pass, free retries, conflict resolution, applying LLM
//! renames through validated rename, the `llm` trail tier),
//! `src/rename/wave-scheduler.ts`, `wave-profile.ts`, `coverage.ts`.
//!
//! - [`generate`] — `@babel/generator` output for a node of the
//!   beautified text (pretty and compact), under an edit list;
//! - [`nodes`] — each function row's babel node handles (body, params);
//! - [`graph_ext`] — the naming graph: node order, dependencies, callee
//!   insertion order, call sites, module-binding prompt texts;
//! - [`render`] — the printer under the CURRENT names (the rename
//!   overlay's edits);
//! - [`probe`] — the naming graph's bisection probe (scaffolding).

pub mod batch;
pub mod dump;
pub mod generate;
pub mod graph_ext;
pub mod jsset;
pub mod nodes;
pub mod probe;
pub mod processor;
pub mod render;
