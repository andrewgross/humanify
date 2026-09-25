//! The phase-3 gate's dump (WP3.2): run the match stage and the transfer
//! pipeline over a TS dump's two texts and write what the TS writes at
//! the mechanical-stage boundary — `transfers-mechanical.json` (the
//! strategy trail: every applied AND rejected rename with tier, outcome,
//! reason and proposed name, attempt order included) and `votes.json`
//! (the vote tallies with witnesses + the ladder outcome). Migration
//! scaffolding — deleted at phase 6 with the TS core (02 §9).
//!
//! Also written, for bisection: `twin-gates.json` — the statement twins
//! gated with the transfer run's own lifecycle inputs (the `twins` section
//! compares it once a `twins.json` sits beside it).

use std::fs;
use std::path::Path;

use serde_json::json;

use crate::libdetect::function_carry::LibraryClassification;
use crate::matching::matches_dump::read_dump_texts;
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::twins::gates::gate_dump;

/// What the gate dump reports.
pub struct TransferDumpSummary {
    pub rows: usize,
    pub votes: usize,
}

/// Run parse → graph → matching → the transfer pipeline on a TS dump's
/// texts and write the phase-3 gate's files into `out_dir`.
pub fn dump_transfers(ts_dump_dir: &Path, out_dir: &Path) -> Result<TransferDumpSummary, String> {
    let (meta, fresh, prior) = read_dump_texts(ts_dump_dir)?;
    let flags = &meta["flags"];
    // The library freeze the TS applied (regions.json libraryFunctions).
    let library = LibraryClassification::from_dump_dir(ts_dump_dir)?;
    let skip_libraries = flags["skipLibraries"].as_bool().unwrap_or(true);
    let input = PriorMatchInput {
        fresh: &fresh,
        prior: &prior,
        bundler: flags["bundler"].as_str(),
        minifier: flags["minifier"].as_str(),
        visit_optional_calls: false,
    };
    match_prior_version(input, |stage| {
        let freeze = super::library_freeze(stage, library.as_ref(), skip_libraries)?;
        let (outcome, twin_output) = super::apply_prior_version(stage, &freeze)?;
        fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
        let write = |file: &str, value: &serde_json::Value| {
            fs::write(out_dir.join(file), serde_json::to_string(value).unwrap())
                .map_err(|e| format!("write {file}: {e}"))
        };
        write("meta.json", &meta)?;
        let rows = outcome.rename.trail().transfer_rows();
        write(
            "transfers-mechanical.json",
            &json!({"schemaVersion": 1, "transfers": rows}),
        )?;
        let votes =
            crate::rename::votes::dump::vote_rows(&outcome.votes_dump, outcome.rename.trail());
        write("votes.json", &json!({"schemaVersion": 1, "votes": votes}))?;
        let mut gates = gate_dump(
            &twin_output,
            &stage.prior.gate_side(),
            &stage.fresh.gate_side(),
        );
        if let Some(obj) = gates.as_object_mut() {
            obj.insert("schemaVersion".into(), json!(1));
        }
        write("twin-gates.json", &gates)?;
        Ok(TransferDumpSummary {
            rows: rows.len(),
            votes: votes.len(),
        })
    })
}
