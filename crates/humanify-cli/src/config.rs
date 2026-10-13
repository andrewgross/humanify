//! THE configuration owner (config sweep step 4, 2026-10-13;
//! docs/config-consolidation.md §3).
//!
//! One typed, immutable [`Config`], built ONCE here from the parsed argv
//! (`surface`, clap) and the one environment variable the binary reads
//! (`HUMANIFY_API_KEY`, through `crate::env`), validated here — every
//! refusal before any work starts — and handed down by reference. Nothing
//! below reads argv, the environment or a file for a setting: the stages
//! take `&Config` or the typed values built from it (`NamingConfig`,
//! `LlmConfig`, …). `tests/env_guard.rs` fails any configuration read
//! outside this module.
//!
//! It replaced four resolvers that each owned a slice of the run's
//! configuration (`Settings`, the kill-switch `SwitchState`, the naming
//! schedule's `Levers`, and the raw option fields `unified.rs` read
//! directly), and the defaults that lived twice (the LLM timeout in two
//! crates; the model in the pipeline and the `unpack` verb, with two
//! different values). Every knob has ONE default, below or at the one
//! constant it names.
//!
//! The whole resolved config, minus the API key, is the run's record
//! ([`Config::record`]): the stats file's `config` block, which the eval's
//! run manifest copies.
//!
//! Shaped for the next two sweep steps: each section is a future
//! `--config` TOML table (step 5), and `library` is where the library
//! decision lands (step 6).

use serde::Serialize;

use humanify_core::fast::{FastTier, Lever, Levers};
use humanify_core::naming::waves::batch::WaveTunables;
use humanify_model::detection::{
    BundlerType, MinifierType, SELECTABLE_BUNDLERS, SELECTABLE_MINIFIERS,
};
use humanify_model::llm::{DEFAULT_LLM_TIMEOUT_MS, RateLimitConfig};

use crate::kill_switches::SwitchState;
use crate::pipeline_config::enum_name;
use crate::surface::CommandOptions;

/// The OpenAI-compatible endpoint when none is given.
pub const DEFAULT_ENDPOINT: &str = "https://api.openai.com/v1";

/// The model when none is given — for the pipeline AND the `unpack` verb
/// (they had two defaults until 2026-10-13: the verb's was the local
/// measurement model, `openai/gpt-oss-20b`).
pub const DEFAULT_MODEL: &str = "gpt-4o-mini";

/// The output directory when none is given.
pub const DEFAULT_OUTPUT_DIR: &str = "output";

/// Function-lane LLM concurrency when `-c` is not given.
pub const DEFAULT_CONCURRENCY: u32 = 50;

/// The module-lane width when `--module-concurrency` is not given. It was
/// the wider entry of a per-bundler lane table (esbuild 40, others 20)
/// whose per-bundler lookup nothing called (docs/plugin-spec.md I24): the
/// table was deleted 2026-10-04, keeping the one value anything read.
pub const DEFAULT_MODULE_CONCURRENCY: u32 = 40;

/// The one environment variable the binary reads: the API key. A secret
/// never goes on the command line (it lands in launch records) nor in the
/// run record; `--api-key` and the `OPENAI_API_KEY` fallback were retired
/// 2026-10-13.
pub const API_KEY_VAR: &str = "HUMANIFY_API_KEY";

/// Refused when no API key is set.
pub const MISSING_API_KEY: &str = "API key required: set HUMANIFY_API_KEY in the environment \
     (or a .env file in the working directory)";

/// LLM request retries when `--retries` is not given: the rate limiter's
/// own default (one number).
pub fn default_retries() -> u32 {
    RateLimitConfig::default().retry_attempts
}

/// The split namer's model context window when `--context-tokens` is not
/// given.
pub use humanify_core::place::assign::namer::DEFAULT_CONTEXT_TOKENS;

/// The completion budget a request gets when `--max-tokens` is not given.
pub use humanify_model::llm::DEFAULT_MAX_TOKENS;

/// The API key: held, never printed, never recorded.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// The secret itself, for the one place that sends it (the provider).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

