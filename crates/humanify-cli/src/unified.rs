//! The pipeline command's action (TS: `src/commands/unified.ts`
//! configureUnifiedCommand's action + runPipeline + the report tail).
//!
//! Order is the TS order, because each step's failure mode is contract
//! (14 §2) — the messages and their ORDER are the surface (the recorded
//! scenario corpus that asserted them end to end was retired 2026-09-28;
//! the e2e drives the real invocations):
//!
//! 1. flag invariants — every violation `Error: <msg>` on stderr, exit 1;
//! 2. kill switches — `Error: <msg>`, exit 1;
//! 3. verbosity and `--log-file` (implies -vv, appends);
//! 4. the renderer (TTY dashboard only when stderr is a terminal and not
//!    `-vv` without a log file);
//! 5. settings — `Error: <msg>`, exit 1;
//! 6. the provider: `--llm-cache` is created here (CachedLLMProvider's
//!    constructor), before the input is even checked;
//! 7. the pipeline, whose `finally` writes the profile and finishes the
//!    renderer: a missing input exits 1 IMMEDIATELY with the red
//!    `File <x> not found` (process.exit skips the finally); every other
//!    failure below runs the finally, then prints `Error: <msg>` and exits
//!    1 — where the TS dies with an uncaught exception and a stack trace,
//!    the Rust binary prints that exception's own `Error:` line (the
//!    declared normalization of the TS crash class, 14 §2).
//!
//! Every stage runs natively (stage 6, the formatter, since WP5.6d).

use humanify_core::place::assign::namer::{DEFAULT_CONTEXT_TOKENS, SplitNamerBudget};
use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use humanify_core::naming::driver::{NamingConfig, NamingInput, NamingOutcome, run_naming};
use humanify_core::naming::waves::batch::WaveTunables;
use humanify_core::naming::waves::processor::DEFAULT_PROMPT_WINDOW;
use humanify_core::place::method::MarkerOffer;
use humanify_core::toolchain::{Toolchain, resolve_toolchain};
use humanify_llm::LlmClient;
use humanify_llm::provider::{LiveOptions, LiveStack};
use humanify_model::detection::{
    BundlerType, MinifierType, SELECTABLE_BUNDLERS, SELECTABLE_MINIFIERS,
};
use humanify_model::llm::{CacheKeyParams, LlmConfig, NameProvider, RateLimitConfig};

use crate::commander::{OptionValues, ValueSource};
use crate::kill_switches::{Switch, SwitchState};
use crate::log::{debug_reset_output, debug_set_output, verbose};
use crate::pipeline_config::enum_name;
use crate::progress::{ProgressRenderer, create_progress_renderer};
use crate::settings::{Settings, SettingsInput, resolve_settings};
use crate::unminify::{filter_libraries, report_vendor_naming, unpack_bundle};
use crate::util::MAX_DEFAULT_MODULE_CONCURRENCY;

/// The parsed options (TS `CommandOptions`): commander's values, typed.
/// Value options stay strings, as commander hands them over; `settings`
/// parses the numeric ones once.
#[derive(Clone, Debug, Default)]
pub struct CommandOptions {
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub output_dir: Option<String>,
    pub verbose: i64,
    pub concurrency: Option<String>,
    pub retries: Option<String>,
    pub timeout: Option<String>,
    pub skip_libraries: Option<bool>,
    pub split: bool,
    pub log_file: Option<String>,
    pub diagnostics: Option<String>,
    pub bundler: Option<String>,
    pub minifier: Option<String>,
    pub batch_size: Option<String>,
    pub max_retries: Option<String>,
    pub max_free_retries: Option<String>,
    pub lane_threshold: Option<String>,
    /// `--rename-retries <n>`: the name-conflict re-ask budget.
    pub rename_retries: Option<String>,
    pub profile: Option<String>,
    pub prior_version: Option<String>,
    pub reconcile_prior_diff: Option<bool>,
    pub naming_floor: Option<bool>,
    pub naming_floor_sweep: Option<bool>,
    pub reasoning_effort: Option<String>,
    pub disable: Option<String>,
    pub probe: Option<String>,
    pub max_tokens: Option<String>,
    pub context_tokens: Option<String>,
    pub module_concurrency: Option<String>,
    pub llm_cache: Option<String>,
    pub split_ledger: Option<String>,
    pub split_pure: bool,
    pub rename_ledger: Option<String>,
    pub stats_json: Option<String>,
    pub dump_artifacts: Option<String>,
    /// `--dump-asks <path>`: the reason-labeled ask log.
    pub dump_asks: Option<String>,
    /// `--sequential`: the conservative naming schedule.
    pub sequential: bool,
    /// `--relaxed-levers <list>` (rust-only): a subset of the relaxed
    /// levers; None = every lever.
    pub relaxed_levers: Option<String>,
    pub simulate_llm_latency: Option<String>,
}

