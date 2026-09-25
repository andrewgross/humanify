//! Stages 3-5 as the driver runs them (TS: `src/unminify.ts` unpackBundle +
//! filterLibraries, and the unpack options `src/commands/unified.ts`
//! builds): unpack through the core registry — the Bun adapter names its
//! vendor files inside the unpack (stage 5: the deterministic cascade, the
//! prior release's carry-over, then the LLM pass over the fallback names) —
//! then library detection picks the files the per-file stages process.

use std::path::{Path, PathBuf};

use humanify_core::libdetect::{MixedFileDetection, detect_libraries, select_library_detector};
use humanify_core::modules::vendor_names::{ProviderVendorNamer, VendorNamer, VendorNamingStats};
use humanify_core::profiling::Profiler;
use humanify_core::unpack::gate::{TsFactoryHash, inject_ts_hashes};
use humanify_core::unpack::webcrack::WebcrackShim;
use humanify_core::unpack::{UnpackAdapter, UnpackedFile, bun, run_adapter};
use humanify_model::llm::NameProvider;
use humanify_model::profiling::JsObject;

use crate::log::verbose;
use crate::progress::ProgressRenderer;

/// The webcrack shim command for a `scripts/webcrack-shim.ts` path: `npx
/// tsx <script> <out>`, run from the repo root (the script's grandparent,
/// where node_modules resolves).
pub fn webcrack_shim(script: &Path) -> WebcrackShim {
    WebcrackShim {
        program: "npx".to_string(),
        args: vec!["tsx".to_string(), script.display().to_string()],
        cwd: script
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf),
    }
}

