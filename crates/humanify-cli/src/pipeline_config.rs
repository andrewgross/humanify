//! Stages 1-2's selection (TS: `src/pipeline/config.ts`,
//! `src/pipeline/selection-record.ts`). The adapter choice is the unpack
//! registry's (`humanify_core::unpack::select_adapter`, the one owner of
//! registry order and each adapter's `supports()`); this module only
//! records it in the config.

use humanify_model::detection::{BundlerDetectionResult, BundlerType, DetectionTier, MinifierType};
use humanify_model::pipeline::{PipelineConfig, PipelineSelectionRecord};

/// `buildPipelineConfig(detection, {bundlerOverride, minifierOverride})`:
/// an override other than "unknown" wins (and makes the bundler tier
/// definitive).
pub fn build_pipeline_config(
    detection: &BundlerDetectionResult,
    bundler_override: Option<BundlerType>,
    minifier_override: Option<MinifierType>,
) -> PipelineConfig {
    let bundler = bundler_override.filter(|b| *b != BundlerType::Unknown);
    let minifier = minifier_override.filter(|m| *m != MinifierType::Unknown);
    PipelineConfig {
        bundler_type: bundler.unwrap_or(detection.bundler.kind),
        bundler_tier: if bundler.is_some() {
            DetectionTier::Definitive
        } else {
            detection.bundler.tier
        },
        minifier_type: minifier.unwrap_or(detection.minifier.kind),
        unpack_adapter_name: humanify_core::unpack::select_adapter(detection, bundler_override)
            .name(),
    }
}

/// The TS string literal of a serde-lowercase enum.
pub fn enum_name<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_string))
        .expect("a unit enum serializes to its string literal")
}

/// `pipelineSelectionRecord(config)`.
pub fn pipeline_selection_record(config: &PipelineConfig) -> PipelineSelectionRecord {
    PipelineSelectionRecord {
        bundler: enum_name(config.bundler_type),
        bundler_tier: enum_name(config.bundler_tier),
        minifier: enum_name(config.minifier_type),
        unpack_adapter: config.unpack_adapter_name.to_string(),
    }
}