pub use humanify_core::fast::{FastTier, Levers};

impl CommandOptions {
    /// The naming schedule: the DEFAULT is the relaxed tier with every
    /// lever on; `--sequential` selects the conservative one (the relaxed
    /// levers OFF, the exact tier's byte-identical parallelization kept —
    /// the pre-2026-09-28 default's bytes at the same `--batch-size`), and
    /// `--relaxed-levers` sizes a subset. The error is a bad lever name or
    /// the flags' contradiction; `run` refuses before anything launches.
    pub fn fast_tier(&self) -> Result<FastTier, String> {
        if self.sequential {
            return if self.relaxed_levers.is_some() {
                Err("--sequential already turns the relaxed levers off; \
                     --relaxed-levers has nothing to select"
                    .to_string())
            } else {
                Ok(FastTier::Exact)
            };
        }
        match self.relaxed_levers.as_deref() {
            None => Ok(FastTier::Relaxed(Levers::all())),
            Some(list) => Levers::parse(list).map(FastTier::Relaxed),
        }
    }
}

impl CommandOptions {
    pub fn from_values(v: &OptionValues) -> CommandOptions {
        let s = |k: &str| v.str(k).map(str::to_string);
        CommandOptions {
            endpoint: s("endpoint"),
            api_key: s("apiKey"),
            model: s("model"),
            output_dir: s("outputDir"),
            verbose: v.get("verbose").and_then(|x| x.as_i64()).unwrap_or(0),
            concurrency: s("concurrency"),
            retries: s("retries"),
            timeout: s("timeout"),
            skip_libraries: v.bool("skipLibraries"),
            split: v.bool("split").unwrap_or(false),
            log_file: s("logFile"),
            diagnostics: s("diagnostics"),
            bundler: s("bundler"),
            minifier: s("minifier"),
            batch_size: s("batchSize"),
            max_retries: s("maxRetries"),
            max_free_retries: s("maxFreeRetries"),
            lane_threshold: s("laneThreshold"),
            rename_retries: s("renameRetries"),
            profile: s("profile"),
            prior_version: s("priorVersion"),
            reconcile_prior_diff: v.bool("reconcilePriorDiff"),
            naming_floor: v.bool("namingFloor"),
            naming_floor_sweep: v.bool("namingFloorSweep"),
            reasoning_effort: s("reasoningEffort"),
            disable: s("disable"),
            probe: s("probe"),
            max_tokens: s("maxTokens"),
            context_tokens: s("contextTokens"),
            module_concurrency: s("moduleConcurrency"),
            llm_cache: s("llmCache"),
            split_ledger: s("splitLedger"),
            split_pure: v.bool("splitPure").unwrap_or(false),
            rename_ledger: s("renameLedger"),
            stats_json: s("statsJson"),
            dump_artifacts: s("dumpArtifacts"),
            dump_asks: s("dumpAsks"),
            sequential: v.bool("sequential").unwrap_or(false),
            relaxed_levers: s("relaxedLevers"),
            simulate_llm_latency: s("simulateLlmLatency"),
        }
    }

