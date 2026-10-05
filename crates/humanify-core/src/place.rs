//! Placement (WP5.1/WP5.2): which FILE every wrapper-body statement of the
//! shipped bundle lands in — TS `src/split/stable-split.ts` (the placement
//! half), `placement-trail.ts`, `content-anchor.ts`, `layout.ts`, and the
//! grouping/assignment strategies under [`assign`] (`fossil-assign.ts`,
//! `fossil-match.ts`, `cluster-assign.ts`, `split-namer.ts`).
//!
//! Three regimes, chosen from what the bundle CONTAINS by [`method`]
//! (finding #87 — it used to be from which bundler wrote it):
//!
//! - FOSSIL (the toolchain offers module markers AND they cover ≥99% of the
//!   app code — Claude Code's shape): the bundle's own `__esm` module
//!   segments are the files ([`assign::fossil`]);
//! - PRIOR-CARRIED (a prior ledger, otherwise): the evidence ladder
//!   `PLACEMENT_TIERS` ([`tiers`]);
//! - FRESH (neither): the seam-clustered grouping ([`assign::cluster`]),
//!   sized to the app, keeping any lazy module's end as a file boundary.
//!
//! Substrate: the statements' ESTree JSON (the inventory's retained values)
//! plus their spans into the shipped text — the same subtrees the statement
//! hashes were computed from. Decision code is sequential, in the TS's
//! iteration order; every JS `Map`/`Set` whose order a read can observe is a
//! `Vec` in insertion order here (lesson 4).

pub mod anchor;
pub mod assign;
pub mod babel_walk;
pub mod declared;
pub mod input;
pub mod layout;
pub mod ledger;
pub mod method;
pub mod placement_dump;
pub mod stems;
pub mod tiers;
pub mod trail;

#[cfg(test)]
mod split_calls_test;
