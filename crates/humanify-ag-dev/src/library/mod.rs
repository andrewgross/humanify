//! The exp096 library-matching scorer (scorer 14) and its instruments,
//! ported from experiments/096-library-matching-score and the exp095
//! "misses" scripts it imports (docs/dev-harness.md §1.4, lane L2b). The
//! cards are byte-identical to score.py's: Python's float repr, sorted
//! keys, `ensure_ascii`, banker's rounding and summation order are
//! reproduced, not approximated.

pub mod chained;
pub mod cjs_placement;
pub mod corpus;
pub mod counter;
pub mod decoder;
pub mod determinism;
pub mod families;
pub mod gz;
pub mod index;
pub mod layers;
pub mod leaderboard;
pub mod origins;
pub mod py;
mod pyclass_table;
pub mod pypath;
pub mod pyre;
pub mod score;
pub mod truth;
pub mod vendor_precision;