    fn settings_input(&self) -> SettingsInput {
        SettingsInput {
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            timeout: self.timeout.clone(),
            retries: self.retries.clone(),
            concurrency: self.concurrency.clone(),
            batch_size: self.batch_size.clone(),
            max_retries: self.max_retries.clone(),
            max_free_retries: self.max_free_retries.clone(),
            lane_threshold: self.lane_threshold.clone(),
            rename_retries: self.rename_retries.clone(),
            llm_cache: self.llm_cache.clone(),
            reasoning_effort: self.reasoning_effort.clone(),
            max_tokens: self.max_tokens.clone(),
            context_tokens: self.context_tokens.clone(),
            module_concurrency: self.module_concurrency.clone(),
            skip_libraries: self.skip_libraries,
            naming_floor: self.naming_floor,
            naming_floor_sweep: self.naming_floor_sweep,
            reconcile_prior_diff: self.reconcile_prior_diff,
            prior_version: self.prior_version.clone(),
        }
    }
}

/// Which default-on flags the user actually typed (commander's value
/// sources). Without it, a true sweep value counts as explicit.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlagExplicitness {
    pub naming_floor_sweep: Option<bool>,
}

fn truthy(v: &Option<String>) -> bool {
    v.as_deref().is_some_and(|s| !s.is_empty())
}

fn enum_violation<T: serde::Serialize + Copy>(
    flag: &str,
    value: &Option<String>,
    allowed: &[T],
) -> Option<String> {
    let v = value.as_deref()?;
    let names: Vec<String> = allowed.iter().map(|a| enum_name(*a)).collect();
    if names.iter().any(|n| n == v) {
        return None;
    }
    Some(format!(
        "{flag} must be one of: {} (got \"{v}\")",
        names.join(", ")
    ))
}

/// `checkFlagInvariants`: one message per violation, preconditions first
/// (in flag-declaration order), then the enum values.
pub fn check_flag_invariants(
    opts: &CommandOptions,
    explicit: Option<FlagExplicitness>,
) -> Vec<String> {
    let sweep_explicitly_on = explicit
        .and_then(|e| e.naming_floor_sweep)
        .unwrap_or(opts.naming_floor_sweep == Some(true));
    let rules = [
        (opts.split_pure, "--split-pure", opts.split, "--split"),
        (
            truthy(&opts.split_ledger),
            "--split-ledger",
            opts.split,
            "--split",
        ),
        (
            sweep_explicitly_on,
            "--naming-floor-sweep",
            opts.naming_floor != Some(false),
            "--naming-floor",
        ),
    ];
    let mut out: Vec<String> = rules
        .iter()
        .filter(|(when, _, needs, _)| *when && !*needs)
        .map(|(_, flag, _, prereq)| format!("{flag} requires {prereq}"))
        .collect();
    out.extend(enum_violation(
        "--bundler",
        &opts.bundler,
        &SELECTABLE_BUNDLERS,
    ));
    out.extend(enum_violation(
        "--minifier",
        &opts.minifier,
        &SELECTABLE_MINIFIERS,
    ));
    out.extend(opts.fast_tier().err());
    out
}

/// A failure below the settings step: `Error: <message>` after the
/// finally (the TS uncaught-exception class, its headline only).
struct Crash(String);

impl From<String> for Crash {
    fn from(s: String) -> Self {
        Crash(s)
    }
}

