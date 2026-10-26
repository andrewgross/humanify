//! The pipeline command's action (TS: `src/commands/unified.ts`
//! configureUnifiedCommand's action + runPipeline + the report tail).
//!
//! Order is the TS order, because each step's failure mode is contract
//! (14 §2) — the messages and their ORDER are the surface (the recorded
//! scenario corpus that asserted them end to end was retired 2026-09-28;
//! the e2e drives the real invocations):
//!
//! 1. the configuration — resolved and validated BEFORE this action runs
//!    (`crate::config`, the one parse point): every refusal `Error: <msg>`
//!    on stderr, all at once, exit 1;
//! 2. the input files: a missing input exits 1 with the red
//!    `File <x> not found`, a missing `--prior-version`/`--split-ledger`
//!    with `Error: <flag> file not found: <path>` — before anything is
//!    created (config sweep step 4: the input used to be checked after
//!    the log file and the cache directory were made);
//! 3. verbosity and `--log-file` (implies -vv, appends);
//! 4. the renderer (TTY dashboard only when stderr is a terminal and not
//!    `-vv` without a log file);
//! 5. the provider: `--llm-cache` is created here (CachedLLMProvider's
//!    constructor);
//! 6. the pipeline, whose `finally` writes the profile and finishes the
//!    renderer: every failure below runs the finally, then prints
//!    `Error: <msg>` and exits 1 — where the TS dies with an uncaught
//!    exception and a stack trace, the Rust binary prints that exception's
//!    own `Error:` line (the declared normalization of the TS crash class,
//!    14 §2).
//!
//! Every stage runs natively (stage 6, the formatter, since WP5.6d).

use humanify_core::place::assign::namer::SplitNamerBudget;
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use humanify_core::libindex::lookup::LibraryIndex;
use humanify_core::naming::driver::{NamingConfig, NamingInput, NamingOutcome, run_naming};
use humanify_core::naming::waves::processor::DEFAULT_PROMPT_WINDOW;
use humanify_core::place::method::MarkerOffer;
use humanify_core::toolchain::{Toolchain, resolve_toolchain};
use humanify_llm::LlmClient;
use humanify_llm::provider::{LiveOptions, LiveStack};
use humanify_model::llm::{CacheKeyParams, LlmConfig, NameProvider, RateLimitConfig};

use crate::config::{Config, LibraryMatching};
use crate::kill_switches::Switch;
use crate::log::{debug_reset_output, debug_set_output, verbose};
use crate::pipeline_config::enum_name;
use crate::progress::{ProgressRenderer, create_progress_renderer};
use crate::unminify::{filter_libraries, report_vendor_naming, unpack_bundle};

/// A failure below the configuration: `Error: <message>` after the
/// finally (the TS uncaught-exception class, its headline only).
struct Crash(String);

impl From<String> for Crash {
    fn from(s: String) -> Self {
        Crash(s)
    }
}

/// A Node `fs` error's message (`ENOENT: no such file or directory, open
/// '<path>'`), the headline the TS crash prints for a failed read.
fn node_fs_error(e: &std::io::Error, syscall: &str, path: &str) -> String {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::NotFound => format!("ENOENT: no such file or directory, {syscall} '{path}'"),
        ErrorKind::PermissionDenied => format!("EACCES: permission denied, {syscall} '{path}'"),
        ErrorKind::IsADirectory => "EISDIR: illegal operation on a directory, read".to_string(),
        _ => format!("{e}, {syscall} '{path}'"),
    }
}

/// `fs.readFileSync(path, "utf-8")`: invalid UTF-8 becomes U+FFFD.
fn read_utf8(path: &str) -> Result<String, Crash> {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| Crash(node_fs_error(&e, "open", path)))
}

/// The action: `humanify <input> [options]`, on the run's validated
/// configuration. Returns the process exit code.
pub fn run(config: &Config) -> i32 {
    let input = config.io.input.as_str();
    // ensureFileExists → err(): red message, exit 1 — now before anything
    // is created.
    if !Path::new(input).exists() {
        eprintln!("\x1b[31mFile {input} not found\x1b[0m");
        return 1;
    }
    let missing = config.missing_inputs();
    if !missing.is_empty() {
        for m in missing {
            eprintln!("Error: {m}");
        }
        return 1;
    }
    verbose().set_level(config.diagnostics.verbosity);
    let log_file = match open_log_file(config.diagnostics.log_file.as_deref()) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("Error: {msg}");
            return 1;
        }
    };
    let use_rich_ui = std::io::stderr().is_terminal()
        && (verbose().level() < 2 || config.diagnostics.log_file.is_some());
    let mut renderer = create_progress_renderer(use_rich_ui);
    let profiler = humanify_core::profiling::Profiler::new(config.perf.perf_trace.is_some());
    profiler.install_global();
    // buildProvider: the cache wrapper mkdirs its dir at construction.
    if let Some(dir) = &config.llm.cache_dir
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return crash(
            &mut *renderer,
            &node_fs_error(&e, "mkdir", dir),
            log_file.is_some(),
        );
    }

    let provider = match build_provider(&config.llm) {
        Ok(p) => p,
        Err(e) => return crash(&mut *renderer, &e, log_file.is_some()),
    };

    let ended = run_pipeline(config, &provider, &profiler, &mut *renderer);
    report_memo(&provider, &mut *renderer);
    // The TS `finally`.
    finalize_profile(config, &profiler, &mut *renderer);
    renderer.finish();
    if log_file.is_some() {
        crate::log::verbose().reset_output();
        debug_reset_output();
    }
    match ended {
        Ok(code) => code,
        Err(Crash(message)) => {
            eprintln!("Error: {message}");
            1
        }
    }
}

/// One line per run: how many requests reached the model and how many
/// identical copies were served an earlier answer instead (finding #57's
/// one-key-one-answer memo, which holds with or without `--llm-cache`).
fn report_memo(provider: &LlmClient<LiveStack>, renderer: &mut dyn ProgressRenderer) {
    let memo = provider.memo_stats();
    if memo.asked + memo.shared > 0 {
        renderer.message(&format!(
            "LLM requests: {} sent to the model, {} identical copies served an earlier answer",
            memo.asked, memo.shared
        ));
    }
}

/// A crash-class failure before the pipeline body: finish, print, exit 1.
fn crash(renderer: &mut dyn ProgressRenderer, message: &str, had_log: bool) -> i32 {
    renderer.finish();
    if had_log {
        crate::log::verbose().reset_output();
        debug_reset_output();
    }
    eprintln!("Error: {message}");
    1
}

type SharedFile = Arc<Mutex<std::fs::File>>;

/// `--log-file`: append the debug AND verbose streams, raise the level to
/// at least 2. (The TS opens its stream lazily and an open failure
/// surfaces asynchronously later; the Rust binary reports it here.)
fn open_log_file(path: Option<&str>) -> Result<Option<SharedFile>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| node_fs_error(&e, "open", path))?;
    let file: SharedFile = Arc::new(Mutex::new(file));
    let writer = |f: SharedFile| {
        Box::new(move |text: &str| {
            let mut f = f.lock().unwrap();
            let _ = writeln!(f, "{text}");
        })
    };
    debug_set_output(writer(file.clone()));
    verbose().set_output(writer(file.clone()));
    verbose().set_level(verbose().level().max(2));
    Ok(Some(file))
}

