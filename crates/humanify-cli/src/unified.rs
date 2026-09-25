//! The pipeline command's action (TS: `src/commands/unified.ts`
//! configureUnifiedCommand's action + runPipeline + the report tail).
//!
//! Order is the TS order, because each step's failure mode is contract
//! (14 §2) and the scenario gate (test/parity/wpb4-scenarios.json) compares
//! them end to end against the TS binary:
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
//! Stages that are not ported end the run with an `ERROR:` block and
//! `stages::EXIT_NOT_YET`, after the ported stages before them have run
//! (and written their part of the tree).

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use humanify_core::naming::driver::{
    NamingConfig, NamingHooks, NamingInput, NamingOutcome, run_naming,
};
use humanify_core::unpack::select_unpack_adapter;
use humanify_llm::LlmClient;
use humanify_llm::provider::{LiveOptions, LiveStack};
use humanify_model::detection::{
    BundlerType, MinifierType, SELECTABLE_BUNDLERS, SELECTABLE_MINIFIERS,
};
use humanify_model::llm::{CacheKeyParams, LlmConfig, NameProvider, RateLimitConfig};

use crate::commander::{OptionValues, ValueSource};
use crate::kill_switches::{Switch, SwitchState};
use crate::log::{debug_reset_output, debug_set_output, verbose};
use crate::pipeline_config::{build_pipeline_config, enum_name};
use crate::progress::{ProgressRenderer, create_progress_renderer};
use crate::settings::{Settings, SettingsInput, resolve_settings};
use crate::stages::{self, EXIT_NOT_YET, Stage};
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
    pub profile: Option<String>,
    pub prior_version: Option<String>,
    pub reconcile_prior_diff: Option<bool>,
    pub naming_floor: Option<bool>,
    pub naming_floor_sweep: Option<bool>,
    pub reasoning_effort: Option<String>,
    pub disable: Option<String>,
    pub probe: Option<String>,
    pub max_tokens: Option<String>,
    pub module_concurrency: Option<String>,
    pub llm_cache: Option<String>,
    pub ambiguity_probe: Option<String>,
    pub split_ledger: Option<String>,
    pub split_pure: bool,
    pub rename_ledger: Option<String>,
    pub stats_json: Option<String>,
    pub dump_artifacts: Option<String>,
    /// Rust-only (surface::RUST_ONLY_OPTIONS).
    pub beautified_input: Option<String>,
    /// Rust-only (surface::RUST_ONLY_OPTIONS): the blessed hash-byte injection.
    pub inject_ts_hashes: Option<String>,
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
            profile: s("profile"),
            prior_version: s("priorVersion"),
            reconcile_prior_diff: v.bool("reconcilePriorDiff"),
            naming_floor: v.bool("namingFloor"),
            naming_floor_sweep: v.bool("namingFloorSweep"),
            reasoning_effort: s("reasoningEffort"),
            disable: s("disable"),
            probe: s("probe"),
            max_tokens: s("maxTokens"),
            module_concurrency: s("moduleConcurrency"),
            llm_cache: s("llmCache"),
            ambiguity_probe: s("ambiguityProbe"),
            split_ledger: s("splitLedger"),
            split_pure: v.bool("splitPure").unwrap_or(false),
            rename_ledger: s("renameLedger"),
            stats_json: s("statsJson"),
            dump_artifacts: s("dumpArtifacts"),
            beautified_input: s("beautifiedInput"),
            inject_ts_hashes: s("injectTsHashes"),
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
            llm_cache: self.llm_cache.clone(),
            reasoning_effort: self.reasoning_effort.clone(),
            max_tokens: self.max_tokens.clone(),
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
    /// An unported stage.
    NotYet(Stage),
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
    // Reaching an unported stage is a documented failure: its ERROR: block
    // goes through the renderer like every TS report, before the finally.
    if let Ok(Ended::NotYet(stage)) = ended {
        renderer.message(&stages::not_yet_block(stage));
    }
    // The TS `finally`.
    finalize_profile(&opts, input, &profiler, &mut *renderer);
    renderer.finish();
    if log_file.is_some() {
        crate::log::verbose().reset_output();
        debug_reset_output();
    }
    match ended {
        Ok(Ended::Done(code)) | Ok(Ended::Immediate(code)) => code,
        Ok(Ended::NotYet(_)) => EXIT_NOT_YET,
        Err(Crash(message)) => {
            eprintln!("Error: {message}");
            1
        }
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
    pipeline_body(
        input, opts, settings, switches, provider, profiler, renderer,
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
    let cache = settings.llm_cache_dir.as_ref().map(|dir| {
        (
            std::path::PathBuf::from(dir),
            CacheKeyParams {
                model: settings.model.clone(),
                temperature: Some(0.0),
                max_tokens,
                reasoning_effort,
            },
        )
    });
    LlmClient::live(LiveOptions {
        config,
        rate,
        cache,
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
    let bundled_code = read_utf8(input)?;

    // Stages 1-2: detect, then select the unpack adapter.
    let span = profiler.pipeline_span("detection");
    let detection = humanify_core::detect::detect_bundle(&bundled_code);
    let config = build_pipeline_config(
        &detection,
        parse_enum::<BundlerType>(&opts.bundler),
        parse_enum::<MinifierType>(&opts.minifier),
    );
    let bundler_name = enum_name(config.bundler_type);
    span.end(Some(
        humanify_model::profiling::JsObject::new()
            .with("bundler", bundler_name.clone())
            .with("adapter", config.unpack_adapter_name),
    ));
    verbose().log(&format!(
        "Bundle detection: bundler={bundler_name} ({}), minifier={}, adapter={}",
        enum_name(config.bundler_tier),
        enum_name(config.minifier_type),
        config.unpack_adapter_name
    ));
    let adapter = select_unpack_adapter(config.unpack_adapter_name)?;
    let fossil_split =
        adapter.provides_module_fossils() && !switches.switch_on(Switch::FossilSplit);
    if fossil_split {
        verbose().log("Fossil split: module fossils will drive statement assignment");
    }
    if !detection.signals.is_empty() {
        let signals: Vec<String> = detection
            .signals
            .iter()
            .map(|s| format!("{}:{}", s.source, s.pattern))
            .collect();
        verbose().debug(&format!("Detection signals: {}", signals.join(", ")));
    }

    let prior = load_prior_version_code(opts, renderer)?;

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
    let ts_hashes = load_ts_hashes(opts)?;
    let unpacked = unpack_bundle(
        &bundled_code,
        Path::new(out_dir),
        adapter,
        provider,
        prior_path,
        ts_hashes.as_ref().map(|h| h.factories.as_slice()),
        profiler,
        renderer,
    )?;
    let files_to_process = if settings.skip_libraries {
        filter_libraries(unpacked.files, adapter, profiler, renderer)?.files_to_process
    } else {
        unpacked.files
    };

    // The per-file stages (processFile): format, then the naming stage
    // (graph, match, transfer, waves, floor, generate, the post-generate
    // passes) — core::naming::driver::run_naming, the one owner.
    let naming_config = naming_config(settings, &config, opts, switches);
    let total = files_to_process.len();
    let mut last: Option<(NamingOutcome, std::path::PathBuf)> = None;
    for (i, file) in files_to_process.iter().enumerate() {
        renderer.message(&format!("Processing file {}/{total}", i + 1));
        let code = read_utf8(&file.path.display().to_string())?;
        if humanify_model::js::trim(&code).is_empty() {
            verbose().log(&format!("Skipping empty file {}", file.path.display()));
            continue;
        }
        // Stage 6 is not ported; its output (the formatted text) is
        // supplied by the TS (`--beautified-input`) — ONE file's text.
        let Some(formatted_path) = &opts.beautified_input else {
            return Ok(Ended::NotYet(stages::FORMAT));
        };
        if total > 1 {
            return Err(Crash(format!(
                "--beautified-input holds one file's formatted text, but {total} files reached the per-file stages"
            )));
        }
        let formatted = read_utf8(formatted_path)?;
        let outcome = run_naming(
            &NamingInput {
                fresh: &formatted,
                prior: prior.as_deref(),
                library: None,
            },
            &naming_config,
            &NamingHooks::default(),
            &provider,
        )?;
        if let Some(text) = &outcome.coverage_text {
            renderer.message(text);
        }
        if outcome.misses > 0 || outcome.errors > 0 {
            verbose().log(&format!(
                "Naming: {} cache miss(es), {} provider error(s)",
                outcome.misses, outcome.errors
            ));
        }
        if !opts.split {
            // skipFileWrite is off without --split: the file is rewritten.
            if let Some(code) = &outcome.code {
                std::fs::write(&file.path, code).map_err(|e| {
                    Crash(node_fs_error(&e, "open", &file.path.display().to_string()))
                })?;
            }
        }
        last = Some((outcome, file.path.clone()));
    }
    renderer.message(&format!(
        "Done! You can find your unminified code in {out_dir}"
    ));
    report_vendor_naming(&unpacked.vendor_naming, renderer);

    if opts.split
        && let Some((outcome, source)) = &last
        && let Some(code) = &outcome.code
    {
        let split = crate::split_stage::SplitStageInput {
            output_dir: Path::new(out_dir),
            input_file: Path::new(input),
            processed_source: Some(source),
            prior_version: prior_path,
            split_ledger: opts.split_ledger.as_deref(),
            split_pure: opts.split_pure,
            fossil: fossil_split,
            switches,
            ts_partitions: ts_hashes.as_ref().map(|h| &h.partitions),
            provider,
        };
        let span = profiler.pipeline_span("split");
        let ended =
            crate::split_stage::run_split(code, outcome.prior_carry.as_ref(), &split, renderer)?;
        span.end(Some(humanify_model::profiling::JsObject::new().with(
            "stable",
            matches!(ended, crate::split_stage::SplitEnded::Complete),
        )));
    }

    // `--stats-json` (writeEvalStats): the naming stage's record + the
    // vendor namer's tally (only when it was asked anything) + the path.
    if let (Some(dest), Some((outcome, _))) = (&opts.stats_json, &last)
        && outcome.coverage.is_some()
    {
        let v = &unpacked.vendor_naming;
        let mut stats = outcome.eval_stats();
        if v.named + v.declined + v.echoed + v.batches_failed > 0 {
            stats.vendor_naming = Some(humanify_model::stats::VendorNamingStats {
                named: v.named as f64,
                declined: v.declined as f64,
                echoed: v.echoed as f64,
                batches_failed: v.batches_failed as f64,
            });
        }
        stats.selection = Some(crate::pipeline_config::pipeline_selection_record(&config));
        crate::writers::write_eval_stats(Path::new(dest), &stats)
            .map_err(|e| Crash(node_fs_error(&e, "open", dest)))?;
        renderer.message(&format!("Eval stats written to {dest}"));
    }
    Ok(Ended::Done(0))
}

/// `--inject-ts-hashes <dir>`: the TS dump's factory hashes (modules.json)
/// and statementHash partition (partitions.json) — the blessed exemption.
struct TsHashBytes {
    factories: Vec<humanify_core::unpack::gate::TsFactoryHash>,
    partitions: humanify_model::dump::PartitionsFile,
}

fn load_ts_hashes(opts: &CommandOptions) -> Result<Option<TsHashBytes>, Crash> {
    let Some(dir) = opts.inject_ts_hashes.as_deref() else {
        return Ok(None);
    };
    let dir = Path::new(dir);
    let factories = humanify_core::unpack::gate::read_ts_factory_hashes(&dir.join("modules.json"))?;
    let path = dir.join("partitions.json");
    let text = read_utf8(&path.display().to_string())?;
    let partitions =
        serde_json::from_str(&text).map_err(|e| Crash(format!("{}: {e}", path.display())))?;
    Ok(Some(TsHashBytes {
        factories,
        partitions,
    }))
}

/// The plugin options the naming stage decides by (createRenamePlugin's).
fn naming_config(
    settings: &Settings,
    config: &humanify_model::pipeline::PipelineConfig,
    opts: &CommandOptions,
    switches: &SwitchState,
) -> NamingConfig {
    NamingConfig {
        bundler: Some(enum_name(config.bundler_type)),
        minifier: Some(enum_name(config.minifier_type)),
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
