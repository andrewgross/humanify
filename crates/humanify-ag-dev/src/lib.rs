//! `humanify-ag-dev`: the measurement harness as one Rust crate
//! (docs/dev-harness.md): the verb table (`cli`), the settings registry
//! (`settings`), the committed-file loaders (`committed`), the run identity
//! and its one comparison guard (`identity`), the one environment read
//! (`env`), the one subprocess helper (`proc`), the pipeline-binary builder
//! (`bin_build`) and the JSON spellings of the records (`json`).
//!
//! Only `cli` and `env` (main reads the command line through it) are
//! public: every other module is crate-private, so rustc's dead-code lint
//! sees every item nothing reaches.

pub(crate) mod bin_build;
pub(crate) mod census;
pub mod cli;
pub(crate) mod committed;
pub(crate) mod difflabel;
pub(crate) mod e2e;
pub mod env;
pub(crate) mod eval;
pub(crate) mod gate;
pub(crate) mod identity;
pub(crate) mod json;
pub(crate) mod launch;
pub(crate) mod library;
pub(crate) mod matchcard;
pub(crate) mod paths;
pub(crate) mod proc;
pub(crate) mod runners;
pub(crate) mod settings;

/// The host environment a verb receives (read once, in `env`).
pub(crate) use env::HostEnv;