impl Serialize for ApiKey {
    /// The record says the key is set, and where it came from — never what
    /// it is.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("<set: {API_KEY_VAR}>"))
    }
}

/// `--reasoning-effort`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

impl ReasoningEffort {
    pub fn name(self) -> &'static str {
        match self {
            ReasoningEffort::Low => "low",
            ReasoningEffort::Medium => "medium",
            ReasoningEffort::High => "high",
        }
    }
}

/// The whole run configuration. Built once ([`Config::build`]), then only
/// ever borrowed: there is no `&mut` API.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub io: Io,
    pub llm: Llm,
    pub matching: Matching,
    pub naming: Naming,
    pub library: Library,
    pub unpack: Unpack,
    pub split: Split,
    pub switches: SwitchState,
    pub diagnostics: Diagnostics,
    pub perf: Perf,
}

/// What is read and where the output goes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Io {
    pub input: String,
    pub output_dir: String,
}

/// The model endpoint and the request budget.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Llm {
    pub endpoint: String,
    pub model: String,
    pub api_key: ApiKey,
    pub timeout_ms: u64,
    pub retries: u32,
    /// Function-lane concurrency (`-c`).
    pub concurrency: u32,
    /// Module-lane concurrency.
    pub module_concurrency: u32,
    /// The per-request completion budget. Unset stays unset: the request
    /// then carries [`DEFAULT_MAX_TOKENS`], but the LLM cache key records
    /// "unset" — a different key from an explicit 6000.
    pub max_tokens: Option<u64>,
    /// The model context window (sizes the split namer's prompts).
    pub context_tokens: u64,
    pub reasoning_effort: Option<ReasoningEffort>,
    pub cache_dir: Option<String>,
}

impl Llm {
    /// The completion budget a request actually gets.
    pub fn completion_tokens(&self) -> u64 {
        self.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS)
    }

    /// The provider stack's outer concurrency bound: both lanes.
    pub fn outer_concurrency(&self) -> usize {
        (self.concurrency + self.module_concurrency) as usize
    }
}

/// Cross-version matching against a prior release.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Matching {
    pub prior_version: Option<String>,
    /// The prior-diff reconcile pass; implied off with no prior.
    pub reconcile_prior_diff: bool,
}

/// The naming stage's schedule and budgets.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Naming {
    #[serde(serialize_with = "serialize_schedule")]
    pub schedule: FastTier,
    pub batch_size: usize,
    /// Per-identifier LLM call limit, initial + retries.
    pub max_retries: u32,
    /// Name-conflict re-ask limit.
    pub rename_retries: usize,
    /// Cross-lane collision retry limit (unset: the processor's own rule).
    pub max_free_retries: Option<u32>,
    pub lane_threshold: usize,
    pub naming_floor: bool,
    /// Implied off when the floor is off — the sweep cannot run without it.
    pub naming_floor_sweep: bool,
}

impl Naming {
    /// The wave tunables the processor reads.
    pub fn tunables(&self) -> WaveTunables {
        WaveTunables {
            batch_size: self.batch_size,
            max_retries: self.max_retries,
            max_free_retries: self.max_free_retries,
            lane_threshold: self.lane_threshold,
            reask_limit: self.rename_retries,
        }
    }
}

/// Library code: whether it is skipped, and the report-only discovery's
/// index. (The library decision, sweep step 6, replaces `skip_libraries`.)
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub skip_libraries: bool,
    pub index: Option<String>,
    pub discovery_units: bool,
}

/// Forced toolchain verdicts (detection decides when unset).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unpack {
    pub bundler: Option<BundlerType>,
    pub minifier: Option<MinifierType>,
}

/// The multi-file tree.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Split {
    pub enabled: bool,
    pub pure: bool,
    pub ledger: Option<String>,
}

/// What the run reports, and where.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// 0, 1 (`-v`) or 2 (`-vv`; `--log-file` raises it to 2 when opened).
    pub verbosity: u8,
    pub log_file: Option<String>,
    pub diagnostics_file: Option<String>,
    pub stats_json: Option<String>,
    pub dump_artifacts: Option<String>,
    pub dump_asks: Option<String>,
    pub rename_ledger: Option<String>,
}

