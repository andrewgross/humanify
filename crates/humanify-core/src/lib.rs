//! The pipeline library: every stage from detect through emit
//! (docs/rust-port/02-rust-target-architecture.md §2). Receives switches as
//! config and never reads the process environment.
#![forbid(unsafe_code)]

pub mod babel_view;
pub mod graph;
pub mod hash;
pub mod ingest;
pub mod matching;
pub mod modules;
pub mod propagation;
pub mod rename;
pub mod twins;

#[cfg(test)]
mod graph_test;

#[cfg(test)]
mod ingest_test;

#[cfg(test)]
mod matching_test;

#[cfg(test)]
mod modules_test;

#[cfg(test)]
mod propagation_test;

#[cfg(test)]
mod rename_test;

#[cfg(test)]
mod twins_test;
