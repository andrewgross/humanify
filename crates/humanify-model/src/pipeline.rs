//! The pipeline's selection records (TS: the record of
//! `src/pipeline/selection-record.ts`) — the selection itself is
//! `humanify_core::toolchain`.
//!
//! `FileContext` (the per-file plugin context) is not here yet: its only
//! consumers are the per-file stages (format, rename) that are NOT-YET in
//! the Rust driver; it lands with them.

use crate::js_record;

js_record! {
    /// TS `PipelineSelectionRecord` — the stats file's `selection` block, in
    /// `pipelineSelectionRecord`'s literal order.
    pub struct PipelineSelectionRecord {
        bundler: String = "bundler",
        bundler_tier: String = "bundlerTier",
        minifier: String = "minifier",
        unpack_adapter: String = "unpackAdapter",
    }
}

js_record! {
    /// One row of the run's TOOLCHAIN (`humanify_core::toolchain`): which
    /// plugin piece the run used, what was chosen, and why ("flag",
    /// "detected", "fallback", "only-implementation"). The stats file's
    /// `toolchain` block is the list of them, in pipeline order.
    pub struct ToolchainPieceRecord {
        piece: String = "piece",
        choice: String = "choice",
        reason: String = "reason",
    }
}