/// Performance instruments.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Perf {
    pub perf_trace: Option<String>,
    pub simulate_llm_latency: Option<String>,
}

fn serialize_schedule<S: serde::Serializer>(tier: &FastTier, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut m = s.serialize_map(Some(2))?;
    match tier {
        FastTier::Off => m.serialize_entry("tier", "off")?,
        FastTier::Exact => m.serialize_entry("tier", "sequential")?,
        FastTier::Relaxed(set) => {
            m.serialize_entry("tier", "relaxed")?;
            let on: Vec<&str> = Lever::ALL
                .iter()
                .filter(|l| set.has(**l))
                .map(|l| l.name())
                .collect();
            m.serialize_entry("levers", &on)?;
        }
    }
    m.end()
}

/// `--relaxed-levers a,b`: unknown names are refused, listing the valid set.
pub fn parse_levers(list: &str) -> Result<Levers, String> {
    let mut out = Levers::default();
    for name in list.split(',').filter(|s| !s.is_empty()) {
        let l = Lever::ALL
            .iter()
            .find(|l| l.name() == name)
            .ok_or_else(|| {
                let valid: Vec<&str> = Lever::ALL.iter().map(|l| l.name()).collect();
                format!(
                    "unknown relaxed lever \"{name}\" (valid: {})",
                    valid.join(", ")
                )
            })?;
        out = out.with(*l);
    }
    Ok(out)
}

