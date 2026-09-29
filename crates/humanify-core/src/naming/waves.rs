//! The LLM naming waves (WP4.3) — TS originals: `src/rename/processor.ts`
//! (processUnified, the wave loop, batching, per-node and wave retries,
//! the straggler pass, free retries, conflict resolution, applying LLM
//! renames through validated rename, the `llm` trail tier),
//! `src/rename/wave-scheduler.ts`, `coverage.ts`. (`wave-profile.ts` is
//! designed out, 2026-09-29: its one -vv line — the wave-structure profile —
//! answered the barrier-scheduling go/no-go and nothing reads it; see
//! PORTING.md's row.)
//!
//! - [`generate`] — `@babel/generator` output for a node of the
//!   beautified text (pretty and compact), under an edit list;
//! - [`nodes`] — each function row's babel node handles (body, params);
//! - [`graph_ext`] — the naming graph: node order, dependencies, callee
//!   insertion order, call sites, module-binding prompt texts;
//! - [`render`] — the printer under the CURRENT names (the rename
//!   overlay's edits);

pub mod batch;
pub mod generate;
pub mod graph_ext;
pub mod jsset;
pub mod nodes;
pub mod processor;
pub mod render;
pub mod used_set;
