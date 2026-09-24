//! Pipeline configuration types (TS: `src/pipeline/types.ts` and the
//! record of `src/pipeline/selection-record.ts`).
//!
//! `FileContext` (the per-file plugin context) is not here yet: its only
//! consumers are the per-file stages (format, rename) that are NOT-YET in
//! the Rust driver; it lands with them.

use crate::detection::{BundlerType, DetectionTier, MinifierType};
use crate::js_record;

/// TS `PipelineConfig`: every selection the pipeline made before any code
/// was transformed. Frozen in TS; a plain value here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineConfig {
    pub bundler_type: BundlerType,
    pub bundler_tier: DetectionTier,
    pub minifier_type: MinifierType,
    /// The selected unpack adapter's name ("webcrack", "bun", "passthrough").
    pub unpack_adapter_name: &'static str,
}

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
