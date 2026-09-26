//! The LLM boundary's data (src/llm/types.ts) and the ONE owner of the disk
//! cache key (`cacheKeyOf`, src/llm/cached-provider.ts).
//!
//! Why the key lives HERE and not in humanify-llm: it is a pure function of
//! these types (canonical JSON + sha256), and three crates need it —
//! humanify-llm (the cache), humanify-core (the vendor namer builds its
//! request and core must not depend on the tokio/reqwest crate), and
//! humanify-parity (re-deriving dumped keys). humanify-model is the only
//! crate all three already depend on, and it stays free of I/O and async:
//! the cache's DISK side (read, atomic write, shard layout) is
//! humanify-llm's alone.
//!
//! `NameProvider` is the synchronous seam 02 §7 specifies: core is generic
//! over it and never sees tokio; humanify-llm implements it by
//! `block_on`-ing a bounded fan-out.

use crate::js::{JsObject, JsValue, canonical, stringify};
use sha2::{Digest, Sha256};

/// An insertion-ordered `Record<string, string>` with JS object semantics
/// (a repeated key keeps its first position, index-like keys lead).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrMap(pub Vec<(String, String)>);

impl StrMap {
    pub fn to_js(&self) -> JsValue {
        JsValue::Object(
            self.0
                .iter()
                .map(|(k, v)| (k.clone(), JsValue::str(v)))
                .collect(),
        )
    }
}

impl<'de> serde::Deserialize<'de> for StrMap {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let JsValue::Object(obj) = value else {
            return Err(serde::de::Error::custom("expected an object of strings"));
        };
        obj.entries()
            .iter()
            .map(|(k, v)| match v {
                JsValue::String(s) => Ok((k.clone(), s.clone())),
                _ => Err(serde::de::Error::custom(format!("{k}: expected a string"))),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(StrMap)
    }
}

/// A callee's signature as the naming context carries it
/// (analysis/types.ts `CalleeSignature`). `snippet` is NOT rendered into
/// any prompt, but it IS cache-key material (canonicalJson recurses into
/// every own property) — so it must be carried for replay parity. It is
/// optional only because the oracle dumps' cache-keys.jsonl dropped it
/// (a TS dump bug, WP4.1); every real naming request sets it.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct CalleeSignature {
    pub name: String,
    pub params: Vec<String>,
    #[serde(default)]
    pub snippet: Option<String>,
}

/// The four failure lists of a retry (`BatchRenameRequest.failures`).
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
pub struct RenameFailures {
    pub duplicates: Vec<String>,
    pub invalid: Vec<String>,
    pub missing: Vec<String>,
    pub unchanged: Vec<String>,
}

/// One batch rename request (types.ts `BatchRenameRequest`). `None` is the
/// TS `undefined` (the field is absent from the key material); an empty
/// Vec is a present-but-empty array — they key differently.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRenameRequest {
    pub code: String,
    pub identifiers: Vec<String>,
    /// A `Set<string>` in the TS: kept in insertion order here, SORTED
    /// (UTF-16) into the key material.
    pub used_names: Vec<String>,
    pub callee_signatures: Vec<CalleeSignature>,
    pub callsites: Vec<String>,
    #[serde(default)]
    pub is_retry: Option<bool>,
    #[serde(default)]
    pub previous_attempt: Option<StrMap>,
    #[serde(default)]
    pub failures: Option<RenameFailures>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub user_prompt: Option<String>,
    #[serde(default)]
    pub prompt_body: Option<String>,
    #[serde(default)]
    pub context_vars: Option<Vec<String>>,
    #[serde(default)]
    pub prior_version_code: Option<String>,
    #[serde(default)]
    pub prior_version_names: Option<Vec<String>>,
    #[serde(default)]
    pub already_renamed: Option<StrMap>,
    #[serde(default)]
    pub prior_name_hints: Option<StrMap>,
}