/// How the pipeline body ended.
enum Ended {
    /// Ran to its end (exit code: 0, or 1 if a report marked it failed).
    Done(i32),
    /// `process.exit(code)` inside the body — skips the finally.
    Immediate(i32),
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

/// The action: `humanify <input> [options]`. Returns the process exit code.
pub fn run(input: &str, values: &OptionValues) -> i32 {
    let opts = CommandOptions::from_values(values);
    let explicit = FlagExplicitness {
        naming_floor_sweep: Some(
            values.source("namingFloorSweep") == Some(ValueSource::Cli)
                && opts.naming_floor_sweep == Some(true),
        ),
    };
    let violations = check_flag_invariants(&opts, Some(explicit));
    if !violations.is_empty() {
        for m in violations {
            eprintln!("Error: {m}");
        }
        return 1;
    }
    let split_list = |v: &Option<String>| -> Vec<String> {
        v.as_deref()
            .map(|s| s.split(',').map(str::to_string).collect())
            .unwrap_or_default()
    };
    let switches =
        match SwitchState::configure(&split_list(&opts.disable), &split_list(&opts.probe)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error: {e}");
                return 1;
            }
        };
    verbose().set_level(opts.verbose.clamp(0, 2) as u8);
    let log_file = match open_log_file(&opts) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("Error: {msg}");
            return 1;
        }
    };
    let use_rich_ui =
        std::io::stderr().is_terminal() && (verbose().level() < 2 || opts.log_file.is_some());
    let mut renderer = create_progress_renderer(use_rich_ui);
    let settings = match resolve_settings(&opts.settings_input()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            return 1;
        }
    };
    let profiler = humanify_core::profiling::Profiler::new(opts.profile.is_some());
    profiler.install_global();
    // buildProvider: the cache wrapper mkdirs its dir at construction.
    if let Some(dir) = &settings.llm_cache_dir
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return crash(
            &mut *renderer,
            &node_fs_error(&e, "mkdir", dir),
            log_file.is_some(),
        );
    }

    let provider = match build_provider(&settings) {
        Ok(p) => p,
        Err(e) => return crash(&mut *renderer, &e, log_file.is_some()),
    };

    let ended = run_pipeline(
        input,
        &opts,
        &settings,
        &switches,
        &provider,
        &profiler,
        &mut *renderer,
    );
    if let Ok(Ended::Immediate(code)) = ended {
        return code;
    }
    report_memo(&provider, &mut *renderer);
    // The TS `finally`.
    finalize_profile(&opts, input, &profiler, &mut *renderer);
    renderer.finish();
    if log_file.is_some() {
        crate::log::verbose().reset_output();
        debug_reset_output();
    }
    match ended {
        Ok(Ended::Done(code)) | Ok(Ended::Immediate(code)) => code,
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
fn open_log_file(opts: &CommandOptions) -> Result<Option<SharedFile>, String> {
    let Some(path) = &opts.log_file else {
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
    opts: &CommandOptions,
    input: &str,
    profiler: &humanify_core::profiling::Profiler,
    renderer: &mut dyn ProgressRenderer,
) {
    let Some(path) = &opts.profile else { return };
    use humanify_core::profiling::{format_profile_summary, to_trace_events};
    let report = profiler.finalize(Some(input));
    let trace =
        serde_json::to_string_pretty(&to_trace_events(&report)).expect("a trace serializes");
    if let Err(e) = std::fs::write(path, trace) {
        renderer.message(&format!("Error: cannot write {path}: {e}"));
        return;
    }
    renderer.message(&format_profile_summary(&report));
    renderer.message(&format!("Profile written to {path}"));
}

fn parse_enum<T: serde::de::DeserializeOwned>(v: &Option<String>) -> Option<T> {
    v.as_deref()
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
}

/// runPipeline, through the ported stages.
fn run_pipeline(
    input: &str,
    opts: &CommandOptions,
    settings: &Settings,
    switches: &SwitchState,
    provider: &dyn NameProvider,
    profiler: &humanify_core::profiling::Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Ended, Crash> {
    // ensureFileExists → err(): red message, process.exit(1), no finally.
    if !Path::new(input).exists() {
        eprintln!("\x1b[31mFile {input} not found\x1b[0m");
        return Ok(Ended::Immediate(1));
    }
    let Some(sim_path) = &opts.simulate_llm_latency else {
        return pipeline_body(
            input, opts, settings, switches, provider, profiler, renderer,
        );
    };
    // The instrument: the run's own answers, priced on a virtual clock
    // with the rate limiter's slot count (crate::llm_sim).
    let sim = crate::llm_sim::LatencySim::new(provider, settings.concurrency as usize);
    let ended = pipeline_body(input, opts, settings, switches, &sim, profiler, renderer);
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

/// The split namer's prompt budget: `--context-tokens` (else the default
/// model context) less the `--max-tokens` completion reserve (finding #39).
fn split_namer_budget(settings: &Settings) -> SplitNamerBudget {
    SplitNamerBudget::for_model(
        settings
            .context_tokens
            .map_or(DEFAULT_CONTEXT_TOKENS, |t| t as u64),
        settings
            .max_tokens
            .map_or(humanify_model::llm::DEFAULT_MAX_TOKENS, |t| t as u64),
    )
}

/// `buildProvider`: cache OUTERMOST (hits bypass the limiter and the debug
/// wrapper), then the rate limiter sized over both lanes, the debug
/// wrapper, the HTTP client. A miss goes to the endpoint and a non-empty
/// answer is written to the cache, exactly as the TS provider stack does.
fn build_provider(settings: &Settings) -> Result<LlmClient<LiveStack>, String> {
    let max_tokens = settings.max_tokens.map(|t| t as u64);
    let reasoning_effort = settings.reasoning_effort.map(str::to_string);
    let mut config = LlmConfig::new(&settings.endpoint, &settings.api_key, &settings.model);
    config.timeout_ms = settings.timeout as u64;
    config.reasoning_effort = reasoning_effort.clone();
    if let Some(t) = max_tokens {
        config.max_tokens = t;
    }
    let defaults = RateLimitConfig::default();
    let rate = RateLimitConfig {
        // The OUTER bound over both of the processor's limiters: the
        // bundler is not detected yet, so the widest default lane.
        max_concurrent: (settings.concurrency
            + settings
                .module_concurrency
                .unwrap_or(f64::from(MAX_DEFAULT_MODULE_CONCURRENCY)))
            as usize,
        retry_attempts: settings
            .retry_attempts
            .map_or(defaults.retry_attempts, |r| r as u32),
        ..defaults
    };
    LlmClient::live(LiveOptions {
        config,
        rate,
        key_params: CacheKeyParams {
            model: settings.model.clone(),
            temperature: Some(0.0),
            max_tokens,
            reasoning_effort,
        },
        cache_dir: settings
            .llm_cache_dir
            .as_ref()
            .map(std::path::PathBuf::from),
        metrics: None,
        log: Some(crate::log::llm_log_sink()),
    })
    .map_err(|e| e.to_string())
}

fn pipeline_body(
    input: &str,
    opts: &CommandOptions,
    settings: &Settings,
    switches: &SwitchState,
    provider: &dyn NameProvider,
    profiler: &humanify_core::profiling::Profiler,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Ended, Crash> {
    use humanify_core::profiling::phase;
    let ph = phase("read+detect");
    let bundled_code = read_utf8(input)?;
    let (toolchain, markers) = detect_stage(&bundled_code, opts, switches, profiler);
    let prior = load_prior_version_code(opts, renderer)?;
    drop(ph);

    // armRecorders: the diagnostics/dump recorders arrive with the stages
    // that feed them.

    // Stages 3-5: unpack (the Bun adapter names its vendor files inside),
    // then library detection picks the files the per-file stages process.
    let out_dir = opts.output_dir.as_deref().unwrap_or("output");
    let prior_path = opts
        .prior_version
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(Path::new);
    // The run's per-dispatch recorder (finding #65): `--dump-artifacts`
    // streams prompts.jsonl + cache-keys.jsonl rows to part files as the
    // dispatches commit; `--dump-asks` retains the small ask rows;
    // WITHOUT either flag nothing per ask survives its dispatch — the
    // accumulated dispatch records were the ~99GB holder of a full-bundle
    // fresh run.
    let naming_config = naming_config(settings, &toolchain, opts, switches);
    let mut dispatch_log = match opts.dump_artifacts.as_deref() {
        Some(dir) => humanify_core::artifact_dump::DispatchLog::dump(
            naming_config.params.clone(),
            Path::new(dir),
        )
        .map_err(|e| Crash(node_fs_error(&e, "open", dir)))?,
        None => {
            if opts.dump_asks.is_some() {
                humanify_core::artifact_dump::DispatchLog::asks(naming_config.params.clone())
            } else {
                humanify_core::artifact_dump::DispatchLog::off(naming_config.params.clone())
            }
        }
    };
    let ph = phase("unpack+vendor");
    let mut unpacked = unpack_bundle(
        &bundled_code,
        Path::new(out_dir),
        &toolchain,
        provider,
        &mut dispatch_log,
        prior_path,
        switches.switch_on(Switch::ManifestPriorOrder),
        profiler,
        renderer,
    )?;
    drop(ph);
    let ph = phase("library-detection");
    let unpacked_files = std::mem::take(&mut unpacked.files);
    let (files_to_process, mixed_files) = if settings.skip_libraries {
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
        opts,
        config: naming_config,
        log: &mut dispatch_log,
        prior: prior.as_deref(),
        provider,
        profiler,
        mixed_files: &mixed_files,
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
    if opts.split
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
            split_ledger: opts.split_ledger.as_deref(),
            split_pure: opts.split_pure,
            markers,
            switches,
            provider,
            log: &mut dispatch_log,
            namer_budget: split_namer_budget(settings),
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
    }
    if let Some(NamedFile {
        outcome,
        fresh,
        path,
    }) = &last
    {
        let _ph = phase("reports");
        let reports = RunReports {
            opts,
            outcome,
            fresh,
            placement: &placement,
            split: split_sections.as_ref(),
            post_split: &post_split,
        };
        reports.write_diagnostics(renderer)?;
        if let Some(dest) = &opts.stats_json {
            write_stats_json(
                dest,
                outcome,
                &post_split.claims,
                &unpacked.vendor_naming,
                &toolchain,
                split_method,
                renderer,
            )?;
        }
        if let Some(dir) = &opts.dump_artifacts {
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
                    flags: dump_flags(opts, settings, &toolchain),
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
        if let Some(dest) = &opts.dump_asks {
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
    Ok(Ended::Done(failures.close(renderer)))
}

/// The run's recorded reports, written after the split in the TS order
/// (unified.ts runPipeline: diagnostics, eval stats, the artifact dump,
/// the rename ledger).
struct RunReports<'a> {
    opts: &'a CommandOptions,
    outcome: &'a NamingOutcome,
    /// The formatted text the naming stage ran on.
    fresh: &'a str,
    placement: &'a humanify_core::place::trail::PlacementTrail,
    /// The split's dump sections (its input text is the placement
    /// trail's anchor); None without a split (the trail is empty then).
    split: Option<&'a humanify_core::artifact_dump::SplitSections>,
    /// The finishing passes' trail rows and claims.
    post_split: &'a crate::split_stage::PostSplitRecords,
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

/// meta.json's `flags` (unified.ts writeDumpArtifacts' call): the resolved
/// selection and the decision levers; unset optional flags are absent.
fn dump_flags(
    opts: &CommandOptions,
    settings: &Settings,
    toolchain: &Toolchain,
) -> humanify_model::js::JsValue {
    use humanify_model::js::{JsObject, JsValue};
    let mut f = JsObject::new();
    f.insert("split", JsValue::Bool(opts.split));
    f.insert("splitPure", JsValue::Bool(opts.split_pure));
    f.insert("bundler", JsValue::str(enum_name(toolchain.bundler)));
    f.insert("minifier", JsValue::str(enum_name(toolchain.minifier)));
    f.insert("skipLibraries", JsValue::Bool(settings.skip_libraries));
    f.insert(
        "reconcilePriorDiff",
        JsValue::Bool(settings.levers.reconcile_prior_diff),
    );
    f.insert("namingFloor", JsValue::Bool(settings.levers.naming_floor));
    f.insert(
        "namingFloorSweep",
        JsValue::Bool(settings.levers.naming_floor_sweep),
    );
    f.insert("model", JsValue::str(settings.model.as_str()));
    f.insert_opt(
        "reasoningEffort",
        settings.reasoning_effort.map(JsValue::str),
    );
    f.insert_opt("disable", opts.disable.as_deref().map(JsValue::str));
    f.insert_opt("probe", opts.probe.as_deref().map(JsValue::str));
    JsValue::Object(f)
}

/// `gitShortSha()`: the CWD's `git rev-parse --short HEAD`, "unknown" when
/// git cannot answer.
fn git_short_sha() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

impl RunReports<'_> {
    /// `--diagnostics` (buildDiagnosticsReport + writeDiagnosticsFile):
    /// the naming stage's report with the split's placement trail after
    /// the strategy trail; indent 2 and a trailing newline.
    fn write_diagnostics(&self, renderer: &mut dyn ProgressRenderer) -> Result<(), Crash> {
        use humanify_core::naming::report::diagnostics::{
            AnchorTexts, DiagnosticsInputs, build_diagnostics_report,
        };
        use humanify_model::js::{JsObject, JsValue, stringify_pretty};
        let (Some(dest), Some(coverage)) = (&self.opts.diagnostics, &self.outcome.coverage) else {
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
                commit: git_short_sha(),
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
        let (Some(dir), Some(bundle)) = (&self.opts.rename_ledger, &self.outcome.rename_ledger)
        else {
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
    opts: &CommandOptions,
    switches: &SwitchState,
    profiler: &humanify_core::profiling::Profiler,
) -> (Toolchain, MarkerOffer) {
    let span = profiler.pipeline_span("detection");
    let detection = humanify_core::detect::detect_bundle(bundled_code);
    let toolchain = resolve_toolchain(
        &detection,
        parse_enum::<BundlerType>(&opts.bundler),
        parse_enum::<MinifierType>(&opts.minifier),
    );
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
    } else if switches.switch_on(Switch::FossilSplit) {
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
    opts: &'a CommandOptions,
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
            if !self.opts.split
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

/// `--stats-json` (writeEvalStats): the naming stage's record + the vendor
/// namer's tally (only when it was asked anything) + the selection record.
fn write_stats_json(
    dest: &str,
    outcome: &NamingOutcome,
    later_claims: &humanify_core::rename::validated::RenameClaimStats,
    vendor: &humanify_core::modules::vendor_names::VendorNamingStats,
    toolchain: &Toolchain,
    split_method: Option<humanify_model::stats::SplitMethodStats>,
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
    stats.split_method = split_method;
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
fn naming_config(
    settings: &Settings,
    toolchain: &Toolchain,
    opts: &CommandOptions,
    switches: &SwitchState,
) -> NamingConfig {
    NamingConfig {
        never_rename: toolchain.never_rename.piece,
        module_group_size: toolchain.tuning.piece.module_group_size(),
        layout: toolchain.layout.piece,
        module_wrappers: toolchain.module_wrappers.piece,
        name_profile: toolchain.name_profile.piece,
        skip_libraries: settings.skip_libraries,
        reconcile_prior_diff: settings.levers.reconcile_prior_diff,
        naming_floor: settings.levers.naming_floor,
        naming_floor_sweep: settings.levers.naming_floor_sweep,
        source_map: false,
        emit_rename_ledger: opts.rename_ledger.is_some(),
        family_permute_disabled: switches.switch_on(Switch::FamilyPermute),
        params: CacheKeyParams {
            model: settings.model.clone(),
            // The TS passes a literal 0 (unified.ts buildProvider).
            temperature: Some(0.0),
            max_tokens: settings.max_tokens.map(|t| t as u64),
            reasoning_effort: settings.reasoning_effort.map(str::to_string),
        },
        capture_dump: opts.dump_artifacts.is_some(),
        shingle_probe: switches.switch_on(Switch::ShingleProbe),
        fast: opts.fast_tier().unwrap_or_default(),
        // The rendered-prompt window (finding #65): every in-flight call
        // plus a backfill headroom over the provider stack's outer
        // concurrency bound (build_provider's), never the whole round.
        prompt_window: DEFAULT_PROMPT_WINDOW.max(
            (settings.concurrency
                + settings
                    .module_concurrency
                    .unwrap_or(f64::from(MAX_DEFAULT_MODULE_CONCURRENCY))) as usize
                + PROMPT_WINDOW_HEADROOM,
        ),
        tunables: {
            let d = WaveTunables::default();
            WaveTunables {
                batch_size: settings.batch_size.map_or(d.batch_size, |n| n as usize),
                max_retries: settings
                    .max_retries_per_identifier
                    .map_or(d.max_retries, |n| n as u32),
                max_free_retries: settings.max_free_retries.map(|n| n as u32),
                lane_threshold: settings
                    .lane_threshold
                    .map_or(d.lane_threshold, |n| n as usize),
                reask_limit: settings
                    .rename_retries
                    .map_or(d.reask_limit, |n| n as usize),
            }
        },
    }
}

/// `loadPriorVersionCode`: an empty prior is an error, never a silent
/// zero-transfer run.
fn load_prior_version_code(
    opts: &CommandOptions,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Option<String>, Crash> {
    let Some(path) = opts.prior_version.as_deref().filter(|p| !p.is_empty()) else {
        return Ok(None);
    };
    let code = read_utf8(path)?;
    if humanify_model::js::trim(&code).is_empty() {
        return Err(Crash(format!("--prior-version file is empty: {path}")));
    }
    renderer.message(&format!("Prior version: loaded from {path}"));
    Ok(Some(code))
}
