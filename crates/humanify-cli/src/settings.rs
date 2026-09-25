//! Every setting, resolved ONCE (TS: `src/commands/settings.ts`).
//!
//! The TS resolver exists because settings used to enter four ways and the
//! re-derivation was where the bugs were (the same default in three places
//! with two values, `reasoningEffort` parsed twice in one function,
//! `skipLibraries` defaulted in two modules). The port keeps the shape:
//! CLI over the two API-key env fallbacks over defaults, parsed once into a
//! value that downstream code only reads (no `&mut` API — the borrow
//! checker is the TS `Object.freeze`).
//!
//! Deliberately NOT here, as in the TS: kill switches (their own registry,
//! `kill_switches`), and `moduleConcurrency` when unset — its default is
//! bundler-aware and the bundler is not detected yet.
//!
//! Numbers are JS numbers (`f64`) parsed with `parseInt(value, 10)`
//! semantics (`util::parse_number`), because a harness argv like `-c 32`
//! and a typo like `-c 12abc` must resolve exactly as the TS resolves them.

use crate::util::{DEFAULT_LLM_TIMEOUT_MS, parse_number};

/// The CLI options this resolver reads (a subset of the parsed options).
#[derive(Clone, Debug, Default)]
pub struct SettingsInput {
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub api_key: Option<String>,
    pub timeout: Option<String>,
    pub retries: Option<String>,
    pub concurrency: Option<String>,
    pub batch_size: Option<String>,
    pub max_retries: Option<String>,
    pub max_free_retries: Option<String>,
    pub lane_threshold: Option<String>,
    pub llm_cache: Option<String>,
    pub reasoning_effort: Option<String>,
    pub max_tokens: Option<String>,
    pub module_concurrency: Option<String>,
    pub skip_libraries: Option<bool>,
    pub naming_floor: Option<bool>,
    pub naming_floor_sweep: Option<bool>,
    pub reconcile_prior_diff: Option<bool>,
    pub prior_version: Option<String>,
}

/// The shipped noise levers, all defaulting ON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeverSettings {
    pub naming_floor: bool,
    /// Implied off when the floor is off — the sweep cannot run without it.
    pub naming_floor_sweep: bool,
    /// Implied off with no prior version — nothing to reconcile against.
    pub reconcile_prior_diff: bool,
}

/// The resolved settings (TS `Settings`).
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub endpoint: String,
    pub model: String,
    pub api_key: String,
    pub timeout: f64,
    pub max_tokens: Option<f64>,
    pub reasoning_effort: Option<&'static str>,
    pub llm_cache_dir: Option<String>,
    pub concurrency: f64,
    /// `None` means "let the bundler-aware default decide".
    pub module_concurrency: Option<f64>,
    pub retry_attempts: Option<f64>,
    pub batch_size: Option<f64>,
    pub max_retries_per_identifier: Option<f64>,
    pub max_free_retries: Option<f64>,
    pub lane_threshold: Option<f64>,
    pub skip_libraries: bool,
    pub levers: LeverSettings,
}

/// The TS `MissingApiKeyError` message.
pub const MISSING_API_KEY: &str = "API key required. Provide --api-key, or set HUMANIFY_API_KEY or \
     OPENAI_API_KEY environment variable.";

const EFFORTS: [&str; 3] = ["low", "medium", "high"];

/// `parseReasoningEffort`: anything outside the enum is an error.
pub fn parse_reasoning_effort(value: Option<&str>) -> Result<Option<&'static str>, String> {
    let Some(v) = value else { return Ok(None) };
    match EFFORTS.iter().find(|e| **e == v) {
        Some(e) => Ok(Some(e)),
        None => Err(format!(
            "invalid reasoning effort {} — expected one of {}",
            humanify_model::js::stringify(&humanify_model::js::JsValue::str(v)),
            EFFORTS.join(", ")
        )),
    }
}

fn num(v: &Option<String>) -> Result<Option<f64>, String> {
    v.as_deref().map(parse_number).transpose()
}

/// `resolveSettings(opts)` with the production environment (the ONE env
/// reader, `crate::env`, which also loads `.env` like the TS dotenv).
pub fn resolve_settings(opts: &SettingsInput) -> Result<Settings, String> {
    resolve_settings_with(opts, |name| crate::env::get(name, None))
}

/// `resolveSettings` over an injected environment lookup. Throw order is
/// the TS's: API key, the CLI-layer endpoint/model check, then the object
/// literal's fields in declaration order.
pub fn resolve_settings_with(
    opts: &SettingsInput,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Settings, String> {
    // `opts.apiKey ?? env(HUMANIFY) ?? env(OPENAI)`, then a FALSY check:
    // `??` only falls through on undefined, `!apiKey` also rejects "".
    let api_key = opts
        .api_key
        .clone()
        .or_else(|| env("HUMANIFY_API_KEY"))
        .or_else(|| env("OPENAI_API_KEY"))
        .filter(|k| !k.is_empty())
        .ok_or_else(|| MISSING_API_KEY.to_string())?;

    let naming_floor = opts.naming_floor.unwrap_or(true);

    let (endpoint, model) = match (opts.endpoint.as_deref(), opts.model.as_deref()) {
        (Some(e), Some(m)) if !e.is_empty() && !m.is_empty() => (e.to_string(), m.to_string()),
        _ => {
            return Err(
                "endpoint and model must be supplied by the CLI layer (commander's \
                 option defaults carry the env fallback); resolveSettings does not \
                 re-derive them."
                    .to_string(),
            );
        }
    };

    let timeout = num(&opts.timeout)?.unwrap_or(DEFAULT_LLM_TIMEOUT_MS as f64);
    let max_tokens = num(&opts.max_tokens)?;
    let reasoning_effort = parse_reasoning_effort(opts.reasoning_effort.as_deref())?;
    let concurrency = num(&opts.concurrency)?.unwrap_or(0.0);
    let module_concurrency = num(&opts.module_concurrency)?;
    let retry_attempts = num(&opts.retries)?;
    let batch_size = num(&opts.batch_size)?;
    let max_retries_per_identifier = num(&opts.max_retries)?;
    let max_free_retries = num(&opts.max_free_retries)?;
    let lane_threshold = num(&opts.lane_threshold)?;

    Ok(Settings {
        endpoint,
        model,
        api_key,
        timeout,
        max_tokens,
        reasoning_effort,
        llm_cache_dir: opts.llm_cache.clone(),
        concurrency,
        module_concurrency,
        retry_attempts,
        batch_size,
        max_retries_per_identifier,
        max_free_retries,
        lane_threshold,
        skip_libraries: opts.skip_libraries.unwrap_or(true),
        levers: LeverSettings {
            naming_floor,
            naming_floor_sweep: opts.naming_floor_sweep.unwrap_or(true) && naming_floor,
            // `Boolean(opts.priorVersion)`: "" is no prior.
            reconcile_prior_diff: opts.reconcile_prior_diff.unwrap_or(true)
                && opts.prior_version.as_deref().is_some_and(|p| !p.is_empty()),
        },
    })
}
