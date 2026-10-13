//! THE configuration owner (config sweep steps 4-5, 2026-10-13;
//! docs/config-consolidation.md §3).
//!
//! One typed, immutable [`Config`], built ONCE here from three inputs and
//! nothing else: the parsed argv (`surface`, clap — only the operational
//! flags), the `--config <file>` TOML file ([`file`], checked against the
//! ONE key registry, [`keys`]), and the one environment variable the binary
//! reads (`HUMANIFY_API_KEY`, through `crate::env`). Every value comes from
//! its flag, else the file, else the registry's compiled-in default; a flag
//! and the file that disagree are refused. Everything is validated here —
//! every refusal before any work starts — and handed down by reference.
//! Nothing below reads argv, the environment or a file for a setting: the
//! stages take `&Config` or the typed values built from it.
//! `tests/env_guard.rs` fails any configuration read outside this module.
//!
//! It replaced four resolvers that each owned a slice of the run's
//! configuration (`Settings`, the kill-switch `SwitchState`, the naming
//! schedule's `Levers`, and the raw option fields `unified.rs` read
//! directly), and the defaults that lived twice. Every knob has ONE
//! default: its registry entry, at the one constant it names.
//!
//! The whole resolved config, minus the API key, is the run's record
//! ([`Config::record`]): the stats file's `config` block, which the eval's
//! run manifest copies — with the config file's path and sha256, and the
//! source of every value that is not a default.

use serde::Serialize;

use humanify_core::fast::{FastTier, Lever, Levers};
use humanify_core::naming::waves::batch::WaveTunables;
use humanify_model::detection::{BundlerType, MinifierType};
use humanify_model::llm::RateLimitConfig;

use crate::kill_switches::SwitchState;
use crate::surface::CommandOptions;

pub mod file;
pub mod keys;

pub use file::ConfigFile;
use file::{FlagValue, Resolved, Val};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize, clap::ValueEnum)]
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
    /// The `--config` file this run read: its path and the sha256 of its
    /// bytes (null: no file, every setting a flag or a default).
    pub config_file: Option<ConfigFileRecord>,
    /// Every setting NOT at its compiled-in default, and who set it:
    /// "flag", "file", or "flag+file" (the two agreed).
    pub sources: std::collections::BTreeMap<String, &'static str>,
}

/// Which config file a run read.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigFileRecord {
    pub path: String,
    pub sha256: String,
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
    /// The most lines of a function the naming prompt shows.
    pub code_window_lines: usize,
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
            code_window_lines: self.code_window_lines,
        }
    }
}

/// `--library-matching` (docs/library-matching-config.md §2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibraryMatching {
    /// Path/banner detection only; the index is never opened.
    Off,
    /// Also the index lookup and graph discovery, REPORTED (the run's
    /// output bytes are `off`'s).
    Report,
    /// Index claims may name code — refused until an eval passes it.
    Names,
}

/// `library.unnamed`: what library code with no index name gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Unnamed {
    /// Today's default (was `--skip-libraries`): library files are not
    /// model-named; library functions in mixed files are frozen and get
    /// the deterministic `<library>_<name>` prefix.
    Leave,
    /// Was `--no-skip-libraries`: no file filter, no freeze, no prefix
    /// pass — the model names everything.
    Model,
}

/// The ONE library decision (config sweep step 6): what matching does, the
/// index, what unnamed library code gets, and the lookup's and
/// discovery's tunables.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub matching: LibraryMatching,
    /// The index as given (a directory or an id).
    pub index: Option<String>,
    pub index_store: Option<String>,
    pub unnamed: Unnamed,
    pub report_units: bool,
    pub lookup: LibraryLookup,
    pub discovery: LibraryDiscovery,
}

impl Library {
    /// Library code is left out of model naming (`unnamed = "leave"`).
    pub fn skips_libraries(&self) -> bool {
        self.unnamed == Unnamed::Leave
    }

    /// The lookup's rules.
    pub fn rules(&self) -> humanify_core::libindex::lookup::LookupRules {
        let l = &self.lookup;
        humanify_core::libindex::lookup::LookupRules {
            min_nodes: l.min_nodes,
            min_content: l.min_content,
            min_function_content: l.min_function_content,
            cover: l.cover,
            max_packages: l.max_packages,
            support: l.support,
            strong: l.strong,
            specificity: l.specificity,
            agreement: l.agreement,
            min_literals: l.min_literals,
            literal_score: l.literal_score,
            literal_cover: l.literal_cover,
            literal_margin: l.literal_margin,
        }
    }