fn finalize_profile(
    config: &Config,
    profiler: &humanify_core::profiling::Profiler,
    renderer: &mut dyn ProgressRenderer,
) {
    let Some(path) = &config.perf.perf_trace else {
        return;
    };
    use humanify_core::profiling::{format_profile_summary, to_trace_events};
    let report = profiler.finalize(Some(&config.io.input));
    let trace =
        serde_json::to_string_pretty(&to_trace_events(&report)).expect("a trace serializes");
    if let Err(e) = std::fs::write(path, trace) {
        renderer.message(&format!("Error: cannot write {path}: {e}"));
        return;
    }
    renderer.message(&format_profile_summary(&report));
    renderer.message(&format!("Profile written to {path}"));
}

/// runPipeline, through the ported stages.
fn run_pipeline(
    config: &Config,
    provider: &dyn NameProvider,
    profiler: &humanify_core::profiling::Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<i32, Crash> {
    let Some(sim_path) = &config.perf.simulate_llm_latency else {
        return pipeline_body(config, provider, profiler, renderer);
    };
    // The instrument: the run's own answers, priced on a virtual clock
    // with the rate limiter's slot count (crate::llm_sim).
    let sim = crate::llm_sim::LatencySim::new(provider, config.llm.concurrency as usize);
    let ended = pipeline_body(config, &sim, profiler, renderer);
    let report = sim.report();
    renderer.message(&format!(
        "LLM latency simulation: {} calls, simulated LLM wall {:.1}s (effective concurrency {:.1})",
        report.calls,
        report.wall_ms / 1000.0,
        if report.wall_ms > 0.0 {
            report.busy_ms / report.wall_ms
        } else {
            0.0
        }
    ));
    let text = serde_json::to_string_pretty(&report.to_json()).expect("a report serializes");
    if let Err(e) = std::fs::write(sim_path, text) {
        renderer.message(&format!("Error: cannot write {sim_path}: {e}"));
    }
    ended
}

/// How much backfill headroom the prompt window keeps beyond the outer
/// concurrency bound (finding #65): enough that a slow request never
/// starves the rate limiter.
const PROMPT_WINDOW_HEADROOM: usize = 16;

/// The split namer's prompt budget: the model context less the
/// completion reserve (finding #39).
fn split_namer_budget(llm: &crate::config::Llm) -> SplitNamerBudget {
    SplitNamerBudget::for_model(llm.context_tokens, llm.completion_tokens())
}

/// `buildProvider`: cache OUTERMOST (hits bypass the limiter and the debug
/// wrapper), then the rate limiter sized over both lanes, the debug
/// wrapper, the HTTP client. A miss goes to the endpoint and a non-empty
/// answer is written to the cache, exactly as the TS provider stack does.
fn build_provider(llm: &crate::config::Llm) -> Result<LlmClient<LiveStack>, String> {
    let reasoning_effort = llm.reasoning_effort.map(|e| e.name().to_string());
    let mut config = LlmConfig::new(&llm.endpoint, llm.api_key.expose(), &llm.model);
    config.timeout_ms = llm.timeout_ms;
    config.reasoning_effort = reasoning_effort.clone();
    if let Some(t) = llm.max_tokens {
        config.max_tokens = t;
    }
    let rate = RateLimitConfig {
        // The OUTER bound over both of the processor's limiters.
        max_concurrent: llm.outer_concurrency(),
        retry_attempts: llm.retries,
        ..RateLimitConfig::default()
    };
    LlmClient::live(LiveOptions {
        config,
        rate,
        key_params: cache_key_params(llm),
        cache_dir: llm.cache_dir.as_ref().map(std::path::PathBuf::from),
        metrics: None,
        log: Some(crate::log::llm_log_sink()),
    })
    .map_err(|e| e.to_string())
}

/// What an LLM cache entry is keyed by beyond the prompt. The TS passes a
/// literal temperature 0 (unified.ts buildProvider).
fn cache_key_params(llm: &crate::config::Llm) -> CacheKeyParams {
    CacheKeyParams {
        model: llm.model.clone(),
        temperature: Some(0.0),
        max_tokens: llm.max_tokens,
        reasoning_effort: llm.reasoning_effort.map(|e| e.name().to_string()),
    }
}

fn pipeline_body(
    config: &Config,
    provider: &dyn NameProvider,
    profiler: &humanify_core::profiling::Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<i32, Crash> {
    use humanify_core::profiling::phase;
    use std::sync::atomic::{AtomicBool, Ordering};
    let input = config.io.input.as_str();
    let switches = &config.switches;
    let outputs = &config.diagnostics;
    let ph = phase("read+detect");
    let bundled_code = read_utf8(input)?;
    let (toolchain, markers) = detect_stage(&bundled_code, config, profiler);
    // The run-time index machinery (exp154, `runtime_index`): the moment
    // detection resolves the bundler the store search — and, on a miss, the
    // build — starts on its own thread. The prior load, the dispatch setup
    // and the naming config overlap it; the library load takes its answer
    // through the joint (a deadline'd channel, never a poll). The scope
    // below must not end before that: `thread::scope` joins the job thread,
    // and the join IS the wait.
    let machinery = config.library.runs_index_machinery();
    let stop = AtomicBool::new(false);
    let (prior, naming_config, mut dispatch_log, library) = std::thread::scope(|scope| {
        let index_job = machinery.then(|| {
            crate::runtime_index::start(
                scope,
                profiler,
                bundled_code.as_str(),
                config,
                toolchain.bundler,
                toolchain.minifier,
                &stop,
            )
        });
        let setup = (|| -> Result<
            (
                Option<String>,
                NamingConfig,
                humanify_core::artifact_dump::DispatchLog,
                LibraryLoad,
            ),
            Crash,
        > {
            let prior = load_prior_version_code(config, renderer)?;
            let mut naming_config = naming_config(config, &toolchain);
            // The module step names each recorded module once, for its
            // wrapper and its file — offered whenever the toolchain offers
            // the module markers (the step checks they describe the bundle,
            // as the split does).
            naming_config.module_naming =
                (markers == MarkerOffer::Offered).then(|| split_namer_budget(&config.llm));
            // The run's per-dispatch recorder (finding #65): `--dump-artifacts`
            // streams prompts.jsonl + cache-keys.jsonl rows to part files as
            // the dispatches commit; `--dump-asks` retains the small ask
            // rows; WITHOUT either flag nothing per ask survives its
            // dispatch — the accumulated dispatch records were the ~99GB
            // holder of a full-bundle fresh run.
            let dispatch_log = match outputs.dump_artifacts.as_deref() {
                Some(dir) => humanify_core::artifact_dump::DispatchLog::dump(
                    naming_config.params.clone(),
                    Path::new(dir),
                )
                .map_err(|e| Crash(node_fs_error(&e, "open", dir)))?,
                None => {
                    if outputs.dump_asks.is_some() {
                        humanify_core::artifact_dump::DispatchLog::asks(
                            naming_config.params.clone(),
                        )
                    } else {
                        humanify_core::artifact_dump::DispatchLog::off(naming_config.params.clone())
                    }
                }
            };
            let library = load_library_index(
                &bundled_code,
                config,
                &toolchain,
                renderer,
                profiler,
                index_job,
                &stop,
            )?;
            Ok((prior, naming_config, dispatch_log, library))
        })();
        if setup.is_err() && machinery {
            // The run failed before it took the job's answer: stop the
            // build so the scope's join waits at most one in-flight
            // subprocess, never the build's own deadline.
            stop.store(true, Ordering::SeqCst);
        }
        setup
    })?;
    drop(ph);

    // armRecorders: the diagnostics/dump recorders arrive with the stages
    // that feed them.

    // Stages 3-5: unpack (the Bun adapter names its vendor files inside),
    // then library detection picks the files the per-file stages process.
    let out_dir = config.io.output_dir.as_str();
    let prior_path = config.matching.prior_version.as_deref().map(Path::new);
    let ph = phase("unpack+vendor");
    let mut unpacked = unpack_bundle(
        &bundled_code,
        Path::new(out_dir),
        &toolchain,
        provider,
        &mut dispatch_log,
        prior_path,
        switches.switch_on(Switch::ManifestPriorOrder),
        // `--library-matching report`: discovery runs, its rows go to the
        // library report (and the diagnostics), nowhere else.
        (config.library.matching == LibraryMatching::Report)
            .then(|| config.library.discovery_request(library.index.as_ref())),
        profiler,
        renderer,
    )?;
    drop(ph);
    let ph = phase("library-detection");
    let unpacked_files = std::mem::take(&mut unpacked.files);
    let (files_to_process, mixed_files) = if config.library.skips_libraries() {
        let filtered = filter_libraries(
            unpacked_files,
            toolchain.library_detector.piece,
            profiler,
            renderer,
        )?;
        (filtered.files_to_process, filtered.mixed_files)
    } else {
        (unpacked_files, Vec::new())
    };

    // Stages 6-9 per file.
    let mut failures = Failures::default();
    let prompt_window = naming_config.prompt_window;
    let mut naming = NamingRun {
        split: config.split.enabled,
        config: naming_config,
        log: &mut dispatch_log,
        prior: prior.as_deref(),
        provider,
        profiler,
        mixed_files: &mixed_files,
        failed_calls: Default::default(),
    };
    drop(ph);
    let ph = phase("format+naming");
    let last = naming.run(
        &files_to_process,
        toolchain.app_file.piece,
        &mut failures,
        renderer,
    )?;
    drop(ph);
    renderer.message(&format!(
        "Done! You can find your unminified code in {out_dir}"
    ));
    report_vendor_naming(&unpacked.vendor_naming, renderer);
    failures.preserve(Path::new(out_dir), renderer);

    // Stages 10-12: the split, emit and finish (crate::split_stage).
    let mut placement = humanify_core::place::trail::PlacementTrail::default();
    let mut split_sections = None;
    let mut post_split = crate::split_stage::PostSplitRecords::default();
    let mut split_method = None;
    let mut failed_calls = naming.failed_calls;
    failed_calls.vendor = unpacked.vendor_naming.batches_failed;
    if config.split.enabled
        && let Some(NamedFile {
            outcome,
            path: source,
            fresh,
            ..
        }) = &last
        && let Some(code) = &outcome.code
    {
        let mut split = crate::split_stage::SplitStageInput {
            output_dir: Path::new(out_dir),
            input_file: Path::new(input),
            processed_source: Some(source),
            toolchain,
            vendor_record: unpacked.vendor_record.as_ref(),
            prior_version: prior_path,
            split_ledger: config.split.ledger.as_deref(),
            split_pure: config.split.pure,
            markers,
            switches,
            provider,
            log: &mut dispatch_log,
            namer_budget: split_namer_budget(&config.llm),
            prompt_window,
            // Finding #60: the vendor bridge resolves the manifest's raw
            // capture names against this PRE-RENAME text.
            fresh: Some(fresh),
            // The ≥50 wrapper-binding gate reads the run's ORIGINAL input
            // (what the unpack stage saw), never the post-extraction
            // runtime the split is handed — being a bundled app is a
            // property of the input.
            input_bundle: Some(bundled_code.as_str()),
        };
        let _ph = phase("split");
        let span = profiler.pipeline_span("split");
        let records = crate::split_stage::run_split(
            code,
            outcome.prior_carry.as_ref(),
            &mut split,
            renderer,
        )?;
        span.end(Some(humanify_model::profiling::JsObject::new().with(
            "stable",
            matches!(records.ended, crate::split_stage::SplitEnded::Complete),
        )));
        placement = records.trail;
        split_sections = records.dump;
        split_method = Some(records.method);
        post_split = records.post_split;
        failed_calls.split = records.failed_calls.namer;
        failed_calls.tree_reviser = records.failed_calls.tree_reviser;
    }
    // Every namer's failed calls, one WARNING (audit R2): the naming
    // stage's, the vendor namer's (stage 3) and the split's.
    if let Some(warning) = failed_calls.warning() {
        renderer.message(&warning);
    }
    if let Some(NamedFile {
        outcome,
        fresh,
        path,
    }) = &last
    {
        let _ph = phase("reports");
        let reports = RunReports {
            config,
            outcome,
            fresh,
            placement: &placement,
            split: split_sections.as_ref(),
            post_split: &post_split,
            content_carry: unpacked.content_carry.as_ref(),
            app_declarations: &unpacked.app_declarations,
            library_discovery: unpacked.library_discovery.as_ref(),
            library: &library,
        };
        reports.write_diagnostics(renderer)?;
        reports.write_library_report(renderer)?;
        if let Some(dest) = &outputs.stats_json {
            write_stats_json(
                dest,
                outcome,
                &post_split.claims,
                &unpacked.vendor_naming,
                &toolchain,
                RunLevelStats {
                    split_method,
                    failed_calls: &failed_calls,
                    config,
                },
                renderer,
            )?;
        }
        if let Some(dir) = &outputs.dump_artifacts {
            let regions: Vec<humanify_core::libdetect::CommentRegion> = mixed_files
                .iter()
                .filter(|(p, _)| p == path)
                .flat_map(|(_, m)| m.regions.iter().cloned())
                .collect();
            // The streamed dispatch rows are the dump's prompts.jsonl +
            // cache-keys.jsonl: assemble them exactly when the dump itself
            // is written (an invalid output keeps the TS behavior — no
            // dump files at all — by discarding the parts).
            if outcome.coverage.is_some() {
                dispatch_log.close().map_err(Crash)?;
            } else {
                dispatch_log.discard();
            }
            reports.write_dump(
                &DumpContext {
                    dir,
                    output_dir: Path::new(out_dir),
                    minified: &bundled_code,
                    prior: prior.as_deref(),
                    flags: dump_flags(config, &toolchain),
                    regions: &regions,
                    layout: toolchain.layout.piece,
                    module_wrappers: toolchain.module_wrappers.piece,
                },
                renderer,
            )?;
        }
        // The naming waves' memory gauges. Finding #65's: the peak count of
        // rendered prompts alive at once over the run, against its bound.
        // Finding #66's (docs/perf-inventory.md item 1), in the same print
        // path: what the wave era RETAINED until it ended, per owner, in
        // estimated deep heap bytes — the split of the fresh run's ~58 GB.
        // The two spans that are bounded or transient (the window above, a
        // round's lanes, the barrier's per-round entries) are not owners;
        // everything here lives until the era drops.
        // The pipeline-named library imports and the collisions handed to
        // a disclosed re-ask instead of a suffix ladder (2026-10-06).
        renderer.message(&format!(
            "Retry hand-offs: {} lane-end, {} cut-off; library imports named: {} ({} left to the model)",
            outcome.processor.lane_end_handoffs,
            outcome.processor.collision_handoffs,
            outcome.library_imports.named.len(),
            outcome.library_imports.declined.len(),
        ));
        // The module step (docs/design/module-naming.md): one name per new
        // module, for its wrapper and its file.
        let m = &outcome.module_names;
        if m.modules > 0 {
            renderer.message(&format!(
                "Module names: {} modules, {} asked + {} barrels ({} by rule, {} asked), {} carried from the \
prior, {} calls ({} failed), {} named, {} retried as duplicates, {} disclosed retries ({} named), \
{} by fallback, {} left to the sweep",
                m.modules,
                m.asked,
                m.barrels,
                m.barrel_rule.len(),
                m.barrels_asked,
                m.carried,
                m.calls,
                m.failed_calls,
                m.named.len(),
                m.retried,
                m.disclosed,
                m.named_on_disclosure.len(),
                m.fallback.len(),
                m.left.len(),
            ));
        }
        let peak = &outcome.waves;
        if peak.peak_live_dispatches > 0 {
            renderer.message(&format!(
                "LLM prompt window: peak {} prompt(s) alive, {:.0} MB (window {})",
                peak.peak_live_dispatches,
                peak.peak_live_prompt_bytes as f64 / (1024.0 * 1024.0),
                prompt_window
            ));
            renderer.message(&format!(
                "Context name-sets: {} name(s) held at era end (finding #56)",
                peak.context_set_names
            ));
            let m = |b: u64| b as f64 / (1024.0 * 1024.0);
            renderer.message(&format!(
                "Wave-era retained: strategies {:.0} MB, node contexts {:.0} MB, used-identifier \
                 layers {:.0} MB, name records {:.0} MB, bookkeeping {:.0} MB (finding #66 gauges)",
                m(peak.gauges.strategy_bytes),
                m(peak.gauges.ctx_bytes),
                m(peak.gauges.used_set_bytes),
                m(peak.gauges.name_record_bytes),
                m(peak.gauges.bookkeeping_bytes),
            ));
            renderer.message(&format!(
                "Strategy split: bindings {:.0} MB, taken sets {:.0} MB ({} names), callees {:.0} \
                 MB, callsites {:.0} MB, context vars {:.0} MB, module {:.0} MB (finding #66 \
                 sub-gauges)",
                m(peak.gauges.strategy_bindings_bytes),
                m(peak.gauges.strategy_taken_bytes),
                peak.gauges.taken_set_names,
                m(peak.gauges.strategy_callee_bytes),
                m(peak.gauges.strategy_callsite_bytes),
                m(peak.gauges.strategy_context_var_bytes),
                m(peak.gauges.strategy_module_bytes),
            ));
        }
        if let Some(dest) = &outputs.dump_asks {
            let n = humanify_core::artifact_dump::write_ask_rows(
                Path::new(dest),
                dispatch_log.ask_rows(),
            )
            .map_err(Crash)?;
            renderer.message(&format!("Ask log: {n} ask(s) → {dest}"));
        }
        reports.write_rename_ledger(renderer)?;
    }
    // No named file: the dump flags wrote nothing today, and the streamed
    // parts go the same way.
    dispatch_log.discard();
    Ok(failures.close(renderer))
}

/// The run's recorded reports, written after the split in the TS order
/// (unified.ts runPipeline: diagnostics, eval stats, the artifact dump,
/// the rename ledger).
struct RunReports<'a> {
    config: &'a Config,
    outcome: &'a NamingOutcome,
    /// The formatted text the naming stage ran on.
    fresh: &'a str,
    placement: &'a humanify_core::place::trail::PlacementTrail,
    /// The split's dump sections (its input text is the placement
    /// trail's anchor); None without a split (the trail is empty then).
    split: Option<&'a humanify_core::artifact_dump::SplitSections>,
    /// The finishing passes' trail rows and claims.
    post_split: &'a crate::split_stage::PostSplitRecords,
    /// The vendor content carry (finding #90): every module carried from a
    /// prior module by content, with its score.
    content_carry: Option<&'a humanify_core::unpack::bun::ContentCarryReport>,
    /// Library declarations read in APP code (finding #95): report-only —
    /// byte ranges of the input bundle and what each declares.
    app_declarations: &'a [humanify_core::modules::identity::AppDeclaration],
    /// Library discovery (exp095): report-only — claimed statements by
    /// rule, round and evidence, the vetoed seeds, library class, the
    /// candidate queue.
    library_discovery: Option<&'a humanify_core::libdetect::discovery::DiscoveryReport>,
    /// What the library decision opened (report mode).
    library: &'a LibraryLoad,
}

/// What `--dump-artifacts` reads beyond the naming outcome.
struct DumpContext<'a> {
    dir: &'a str,
    output_dir: &'a Path,
    minified: &'a str,
    prior: Option<&'a str>,
    flags: humanify_model::js::JsValue,
    regions: &'a [humanify_core::libdetect::CommentRegion],
    layout: humanify_core::toolchain::BundleLayout,
    module_wrappers: humanify_core::toolchain::ModuleWrapperGrammar,
}

