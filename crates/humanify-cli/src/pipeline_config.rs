//! Stages 1-2's records (TS: `src/pipeline/selection-record.ts`). The
//! selection itself is the run's toolchain (`humanify_core::toolchain::
//! resolve_toolchain`, the one place a run's plugin pieces are chosen);
//! this module only writes down what it chose.

use humanify_core::toolchain::Toolchain;
use humanify_model::pipeline::{PipelineSelectionRecord, ToolchainPieceRecord};

/// The TS string literal of a serde-lowercase enum.
pub fn enum_name<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_string))
        .expect("a unit enum serializes to its string literal")
}

/// `pipelineSelectionRecord(config)`: the four TS fields (frozen by the
/// TS vectors).
pub fn pipeline_selection_record(toolchain: &Toolchain) -> PipelineSelectionRecord {
    PipelineSelectionRecord {
        bundler: enum_name(toolchain.bundler),
        bundler_tier: enum_name(toolchain.bundler_tier),
        minifier: enum_name(toolchain.minifier),
        unpack_adapter: toolchain.unpack.piece.name().to_string(),
    }
}

/// The stats file's `toolchain` block: every piece, its choice, and why.
pub fn toolchain_record(toolchain: &Toolchain) -> Vec<ToolchainPieceRecord> {
    toolchain
        .record()
        .into_iter()
        .map(|r| ToolchainPieceRecord {
            piece: r.piece.to_string(),
            choice: r.choice,
            reason: r.reason.name().to_string(),
        })
        .collect()
}
