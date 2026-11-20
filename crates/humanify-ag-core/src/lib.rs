//! The pipeline library: every stage from detect through emit
//! (docs/rust-port/02-rust-target-architecture.md §2). Receives switches as
//! config and never reads the process environment.
#![forbid(unsafe_code)]
// Effects and swallowed errors (see clippy.toml beside this crate): an
// error mapped away with `map_err(|_| ..)` or discarded with `let _ =`
// needs an allow tagged `// default: <reason>` (enforced: census:effects).
#![deny(clippy::map_err_ignore, clippy::let_underscore_must_use)]

pub mod artifact_dump;
pub mod babel_view;
pub mod cut;
pub mod detect;
pub mod emit;
pub mod error;
pub mod finish;
pub mod format;
pub mod graph;
pub mod hash;
pub mod ingest;
pub mod layout_record;
pub mod libdetect;
pub mod libindex;
pub mod matching;
pub mod modules;
pub mod naming;
pub mod output_tree;
pub mod par;
pub mod place;
pub mod prior;
pub mod prior_tree;
pub mod profiling;
pub mod propagation;
pub mod rename;
pub mod similarity;
pub mod toolchain;
pub mod trail;
pub mod twins;
pub mod unpack;

#[cfg(test)]
mod detect_test;

#[cfg(test)]
mod error_text_pin_test;

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
mod matching_helpers_pin_test;

#[cfg(test)]
mod hash_forms_pin_test;

#[cfg(test)]
mod cut_graph_pin_test;

#[cfg(test)]
mod cut_test;

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
