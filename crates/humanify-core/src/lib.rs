//! The pipeline library: every stage from detect through emit
//! (docs/rust-port/02-rust-target-architecture.md §2). Receives switches as
//! config and never reads the process environment.
#![forbid(unsafe_code)]
