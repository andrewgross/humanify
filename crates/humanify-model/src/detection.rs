//! Bundle-detection types (TS: src/detection/types.ts, WPB.1).
//!
//! The serialized shape IS the TS `JSON.stringify(detectBundle(code))`
//! byte-for-byte: field order follows the TS object-literal insertion order
//! (`source, pattern, bundler|minifier, tier`), absent optionals are omitted
//! (JSON.stringify drops `undefined`), enum values are the TS string
//! literals. The WPB.1 gate compares those bytes.

use serde::{Deserialize, Serialize};

/// TS `BundlerType`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum BundlerType {
    Webpack,
    Browserify,
    Rollup,
    Esbuild,
    Parcel,
    Bun,
    Unknown,
}

/// TS `MinifierType`.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MinifierType {
    Terser,
    Esbuild,
    Swc,
    Bun,
    None,
    Unknown,
}

/// TS `DetectionTier`. Declared lowest-first so the derived `Ord` is the
/// TS `TIER_RANK` (unknown 0 < likely 1 < definitive 2).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
pub enum DetectionTier {
    Unknown,
    Likely,
    Definitive,
}

/// TS `SELECTABLE_BUNDLERS`: the values `--bundler` may force (the
/// `unknown` sentinel is the no-override signal, so it is excluded).
pub const SELECTABLE_BUNDLERS: [BundlerType; 6] = [
    BundlerType::Webpack,
    BundlerType::Browserify,
    BundlerType::Rollup,
    BundlerType::Esbuild,
    BundlerType::Parcel,
    BundlerType::Bun,
];

/// TS `SELECTABLE_MINIFIERS`.
pub const SELECTABLE_MINIFIERS: [MinifierType; 5] = [
    MinifierType::Terser,
    MinifierType::Esbuild,
    MinifierType::Swc,
    MinifierType::Bun,
    MinifierType::None,
];

/// TS `DetectionSignal`. Exactly one of `bundler` / `minifier` is set by
/// every detector.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct DetectionSignal {
    pub source: String,
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bundler: Option<BundlerType>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub minifier: Option<MinifierType>,
    pub tier: DetectionTier,
}

/// TS `BundlerDetectionResult["bundler"]`. `version` is declared by the TS
/// type but never set by any detector.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct BundlerVerdict {
    #[serde(rename = "type")]
    pub kind: BundlerType,
    pub tier: DetectionTier,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub version: Option<String>,
}

/// TS `BundlerDetectionResult["minifier"]`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct MinifierVerdict {
    #[serde(rename = "type")]
    pub kind: MinifierType,
    pub tier: DetectionTier,
}

/// TS `BundlerDetectionResult`. The TS marks `bundler`/`minifier` optional
/// but `detectBundle` always sets both, so here they are plain fields.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct BundlerDetectionResult {
    pub bundler: BundlerVerdict,
    pub minifier: MinifierVerdict,
    pub signals: Vec<DetectionSignal>,
}
