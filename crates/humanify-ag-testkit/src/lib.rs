//! Test support for every crate in the workspace (docs/dev-harness.md §6),
//! a DEV-dependency only, never linked into a product binary:
//!
//! - [`stub_server`]: the ONE OpenAI-compatible stub LLM server — an
//!   in-process HTTP/1.1 keep-alive server on 127.0.0.1 that records every
//!   request and the peak number in flight, and answers from a handler;
//! - [`answers`]: the deterministic answer policies (`<id>Renamed` for
//!   every asked identifier, module-file names, the collision-forcing
//!   policy), derived from the request bytes alone;
//! - [`fake`]: the seeded-shuffled fake name provider, whose
//!   `run_pipelined` completes calls in a seeded random order the way the
//!   real provider completes them out of order.
//!
//! Porting the seven hand-written stub servers and the 27 in-order fake
//! providers onto this crate is lane L4. It depends on humanify-ag-model
//! alone, never on core, so core can dev-depend on it without a second
//! copy of core's types.

pub mod answers;
pub mod fake;
pub mod stub_server;
