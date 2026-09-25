//! Finish (WP5.4): the passes that run AFTER the split tree is on disk —
//! TS `src/split/bun-relink.ts`, `vendor-body-inherit.ts`,
//! `using-desugar.ts`, `runnable-scaffold.ts`, `post-split-reconcile.ts`
//! (its TEXT form, 12 §5), `bundle-carry.ts`, and the driver half of
//! `src/commands/unified.ts` (`finishSplitOutput`, `reconcilePostSplit`,
//! `carryIntoBundle`).
//!
//! Order is the TS's, and it is a decision input (exp054): the Bun re-link
//! and the `using` desugar are the last passes to rewrite `src/`, so the
//! reconcile runs on the FINAL text, against a prior tree that went
//! through the same passes.
//!
//! - [`relink`] — re-bind extracted Bun factories into a runnable graph;
//! - [`vendor_inherit`] — reuse the prior release's vendor bytes when the
//!   file is the same program;
//! - [`using`] — compile `using` / `await using` away (the Babel plugin's
//!   output bytes: [`using::transform`] + [`using::generator`]);
//! - [`scaffold`] — `run.cjs`, `package.json`, `RUNNABLE.md`;
//! - [`reconcile`] — the post-split prior-diff reconcile (per file);
//! - [`carry`] — the reconcile's inner-local renames into the bundle;
//! - [`driver`] — the stage in the TS's order over a tree on disk.

pub mod driver;
pub mod relink;
pub mod scaffold;
pub mod using;
pub mod vendor_inherit;
