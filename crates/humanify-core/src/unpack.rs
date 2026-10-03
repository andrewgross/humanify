//! Stage 2: unpack (WPB.2) — split a bundle into the files the rest of the
//! pipeline processes. TS originals: `src/unpack/index.ts` (the adapter
//! registry), `src/unpack/types.ts`, `src/unpack/adapters/{bun,passthrough,
//! webcrack}.ts`; `src/plugins/webcrack.ts` is NOT ported — webcrack runs as
//! a subprocess shim (`webcrack`). The registry's post-cutover fourth
//! adapter is esbuild (exp075's module form, ported to Rust 2026-10-02):
//! the same vendor-extraction flow as bun (`bun::unpack_bun`, stamped
//! with the adapter's name) — the two bundlers differ only in two wrapper
//! shapes ([`crate::modules::factory_arg_function`]) and esbuild's
//! unminified builds hand each module's original source path through the
//! object key ([`crate::modules::FactoryRecord::source_path`]).
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

use crate::toolchain::{Chosen, Reason};

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

/// The registered adapters — the TS registry (`adapters` in
/// src/unpack/index.ts`) plus esbuild (exp075's second bundler, ported to
/// the Rust pipeline), in registry order — the first whose `supports`
/// holds wins, and passthrough is last because it supports everything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnpackAdapter {
    Webcrack,
    Bun,
    /// esbuild's bundle reader (`unpack::bun::unpack_bun` with the esbuild
    /// stamp): the same
    /// vendor-extraction flow as bun, whose module form differs in two
    /// wrapper shapes and hands over each unminified module's original
    /// source path.
    Esbuild,
    Passthrough,
}

/// Registry order (`adapters`).
pub const ADAPTERS: [UnpackAdapter; 4] = [
    UnpackAdapter::Webcrack,
    UnpackAdapter::Bun,
    UnpackAdapter::Esbuild,
    UnpackAdapter::Passthrough,
];

impl UnpackAdapter {
    /// The adapter's registered name (`adapter.name`) — what the pipeline
    /// config carries as `unpackAdapterName`.
    pub fn name(self) -> &'static str {
        match self {
            UnpackAdapter::Webcrack => "webcrack",
            UnpackAdapter::Bun => "bun",
            UnpackAdapter::Esbuild => "esbuild",
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
            UnpackAdapter::Esbuild => detection.bundler.kind == BundlerType::Esbuild,
            UnpackAdapter::Passthrough => true,
        }
    }

    /// `providesModuleFossils`: the bundle records its original module
    /// layout as `__esm` fossils (exp070; the grammar covers both bundlers
    /// — esbuild's init thunks and object-wrapped modules too, exp075).
    /// The bun and esbuild adapters declare it; webcrack and passthrough
    /// do not.
    pub fn provides_module_fossils(self) -> bool {
        matches!(self, UnpackAdapter::Bun | UnpackAdapter::Esbuild)
    }

    /// The stamp this adapter writes into the vendor record
    /// (`vendor/_bun-modules.json`'s `adapter`), or None when it writes no
    /// vendor record (docs/plugin-spec.md P5). Bun and esbuild share the
    /// one record format; the stamp is their registered name.
    pub fn vendor_record_stamp(self) -> Option<&'static str> {
        match self {
            UnpackAdapter::Bun | UnpackAdapter::Esbuild => Some(self.name()),
            UnpackAdapter::Webcrack | UnpackAdapter::Passthrough => None,
        }
    }

    /// The registered adapter whose vendor record carries `stamp` — the
    /// finish's "is this a record we can re-link?" (spec I14), answered by
    /// the registry instead of a list of names.
    pub fn of_vendor_record_stamp(stamp: &str) -> Option<UnpackAdapter> {
        ADAPTERS
            .into_iter()
            .find(|a| a.vendor_record_stamp() == Some(stamp))
    }
}

