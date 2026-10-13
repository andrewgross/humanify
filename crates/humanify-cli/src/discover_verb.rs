//! `humanify discover` — library discovery (exp095,
//! docs/design/library-discovery.md) run on its own, as the instrument of
//! the library-matching score (experiments/096-library-matching-score):
//! detect → resolve the toolchain → the selected unpack adapter (no vendor
//! namer, no prior: the run's own extraction, into a scratch dir) with
//! discovery on → the `libraryDiscovery` report, every claimed, vetoed and
//! queued unit listed.
//!
//! The SAME code path a pipeline run with `--diagnostics --library-index`
//! takes (the adapter calls discovery on the RAW bundle before any name is
//! chosen), stopped before naming: NO LLM path executes, and the verb never
//! constructs a provider. Two things only the verb can ask for:
//!
//! - `--rule structural`: the lookup without the literal agreement rule
//!   (the pipeline always runs the default, agreement);
//! - `--unit-prints`: every unit with the fingerprints the lookup read
//!   (`unitRows`), so a scorer can say WHY a library unit was missed
//!   without re-implementing the unit model.
//!
//! An index that cannot be read FAILS the verb (the pipeline, report-only,
//! carries on without it): a score taken without its index would read as a
//! real zero.

use std::path::Path;

use humanify_core::libdetect::discovery::DiscoveryRequest;
use humanify_core::libindex::IndexSlice;
use humanify_core::libindex::lookup::{LibraryIndex, LookupRules};
use serde_json::{Value, json};

use crate::pipeline_config::enum_name;

/// The output's schema; bump on a breaking shape change.
pub const DISCOVER_SCHEMA: &str = "humanify-discover/1";

/// The verb's inputs.
pub struct DiscoverArgs<'a> {
    pub input: &'a str,
    /// The index directory (None: discovery's own evidence seeds only).
    pub library_index: Option<&'a str>,
    /// `agreement` (the pipeline's rule) or `structural`.
    pub rule: &'a str,
    /// `--bundler`: the pipeline's override (the detector's verdict is
    /// recorded either way, under `detected`).
    pub bundler: Option<&'a str>,
    pub unit_prints: bool,
    /// Where the adapter unpacks (default: a scratch dir, removed after).
    pub work_dir: Option<&'a str>,
}

/// The lookup rules a `--rule` names: None = the pipeline's default.
pub fn rules_for(rule: &str) -> Result<Option<LookupRules>, String> {
    match rule {
        "agreement" => Ok(None),
        "structural" => Ok(Some(LookupRules {
            agreement: false,
            ..LookupRules::default()
        })),
        other => Err(format!("unknown --rule {other:?} (agreement | structural)")),
    }
}

/// Run the verb on `code`; the output document (`libraryDiscovery` null and
/// `noDiscovery` saying why when the adapter ran none).
pub fn discover(code: &str, args: &DiscoverArgs<'_>) -> Result<Value, String> {
    use humanify_model::detection::{BundlerType, SELECTABLE_BUNDLERS};
    let rules = rules_for(args.rule)?;
    let detection = humanify_core::detect::detect_bundle(code);
    let toolchain = humanify_core::toolchain::resolve_toolchain(
        &detection,
        crate::match_verb::parse_override::<BundlerType>(
            args.bundler,
            &SELECTABLE_BUNDLERS,
            "bundler",
        )?,
        None,
    );
    let index = match args.library_index {
        Some(dir) => {
            let slice = IndexSlice::for_bundler(toolchain.bundler).ok_or_else(|| {
                format!(
                    "the index has no slice for bundler {}",
                    enum_name(toolchain.bundler)
                )
            })?;
            Some(
                LibraryIndex::load(Path::new(dir), slice)
                    .map_err(|e| format!("library index {dir}: {e}"))?,
            )
        }
        None => None,
    };
    let (work_dir, owned) = match args.work_dir {
        Some(dir) => (dir.to_string(), false),
        None => (
            std::env::temp_dir()
                .join(format!("humanify-discover-{}", std::process::id()))
                .display()
                .to_string(),
            true,
        ),
    };
    std::fs::create_dir_all(&work_dir).map_err(|e| format!("mkdir {work_dir}: {e}"))?;
    let adapter = toolchain.unpack.piece;
    let outcome = humanify_core::unpack::run_adapter(
        adapter,
        toolchain.layout.piece,
        code,
        Path::new(&work_dir),
        humanify_core::unpack::AdapterRun {
            library_discovery: Some(DiscoveryRequest {
                list_units: true,
                index: index.as_ref(),
                rules,
                unit_prints: args.unit_prints,
            }),
            ..humanify_core::unpack::AdapterRun::new(
                toolchain.interop.piece,
                toolchain.module_wrappers.piece,
                toolchain.module_identity.piece,
            )
        },
    );
    if owned {
        let _ = std::fs::remove_dir_all(&work_dir);
    }
    // The adapter's no-module floor (the whole input as one file) runs no
    // discovery — nor does the pipeline on such a bundle: said, not failed,
    // so a score can count it as "discovery never ran".
    let report = match outcome? {
        humanify_core::unpack::AdapterOutcome::VendorRecord(o) => o.library_discovery,
        humanify_core::unpack::AdapterOutcome::Files(_) => None,
    };
    let no_discovery = report.is_none().then(|| {
        format!(
            "the {} adapter unpacked no bundled module (the one-file floor): no discovery ran",
            adapter.name()
        )
    });
    Ok(json!({
        "schema": DISCOVER_SCHEMA,
        "detected": {
            "bundler": enum_name(detection.bundler.kind),
            "minifier": enum_name(detection.minifier.kind),
        },
        "bundlerOverride": args.bundler,
        "toolchain": {
            "bundler": enum_name(toolchain.bundler),
            "minifier": enum_name(toolchain.minifier),
            "adapter": adapter.name(),
            "slice": IndexSlice::for_bundler(toolchain.bundler).map(IndexSlice::name),
        },
        "noDiscovery": no_discovery,
        "rule": args.rule,
        "libraryDiscovery": report,
    }))
}

/// The verb: read the input, run, write `out` (or stdout).
pub fn run_discover(args: &DiscoverArgs<'_>, out: Option<&str>) -> Result<(), String> {
    let code = std::fs::read(args.input)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("cannot read {}: {e}", args.input))?;
    let doc = discover(&code, args)?;
    let text = serde_json::to_string(&doc).map_err(|e| e.to_string())? + "\n";
    match out {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("write {path}: {e}")),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}