/// The naming schedule: the DEFAULT is the relaxed tier with every lever
/// on; `--sequential` selects the conservative one (the relaxed levers
/// OFF, the exact tier's byte-identical parallelization kept — the
/// pre-2026-09-28 default's bytes at the same `--batch-size`), and
/// `--relaxed-levers` sizes a subset. Refused: a bad lever name, or both.
pub fn schedule(sequential: bool, relaxed_levers: Option<&str>) -> Result<FastTier, String> {
    match (sequential, relaxed_levers) {
        (true, Some(_)) => Err("--sequential already turns the relaxed levers off; \
                                --relaxed-levers has nothing to select"
            .to_string()),
        (true, None) => Ok(FastTier::Exact),
        (false, None) => Ok(FastTier::Relaxed(Levers::all())),
        (false, Some(list)) => parse_levers(list).map(FastTier::Relaxed),
    }
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

fn parse_enum<T: serde::de::DeserializeOwned>(v: &Option<String>) -> Option<T> {
    v.as_deref()
        .and_then(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
}

/// `checkFlagInvariants`: one message per violation, preconditions first
/// (in flag-declaration order), then the enum values, then the schedule.
pub fn check_flag_invariants(opts: &CommandOptions) -> Vec<String> {
    let truthy = |v: &Option<String>| v.as_deref().is_some_and(|s| !s.is_empty());
    let rules = [
        (opts.split_pure, "--split-pure", opts.split, "--split"),
        (
            truthy(&opts.split_ledger),
            "--split-ledger",
            opts.split,
            "--split",
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
    out.extend(schedule(opts.sequential, opts.relaxed_levers.as_deref()).err());
    out
}

/// Every value option that names something (a path, a URL, a model) must
/// name SOMETHING: an empty value was silently "unset" for some options
/// and an empty path for others.
fn empty_value_violations(opts: &CommandOptions) -> Vec<String> {
    let named: [(&str, &Option<String>); 17] = [
        ("--endpoint", &opts.endpoint),
        ("--model", &opts.model),
        ("--output-dir", &opts.output_dir),
        ("--llm-cache", &opts.llm_cache),
        ("--log-file", &opts.log_file),
        ("--diagnostics", &opts.diagnostics),
        ("--library-index", &opts.library_index),
        ("--stats-json", &opts.stats_json),
        ("--dump-artifacts", &opts.dump_artifacts),
        ("--dump-asks", &opts.dump_asks),
        ("--prior-version", &opts.prior_version),
        ("--split-ledger", &opts.split_ledger),
        ("--rename-ledger", &opts.rename_ledger),
        ("--simulate-llm-latency", &opts.simulate_llm_latency),
        ("--perf-trace", &opts.perf_trace),
        ("--disable", &opts.disable),
        ("--probe", &opts.probe),
    ];
    named
        .iter()
        .filter(|(_, v)| v.as_deref().is_some_and(|s| s.trim().is_empty()))
        .map(|(flag, _)| format!("{flag} must not be empty"))
        .collect()
}

/// Counts that must be at least 1: zero would stall or skip the work.
fn range_violations(opts: &CommandOptions) -> Vec<String> {
    let positive: [(&str, Option<u64>); 7] = [
        ("--concurrency", opts.concurrency.map(u64::from)),
        (
            "--module-concurrency",
            opts.module_concurrency.map(u64::from),
        ),
        ("--timeout", opts.timeout),
        ("--max-tokens", opts.max_tokens),
        ("--context-tokens", opts.context_tokens),
        ("--batch-size", opts.batch_size.map(|n| n as u64)),
        ("--max-retries", opts.max_retries.map(u64::from)),
    ];
    let mut out: Vec<String> = positive
        .iter()
        .filter(|(_, v)| *v == Some(0))
        .map(|(flag, _)| format!("{flag} must be at least 1 (got 0)"))
        .collect();
    let context = opts.context_tokens.unwrap_or(DEFAULT_CONTEXT_TOKENS);
    let completion = opts.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS);
    if context > 0 && completion > 0 && context <= completion {
        out.push(format!(
            "--context-tokens ({context}) must exceed --max-tokens ({completion}): the \
             completion reserve would leave the split namer's prompts no room"
        ));
    }
    if let Some(e) = &opts.endpoint
        && !e.trim().is_empty()
        && !(e.starts_with("http://") || e.starts_with("https://"))
    {
        out.push(format!(
            "--endpoint must be an http:// or https:// URL (got \"{e}\")"
        ));
    }
    out
}

fn split_list(v: &Option<String>) -> Vec<String> {
    v.as_deref()
        .map(|s| s.split(',').map(str::to_string).collect())
        .unwrap_or_default()
}

impl Config {
    /// The production build: the API key from the ONE environment reader
    /// (`crate::env`, which also loads `.env`).
    pub fn from_process(input: &str, opts: &CommandOptions) -> Result<Config, Vec<String>> {
        Config::build(input, opts, |name| crate::env::get(name, None))
    }

    /// Resolve and validate everything, over an injected environment
    /// lookup. Every refusal is collected — the user sees all of them at
    /// once — and nothing has run yet when they are returned.
    pub fn build(
        input: &str,
        opts: &CommandOptions,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Config, Vec<String>> {
        let mut errors = check_flag_invariants(opts);
        errors.extend(empty_value_violations(opts));
        errors.extend(range_violations(opts));
        let switches = SwitchState::configure(&split_list(&opts.disable), &split_list(&opts.probe))
            .map_err(|e| errors.push(e))
            .ok();
        let api_key = env(API_KEY_VAR).filter(|k| !k.is_empty());
        if api_key.is_none() {
            errors.push(MISSING_API_KEY.to_string());
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let schedule = schedule(opts.sequential, opts.relaxed_levers.as_deref())
            .expect("validated by check_flag_invariants");
        let d = WaveTunables::default();
        let naming_floor = !opts.no_naming_floor;
        let prior_version = opts.prior_version.clone();
        Ok(Config {
            io: Io {
                input: input.to_string(),
                output_dir: opts
                    .output_dir
                    .clone()
                    .unwrap_or_else(|| DEFAULT_OUTPUT_DIR.to_string()),
            },
            llm: Llm {
                endpoint: opts
                    .endpoint
                    .clone()
                    .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
                model: opts
                    .model
                    .clone()
                    .unwrap_or_else(|| DEFAULT_MODEL.to_string()),
                api_key: ApiKey(api_key.expect("checked above")),
                timeout_ms: opts.timeout.unwrap_or(DEFAULT_LLM_TIMEOUT_MS),
                retries: opts.retries.unwrap_or_else(default_retries),
                concurrency: opts.concurrency.unwrap_or(DEFAULT_CONCURRENCY),
                module_concurrency: opts
                    .module_concurrency
                    .unwrap_or(DEFAULT_MODULE_CONCURRENCY),
                max_tokens: opts.max_tokens,
                context_tokens: opts.context_tokens.unwrap_or(DEFAULT_CONTEXT_TOKENS),
                reasoning_effort: opts.reasoning_effort,
                cache_dir: opts.llm_cache.clone(),
            },
            matching: Matching {
                reconcile_prior_diff: !opts.no_reconcile_prior_diff && prior_version.is_some(),
                prior_version,
            },
            naming: Naming {
                schedule,
                batch_size: opts.batch_size.unwrap_or(d.batch_size),
                max_retries: opts.max_retries.unwrap_or(d.max_retries),
                rename_retries: opts.rename_retries.unwrap_or(d.reask_limit),
                max_free_retries: opts.max_free_retries.or(d.max_free_retries),
                lane_threshold: opts.lane_threshold.unwrap_or(d.lane_threshold),
                naming_floor,
                naming_floor_sweep: naming_floor && !opts.no_naming_floor_sweep,
            },
            library: Library {
                // The two spellings conflict at the parse (clap); unset is
                // skip.
                skip_libraries: !opts.no_skip_libraries,
                index: opts.library_index.clone(),
                discovery_units: opts.library_discovery_units,
            },
            unpack: Unpack {
                bundler: parse_enum(&opts.bundler),
                minifier: parse_enum(&opts.minifier),
            },
            split: Split {
                enabled: opts.split,
                pure: opts.split_pure,
                ledger: opts.split_ledger.clone(),
            },
            switches: switches.expect("checked above"),
            diagnostics: Diagnostics {
                verbosity: opts.verbose.min(2),
                log_file: opts.log_file.clone(),
                diagnostics_file: opts.diagnostics.clone(),
                stats_json: opts.stats_json.clone(),
                dump_artifacts: opts.dump_artifacts.clone(),
                dump_asks: opts.dump_asks.clone(),
                rename_ledger: opts.rename_ledger.clone(),
            },
            perf: Perf {
                perf_trace: opts.perf_trace.clone(),
                simulate_llm_latency: opts.simulate_llm_latency.clone(),
            },
        })
    }

    /// The run's record: the whole resolved config, the API key redacted.
    pub fn record(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("a config serializes")
    }

    /// Refusals that need the filesystem, still before any work: the
    /// files the run will read must exist. (Their CONTENT is the pipeline's
    /// business — an empty prior is refused when it is read.)
    pub fn missing_inputs(&self) -> Vec<String> {
        let mut out = Vec::new();
        let file = |flag: &str, p: &str, out: &mut Vec<String>| {
            if !std::path::Path::new(p).exists() {
                out.push(format!("{flag} file not found: {p}"));
            }
        };
        if let Some(p) = &self.matching.prior_version {
            file("--prior-version", p, &mut out);
        }
        if let Some(p) = &self.split.ledger {
            file("--split-ledger", p, &mut out);
        }
        out
    }
}

/// Every key the record carries, dotted (`llm.model`) — the setting names
/// the guard test refuses to see looked up in a JSON map outside this
/// module.
pub fn setting_keys() -> Vec<String> {
    fn walk(prefix: &str, v: &serde_json::Value, out: &mut Vec<String>) {
        if let serde_json::Value::Object(m) = v {
            for (k, child) in m {
                let key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                out.push(key.clone());
                walk(&key, child, out);
            }
        }
    }
    let probe = Config::build("in.js", &CommandOptions::default(), |_| {
        Some("probe".to_string())
    })
    .expect("the defaults are a valid config");
    let mut out = Vec::new();
    walk("", &probe.record(), &mut out);
    out
}