    /// The fixpoint's tunables.
    pub fn discovery_options(&self) -> humanify_core::libdetect::discovery::DiscoveryOptions {
        let d = &self.discovery;
        humanify_core::libdetect::discovery::DiscoveryOptions {
            closure_floor: d.closure_floor,
            queue_cap: d.queue_cap_bytes,
            max_rounds: d.max_rounds,
            veto_support: d.veto_support,
        }
    }

    /// Discovery's request for a run: the index (if any) and every tunable.
    pub fn discovery_request<'x>(
        &self,
        index: Option<&'x humanify_core::libindex::lookup::LibraryIndex>,
    ) -> humanify_core::libdetect::discovery::DiscoveryRequest<'x> {
        humanify_core::libdetect::discovery::DiscoveryRequest {
            list_units: self.report_units,
            index,
            rules: Some(self.rules()),
            targeted: Some((self.lookup.targeted_min_nodes, self.lookup.targeted_support)),
            options: self.discovery_options(),
            unit_prints: false,
        }
    }

    /// The index directory `index` names: a directory as given, else an id
    /// under `index_store`. Err: why it cannot be resolved.
    pub fn index_dir(&self) -> Option<Result<std::path::PathBuf, String>> {
        let given = self.index.as_deref()?;
        let p = std::path::Path::new(given);
        if p.is_dir() {
            return Some(Ok(p.to_path_buf()));
        }
        let is_id = given.len() == 64 && given.bytes().all(|b| b.is_ascii_hexdigit());
        Some(match (&self.index_store, is_id) {
            (Some(store), true) => {
                let dir = std::path::Path::new(store).join(given);
                if dir.is_dir() {
                    Ok(dir)
                } else {
                    Err(format!(
                        "library index {given}: no such index under library.index_store \
                         ({store})"
                    ))
                }
            }
            (None, true) => Err(format!(
                "library index {given}: an index id needs library.index_store (or pass the \
                 index directory)"
            )),
            _ => Err(format!("library index {given}: no such directory")),
        })
    }
}

/// `[library.lookup]` + `[library.lookup.targeted]`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryLookup {
    pub min_nodes: u32,
    pub min_content: u32,
    pub min_function_content: u32,
    pub cover: f64,
    pub max_packages: usize,
    pub support: usize,
    pub strong: u32,
    pub specificity: Option<(u64, u64)>,
    pub agreement: bool,
    pub min_literals: usize,
    pub literal_score: f64,
    pub literal_cover: f64,
    pub literal_margin: f64,
    pub targeted_min_nodes: u32,
    pub targeted_support: usize,
}

/// `[library.discovery]`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDiscovery {
    pub closure_floor: u32,
    pub queue_cap_bytes: u32,
    pub max_rounds: u32,
    #[serde(serialize_with = "serialize_veto")]
    pub veto_support: humanify_core::libdetect::discovery::VetoSupport,
}

fn serialize_veto<S: serde::Serializer>(
    v: &humanify_core::libdetect::discovery::VetoSupport,
    s: S,
) -> Result<S::Ok, S::Error> {
    s.serialize_str(keys::veto_support_name(*v))
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

/// `naming.relaxed_levers`: the named levers (each name was checked
/// against the registry when the file was read).
pub fn parse_levers(list: &[String]) -> Levers {
    Lever::ALL
        .iter()
        .filter(|l| list.iter().any(|n| n == l.name()))
        .fold(Levers::default(), |acc, l| acc.with(*l))
}

/// The naming schedule: the DEFAULT is the relaxed tier with every lever
/// on; `naming.schedule = "sequential"` selects the conservative one (the
/// relaxed levers OFF, the exact tier's byte-identical parallelization
/// kept — the pre-2026-09-28 default's bytes at the same batch size), and
/// `naming.relaxed_levers` sizes a subset. Refused: both.
pub fn schedule(sequential: bool, relaxed_levers: Option<Vec<String>>) -> Result<FastTier, String> {
    match (sequential, relaxed_levers) {
        (true, Some(_)) => Err(
            "naming.schedule = \"sequential\" already turns the relaxed \
                                levers off; naming.relaxed_levers has nothing to select"
                .to_string(),
        ),
        (true, None) => Ok(FastTier::Exact),
        (false, None) => Ok(FastTier::Relaxed(Levers::all())),
        (false, Some(list)) => Ok(FastTier::Relaxed(parse_levers(&list))),
    }
}

fn parse_enum<T: serde::de::DeserializeOwned>(v: Option<String>) -> Option<T> {
    v.and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok())
}

