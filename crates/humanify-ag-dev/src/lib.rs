//! `humanify-ag-dev`: the measurement harness as one Rust crate
//! (docs/dev-harness.md). Lane L1 is the skeleton every later lane plugs
//! into: the verb table (main.rs), the settings registry (`settings`), the
//! committed-file loaders (`committed`), the run identity and its one
//! comparison guard (`identity`), the one environment read (`env`), the
//! one subprocess helper (`proc`), the pipeline-binary builder
//! (`bin_build`) and the two JSON spellings of the legacy records (`json`).

pub mod bin_build;
pub mod committed;
pub mod env;
pub mod identity;
pub mod json;
pub mod paths;
pub mod proc;
pub mod settings;