/// The model parameters that shape a response (`CacheKeyParams`) — part of
/// the key. `temperature` is a JS number: 0 and 0.0 are the same key
/// ("temperature":0), -0 too.
#[derive(Clone, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeyParams {
    pub model: String,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

impl CacheKeyParams {
    fn to_js(&self) -> JsValue {
        let mut obj = JsObject::new();
        obj.insert("model", JsValue::str(&self.model));
        obj.insert_opt("temperature", self.temperature.map(JsValue::Number));
        obj.insert_opt(
            "maxTokens",
            self.max_tokens.map(|n| JsValue::Number(n as f64)),
        );
        obj.insert_opt(
            "reasoningEffort",
            self.reasoning_effort.as_deref().map(JsValue::str),
        );
        JsValue::Object(obj)
    }
}

fn callee_to_js(callee: &CalleeSignature) -> JsValue {
    let mut obj = JsObject::new();
    obj.insert("name", JsValue::str(&callee.name));
    obj.insert("params", JsValue::str_array(&callee.params));
    obj.insert_opt("snippet", callee.snippet.as_deref().map(JsValue::str));
    JsValue::Object(obj)
}

fn failures_to_js(f: &RenameFailures) -> JsValue {
    let mut obj = JsObject::new();
    obj.insert("duplicates", JsValue::str_array(&f.duplicates));
    obj.insert("invalid", JsValue::str_array(&f.invalid));
    obj.insert("missing", JsValue::str_array(&f.missing));
    obj.insert("unchanged", JsValue::str_array(&f.unchanged));
    JsValue::Object(obj)
}

impl BatchRenameRequest {
    /// The request half of the key material: the TS literal's sixteen
    /// fields, `undefined` ones absent, the Set sorted by UTF-16 units
    /// (`[...set].map(String).sort()`).
    pub fn key_material(&self) -> JsValue {
        let mut used = self.used_names.clone();
        used.sort_by(|a, b| crate::js::cmp_utf16(a, b));
        let opt_str = |s: &Option<String>| s.as_deref().map(JsValue::str);
        let mut obj = JsObject::new();
        obj.insert("code", JsValue::str(&self.code));
        obj.insert("identifiers", JsValue::str_array(&self.identifiers));
        obj.insert("usedNames", JsValue::str_array(&used));
        obj.insert(
            "calleeSignatures",
            JsValue::Array(self.callee_signatures.iter().map(callee_to_js).collect()),
        );
        obj.insert("callsites", JsValue::str_array(&self.callsites));
        obj.insert_opt(
            "contextVars",
            self.context_vars.as_deref().map(JsValue::str_array),
        );
        obj.insert_opt("priorVersionCode", opt_str(&self.prior_version_code));
        obj.insert_opt(
            "priorVersionNames",
            self.prior_version_names.as_deref().map(JsValue::str_array),
        );
        obj.insert_opt(
            "priorNameHints",
            self.prior_name_hints.as_ref().map(StrMap::to_js),
        );
        obj.insert_opt(
            "alreadyRenamed",
            self.already_renamed.as_ref().map(StrMap::to_js),
        );
        obj.insert_opt("isRetry", self.is_retry.map(JsValue::Bool));
        obj.insert_opt(
            "previousAttempt",
            self.previous_attempt.as_ref().map(StrMap::to_js),
        );
        obj.insert_opt("failures", self.failures.as_ref().map(failures_to_js));
        obj.insert_opt("promptBody", opt_str(&self.prompt_body));
        obj.insert_opt("userPrompt", opt_str(&self.user_prompt));
        obj.insert_opt("systemPrompt", opt_str(&self.system_prompt));
        JsValue::Object(obj)
    }
}

/// The canonical JSON the key hashes: `canonicalJson({cacheVersion: 1,
/// params, request})`.
pub fn cache_key_material(request: &BatchRenameRequest, params: &CacheKeyParams) -> String {
    let mut obj = JsObject::new();
    obj.insert("cacheVersion", JsValue::Number(1.0));
    obj.insert("params", params.to_js());
    obj.insert("request", request.key_material());
    canonical(&JsValue::Object(obj))
}