/// Claimed units in a serialized discovery report (`claims`, any shape).
fn count_claims(report: &serde_json::Value) -> usize {
    match &report["claims"] {
        serde_json::Value::Array(a) => a.len(),
        serde_json::Value::Object(o) => o.len(),
        _ => 0,
    }
}

/// What the library decision opened for this run: the index slice, its
/// id, the run-time machinery's usage trail when it ran, and — `report`
/// with no slice for the bundle's recipe — the loud warning's text (the
/// run goes on with path/banner detection).
#[derive(Default)]
struct LibraryLoad {
    index: Option<LibraryIndex>,
    index_id: Option<String>,
    recipe_missing: Option<String>,
    /// The machinery's answer (`indexSource` in the library report): set
    /// only when the run found or built its index at run time (exp154).
    source: Option<crate::runtime_index::IndexSource>,
}

/// `--library-matching report`: open the index's slice for the recipe the
/// bundle was built with (`libindex::recipe`: read once off the bundle and
/// the toolchain; the slice chosen in one place, `choose_slice`). The
/// index's stamps were checked before any work
/// (`Config::library_index_problem`); a missing slice is a WARNING, not a
/// refusal — report changes no output byte, so a run that cannot look up
/// has lost only its report.
///
/// The index itself comes one of two ways (exp154): as given
/// (`--library-index <dir|id>` — an explicit index wins, the machinery
/// stays off), or from the machinery — `index_job` is the handle to the
/// store search / run-time build `pipeline_body` started beside the run;
/// taking its answer here is the joint. A machinery failure fails the run
/// loudly (matching is `report` with no index passed, so there is nothing
/// to fall back to).
fn load_library_index(
    code: &str,
    config: &Config,
    toolchain: &Toolchain,
    renderer: &mut dyn ProgressRenderer,
    profiler: &humanify_core::profiling::Profiler,
    index_job: Option<crate::runtime_index::IndexJob>,
    stop: &std::sync::atomic::AtomicBool,
) -> Result<LibraryLoad, Crash> {
    use humanify_core::libindex::lookup::LoadError;
    if config.library.matching != LibraryMatching::Report {
        return Ok(LibraryLoad::default());
    }
    let (dir, source) = match index_job {
        Some(job) => match job.join(profiler, stop) {
            crate::runtime_index::JobAnswer::Found(dir, source)
            | crate::runtime_index::JobAnswer::Built(dir, source) => (dir, Some(source)),
            crate::runtime_index::JobAnswer::Failed(error) => {
                return Err(Crash(format!(
                    "run-time library index (library.index_build = \"auto\"): {error}"
                )));
            }
        },
        None => {
            if config.library.index_build == crate::config::IndexBuild::Auto {
                // index_build = "auto" but an explicit index was passed: the
                // explicit index wins, the machinery never runs. Say so —
                // the run's cache is not being searched.
                renderer.message(
                    "library.index_build = \"auto\" stays off: --library-index was passed",
                );
            }
            (
                config
                    .library
                    .index_dir()
                    .expect("report needs an index or the machinery (checked at the parse point)")
                    .map_err(Crash)?,
                None,
            )
        }
    };
    let recipe =
        humanify_core::libindex::recipe::detect_recipe(code, toolchain.bundler, toolchain.minifier);
    match LibraryIndex::load(&dir, &recipe, config.library.lookup.terser_slices) {
        Ok(index) => Ok(LibraryLoad {
            index_id: Some(index.id.clone()),
            index: Some(index),
            recipe_missing: None,
            source,
        }),
        Err(LoadError::NoSlice { id, why }) => {
            let warning =
                format!("WARNING: {why} — library matching reports path/banner detection only");
            renderer.message(&warning);
            Ok(LibraryLoad {
                index: None,
                index_id: Some(id),
                recipe_missing: Some(warning),
                source,
            })
        }
        Err(LoadError::Unusable(e)) => Err(Crash(format!("library index {}: {e}", dir.display()))),
    }
}