/// The operational flags' values, keyed by the setting each also is (the
/// rest of the run's settings live only in the `--config` file).
fn flag_values(opts: &CommandOptions) -> Vec<FlagValue> {
    let text = |k: &'static str, v: &Option<String>| v.clone().map(|s| (k, Val::Text(s)));
    [
        text("io.output_dir", &opts.output_dir),
        text("llm.endpoint", &opts.endpoint),
        text("llm.model", &opts.model),
        text("matching.prior_version", &opts.prior_version),
        text("library.matching", &opts.library_matching),
        text("library.index", &opts.library_index),
        (opts.verbose > 0).then(|| ("diagnostics.verbosity", Val::Int(u64::from(opts.verbose)))),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// The `[library]` section, resolved.
fn library_of(r: &Resolved) -> Library {
    let n = |k: &str| r.int(k).expect("defaulted");
    let veto = match r.text("library.discovery.veto_support").as_deref() {
        Some("all-rules") => humanify_core::libdetect::discovery::VetoSupport::AllRules,
        _ => humanify_core::libdetect::discovery::VetoSupport::WithoutModule,
    };
    Library {
        matching: parse_enum(r.text("library.matching")).expect("a checked choice"),
        index: r.text("library.index"),
        index_store: r.text("library.index_store"),
        unnamed: parse_enum(r.text("library.unnamed")).expect("a checked choice"),
        report_units: r.flag_on("library.report_units"),
        lookup: LibraryLookup {
            min_nodes: n("library.lookup.min_nodes") as u32,
            min_content: n("library.lookup.min_content") as u32,
            min_function_content: n("library.lookup.min_function_content") as u32,
            cover: r.float("library.lookup.cover"),
            max_packages: n("library.lookup.max_packages") as usize,
            support: n("library.lookup.support") as usize,
            strong: n("library.lookup.strong") as u32,
            specificity: r.flag_on("library.lookup.specificity").then(|| {
                (
                    n("library.lookup.specificity_factor"),
                    n("library.lookup.specificity_slack"),
                )
            }),
            agreement: r.flag_on("library.lookup.agreement"),
            min_literals: n("library.lookup.min_literals") as usize,
            literal_score: r.float("library.lookup.literal_score"),
            literal_cover: r.float("library.lookup.literal_cover"),
            literal_margin: r.float("library.lookup.literal_margin"),
            targeted_min_nodes: n("library.lookup.targeted.min_nodes") as u32,
            targeted_support: n("library.lookup.targeted.support") as usize,
        },
        discovery: LibraryDiscovery {
            closure_floor: n("library.discovery.closure_floor") as u32,
            queue_cap_bytes: n("library.discovery.queue_cap_bytes") as u32,
            max_rounds: n("library.discovery.max_rounds") as u32,
            veto_support: veto,
        },
    }
}

/// The library section a file alone configures (no flags, no API key):
/// the `discover` verb's tunables.
pub fn library_from_file(file: Option<&ConfigFile>) -> Result<Library, Vec<String>> {
    let (r, errors) = file::resolve(&[], file);
    if errors.is_empty() {
        Ok(library_of(&r))
    } else {
        Err(errors)
    }
}

/// The library decision's rules (docs/library-matching-config.md §2).
fn library_violations(r: &Resolved) -> Vec<String> {
    let mode = r.text("library.matching").unwrap_or_default();
    let index = r.text("library.index").is_some();
    let mut out = Vec::new();
    match (mode.as_str(), index) {
        ("names", _) => out.push(
            "--library-matching names is refused until an eval passes it \
             (docs/library-matching-config.md §2): use --library-matching report"
                .to_string(),
        ),
        ("report", false) => {
            out.push("--library-matching report needs --library-index <dir|id>".to_string())
        }
        ("off", true) => out.push(
            "--library-index given with --library-matching off: the index would be \
             silently ignored — pass --library-matching report, or drop the index"
                .to_string(),
        ),
        _ => {}
    }
    out
}

/// The rules between settings, after each value is accepted on its own:
/// one message per violation.
pub fn cross_key_violations(r: &Resolved) -> Vec<String> {
    let split = r.flag_on("split.enabled");
    let mut out = library_violations(r);
    if r.flag_on("split.pure") && !split {
        out.push("split.pure requires split.enabled".to_string());
    }
    if r.text("split.ledger").is_some() && !split {
        out.push("split.ledger requires split.enabled".to_string());
    }
    let sequential = r.text("naming.schedule").as_deref() == Some("sequential");
    out.extend(schedule(sequential, r.list("naming.relaxed_levers")).err());
    let context = r
        .int("llm.context_tokens")
        .unwrap_or(DEFAULT_CONTEXT_TOKENS);
    let completion = r.int("llm.max_tokens").unwrap_or(DEFAULT_MAX_TOKENS);
    if context <= completion {
        out.push(format!(
            "llm.context_tokens ({context}) must exceed llm.max_tokens ({completion}): the \
             completion reserve would leave the split namer's prompts no room"
        ));
    }
    out
}

impl Config {
    /// The production build: the `--config` file read from disk, the API
    /// key from the ONE environment reader (`crate::env`, which also loads
    /// `.env`).
    pub fn from_process(input: &str, opts: &CommandOptions) -> Result<Config, Vec<String>> {
        let file = match &opts.config {
            Some(path) => Some(ConfigFile::read(path)?),
            None => None,
        };
        Config::build(input, opts, file.as_ref(), |name| {
            crate::env::get(name, None)
        })
    }

    /// Resolve and validate everything — the flags, the checked file, the
    /// compiled-in defaults — over an injected environment lookup. Every
    /// refusal is collected (the user sees all of them at once) and nothing
    /// has run yet when they are returned.
    pub fn build(
        input: &str,
        opts: &CommandOptions,
        file: Option<&ConfigFile>,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Config, Vec<String>> {
        let (r, mut errors) = file::resolve(&flag_values(opts), file);
        errors.extend(cross_key_violations(&r));
        let list = |k: &str| r.list(k).unwrap_or_default();
        let switches = SwitchState::configure(&list("switches.disable"), &list("switches.probe"))
            .map_err(|e| errors.push(e))
            .ok();
        let api_key = env(API_KEY_VAR).filter(|k| !k.is_empty());
        if api_key.is_none() {
            errors.push(MISSING_API_KEY.to_string());
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        let sequential = r.text("naming.schedule").as_deref() == Some("sequential");
        let schedule =
            schedule(sequential, r.list("naming.relaxed_levers")).expect("validated above");
        Ok(Config {
            io: Io {
                input: input.to_string(),
                output_dir: r.text("io.output_dir").expect("defaulted"),
            },
            llm: Config::llm(&r, ApiKey(api_key.expect("checked above"))),
            matching: Matching {
                reconcile_prior_diff: r.flag_on("matching.reconcile_prior_diff")
                    && r.text("matching.prior_version").is_some(),
                prior_version: r.text("matching.prior_version"),
            },
            naming: Config::naming(&r, schedule),
            library: library_of(&r),
            unpack: Unpack {
                bundler: parse_enum(r.text("unpack.bundler")),
                minifier: parse_enum(r.text("unpack.minifier")),
            },
            split: Split {
                enabled: r.flag_on("split.enabled"),
                pure: r.flag_on("split.pure"),
                ledger: r.text("split.ledger"),
            },
            switches: switches.expect("checked above"),
            diagnostics: Diagnostics {
                verbosity: r.int("diagnostics.verbosity").expect("defaulted") as u8,
                log_file: r.text("diagnostics.log_file"),
                diagnostics_file: r.text("diagnostics.diagnostics_file"),
                stats_json: r.text("diagnostics.stats_json"),
                dump_artifacts: r.text("diagnostics.dump_artifacts"),
                dump_asks: r.text("diagnostics.dump_asks"),
                rename_ledger: r.text("diagnostics.rename_ledger"),
            },
            perf: Perf {
                perf_trace: r.text("perf.perf_trace"),
                simulate_llm_latency: r.text("perf.simulate_llm_latency"),
            },
            config_file: file.map(|f| ConfigFileRecord {
                path: f.path.clone(),
                sha256: f.sha256.clone(),
            }),
            sources: r.sources(),
        })
    }

    fn llm(r: &Resolved, api_key: ApiKey) -> Llm {
        let count = |k: &str| r.int(k).expect("defaulted");
        Llm {
            endpoint: r.text("llm.endpoint").expect("defaulted"),
            model: r.text("llm.model").expect("defaulted"),
            api_key,
            timeout_ms: count("llm.timeout_ms"),
            retries: count("llm.retries") as u32,
            concurrency: count("llm.concurrency") as u32,
            module_concurrency: count("llm.module_concurrency") as u32,
            max_tokens: r.int("llm.max_tokens"),
            context_tokens: count("llm.context_tokens"),
            reasoning_effort: parse_enum(r.text("llm.reasoning_effort")),
            cache_dir: r.text("llm.cache_dir"),
        }
    }

    fn naming(r: &Resolved, schedule: FastTier) -> Naming {
        let count = |k: &str| r.int(k).expect("defaulted");
        let naming_floor = r.flag_on("naming.naming_floor");
        Naming {
            schedule,
            batch_size: count("naming.batch_size") as usize,
            max_retries: count("naming.max_retries") as u32,
            rename_retries: count("naming.rename_retries") as usize,
            max_free_retries: r.int("naming.max_free_retries").map(|n| n as u32),
            lane_threshold: count("naming.lane_threshold") as usize,
            code_window_lines: count("naming.code_window_lines") as usize,
            naming_floor,
            // Implied off when the floor is off.
            naming_floor_sweep: naming_floor && r.flag_on("naming.naming_floor_sweep"),
        }
    }

    /// With library matching on, the index must be there, readable, and
    /// built with this binary's format and fingerprint versions — checked
    /// before any work (its recipe slice needs detection, later).
    pub fn library_index_problem(&self) -> Option<String> {
        if self.library.matching == LibraryMatching::Off {
            return None;
        }
        let dir = match self.library.index_dir()? {
            Ok(d) => d,
            Err(e) => return Some(e),
        };
        library_index_stale(&dir).err()
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
        let file = |what: &str, p: &str, out: &mut Vec<String>| {
            if !std::path::Path::new(p).exists() {
                out.push(format!("{what} file not found: {p}"));
            }
        };
        if let Some(p) = &self.matching.prior_version {
            file("prior version (--prior-version)", p, &mut out);
        }
        if let Some(p) = &self.split.ledger {
            file("split.ledger", p, &mut out);
        }
        out.extend(self.library_index_problem());
        out
    }
}

/// The index at `dir` is readable and built with this binary's format and
/// compatibility stamps, else why not (naming both versions and the
/// rebuild). The stamps are compared by ONE owner,
/// `libindex::format::Manifest::check_compatible` (the loader's too).
pub fn library_index_stale(dir: &std::path::Path) -> Result<(), String> {
    use humanify_core::libindex::format::{Manifest, check_manifest_stamps};
    let shown = dir.display();
    let text = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| format!("library index {shown}: manifest.json unreadable: {e}"))?;
    // an older era may not parse as this binary's manifest: its stamps
    // first, so the refusal names them
    if serde_json::from_str::<serde_json::Value>(&text).is_ok() {
        check_manifest_stamps(&text).map_err(|e| format!("library index {shown}: {e}"))?;
    }
    let m: Manifest = serde_json::from_str(&text).map_err(|e| {
        format!("library index {shown}: manifest.json is not an index manifest: {e}")
    })?;
    m.check_compatible()
        .map_err(|e| format!("library index {shown}: {e}"))
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
    let probe = Config::build("in.js", &CommandOptions::default(), None, |_| {
        Some("probe".to_string())
    })
    .expect("the defaults are a valid config");
    let mut out = Vec::new();
    let mut record = probe.record();
    // Not settings: where the settings came from.
    if let Some(m) = record.as_object_mut() {
        m.remove("configFile");
        m.remove("sources");
    }
    walk("", &record, &mut out);
    out
}
