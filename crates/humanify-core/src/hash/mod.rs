//! The hash families (WP1.3): the canonical serialization's three consumers
//! — `MatchKey` (blurred literals; cross-version matching), `IdentityKey`
//! (verbatim literals; the declaration-body hash), and the STATEMENT hash
//! (the split's rename-invariant statement identity, its own masked walk).
//! TS originals: analysis/structural-hash.ts, split/statement-hash.ts,
//! analysis/enclosing-statement.ts.

pub mod serialize;
pub mod statement_hash;

#[cfg(test)]
mod statement_hash_test;
