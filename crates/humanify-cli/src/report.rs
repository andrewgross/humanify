//! The run's `ERROR:` blocks and closing reports (TS: unified.ts
//! reportParseFailures / reportSemanticFailures / reportInternalErrors /
//! reportVendorNaming).
//!
//! The `ERROR:` blocks are harness contract (14 §2): `run.sh` records each
//! headline plus its indented continuation lines into `-run-status.json`
//! (experiments/lib/invariants.ts extractErrorBlocks). Each function
//! returns the renderer messages in order and whether the run is marked
//! failed (the TS `process.exitCode = 1`); the byte text is gated against
//! the TS functions (test/parity/wpb4-vectors.json `errorBlocks`).
//!
//! Known TS finding reproduced, not fixed (WPB.4 hand-back): a parse
//! failure's code frame marks the failing line with a leading `>`, which is
//! NOT indentation — so the harness's extractor keeps the frame's first
//! context lines, drops the marked line, and ends the block there.

use crate::output_validation::{OutputParseFailure, OutputSemanticFailure};
use humanify_model::stats::VendorNamingStats;

/// `FAILED_OUTPUT_DIR` (failed-output.ts).
pub const FAILED_OUTPUT_DIR: &str = ".humanify/failed";

/// What a report adds to the run: messages (in order) and whether it marks
/// the run failed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub messages: Vec<String>,
    pub fails_run: bool,
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}

/// `reportParseFailures`.
pub fn report_parse_failures(failures: &[(String, OutputParseFailure)]) -> Report {
    if failures.is_empty() {
        return Report::default();
    }
    let mut messages: Vec<String> = failures
        .iter()
        .map(|(file, f)| {
            let location = match (f.line, f.column) {
                (Some(l), Some(c)) => format!(" (line {l}, column {c})"),
                (Some(l), None) => format!(" (line {l})"),
                _ => String::new(),
            };
            let excerpt = match &f.excerpt {
                Some(e) if !e.is_empty() => format!("\n{e}"),
                _ => String::new(),
            };
            format!(
                "ERROR: Generated output for {file} is not valid JavaScript{location}: {}{excerpt}",
                f.message
            )
        })
        .collect();
    messages.push(format!(
        "ERROR: {} output file{} failed to parse — output was written for inspection, but this run is marked failed.",
        failures.len(),
        plural(failures.len())
    ));
    Report {
        messages,
        fails_run: true,
    }
}

/// `reportSemanticFailures`.
pub fn report_semantic_failures(failures: &[(String, OutputSemanticFailure)]) -> Report {
    if failures.is_empty() {
        return Report::default();
    }
    let mut messages: Vec<String> = failures
        .iter()
        .map(|(file, f)| format!("ERROR: {file}: {}", f.message))
        .collect();
    messages.push(format!(
        "ERROR: {} output file{} violated rename invariants — the rejected file(s) and their pre-rename sources are preserved under {FAILED_OUTPUT_DIR}/; this run is marked failed.",
        failures.len(),
        plural(failures.len())
    ));
    Report {
        messages,
        fails_run: true,
    }
}

/// `reportInternalErrors`.
pub fn report_internal_errors(internal_errors: usize) -> Report {
    if internal_errors == 0 {
        return Report::default();
    }
    Report {
        messages: vec![format!(
            "ERROR: {internal_errors} function{} hit an internal error during renaming (see debug log) — output was written, but this run is marked failed.",
            plural(internal_errors)
        )],
        fails_run: true,
    }
}

/// `reportVendorNaming`: silent when the namer never ran.
pub fn report_vendor_naming(stats: &VendorNamingStats) -> Report {
    if !stats.attempted() {
        return Report::default();
    }
    let n = humanify_model::js::number_to_string;
    let mut parts = vec![format!("{} named", n(stats.named))];
    if stats.declined > 0.0 {
        parts.push(format!("{} declined", n(stats.declined)));
    }
    if stats.echoed > 0.0 {
        parts.push(format!("{} echoed the key", n(stats.echoed)));
    }
    if stats.batches_failed > 0.0 {
        parts.push(format!("{} batch(es) failed", n(stats.batches_failed)));
    }
    Report {
        messages: vec![format!("Vendor naming: {}", parts.join(", "))],
        fails_run: false,
    }
}
