//! Pure data: version-record tables, dump schemas, hash newtypes, the
//! `NameProvider` trait. No oxc, no rayon, no tokio — ever
//! (docs/rust-port/02-rust-target-architecture.md §2).
//!
//! WP0.3 lands the dump schemas here first so that `humanify-parity` can
//! build against TS dumps before any pipeline Rust exists.

pub mod detection;
pub mod dump;
pub mod hash_keys;
pub mod js;
pub mod llm;
pub mod profiling;
pub use dump::*;

#[cfg(test)]
mod js_test;
