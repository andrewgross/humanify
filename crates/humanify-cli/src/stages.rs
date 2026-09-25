//! The pipeline's one unported stage and its NOT-YET stub.
//!
//! Every stage of docs/pipeline-stages.md runs in Rust (M3, phase 5a):
//! 1 detect, 2 select the unpack adapter, 3 unpack, 4 library detection,
//! 5 vendor naming, 7-9 the naming stage (core::naming::driver — graph,
//! match, transfer, waves, passes), 10-12 the split, emit and finish
//! (crate::split_stage). The exception is stage 6, the formatter: through
//! phase 5a the binary ingests the TS-beautified text via the Rust-only
//! `--beautified-input <path>` (00-control §3, 2026-09-19). Without it the
//! run stops at stage 6 with an `ERROR:` block and [`EXIT_NOT_YET`] —
//! never a silent skip; what stages 3-5 wrote stays on disk.

/// The driver's exit code for a run that reached an unported stage:
/// distinct from 0 (success), 1 (documented failure), 2 (reserved for the
/// harness layer, contract 14 §2) and 101 (a panic).
pub const EXIT_NOT_YET: i32 = 3;

/// One pipeline stage (docs/pipeline-stages.md numbering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    pub number: u8,
    pub name: &'static str,
    /// Who ports it (docs/rust-port/10-work-breakdown.md).
    pub owner: &'static str,
}

pub const FORMAT: Stage = Stage {
    number: 6,
    name: "format",
    owner: "phase 5b / WP5.6 (the TS formatter's output is ingested meanwhile: --beautified-input)",
};

/// Every stage the Rust driver cannot run yet, in pipeline order.
pub const NOT_YET: [Stage; 1] = [FORMAT];

/// The `ERROR:` block for reaching an unported stage: a headline, then
/// INDENTED detail (the harness keeps it with the headline).
pub fn not_yet_block(stage: Stage) -> String {
    let list: Vec<String> = NOT_YET
        .iter()
        .map(|s| format!("{} {}", s.number, s.name))
        .collect();
    format!(
        "ERROR: stage {} ({}) is NOT YET PORTED to Rust (owner: {}) — the output holds only what the stages before it wrote; this run is marked failed.\n  not yet ported: {}\n  run the TS pipeline (npx tsx src/index.ts) for a complete run",
        stage.number,
        stage.name,
        stage.owner,
        list.join(", ")
    )
}
