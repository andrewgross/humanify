//! Stages 3-5 as the driver runs them (TS: `src/unminify.ts` unpackBundle +
//! filterLibraries, and the unpack options `src/commands/unified.ts`
//! builds): unpack through the core registry — the Bun adapter names its
//! vendor files inside the unpack (stage 5: the deterministic cascade, the
//! prior release's carry-over, then the LLM pass over the fallback names) —
//! then library detection picks the files the per-file stages process.

use std::path::{Path, PathBuf};

use humanify_core::libdetect::{LibraryDetector, MixedFileDetection, detect_libraries};
use humanify_core::modules::vendor_names::{
    BunModulesManifest, ProviderVendorNamer, VendorNamer, VendorNamingStats,
};
use humanify_core::profiling::Profiler;
use humanify_core::unpack::webcrack::WebcrackShim;
use humanify_core::unpack::{AdapterOutcome, AdapterRun, UnpackedFile, bun, run_adapter};
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
    /// The vendor record THIS run's adapter wrote (None when it writes
    /// none, or fell back to a single file) — handed to the split and the
    /// finish in memory, so neither trusts whatever record sits in the
    /// output folder (toolchain review R6).
    pub vendor_record: Option<BunModulesManifest>,
    /// The vendor content carry (finding #88), for the diagnostics.
    pub content_carry: Option<bun::ContentCarryReport>,
}

/// `unpackBundle`: run the run's adapter into `out_dir` through the one
/// dispatch site (`unpack::run_adapter`), handing it the vendor namer over
/// `provider` and the prior release's vendor names + manifest order
/// (discovered from `--prior-version`, as the TS does); an adapter that
/// writes no vendor record ignores both.
#[allow(clippy::too_many_arguments)]
pub fn unpack_bundle(
    code: &str,
    out_dir: &Path,
    toolchain: &humanify_core::toolchain::Toolchain,
    provider: &dyn NameProvider,
    log: &mut humanify_core::artifact_dump::DispatchLog,
    prior_version: Option<&Path>,
    manifest_prior_order_disabled: bool,
    profiler: &Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Unpacked, String> {
    // unified.ts loads both before unminify runs, whichever adapter is
    // selected — the carry-over line prints for any prior with a manifest.
    let prior_vendor = prior_version.and_then(bun::load_prior_vendor);
    if let Some(p) = &prior_vendor {
        if let Some(names) = &p.names {
            renderer.message(&format!(
                "Vendor names: carrying {} over from the prior release ({} structural groups)",
                p.carried_entries(),
                names.len()
            ));
        } else if p.stale_era.is_some() {
            renderer.message(&format!(
                "Vendor names: the prior manifest is from another hash era (this run writes \
                 hashVersion {}): its {} entries carry by CONTENT, never by hash bytes",
                humanify_core::modules::FACTORY_HASH_VERSION,
                p.carried_entries()
            ));
        }
    }
    let mut namer = ProviderVendorNamer::new(provider, log);
    let span = profiler.pipeline_span("unpack");
    // The one dispatch site: every adapter is handed everything it may
    // use (the vendor namer, the prior's vendor record, the webcrack
    // shim) and takes what it needs.
    let shim = repo_webcrack_shim();
    let adapter = toolchain.unpack.piece;
    let outcome = run_adapter(
        adapter,
        toolchain.layout.piece,
        code,
        out_dir,
        AdapterRun {
            interop: toolchain.interop.piece,
            module_wrappers: toolchain.module_wrappers.piece,
            namer: Some(&mut namer as &mut dyn VendorNamer),
            prior: prior_vendor,
            manifest_prior_order_disabled,
            webcrack_shim: Some(&shim),
        },
    )?;
    let (files, vendor_record, content_carry) = match outcome {
        AdapterOutcome::VendorRecord(outcome) => {
            report_vendor_unpack(&outcome, renderer);
            let outcome = *outcome;
            (
                outcome.result.files,
                outcome.manifest,
                outcome.content_carry,
            )
        }
        AdapterOutcome::Files(result) => (result.files, None, None),
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
        vendor_record,
        content_carry,
    })
}

/// A vendor-record adapter's progress lines: the content re-key, then the
/// name sources.
fn report_vendor_unpack(outcome: &bun::BunUnpackOutcome, renderer: &mut dyn ProgressRenderer) {
    if let Some(r) = outcome.rekey {
        renderer.message(&format!(
            "Vendor names re-keyed by content: {} of {} prior entries readable; {} factories \
             in {} structural groups joined a prior group ({} prior groups ambiguous, refused)",
            r.prior_keyed,
            r.prior_entries,
            r.factories_joined,
            r.groups_joined,
            r.prior_groups_ambiguous
        ));
    }
    if let Some(r) = &outcome.content_carry
        && r.candidates > 0
    {
        let identity = r
            .carries
            .iter()
            .filter(|c| c.carried == humanify_core::modules::vendor_pairing::Carried::Identity)
            .count();
        renderer.message(&format!(
            "Vendor content carry: {} of {} unmatched modules paired with a prior module ({} \
             name+file+identifier, {} identifier only; {} prior modules unmatched)",
            r.carries.len(),
            r.candidates,
            identity,
            r.carries.len() - identity,
            r.leftovers
        ));
    }
    log_name_sources(outcome);
}

/// The Bun adapter's two verbose lines (verboseLogNameSources, then
/// verboseLogVendorNaming when the LLM pass renamed anything).
fn log_name_sources(outcome: &bun::BunUnpackOutcome) {
    let Some(c) = &outcome.name_counts else {
        return;
    };
    let total = c.banner + c.url + c.carry_over + c.content_pair + c.llm + c.fallback + c.asset;
    if total == 0 {
        return;
    }
    verbose().log(&format!(
        "Vendor name sources ({total} factories): {} carry-over, {} content-pair, {} banner, {} url, \
         {} llm, {} fallback, {} app text asset",
        c.carry_over, c.content_pair, c.banner, c.url, c.llm, c.fallback, c.asset
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

/// `filterLibraries`: detect with the run's detector (the toolchain's P6
/// piece), report what is skipped, keep every file that is not a whole
/// library.
pub fn filter_libraries(
    files: Vec<UnpackedFile>,
    detector: LibraryDetector,
    profiler: &Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Filtered, String> {
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
