//! The library-matching scorer (version 16) and its instruments: the
//! truth-layer builders, the corpus build and freeze, the npm caches. Its
//! records are plain JSON (`crate::json::text`); a recorded identity is the
//! sha of the frozen identity spelling (`crate::json::frozen`).

pub mod acorn;
pub mod audit15;
#[rustfmt::skip]
mod acorn_tables;
pub mod chained;
pub mod copies2;
pub mod copy_credit;
pub mod corpus;
pub mod corpus_build;
pub mod counter;
pub mod decoder;
pub mod defect15;
pub mod determinism;
pub mod families;
pub mod freeze;
pub mod gz;
pub mod index;
pub mod layers;
pub mod leaderboard;
pub mod mapdefect14;
pub mod misattr13;
pub mod npm;
pub mod origins;
mod pyclass_table;
pub mod pypath;
pub mod pyre;
pub mod score;
pub mod tokens14;
pub mod truth;
pub mod value;
pub mod vendor_precision;
pub mod wrong_families;
