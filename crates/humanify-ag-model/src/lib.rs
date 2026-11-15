//! Pure data: version-record tables, dump schemas, hash newtypes, the
//! `NameProvider` trait. No oxc, no rayon, no tokio — ever
//! (docs/rust-port/02-rust-target-architecture.md §2).
//!
//! WP0.3 lands the dump schemas here first so that `humanify-parity` can
//! build against TS dumps before any pipeline Rust exists.
// Effects and swallowed errors (see clippy.toml beside this crate): an
// error mapped away with `map_err(|_| ..)` or discarded with `let _ =`
// needs an allow tagged `// default: <reason>` (enforced: census:effects).
#![deny(clippy::map_err_ignore, clippy::let_underscore_must_use)]

pub mod call_ledger;
pub mod detection;
pub mod dump;
pub mod hash_keys;
pub mod js;
pub mod llm;
pub mod numfmt;
pub mod ordered;
pub mod pipeline;
pub mod profiling;
pub mod stats;
pub mod time;
pub use dump::*;

#[cfg(test)]
mod call_ledger_test;
#[cfg(test)]
mod js_test;
#[cfg(test)]
mod llm_test;
#[cfg(test)]
mod numfmt_test;
#[cfg(test)]
mod ordered_test;
#[cfg(test)]
mod stats_test;
#[cfg(test)]
mod time_test;
