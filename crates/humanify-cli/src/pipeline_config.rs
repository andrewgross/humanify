//! Stages 1-2's selection (TS: `src/pipeline/config.ts`,
//! `src/pipeline/selection-record.ts`, and the adapter choice of
//! `selectAdapter` in `src/unpack/index.ts`).
//!
//! The adapter REGISTRY (its `unpack` implementations) is WPB.2's; what the
//! driver needs from it is only which adapter a detection selects, which is
//! decided by each adapter's `supports()` in registry order. That order and
//! those predicates are mirrored in [`ADAPTERS`]; when the unpack registry
//! lands in humanify-core it becomes the owner and this table is deleted
//! (recorded in the WPB.4 hand-back).

use humanify_model::detection::{BundlerDetectionResult, BundlerType, DetectionTier, MinifierType};
use humanify_model::pipeline::{PipelineConfig, PipelineSelectionRecord};

/// One unpack adapter as selection sees it.
pub struct AdapterSpec {
    pub name: &'static str,
    /// `supports(detection)`, over the (possibly overridden) bundler type.
    pub supports: fn(BundlerType) -> bool,
    /// `providesModuleFossils` (exp070): the fossil-split capability.
    pub provides_module_fossils: bool,
}

/// The TS registry, in order (`src/unpack/index.ts`: webcrack, bun,
/// passthrough last — the fallback that supports everything).
pub const ADAPTERS: [AdapterSpec; 3] = [
    AdapterSpec {
        name: "webcrack",
        supports: |b| matches!(b, BundlerType::Webpack | BundlerType::Browserify),
        provides_module_fossils: false,
    },
    AdapterSpec {
        name: "bun",
        supports: |b| b == BundlerType::Bun,
        provides_module_fossils: true,
    },
    AdapterSpec {
        name: "passthrough",
        supports: |_| true,
        provides_module_fossils: false,
    },
];

/// `selectUnpackAdapter(config)`: the adapter named by the config.
pub fn adapter_named(name: &str) -> &'static AdapterSpec {
    ADAPTERS
        .iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("No unpack adapter named \"{name}\""))
}

fn select_adapter(
    detection: &BundlerDetectionResult,
    bundler_override: Option<BundlerType>,
) -> &'static AdapterSpec {
    if let Some(b) = bundler_override.filter(|b| *b != BundlerType::Unknown)
        && let Some(a) = ADAPTERS.iter().find(|a| (a.supports)(b))
    {
        return a;
    }
    ADAPTERS
        .iter()
        .find(|a| (a.supports)(detection.bundler.kind))
        .expect("passthrough supports everything")
}

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
        unpack_adapter_name: select_adapter(detection, bundler_override).name,
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