/// `selectAdapter(detection, { bundlerOverride })`, with WHY: a forced
/// bundler type (other than "unknown") is tried first as a synthetic
/// definitive verdict ([`Reason::Flag`]); otherwise the first adapter
/// supporting the real detection wins ([`Reason::Detected`]). The
/// do-nothing passthrough (last, supports everything) is always the
/// [`Reason::Fallback`] — also for a forced bundler that has no adapter. Called by the toolchain
/// (`crate::toolchain::resolve_toolchain`), the one place a run's pieces
/// are chosen.
pub fn choose_adapter(
    detection: &BundlerDetectionResult,
    bundler_override: Option<BundlerType>,
) -> Chosen<UnpackAdapter> {
    if let Some(kind) = bundler_override
        && kind != BundlerType::Unknown
    {
        let mut overridden = detection.clone();
        overridden.bundler.kind = kind;
        overridden.bundler.tier = humanify_model::detection::DetectionTier::Definitive;
        overridden.bundler.version = None;
        if let Some(piece) = ADAPTERS.into_iter().find(|a| a.supports(&overridden)) {
            return Chosen {
                piece,
                // A forced bundler with no adapter of its own (rollup,
                // parcel) lands on the do-nothing adapter: a fallback.
                reason: if piece == UnpackAdapter::Passthrough {
                    Reason::Fallback
                } else {
                    Reason::Flag
                },
            };
        }
    }
    let piece = ADAPTERS
        .into_iter()
        .find(|a| a.supports(detection))
        .unwrap_or(UnpackAdapter::Passthrough);
    Chosen {
        piece,
        reason: if piece == UnpackAdapter::Passthrough {
            Reason::Fallback
        } else {
            Reason::Detected
        },
    }
}

/// What a caller hands the selected adapter. Every field is optional: an
/// adapter takes what it uses and ignores the rest, so no caller has to
/// know which adapter needs what (docs/plugin-spec.md I9 — the adapter
/// used to be run from three places, two of which special-cased bun and
/// esbuild to pass the namer and the prior).
pub struct AdapterRun<'n> {
    /// The run's interop helpers (the toolchain's P8 piece; vendor-record
    /// adapters recognise and rename the bundle's helpers with it).
    pub interop: crate::toolchain::InteropHelpers,
    /// The LLM vendor namer (vendor-record adapters; None skips the pass).
    pub namer: Option<&'n mut dyn crate::modules::vendor_names::VendorNamer>,
    /// The prior release's vendor record (`bun::load_prior_vendor`).
    pub prior: Option<bun::PriorVendor>,
    /// `--disable manifest-prior-order`.
    pub manifest_prior_order_disabled: bool,
    /// The webcrack subprocess shim (the webcrack adapter errors without it).
    pub webcrack_shim: Option<&'n webcrack::WebcrackShim>,
}

impl<'n> AdapterRun<'n> {
    /// A run with the toolchain's interop piece and nothing optional (no
    /// namer, no prior, no shim). The interop piece has no default: it is
    /// the run's choice (`toolchain::resolve_toolchain`), never a fallback
    /// picked here.
    pub fn new(interop: crate::toolchain::InteropHelpers) -> AdapterRun<'n> {
        AdapterRun {
            interop,
            namer: None,
            prior: None,
            manifest_prior_order_disabled: false,
            webcrack_shim: None,
        }
    }
}

/// What the adapter produced: a vendor-record adapter's full outcome, or
/// the plain file list.
pub enum AdapterOutcome {
    VendorRecord(Box<bun::BunUnpackOutcome>),
    Files(UnpackResult),
}

impl AdapterOutcome {
    /// The files handed downstream, in the adapter's order.
    pub fn into_result(self) -> UnpackResult {
        match self {
            AdapterOutcome::VendorRecord(o) => o.result,
            AdapterOutcome::Files(r) => r,
        }
    }
}

/// Run the selected adapter (`adapter.unpack(code, outputDir, options)`) —
/// THE one dispatch site: the pipeline, the `unpack`, `libdetect` and
/// `match` verbs all come through here, so a registered adapter is run
/// the same way everywhere.
pub fn run_adapter(
    adapter: UnpackAdapter,
    layout: crate::toolchain::BundleLayout,
    code: &str,
    out_dir: &Path,
    run: AdapterRun<'_>,
) -> Result<AdapterOutcome, String> {
    match adapter {
        // The vendor-record adapters share the one extraction flow; the
        // adapter only stamps the record.
        UnpackAdapter::Bun | UnpackAdapter::Esbuild => {
            Ok(AdapterOutcome::VendorRecord(Box::new(bun::unpack_bun(
                code,
                out_dir,
                bun::BunUnpackOptions {
                    namer: run.namer,
                    prior: run.prior,
                    manifest_prior_order_disabled: run.manifest_prior_order_disabled,
                    adapter,
                    layout,
                    interop: run.interop,
                },
            )?)))
        }
        UnpackAdapter::Webcrack => webcrack::unpack_webcrack(
            code,
            out_dir,
            run.webcrack_shim
                .ok_or("the webcrack adapter needs its subprocess shim")?,
        )
        .map(AdapterOutcome::Files),
        UnpackAdapter::Passthrough => write_passthrough(code, out_dir).map(AdapterOutcome::Files),
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
