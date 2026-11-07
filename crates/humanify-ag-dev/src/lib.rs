//! `humanify-ag-dev`: the measurement harness as one Rust crate
//! (docs/dev-harness.md). Lane L1 is the skeleton every later lane plugs
//! into: the verb table (main.rs), the settings registry (`settings`), the
//! committed-file loaders (`committed`), the run identity and its one
//! comparison guard (`identity`), the one environment read (`env`), the
//! one subprocess helper (`proc`), the pipeline-binary builder
//! (`bin_build`) and the two JSON spellings of the legacy records (`json`).

pub mod bin_build;
pub mod census;
pub mod committed;
pub mod e2e;
pub mod env;
pub mod gate;
pub mod identity;
pub mod json;
pub mod match_truth;
pub mod paths;
pub mod proc;
pub mod settings;

/// What a verb whose L4 sub-lane has not landed yet answers.
pub fn under_construction(verb: &str) -> (u8, String, String) {
    (
        2,
        String::new(),
        format!("humanify-ag-dev {verb}: under construction (lane L4)\n"),
    )
}