/// meta.json's `flags` (unified.ts writeDumpArtifacts' call): the resolved
/// selection and the decision levers; unset optional flags are absent.
/// The switch lists are the applied switches in registry order.
fn dump_flags(config: &Config, toolchain: &Toolchain) -> humanify_model::js::JsValue {
    use humanify_model::js::{JsObject, JsValue};
    let mut f = JsObject::new();
    f.insert("split", JsValue::Bool(config.split.enabled));
    f.insert("splitPure", JsValue::Bool(config.split.pure));
    f.insert("bundler", JsValue::str(enum_name(toolchain.bundler)));
    f.insert("minifier", JsValue::str(enum_name(toolchain.minifier)));
    f.insert(
        "skipLibraries",
        JsValue::Bool(config.library.skips_libraries()),
    );
    f.insert(
        "reconcilePriorDiff",
        JsValue::Bool(config.matching.reconcile_prior_diff),
    );
    f.insert("namingFloor", JsValue::Bool(config.naming.naming_floor));
    f.insert(
        "namingFloorSweep",
        JsValue::Bool(config.naming.naming_floor_sweep),
    );
    f.insert("model", JsValue::str(config.llm.model.as_str()));
    f.insert_opt(
        "reasoningEffort",
        config.llm.reasoning_effort.map(|e| JsValue::str(e.name())),
    );
    let list = |disable: bool| -> Option<JsValue> {
        let names: Vec<&str> = config
            .switches
            .active()
            .into_iter()
            .filter(|s| s.is_disable() == disable)
            .map(|s| s.name())
            .collect();
        (!names.is_empty()).then(|| JsValue::str(names.join(",")))
    };
    f.insert_opt("disable", list(true));
    f.insert_opt("probe", list(false));
    JsValue::Object(f)
}