/// The disk cache key (`cacheKeyOf`): lowercase-hex sha256 over the UTF-8
/// bytes of [`cache_key_material`].
pub fn cache_key_of(request: &BatchRenameRequest, params: &CacheKeyParams) -> String {
    let digest = Sha256::digest(cache_key_material(request, params).as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// A rename map as the model returned it, in JS enumeration order. A value
/// is `None` for a JSON `null` (the vendor namer's decline, as some cache
/// entries record it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Renames(Vec<(String, Option<String>)>);

impl Renames {
    /// Build with JS assignment semantics (`renames[k] = v` in order).
    pub fn from_entries(entries: impl IntoIterator<Item = (String, Option<String>)>) -> Self {
        let obj: JsObject = entries
            .into_iter()
            .map(|(k, v)| (k, v.map_or(JsValue::Null, JsValue::String)))
            .collect();
        Renames(
            obj.entries()
                .iter()
                .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string)))
                .collect(),
        )
    }

    /// From a parsed JSON object; None when a value is neither a string nor
    /// null (never seen: 0 of 188,720 values across the standing cache's
    /// 61,812 entries, 2026-09-24).
    pub fn from_js(obj: &JsObject) -> Option<Self> {
        let mut out = Vec::with_capacity(obj.len());
        for (k, v) in obj.entries() {
            match v {
                JsValue::String(s) => out.push((k.clone(), Some(s.clone()))),
                JsValue::Null => out.push((k.clone(), None)),
                _ => return None,
            }
        }
        Some(Renames(out))
    }

    pub fn to_js(&self) -> JsValue {
        JsValue::Object(
            self.0
                .iter()
                .map(|(k, v)| (k.clone(), v.clone().map_or(JsValue::Null, JsValue::String)))
                .collect(),
        )
    }

    /// `renames[key]` when it is a string.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_deref())
    }

    pub fn entries(&self) -> &[(String, Option<String>)] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Token usage as the pipeline records it (`BatchRenameResponse.usage`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub total_tokens: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

impl Usage {
    pub fn zero() -> Self {
        Usage {
            total_tokens: Some(0),
            input_tokens: Some(0),
            output_tokens: Some(0),
        }
    }

    pub fn to_js(&self) -> JsValue {
        let num = |n: Option<u64>| n.map(|n| JsValue::Number(n as f64));
        let mut obj = JsObject::new();
        obj.insert_opt("totalTokens", num(self.total_tokens));
        obj.insert_opt("inputTokens", num(self.input_tokens));
        obj.insert_opt("outputTokens", num(self.output_tokens));
        JsValue::Object(obj)
    }

    pub fn from_js(value: &JsValue) -> Option<Self> {
        let obj = value.as_object()?;
        let num = |key: &str| match obj.get(key) {
            Some(JsValue::Number(n)) => Some(*n as u64),
            _ => None,
        };
        Some(Usage {
            total_tokens: num("totalTokens"),
            input_tokens: num("inputTokens"),
            output_tokens: num("outputTokens"),
        })
    }
}

/// One batch rename response (types.ts `BatchRenameResponse`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatchRenameResponse {
    pub renames: Renames,
    pub finish_reason: Option<String>,
    pub usage: Option<Usage>,
}

impl BatchRenameResponse {
    /// `JSON.stringify(response)` — field order renames, finishReason,
    /// usage; undefined fields absent.
    pub fn to_json(&self) -> String {
        let mut obj = JsObject::new();
        obj.insert("renames", self.renames.to_js());
        obj.insert_opt(
            "finishReason",
            self.finish_reason.as_deref().map(JsValue::str),
        );
        obj.insert_opt("usage", self.usage.as_ref().map(Usage::to_js));
        stringify(&JsValue::Object(obj))
    }
}

