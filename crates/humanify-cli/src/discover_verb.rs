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
//! - the lookup's and discovery's tunables from a `--config` file's
//!   `[library]` keys (the structural rule is `[library.lookup] agreement
//!   = false`; it was `--rule structural` until config sweep step 6) — the
//!   lab's knobs, read by the same owner as the pipeline's;
//! - `--unit-prints`: every unit with the fingerprints the lookup read
//!   (`unitRows`), so a scorer can say WHY a library unit was missed
//!   without re-implementing the unit model.
//!
//! An index that cannot be read FAILS the verb (the pipeline, report-only,
//! carries on without it): a score taken without its index would read as a
//! real zero.

use std::path::Path;

use humanify_core::libindex::lookup::LibraryIndex;
use serde_json::{Value, json};

use crate::config::{ConfigFile, Library, library_from_file};

use crate::pipeline_config::enum_name;

/// The output's schema; bump on a breaking shape change.
pub const DISCOVER_SCHEMA: &str = "humanify-discover/1";

/// The verb's inputs.
pub struct DiscoverArgs<'a> {
    pub input: &'a str,
    /// The index directory (None: discovery's own evidence seeds only).
    pub library_index: Option<&'a str>,
    /// The `[library]` settings (the lookup rules, discovery's tunables).
    pub library: &'a Library,
    /// `--bundler`: the pipeline's override (the detector's verdict is
    /// recorded either way, under `detected`).
    pub bundler: Option<&'a str>,
    pub unit_prints: bool,
    /// Where the adapter unpacks (default: a scratch dir, removed after).
    pub work_dir: Option<&'a str>,
}

/// The verb's `[library]` settings: the `--config` file's, else the
/// compiled-in defaults. Any refused key fails the verb.
pub fn library_settings(config: Option<&str>) -> Result<Library, String> {
    let file = config
        .map(ConfigFile::read)
        .transpose()
        .map_err(|e| e.join("\n"))?;
    library_from_file(file.as_ref()).map_err(|e| e.join("\n"))
}

/// The rule's name in the output (`agreement` | `structural`).
fn rule_name(library: &Library) -> &'static str {
    if library.lookup.agreement {
        "agreement"
    } else {
        "structural"
    }
}

/// Run the verb on `code`; the output document (`libraryDiscovery` null and
/// `noDiscovery` saying why when the adapter ran none).
pub fn discover(code: &str, args: &DiscoverArgs<'_>) -> Result<Value, String> {
    use humanify_model::detection::{BundlerType, SELECTABLE_BUNDLERS};
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
    let recipe =
        humanify_core::libindex::recipe::detect_recipe(code, toolchain.bundler, toolchain.minifier);
    // An instrument fails on a missing slice too: a score taken without
    // its index would read as a real zero.
    let index = match args.library_index {
        Some(dir) => Some(
            LibraryIndex::load(Path::new(dir), &recipe, args.library.lookup.terser_slices)
                .map_err(|e| format!("library index {dir}: {e}"))?,
        ),
        None => None,
    };
    let (work_dir, owned) = match args.work_dir {
        Some(dir) => (dir.to_string(), false),
        None => (
            humanify_core::scratch::temp_dir("humanify-discover")?
                .display()
                .to_string(),
            true,
        ),
    };
    std::fs::create_dir_all(&work_dir).map_err(|e| format!("mkdir {work_dir}: {e}"))?;
    let adapter = toolchain.unpack.piece;
    let run = humanify_core::unpack::AdapterRun {
        library_discovery: Some(humanify_core::libdetect::discovery::DiscoveryRequest {
            list_units: true,
            unit_prints: args.unit_prints,
            ..args.library.discovery_request(index.as_ref())
        }),
        ..humanify_core::unpack::AdapterRun::new(
            toolchain.interop.piece,
            toolchain.module_wrappers.piece,
            toolchain.module_identity.piece,
        )
    };
    // A vendor-record adapter runs discovery inside its flow; the others
    // (webcrack for webpack, passthrough for rollup or an unknown bundler)
    // run it on the raw bundle before they unpack — the same call, made
    // here without unpacking (the verb needs no file of theirs, and
    // webcrack's subprocess has nothing to add to discovery).
    let report = if adapter.vendor_record_stamp().is_some() {
        let outcome = humanify_core::unpack::run_adapter(
            adapter,
            toolchain.layout.piece,
            code,
            Path::new(&work_dir),
            run,
        );
        outcome.map(|o| o.library_discovery().cloned())
    } else {
        Ok(humanify_core::unpack::discover_without_vendor_record(
            code,
            toolchain.layout.piece,
            &run,
        ))
    };
    if owned {
        let _ = std::fs::remove_dir_all(&work_dir);
    }
    // A bundle that does not parse runs no discovery — nor does the
    // pipeline on it: said, not failed, so a score can count it as
    // "discovery never ran".
    let report = report?;
    let no_discovery = report.is_none().then(|| {
        format!(
            "the {} adapter ran no library discovery (the input does not parse)",
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
            "slice": index.as_ref().map(|ix| ix.slice.as_str()),
        },
        "recipe": {
            "detected": recipe.describe(),
            "targetReason": recipe.target_reason,
            "slice": index.as_ref().map(|ix| ix.slice.as_str()),
            "sliceReason": index.as_ref().map(|ix| ix.slice_reason()),
        },
        "noDiscovery": no_discovery,
        "rule": rule_name(args.library),
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