/// `gitShortSha()`: the CWD's `git rev-parse --short HEAD`, "unknown" when
/// git cannot answer.
/// The `git rev-parse` deadline: 1 minute. It answers in milliseconds
/// from a local checkout; only a hung git (a stuck network filesystem, a
/// wedged credential helper) reaches it.
const GIT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// The dump manifest's commit, `git rev-parse --short HEAD`.
fn git_short_sha(renderer: &mut dyn ProgressRenderer) -> String {
    let mut command = std::process::Command::new("git");
    command.args(["rev-parse", "--short", "HEAD"]);
    let (sha, warning) = short_sha(&mut command, GIT_DEADLINE);
    if let Some(warning) = warning {
        renderer.message(&warning);
    }
    sha
}

/// Run a `rev-parse`-shaped command: its trimmed stdout, or "unknown".
/// Not a git checkout, or no git at all, is the decided quiet "unknown"
/// (the binary runs outside checkouts); a git past `deadline` is killed
/// and the "unknown" comes with a WARNING (docs/concurrency.md rule 4).
fn short_sha(
    command: &mut std::process::Command,
    deadline: std::time::Duration,
) -> (String, Option<String>) {
    use humanify_core::subprocess::SubprocessError;
    match humanify_core::subprocess::run(command, None, deadline) {
        Ok(o) if o.status.success() => {
            (String::from_utf8_lossy(&o.stdout).trim().to_string(), None)
        }
        Ok(_) | Err(SubprocessError::Spawn(_)) => ("unknown".to_string(), None),
        Err(e) => (
            "unknown".to_string(),
            Some(format!(
                "WARNING: git rev-parse failed ({e}) — the dump's manifest records commit \"unknown\""
            )),
        ),
    }
}

