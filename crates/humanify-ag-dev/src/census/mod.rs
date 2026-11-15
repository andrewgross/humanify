//! The censuses: concurrency and effects (pass/fail), clones, slop and
//! comments (advisory).

pub mod allowlist;
pub mod clones;
pub mod comments;
pub mod concurrency;
pub mod effects;
pub mod rust_fns;
pub mod slop;

#[cfg(test)]
mod planted_test;