/// What kind of failure an LLM call hit — the TS errors are classes of the
/// openai SDK (APIError subclasses, APIConnectionError,
/// APIConnectionTimeoutError) or anything a provider throws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LlmErrorKind {
    /// A non-2xx HTTP response (`APIError`, `status` set).
    Status,
    /// The connection failed (`APIConnectionError`, "Connection error.").
    Connection,
    /// The per-attempt timeout fired (`APIConnectionTimeoutError`,
    /// "Request timed out.").
    Timeout,
    /// A replay-only cache had no entry for the request.
    CacheMiss,
    /// Anything else (a malformed response body, a provider's own error).
    Other,
}

/// An LLM call failure: the message and status the TS retry classifier
/// reads (`isRetryableError` matches on both).
#[derive(Clone, Debug, PartialEq)]
pub struct LlmError {
    pub kind: LlmErrorKind,
    pub message: String,
    pub status: Option<u16>,
}

impl LlmError {
    pub fn new(kind: LlmErrorKind, message: impl Into<String>) -> Self {
        LlmError {
            kind,
            message: message.into(),
            status: None,
        }
    }
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for LlmError {}

/// A request plus the prompts rendered for it. The TS provider renders the
/// user prompt itself (`buildBatchUserPrompt`, prompts.ts — WP4.2); in the
/// port rendering is core's (`core::naming::prompts`), a pure function done
/// before dispatch, so the client only sends what it is given. The cache
/// keys on `request`, never on the rendered text.
#[derive(Clone, Debug, PartialEq)]
pub struct LlmCall {
    pub request: BatchRenameRequest,
    pub system_prompt: String,
    pub user_prompt: String,
}

/// The synchronous LLM seam (02 §7): one wave of calls in, one result per
/// call out, same order. Implementations bound their own concurrency.
pub trait NameProvider {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>>;
}

/// A borrowed provider is a provider — the pipeline holds `&dyn
/// NameProvider` and hands it to the generic stage drivers.
impl<T: NameProvider + ?Sized> NameProvider for &T {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        (**self).run_wave(calls)
    }
}

/// Endpoint configuration (types.ts `LLMConfig`), resolved by the CLI.
#[derive(Clone, Debug, PartialEq)]
pub struct LlmConfig {
    /// Base URL, e.g. "http://192.168.1.234:8000/v1".
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    /// `max_tokens` per request (TS default 6000).
    pub max_tokens: u64,
    /// Sent as-is (TS default 0 — reproducible reruns).
    pub temperature: f64,
    /// Per-attempt timeout (`DEFAULT_LLM_TIMEOUT_MS` = 300000).
    pub timeout_ms: u64,
    /// `reasoning_effort` in the body only when set.
    pub reasoning_effort: Option<String>,
    /// The openai SDK's own retry budget under the rate limiter's (SDK
    /// default 2 — the TS never overrides it).
    pub sdk_max_retries: u32,
}

/// The TS `DEFAULT_LLM_TIMEOUT_MS` (commands/default-args.ts).
pub const DEFAULT_LLM_TIMEOUT_MS: u64 = 300_000;

/// The TS default `max_tokens` (completion budget per request).
pub const DEFAULT_MAX_TOKENS: u64 = 6000;

impl LlmConfig {
    /// The TS constructor's defaults for everything the caller leaves out.
    pub fn new(endpoint: &str, api_key: &str, model: &str) -> Self {
        LlmConfig {
            endpoint: endpoint.to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            max_tokens: DEFAULT_MAX_TOKENS,
            temperature: 0.0,
            timeout_ms: DEFAULT_LLM_TIMEOUT_MS,
            reasoning_effort: None,
            sdk_max_retries: 2,
        }
    }
}

/// Rate limiting (types.ts `RateLimitConfig`; defaults from
/// rate-limiter.ts's constructor).
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitConfig {
    pub max_concurrent: usize,
    /// 0 = unlimited.
    pub requests_per_minute: usize,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        RateLimitConfig {
            max_concurrent: 50,
            requests_per_minute: 0,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}
