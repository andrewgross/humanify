//! The pipeline's selection records (TS: the record of
//! `src/pipeline/selection-record.ts`) — the selection itself is
//! `humanify_ag_core::toolchain`.
//!
//! `FileContext` (the per-file plugin context) is not here yet: its only
//! consumers are the per-file stages (format, rename) that are NOT-YET in
//! the Rust driver; it lands with them.

/// TS `PipelineSelectionRecord` — the stats file's `selection` block, in
/// `pipelineSelectionRecord`'s literal order.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PipelineSelectionRecord {
    pub bundler: String,
    pub bundler_tier: String,
    pub minifier: String,
    pub unpack_adapter: String,
}

/// One row of the run's TOOLCHAIN (`humanify_ag_core::toolchain`): which
/// plugin piece the run used, what was chosen, and why ("flag",
/// "detected", "fallback", "only-implementation"). The stats file's
/// `toolchain` block is the list of them, in pipeline order.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainPieceRecord {
    pub piece: String,
    pub choice: String,
    pub reason: String,
}