impl RunReports<'_> {
    /// The library decision's record: the mode, the index id, and a
    /// missing recipe slice (`recipeMissing`, the warning's text).
    fn library_matching_block(&self) -> serde_json::Value {
        let mut doc = serde_json::json!({
            "matching": "report",
            "indexId": self.library.index_id,
            "recipeMissing": self.library.recipe_missing,
        });
        // The machinery's usage trail — only when it ran (the explicit
        // index path records nothing new, exp154).
        if let Some(source) = &self.library.source {
            doc["indexSource"] = serde_json::to_value(source).expect("serializable");
        }
        doc
    }

    /// `--library-matching report` writes its report whether or not
    /// diagnostics are asked for: `.humanify/library-report.json`, a
    /// diagnostics-only sidecar (no decision reads it; neutrality excludes
    /// it), plus one line on stderr.
    fn write_library_report(&self, renderer: &mut dyn ProgressRenderer) -> Result<(), Crash> {
        if self.config.library.matching != LibraryMatching::Report {
            return Ok(());
        }
        let mut doc = self.library_matching_block();
        doc["libraryDiscovery"] =
            serde_json::to_value(self.library_discovery).expect("serializable");
        let claimed = self.library_discovery.map_or(0, |r| {
            serde_json::to_value(r).ok().map_or(0, |v| count_claims(&v))
        });
        let path = Path::new(&self.config.io.output_dir).join(crate::writers::LIBRARY_REPORT_PATH);
        crate::writers::write_json_file(&path, &doc)
            .map_err(|e| Crash(node_fs_error(&e, "open", &path.display().to_string())))?;
        renderer.message(&format!(
            "Library matching (report): {claimed} claimed unit(s); index {}; written to {}",
            self.library.index_id.as_deref().unwrap_or("-"),
            path.display()
        ));
        Ok(())
    }

    /// `--diagnostics` (buildDiagnosticsReport + writeDiagnosticsFile):
    /// the naming stage's report with the split's placement trail after
    /// the strategy trail; indent 2 and a trailing newline.
    fn write_diagnostics(&self, renderer: &mut dyn ProgressRenderer) -> Result<(), Crash> {
        use humanify_core::naming::report::diagnostics::{
            AnchorTexts, DiagnosticsInputs, build_diagnostics_report,
        };
        use humanify_model::js::{JsObject, JsValue, stringify_pretty};
        let (Some(dest), Some(coverage)) = (
            &self.config.diagnostics.diagnostics_file,
            &self.outcome.coverage,
        ) else {
            return Ok(());
        };
        let out = self.outcome;
        let transfer = out
            .prior
            .as_ref()
            .map(humanify_core::naming::driver::transfer_stats_by_tier);
        let report = build_diagnostics_report(&DiagnosticsInputs {
            timestamp: humanify_model::js::iso_now(),
            reports: &out.reports,
            coverage,
            transfer_stats: transfer.as_ref(),
            trail: &out.trail,
            texts: AnchorTexts {
                fresh: self.fresh,
                generated: out.generated.as_deref(),
                reconciled: out.reconcile.as_ref().and_then(|r| r.code.as_deref()),
                shipped: out.code.as_deref(),
            },
            contention: &out.processor.contention,
            extra_trail: &self.post_split.trail,
        });
        let JsValue::Object(report) = report else {
            unreachable!("the report is an object")
        };
        let placement = self
            .placement
            .diagnostics_report(self.split.map_or("", |s| s.shipped.as_str()));
        let mut with_placement = JsObject::new();
        for (k, v) in report.entries() {
            with_placement.insert(k.clone(), v.clone());
            if k == "strategyTrails" {
                with_placement.insert("placementTrails", placement.clone());
            }
        }
        if let Some(r) = self.content_carry {
            let json = serde_json::json!({
                "candidates": r.candidates,
                "leftovers": r.leftovers,
                "carries": r.carries,
            });
            with_placement.insert(
                "vendorContentCarry",
                JsValue::parse(&json.to_string()).expect("serde_json writes JSON"),
            );
        }
        if !self.app_declarations.is_empty() {
            let json = serde_json::to_string(self.app_declarations).expect("serializable");
            with_placement.insert(
                "appDeclaredLibraries",
                JsValue::parse(&json).expect("serde_json writes JSON"),
            );
        }
        if let Some(report) = self.library_discovery {
            let json = serde_json::to_string(report).expect("serializable");
            with_placement.insert(
                "libraryDiscovery",
                JsValue::parse(&json).expect("serde_json writes JSON"),
            );
        }
        if self.config.library.matching == LibraryMatching::Report {
            let block = self.library_matching_block().to_string();
            with_placement.insert(
                "libraryMatching",
                JsValue::parse(&block).expect("serde_json writes JSON"),
            );
        }
        std::fs::write(
            dest,
            format!(
                "{}\n",
                stringify_pretty(&JsValue::Object(with_placement), 2)
            ),
        )
        .map_err(|e| Crash(node_fs_error(&e, "open", dest)))?;
        renderer.message(&format!("Diagnostics written to {dest}"));
        Ok(())
    }

    /// `--dump-artifacts` (writeDumpArtifacts): the 07 §2 catalog.
    fn write_dump(
        &self,
        ctx: &DumpContext<'_>,
        renderer: &mut dyn ProgressRenderer,
    ) -> Result<(), Crash> {
        if self.outcome.coverage.is_none() {
            return Ok(());
        }
        humanify_core::artifact_dump::write_artifact_dump(
            &humanify_core::artifact_dump::DumpInputs {
                dir: Path::new(ctx.dir),
                output_dir: ctx.output_dir,
                flags: ctx.flags.clone(),
                commit: git_short_sha(renderer),
                generated_at: humanify_model::js::iso_now(),
                minified: ctx.minified,
                prior: ctx.prior,
                fresh: self.fresh,
                outcome: self.outcome,
                split: self.split,
                comment_regions: ctx.regions,
                layout: ctx.layout,
                module_wrappers: ctx.module_wrappers,
                extra_trail: &self.post_split.trail,
            },
        )
        .map_err(Crash)?;
        renderer.message(&format!("Artifact dump written to {}", ctx.dir));
        Ok(())
    }

    /// `--rename-ledger` (writeRenameLedger): the ledger, its source
    /// snapshot, the standalone applier.
    fn write_rename_ledger(&self, renderer: &mut dyn ProgressRenderer) -> Result<(), Crash> {
        let (Some(dir), Some(bundle)) = (
            &self.config.diagnostics.rename_ledger,
            &self.outcome.rename_ledger,
        ) else {
            return Ok(());
        };
        crate::writers::write_rename_ledger(Path::new(dir), bundle)
            .map_err(|e| Crash(node_fs_error(&e, "open", dir)))?;
        renderer.message(&format!(
            "Rename ledger: {} rename(s) → {dir}/ (apply: node {dir}/apply.mjs)",
            bundle.ledger.entries.len()
        ));
        Ok(())
    }
}

/// Stages 1-2: detect, resolve the run's toolchain (every plugin piece,
/// chosen ONCE — `humanify_core::toolchain`), decide the fossil split
/// (once, from the adapter's capability).
fn detect_stage(
    bundled_code: &str,
    config: &Config,
    profiler: &humanify_core::profiling::Profiler,
) -> (Toolchain, MarkerOffer) {
    let span = profiler.pipeline_span("detection");
    let detection = humanify_core::detect::detect_bundle(bundled_code);
    let toolchain = resolve_toolchain(&detection, config.unpack.bundler, config.unpack.minifier);
    let bundler_name = enum_name(toolchain.bundler);
    let adapter = toolchain.unpack.piece;
    span.end(Some(
        humanify_model::profiling::JsObject::new()
            .with("bundler", bundler_name.clone())
            .with("adapter", adapter.name()),
    ));
    verbose().log(&format!(
        "Bundle detection: bundler={bundler_name} ({}), minifier={}, adapter={}",
        enum_name(toolchain.bundler_tier),
        enum_name(toolchain.minifier),
        adapter.name()
    ));
    if let Some(conflict) = &detection.bundler.conflict {
        let named: Vec<String> = conflict
            .candidates
            .iter()
            .map(|c| {
                format!(
                    "{} ({}: {})",
                    enum_name(c.bundler),
                    enum_name(c.strength),
                    c.pattern
                )
            })
            .collect();
        verbose().log(&format!(
            "Bundle detection conflict, settled by {}: {}",
            enum_name(conflict.resolution),
            named.join(" > ")
        ));
    }
    verbose().debug(&format!(
        "Toolchain: {}",
        toolchain
            .record()
            .iter()
            .map(|r| format!("{}={} ({})", r.piece, r.choice, r.reason.name()))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    // The split method is chosen from what the bundle CONTAINS, at the
    // split (place::method); detection only says whether the toolchain
    // offers the module markers at all.
    let markers = if !adapter.provides_module_fossils() {
        MarkerOffer::NotProvided
    } else if config.switches.switch_on(Switch::FossilSplit) {
        MarkerOffer::Disabled
    } else {
        verbose()
            .log("Module markers offered: the split measures how much of the bundle they cover");
        MarkerOffer::Offered
    };
    if !detection.signals.is_empty() {
        let signals: Vec<String> = detection
            .signals
            .iter()
            .map(|s| format!("{}:{}", s.source, s.pattern))
            .collect();
        verbose().debug(&format!("Detection signals: {}", signals.join(", ")));
    }
    (toolchain, markers)
}

/// The last processed file: its naming outcome, its unpacked path, and
/// the formatted text the stage ran on (the diagnostics' fresh anchor).
struct NamedFile {
    outcome: NamingOutcome,
    path: std::path::PathBuf,
    fresh: String,
}

/// Stages 6-9 per processed file (unminify's plugin loop): the formatted
/// text, then the naming stage (graph, match, transfer, waves, floor,
/// generate, the post-generate passes) — core::naming::driver::run_naming,
/// the one owner.
struct NamingRun<'a> {
    /// `--split`: the tree is written by the split, not file by file.
    split: bool,
    config: NamingConfig,
    /// The run's per-dispatch recorder (finding #65).
    log: &'a mut humanify_core::artifact_dump::DispatchLog,
    prior: Option<&'a str>,
    provider: &'a dyn NameProvider,
    profiler: &'a humanify_core::profiling::Profiler,
    /// Stage 4's mixed files (their banner regions): the library carry.
    mixed_files: &'a [(
        std::path::PathBuf,
        humanify_core::libdetect::MixedFileDetection,
    )],
    /// The naming stage's failed calls over every file (the waves and
    /// sweeps, the module namer) — the run's `providerErrors`.
    failed_calls: crate::provider_failures::ProviderFailures,
}

