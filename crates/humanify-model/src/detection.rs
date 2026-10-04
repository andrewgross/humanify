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

/// How strongly one BUNDLER signal names its bundler — the bundler verdict
/// is decided by this ranking, never by the order the detectors run in
/// (toolchain review R3/R19, 2026-10-04; docs/plugin-spec.md P1). Declared
/// weakest-first so the derived `Ord` is the ranking. The rules:
///
/// 1. [`Banner`](Self::Banner) — the bundler wrote its own name into the
///    output's header (`// @bun`). Nothing else writes it by accident.
/// 2. [`OwnRuntimeName`](Self::OwnRuntimeName) — a runtime name that only
///    this bundler's runtime declares (`__webpack_require__`,
///    `webpackChunk`, `parcelRequire`, `require("_bundle_loader")`). A
///    vendored library can MENTION one (`typeof __webpack_require__`), so
///    it ranks under a banner.
/// 3. [`Shape`](Self::Shape) — two structural pieces that together only
///    this bundler writes (browserify's `[0].call(` + `.exports}`; Bun's
///    `{exports:{}}` + `createRequire` import). Each piece alone is
///    common minified text, so the pair ranks under a name that carries
///    the bundler's own name.
/// 4. [`SharedHelperName`](Self::SharedHelperName) — a helper name more
///    than one bundler writes: esbuild's runtime names (`__commonJS`,
///    `__toESM`, `__toCommonJS`, `var __export`, `__require`) are also
///    Bun's (Bun's runtime copies them; checked on a real `bun build`,
///    test/e2e/fixtures/bun-bundle). They say "esbuild-family" and lose to
///    anything more specific.
/// 5. [`WeakToken`](Self::WeakToken) — one common word that suggests a
///    bundler but proves nothing (`installedModules`, a local name in
///    webpack 4 bootstraps and in hand-written loaders). The only rank
///    whose tier is `likely`, not `definitive`; the toolchain acts on
///    definitive verdicts only.
///
/// Two DIFFERENT bundlers sharing the strongest rank is a tie: the verdict
/// is `unknown` and the conflict says so (never the first one listed).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum SignalStrength {
    WeakToken,
    SharedHelperName,
    Shape,
    OwnRuntimeName,
    Banner,
}

impl SignalStrength {
    /// The tier a signal of this strength carries: `likely` for a weak
    /// token, `definitive` for everything else.
    pub fn tier(self) -> DetectionTier {
        match self {
            SignalStrength::WeakToken => DetectionTier::Likely,
            _ => DetectionTier::Definitive,
        }
    }
}

/// TS `DetectionSignal`. Exactly one of `bundler` / `minifier` is set by
/// every detector. `strength` is set on every bundler signal (None on a
/// minifier signal) and is NOT serialized — the `detect` JSON keeps its
/// shape; a verdict that had to choose between bundlers shows the
/// strengths in its `conflict`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct DetectionSignal {
    pub source: String,
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub bundler: Option<BundlerType>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub minifier: Option<MinifierType>,
    pub tier: DetectionTier,
    #[serde(skip)]
    pub strength: Option<SignalStrength>,
}

/// How a bundler conflict was settled.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ConflictResolution {
    /// The verdict's strongest signal outranks every other bundler's.
    Strength,
    /// Two or more bundlers share the strongest rank: no verdict.
    Tie,
}

/// One bundler a signal named, at its strongest signal.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ConflictCandidate {
    pub bundler: BundlerType,
    pub pattern: String,
    pub strength: SignalStrength,
}

/// Signals named more than one bundler: every candidate (strongest first)
/// and how the verdict was reached.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct BundlerConflict {
    pub resolution: ConflictResolution,
    pub candidates: Vec<ConflictCandidate>,
}

/// TS `BundlerDetectionResult["bundler"]`. `version` is declared by the TS
/// type but never set by any detector. `conflict` is present only when
/// the signals named more than one bundler.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct BundlerVerdict {
    #[serde(rename = "type")]
    pub kind: BundlerType,
    pub tier: DetectionTier,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub conflict: Option<BundlerConflict>,
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
