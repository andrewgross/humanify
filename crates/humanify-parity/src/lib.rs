//! The dump comparer (docs/rust-port/07-differential-validation.md §4):
//! `compare`, `selftest`. Depends on humanify-model only.
//! Deliberately the FIRST Rust code written (WP0.3).

pub mod engine;
pub mod selftest;

#[cfg(test)]
mod engine_test;