impl NamingRun<'_> {
    /// Every file named; the app file's outcome (what the split reads) and
    /// its path — the toolchain's P13 rule picks which file that is.
    fn run(
        &mut self,
        files: &[humanify_core::unpack::UnpackedFile],
        app_file: humanify_core::toolchain::AppFile,
        failures: &mut Failures,
        renderer: &mut dyn ProgressRenderer,
    ) -> Result<Option<NamedFile>, Crash> {
        let total = files.len();
        let mut last = None;
        for (i, file) in files.iter().enumerate() {
            renderer.message(&format!("Processing file {}/{total}", i + 1));
            let path = file.path.display().to_string();
            let code = read_utf8(&path)?;
            if humanify_model::js::trim(&code).is_empty() {
                verbose().log(&format!("Skipping empty file {path}"));
                continue;
            }
            // Stage 6, the formatter (createBabelPlugin), with the library
            // carry when the file has banner regions (finding #32).
            let formatted = self.format_one(&code, &file.path)?;
            let library = formatted
                .library_carry
                .map(humanify_core::libdetect::function_carry::LibraryClassification::Carried);
            let outcome = self.name_one(&formatted.text, library.as_ref(), renderer)?;
            log_input_output(&code, outcome.code.as_deref().unwrap_or_default());
            if !self.split
                && let Some(shipped) = &outcome.code
            {
                // skipFileWrite is off without --split: the file is rewritten.
                std::fs::write(&file.path, shipped)
                    .map_err(|e| Crash(node_fs_error(&e, "open", &path)))?;
            }
            failures.record(&outcome, &path, &formatted.text);
            if last.is_none() || app_file.replaces_earlier() {
                last = Some(NamedFile {
                    outcome,
                    path: file.path.clone(),
                    fresh: formatted.text,
                });
            }
        }
        Ok(last)
    }

    /// Stage 6 for one file: `createBabelPlugin()(code, context)` — the
    /// native formatter (core::format), carrying the library classification
    /// when stage 4 found banner regions in this file. A formatter error is
    /// the TS's transform throw: the run crashes.
    fn format_one(
        &self,
        code: &str,
        file: &Path,
    ) -> Result<humanify_core::format::Formatted, Crash> {
        let regions = self
            .mixed_files
            .iter()
            .find(|(p, _)| p == file)
            .map_or(&[][..], |(_, m)| m.regions.as_slice());
        let span = self.profiler.pipeline_span("babel-transforms");
        let formatted = humanify_core::format::format_file(
            code,
            &humanify_core::format::FormatOptions::default(),
            regions,
        )?;
        span.end(None);
        Ok(formatted)
    }

    fn name_one(
        &mut self,
        formatted: &str,
        library: Option<&humanify_core::libdetect::function_carry::LibraryClassification>,
        renderer: &mut dyn ProgressRenderer,
    ) -> Result<NamingOutcome, Crash> {
        // Every file's naming rows start a fresh dump section (the TS
        // dump's last-file rule — finding #65).
        self.log.begin_named_file();
        let outcome = run_naming(
            &NamingInput {
                fresh: formatted,
                prior: self.prior,
                library,
            },
            &self.config,
            &self.provider,
            self.log,
        )?;
        // buildRenameLedgerBundle's self-check (non-fatal: the ledger is a
        // diagnostic artifact): replayed, it must reproduce the shipped code.
        if let Some(bundle) = &outcome.rename_ledger {
            use humanify_core::rename::validated::ledger::apply_rename_ledger;
            let problem = match apply_rename_ledger(&bundle.source, &bundle.ledger) {
                Ok(replayed) if Some(&replayed) == outcome.code.as_ref() => None,
                Ok(_) => Some("the replay differs from the shipped output".to_string()),
                Err(e) => Some(e.to_string()),
            };
            if let Some(problem) = problem {
                crate::log::debug_log(
                    "rename-ledger",
                    &format!("WARNING: replay does not reproduce the shipped output ({problem})"),
                );
            }
        }
        // `--probe shingle-probe`: the close tier's per-pair census (the TS
        // logs it inside the match; here after the stage, same lines).
        for line in &outcome.probe_lines {
            crate::log::debug_log("prior-version", line);
        }
        if let Some(text) = &outcome.coverage_text {
            renderer.message(text);
        }
        // The useless-prior WARNING (docs/perf-inventory.md item 4): a
        // prior that bound nothing silently degrades the run to a full
        // fresh pass; this is the loud line the incident had no version
        // of. Print-only — no decision reads it.
        if let Some(warning) = outcome.broken_prior_warning() {
            renderer.message(&warning);
        }
        // A prior-diff reconcile that was due and did not run (its `diff`
        // failed or ran past its deadline, audit R3). Print-only.
        if let Some(warning) = outcome.reconcile_skipped_warning() {
            renderer.message(&warning);
        }
        // A failed call is a hole in the output (the run is fail-soft and
        // still exits 0), so it is never only a `-v` line (concurrency
        // audit 2026-10, R2): tallied here, WARNED once for every namer
        // after the split (crate::provider_failures).
        self.failed_calls.naming += outcome.errors;
        self.failed_calls.modules += outcome.module_names.failed_calls;
        if outcome.misses > 0 || outcome.errors > 0 {
            verbose().log(&format!(
                "Naming: {} cache miss(es), {} provider error(s)",
                outcome.misses, outcome.errors
            ));
        }
        Ok(outcome)
    }
}

/// processFile's two `-vv` lines: the file as read and the plugin chain's
/// output, each cut to 2,000 UTF-16 units (`slice(0, 2000)`).
fn log_input_output(input: &str, output: &str) {
    if verbose().level() < 2 {
        return;
    }
    let head = |s: &str| {
        let cut = humanify_model::js::utf16_prefix(s, 2000);
        if humanify_model::js::utf16_len(s) > 2000 {
            format!("{cut}\n... truncated")
        } else {
            cut.to_string()
        }
    };
    verbose().debug(&format!("Input:  {}", head(input)));
    verbose().debug(&format!("Output:  {}", head(output)));
}

/// What only the whole run knows, for `--stats-json`: the split's method
/// and every namer's failed calls.
struct RunLevelStats<'a> {
    split_method: Option<humanify_model::stats::SplitMethodStats>,
    failed_calls: &'a crate::provider_failures::ProviderFailures,
    /// The run's configuration: the stats file's `config` block.
    config: &'a Config,
}