/// The pipeline's shim: this checkout's `scripts/webcrack-shim.ts` (the
/// webcrack plugin is absorbed by the shim, never ported — WPB.2).
pub fn repo_webcrack_shim() -> WebcrackShim {
    webcrack_shim(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/webcrack-shim.ts"))
}

/// What stage 3 handed downstream.
pub struct Unpacked {
    pub files: Vec<UnpackedFile>,
    /// The vendor namer's tally (`VendorNamingStats`) — all zero when the
    /// adapter never ran the LLM pass.
    pub vendor_naming: VendorNamingStats,
    /// The vendor namer's calls in dispatch order (the dump's `vendor`
    /// prompt site).
    pub vendor_dispatched: Vec<humanify_model::llm::LlmCall>,
}

/// `unpackBundle`: run the selected adapter into `out_dir`. The Bun adapter
/// gets the vendor namer over `provider` and the prior release's vendor
/// names + manifest order (discovered from `--prior-version`, as the TS
/// does); the others take the bundle as-is.
#[allow(clippy::too_many_arguments)]
pub fn unpack_bundle(
    code: &str,
    out_dir: &Path,
    adapter: UnpackAdapter,
    provider: &dyn NameProvider,
    prior_version: Option<&Path>,
    ts_factory_hashes: Option<&[TsFactoryHash]>,
    manifest_prior_order_disabled: bool,
    profiler: &Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Unpacked, String> {
    // unified.ts loads both before unminify runs, whichever adapter is
    // selected — the carry-over line prints for any prior with a manifest.
    let prior_vendor_names = prior_version.and_then(bun::load_prior_vendor_names_from);
    if let Some(names) = &prior_vendor_names {
        let factories: usize = names.values().map(Vec::len).sum();
        renderer.message(&format!(
            "Vendor names: carrying {factories} over from the prior release ({} structural groups)",
            names.len()
        ));
    }
    let mut namer = ProviderVendorNamer::new(provider);
    let span = profiler.pipeline_span("unpack");
    let files = if adapter == UnpackAdapter::Bun {
        // `--inject-ts-hashes` (the blessed structuralSignature exemption,
        // lesson 16): the TS factory hashes replace the Rust's at the one
        // seam, after the bijection is proven.
        let injected = std::cell::Cell::new(None);
        let hook = |c: &mut humanify_core::modules::BunModuleClassification| {
            if let Some(rows) = ts_factory_hashes {
                injected.set(Some(inject_ts_hashes(c, rows)?));
            }
            Ok(())
        };
        let outcome = bun::unpack_bun(
            code,
            out_dir,
            bun::BunUnpackOptions {
                namer: Some(&mut namer as &mut dyn VendorNamer),
                prior_vendor_names,
                prior_manifest_factories: prior_version
                    .and_then(bun::load_prior_manifest_factories_from),
                classification_hook: Some(&hook),
                manifest_prior_order_disabled,
            },
        )?;
        if let Some(r) = injected.get() {
            verbose().log(&format!(
                "TS hash bytes injected (unpack): {} factories / {} classes (bijection)",
                r.factories, r.classes
            ));
        }
        log_name_sources(&outcome);
        outcome.result.files
    } else {
        let shim = repo_webcrack_shim();
        run_adapter(
            adapter,
            code,
            out_dir,
            bun::BunUnpackOptions::default(),
            Some(&shim),
        )?
        .files
    };
    span.end(Some(
        JsObject::new()
            .with("fileCount", files.len())
            .with("adapter", adapter.name()),
    ));
    verbose().log(&format!(
        "Unpacked {} file(s) via {}",
        files.len(),
        adapter.name()
    ));
    Ok(Unpacked {
        files,
        vendor_naming: namer.stats,
        vendor_dispatched: namer.dispatched,
    })
}

/// The Bun adapter's two verbose lines (verboseLogNameSources, then
/// verboseLogVendorNaming when the LLM pass renamed anything).
fn log_name_sources(outcome: &bun::BunUnpackOutcome) {
    let Some(c) = &outcome.name_counts else {
        return;
    };
    let total = c.banner + c.url + c.carry_over + c.llm + c.fallback;
    if total == 0 {
        return;
    }
    verbose().log(&format!(
        "Vendor name sources ({total} factories): {} carry-over, {} banner, {} url, {} llm, {} fallback",
        c.carry_over, c.banner, c.url, c.llm, c.fallback
    ));
    if outcome.llm_renamed > 0 {
        verbose().log(&format!(
            "Vendor naming: LLM named {}/{total} factories",
            outcome.llm_renamed
        ));
    }
}

/// What stage 4 decided.
pub struct Filtered {
    pub files_to_process: Vec<UnpackedFile>,
    pub mixed_files: Vec<(PathBuf, MixedFileDetection)>,
}

/// `filterLibraries`: detect with the selected detector, report what is
/// skipped, keep every file that is not a whole library.
pub fn filter_libraries(
    files: Vec<UnpackedFile>,
    adapter: UnpackAdapter,
    profiler: &Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Filtered, String> {
    let detector = select_library_detector(adapter.name());
    let span = profiler.pipeline_span("library-detection");
    let detection = detect_libraries(detector, &files)?;
    span.end(Some(
        JsObject::new()
            .with("libraryCount", detection.library_files.len())
            .with("mixedCount", detection.mixed_files.len())
            .with("detector", detector.name()),
    ));
    if !detection.library_files.is_empty() {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for (_, d) in &detection.library_files {
            let name = d.library_name.as_deref().unwrap_or("unknown");
            match counts.iter_mut().find(|(n, _)| n == name) {
                Some((_, c)) => *c += 1,
                None => counts.push((name.to_string(), 1)),
            }
        }
        let summary: Vec<String> = counts
            .iter()
            .map(|(name, c)| format!("{name} ({c} file{})", if *c > 1 { "s" } else { "" }))
            .collect();
        let n = detection.library_files.len();
        renderer.message(&format!(
            "Skipping {n} library file{}: {}",
            if n > 1 { "s" } else { "" },
            summary.join(", ")
        ));
    }
    for (path, mixed) in &detection.mixed_files {
        renderer.message(&format!(
            "Mixed file {}: will skip library functions ({})",
            path.display(),
            mixed.library_names.join(", ")
        ));
    }
    let files_to_process = files
        .into_iter()
        .filter(|f| !detection.library_files.iter().any(|(p, _)| *p == f.path))
        .collect();
    Ok(Filtered {
        files_to_process,
        mixed_files: detection.mixed_files,
    })
}

/// `reportVendorNaming`: silent when the namer never ran; otherwise what
/// it did, declines and failed batches counted apart.
pub fn report_vendor_naming(stats: &VendorNamingStats, renderer: &mut dyn ProgressRenderer) {
    let attempted = stats.named + stats.declined + stats.echoed + stats.batches_failed;
    if attempted == 0 {
        return;
    }
    let mut parts = vec![format!("{} named", stats.named)];
    if stats.declined > 0 {
        parts.push(format!("{} declined", stats.declined));
    }
    if stats.echoed > 0 {
        parts.push(format!("{} echoed the key", stats.echoed));
    }
    if stats.batches_failed > 0 {
        parts.push(format!("{} batch(es) failed", stats.batches_failed));
    }
    renderer.message(&format!("Vendor naming: {}", parts.join(", ")));
}
