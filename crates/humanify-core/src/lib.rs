//! The pipeline library: every stage from detect through emit
//! (docs/rust-port/02-rust-target-architecture.md §2). Receives switches as
//! config and never reads the process environment.
#![forbid(unsafe_code)]

pub mod babel_view;
pub mod detect;
pub mod emit;
pub mod finish;
pub mod format;
pub mod graph;
pub mod hash;
pub mod ingest;
pub mod libdetect;
pub mod matching;
pub mod modules;
pub mod naming;
pub mod par;
pub mod place;
pub mod prior;
pub mod profiling;
pub mod propagation;
pub mod rename;
pub mod trail;
pub mod twins;
pub mod unpack;

#[cfg(test)]
mod detect_test;

#[cfg(test)]
mod graph_test;

#[cfg(test)]
mod prior_test;

#[cfg(test)]
mod ingest_test;

#[cfg(test)]
mod libdetect_test;

#[cfg(test)]
mod matching_test;

#[cfg(test)]
mod modules_test;

#[cfg(test)]
mod profiling_test;

#[cfg(test)]
mod propagation_test;

#[cfg(test)]
mod rename_test;

#[cfg(test)]
mod twins_test;

#[cfg(test)]
mod unpack_test;