/// `--stats-json` (writeEvalStats): the naming stage's record + the vendor
/// namer's tally (only when it was asked anything) + the selection record
/// + the run-level blocks.
fn write_stats_json(
    dest: &str,
    outcome: &NamingOutcome,
    later_claims: &humanify_core::rename::validated::RenameClaimStats,
    vendor: &humanify_core::modules::vendor_names::VendorNamingStats,
    toolchain: &Toolchain,
    run: RunLevelStats<'_>,
    renderer: &mut dyn ProgressRenderer,
) -> Result<(), Crash> {
    if outcome.coverage.is_none() {
        return Ok(());
    }
    let mut stats = outcome.eval_stats_with(later_claims);
    if vendor.named + vendor.declined + vendor.echoed + vendor.batches_failed > 0 {
        stats.vendor_naming = Some(humanify_model::stats::VendorNamingStats {
            named: vendor.named as f64,
            declined: vendor.declined as f64,
            echoed: vendor.echoed as f64,
            batches_failed: vendor.batches_failed as f64,
        });
    }
    stats.selection = Some(crate::pipeline_config::pipeline_selection_record(toolchain));
    stats.toolchain = Some(crate::pipeline_config::toolchain_record(toolchain));
    stats.split_method = run.split_method;
    stats.provider_errors = Some(run.failed_calls.stats());
    stats.config = Some(
        humanify_model::js::JsValue::parse(&run.config.record().to_string())
            .expect("serde_json writes JSON"),
    );
    crate::writers::write_eval_stats(Path::new(dest), &stats)
        .map_err(|e| Crash(node_fs_error(&e, "open", dest)))?;
    renderer.message(&format!("Eval stats written to {dest}"));
    Ok(())
}

/// The per-file failures the closing reports read (unified.ts
/// `parseFailures`, `semanticFailures`, `totalInternalErrors`).
#[derive(Default)]
struct Failures {
    parse: Vec<(String, crate::output_validation::OutputParseFailure)>,
    semantic: Vec<(String, crate::output_validation::OutputSemanticFailure)>,
    preserved: Vec<crate::failed_output::FailedOutputFile>,
    internal_errors: usize,
}

/// `checkStructuralInvariant`'s headline; the first diverging token window
/// (`describeStructuralDivergence`) follows it on indented lines — over
/// the Rust serializer's token stream, so its index and token texts are
/// the Rust's own (the blessed serializer exemption; finding #41).
const STRUCTURAL_FAILURE: &str = "Rename changed program structure beyond identifier names \
(structural signature mismatch): the output is not a pure rename of the input — a statement, \
literal, operator, or property access differs.";

/// `checkStructuralInvariant`'s message: the headline + `divergenceSuffix`.
fn structural_failure_message(original: &str, generated: &str) -> String {
    use humanify_core::naming::driver::validate::describe_structural_divergence;
    match describe_structural_divergence(original, generated) {
        Some(detail) => format!("{STRUCTURAL_FAILURE}\n{detail}"),
        None => STRUCTURAL_FAILURE.to_string(),
    }
}

impl Failures {
    /// `preserveFailedOutput` — BEFORE the split, which consumes and
    /// deletes the processed source (the only window a rejected file
    /// still exists).
    fn preserve(&self, out_dir: &Path, renderer: &mut dyn ProgressRenderer) {
        if self.preserved.is_empty() {
            return;
        }
        crate::failed_output::preserve_failed_output(out_dir, &self.preserved);
        renderer.message(&format!(
            "Preserved {} rejected file(s) for inspection under {}/",
            self.preserved.len(),
            crate::report::FAILED_OUTPUT_DIR
        ));
    }

    /// The closing reports (parse, semantic, internal errors), in order;
    /// each marks the run failed when it fires — the output was written
    /// for inspection either way. Returns the exit code.
    fn close(&self, renderer: &mut dyn ProgressRenderer) -> i32 {
        let mut code = 0;
        for report in [
            crate::report::report_parse_failures(&self.parse),
            crate::report::report_semantic_failures(&self.semantic),
            crate::report::report_internal_errors(self.internal_errors),
        ] {
            for m in &report.messages {
                renderer.message(m);
            }
            if report.fails_run {
                code = 1;
            }
        }
        code
    }

    fn record(&mut self, outcome: &NamingOutcome, file: &str, original: &str) {
        use crate::output_validation::{
            FreeNameMeasure, OutputSemanticFailure, compare_semantics, parse_failure_of,
        };
        use humanify_core::naming::driver::validate::Verdict;
        self.internal_errors += outcome.processor.failed;
        let generated = outcome.generated.as_deref().unwrap_or_default();
        let semantic = match &outcome.verdict {
            None | Some(Verdict::Valid) => None,
            Some(Verdict::ParseFailed) => {
                if let Some(f) = parse_failure_of(generated) {
                    self.parse.push((file.to_string(), f));
                }
                None
            }
            Some(Verdict::Structural) => Some(OutputSemanticFailure {
                message: structural_failure_message(original, generated),
                ..OutputSemanticFailure::default()
            }),
            Some(Verdict::Semantic {
                free_before,
                free_after,
                bindings_before,
                bindings_after,
            }) => compare_semantics(
                &FreeNameMeasure {
                    free_names: free_before.clone(),
                    total_binding_count: *bindings_before as u64,
                },
                &FreeNameMeasure {
                    free_names: free_after.clone(),
                    total_binding_count: *bindings_after as u64,
                },
            ),
        };
        if let Some(f) = semantic {
            self.semantic.push((file.to_string(), f));
            self.preserved.push(crate::failed_output::FailedOutputFile {
                file_path: file.to_string(),
                original_code: original.to_string(),
                validated_code: Some(generated.to_string()),
            });
        }
    }
}

/// The plugin options the naming stage decides by (createRenamePlugin's).
fn naming_config(config: &Config, toolchain: &Toolchain) -> NamingConfig {
    NamingConfig {
        never_rename: toolchain.never_rename.piece,
        module_group_size: toolchain.tuning.piece.module_group_size(),
        layout: toolchain.layout.piece,
        module_wrappers: toolchain.module_wrappers.piece,
        name_profile: toolchain.name_profile.piece,
        skip_libraries: config.library.skips_libraries(),
        reconcile_prior_diff: config.matching.reconcile_prior_diff,
        naming_floor: config.naming.naming_floor,
        naming_floor_sweep: config.naming.naming_floor_sweep,
        source_map: false,
        emit_rename_ledger: config.diagnostics.rename_ledger.is_some(),
        family_permute_disabled: config.switches.switch_on(Switch::FamilyPermute),
        params: cache_key_params(&config.llm),
        capture_dump: config.diagnostics.dump_artifacts.is_some(),
        shingle_probe: config.switches.switch_on(Switch::ShingleProbe),
        fast: config.naming.schedule,
        // The rendered-prompt window (finding #65): every in-flight call
        // plus a backfill headroom over the provider stack's outer
        // concurrency bound (build_provider's), never the whole round.
        prompt_window: DEFAULT_PROMPT_WINDOW
            .max(config.llm.outer_concurrency() + PROMPT_WINDOW_HEADROOM),
        tunables: config.naming.tunables(),
        // Set by the caller from the toolchain's marker offer.
        module_naming: None,
    }
}

/// `loadPriorVersionCode`: an empty prior is an error, never a silent
/// zero-transfer run.
fn load_prior_version_code(
    config: &Config,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Option<String>, Crash> {
    let Some(path) = config.matching.prior_version.as_deref() else {
        return Ok(None);
    };
    let code = read_utf8(path)?;
    if humanify_model::js::trim(&code).is_empty() {
        return Err(Crash(format!("--prior-version file is empty: {path}")));
    }
    renderer.message(&format!("Prior version: loaded from {path}"));
    Ok(Some(code))
}

#[cfg(test)]
mod unified_test;
