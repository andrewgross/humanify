//! Placement (WP5.1/WP5.2): which FILE every wrapper-body statement of the
//! shipped bundle lands in — TS `src/split/stable-split.ts` (the placement
//! half), `placement-trail.ts`, `content-anchor.ts`, `layout.ts`, and the
//! grouping/assignment strategies under [`assign`] (`fossil-assign.ts`,
//! `fossil-match.ts`, `cluster-assign.ts`, `split-namer.ts`).
//!
//! Three regimes, chosen exactly as `stableSplitFromCode` chooses them:
//!
//! - FOSSIL (the adapter provides module fossils — every Bun bundle): the
//!   bundle's own `__esm` module segments are the files ([`assign::fossil`]);
//! - PRIOR-CARRIED (a prior ledger, no fossils): the evidence ladder
//!   `PLACEMENT_TIERS` ([`tiers`]);
//! - FRESH (neither): the seam-clustered grouping ([`assign::cluster`]).
//!
//! Substrate: the statements' ESTree JSON (the inventory's retained values)
//! plus their spans into the shipped text — the same subtrees the statement
//! hashes were computed from. Decision code is sequential, in the TS's
//! iteration order; every JS `Map`/`Set` whose order a read can observe is a
//! `Vec` in insertion order here (lesson 4).

pub mod assign;
pub mod input;
pub mod ledger;
pub mod placement_dump;
pub mod stems;
pub mod trail;
