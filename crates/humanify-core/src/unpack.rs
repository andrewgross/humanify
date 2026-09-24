//! Stage 2: unpack (WPB.2) — split a bundle into the files the rest of the
//! pipeline processes. TS originals: `src/unpack/index.ts` (the adapter
//! registry), `src/unpack/types.ts`, `src/unpack/adapters/{bun,passthrough,
//! webcrack}.ts`; `src/plugins/webcrack.ts` is NOT ported — webcrack runs as
//! a subprocess shim (`webcrack`).
//!
//! The Bun adapter's classification and naming halves live in
//! `crate::modules` (classification, WP1.5) and `crate::modules::
//! vendor_names` (the naming cascade's file names, the manifest, the LLM
//! fallback pass); this module owns the ADAPTER — which one runs, the tree
//! it writes, and the files it hands downstream.

pub mod bun;
pub mod gate;
pub mod webcrack;

use std::fs;
use std::path::{Path, PathBuf};

use humanify_model::detection::{BundlerDetectionResult, BundlerType};

/// Module metadata webcrack reports for one extracted file
/// (`plugins/webcrack.ts ModuleMetadata`): the default library detector's
/// path layer reads `module_path`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct ModuleMetadata {
    /// Module ID from the bundler (e.g. "0", "abc123").
    pub id: String,
    /// Module path as resolved by webcrack (e.g. "./node_modules/react/index.js").
    #[serde(rename = "modulePath")]
    pub module_path: String,
    /// Whether this module is the bundle entry point.
    #[serde(rename = "isEntry")]
    pub is_entry: bool,
}

/// One unpacked file (`WebcrackFile`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnpackedFile {
    pub path: PathBuf,
    pub metadata: Option<ModuleMetadata>,
}

/// What an adapter produced (`UnpackResult`), in the order the adapter
/// emitted the files.
#[derive(Clone, Debug, Default)]
pub struct UnpackResult {
    pub files: Vec<UnpackedFile>,
}

/// The registered adapters (`adapters` in src/unpack/index.ts), in
/// registry order — the first whose `supports` holds wins, and passthrough
/// is last because it supports everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnpackAdapter {
    Webcrack,
    Bun,
    Passthrough,
}

/// Registry order (`adapters`).
pub const ADAPTERS: [UnpackAdapter; 3] = [
    UnpackAdapter::Webcrack,
    UnpackAdapter::Bun,
    UnpackAdapter::Passthrough,
];

impl UnpackAdapter {
    /// The adapter's registered name (`adapter.name`) — what the pipeline
    /// config carries as `unpackAdapterName`.
    pub fn name(self) -> &'static str {
        match self {
            UnpackAdapter::Webcrack => "webcrack",
            UnpackAdapter::Bun => "bun",
            UnpackAdapter::Passthrough => "passthrough",
        }
    }

    /// `adapter.supports(detection)`.
    pub fn supports(self, detection: &BundlerDetectionResult) -> bool {
        match self {
            UnpackAdapter::Webcrack => matches!(
                detection.bundler.kind,
                BundlerType::Webpack | BundlerType::Browserify
            ),
            UnpackAdapter::Bun => detection.bundler.kind == BundlerType::Bun,
            UnpackAdapter::Passthrough => true,
        }
    }

    /// `providesModuleFossils`: the bundle records its original module
    /// layout as `__esm` fossils (exp070). Only the Bun adapter declares it.
    pub fn provides_module_fossils(self) -> bool {
        self == UnpackAdapter::Bun
    }
}

/// `selectUnpackAdapter(config)`: the adapter registered under `name`; an
/// unknown name is an error (the TS throws).
pub fn select_unpack_adapter(name: &str) -> Result<UnpackAdapter, String> {
    ADAPTERS
        .into_iter()
        .find(|a| a.name() == name)
        .ok_or_else(|| format!("No unpack adapter named \"{name}\""))
}

/// `selectAdapter(detection, { bundlerOverride })`: a forced bundler type
/// (other than "unknown") is tried first as a synthetic definitive verdict;
/// otherwise — or when nothing supports the override — the first adapter
/// supporting the real detection wins.
pub fn select_adapter(
    detection: &BundlerDetectionResult,
    bundler_override: Option<BundlerType>,
) -> UnpackAdapter {
    if let Some(kind) = bundler_override
        && kind != BundlerType::Unknown
    {
        let mut overridden = detection.clone();
        overridden.bundler.kind = kind;
        overridden.bundler.tier = humanify_model::detection::DetectionTier::Definitive;
        overridden.bundler.version = None;
        if let Some(a) = ADAPTERS.into_iter().find(|a| a.supports(&overridden)) {
            return a;
        }
    }
    ADAPTERS
        .into_iter()
        .find(|a| a.supports(detection))
        .unwrap_or(UnpackAdapter::Passthrough)
}

/// Run the selected adapter (`adapter.unpack(code, outputDir, options)`).
/// The webcrack adapter needs its shim; running it without one is an error.
pub fn run_adapter(
    adapter: UnpackAdapter,
    code: &str,
    out_dir: &Path,
    bun_options: bun::BunUnpackOptions<'_>,
    webcrack_shim: Option<&webcrack::WebcrackShim>,
) -> Result<UnpackResult, String> {
    match adapter {
        UnpackAdapter::Bun => Ok(bun::unpack_bun(code, out_dir, bun_options)?.result),
        UnpackAdapter::Webcrack => webcrack::unpack_webcrack(
            code,
            out_dir,
            webcrack_shim.ok_or("the webcrack adapter needs its subprocess shim")?,
        ),
        UnpackAdapter::Passthrough => write_passthrough(code, out_dir),
    }
}

/// The passthrough adapter (`PassthroughAdapter.unpack`), also the Bun
/// adapter's no-factory floor: the whole input as `<out>/index.js`.
pub fn write_passthrough(code: &str, out_dir: &Path) -> Result<UnpackResult, String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    let path = out_dir.join("index.js");
    fs::write(&path, code).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(UnpackResult {
        files: vec![UnpackedFile {
            path,
            metadata: None,
        }],
    })
}
