//! The LLM boundary's data (src/llm/types.ts) and the ONE owner of the disk
//! cache key ([`cache_key_of`]: what the endpoint receives, exp233).
//!
//! Why the key lives HERE and not in humanify-ag-llm: it is a pure function of
//! these types (canonical JSON + sha256), and three crates need it —
//! humanify-ag-llm (the cache), humanify-ag-core (the vendor namer builds its
//! request and core must not depend on the tokio/reqwest crate), and
//! humanify-parity (re-deriving dumped keys). humanify-ag-model is the only
//! crate all three already depend on, and it stays free of I/O and async:
//! the cache's DISK side (read, atomic write, shard layout) is
//! humanify-ag-llm's alone.
//!
//! `NameProvider` is the synchronous seam 02 §7 specifies: core is generic
//! over it and never sees tokio; humanify-ag-llm implements it by
//! `block_on`-ing a bounded fan-out.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

pub use crate::ordered::{JsRecord, js_assign, serialize_entries};

/// A number as JavaScript's `String(x)` writes it in JSON: an integral
/// value has no fraction (`0`, not `0.0`; `-0` is `0`), a non-finite one
/// is `null`. Every value from 1e-5 up prints as `String(x)` does
/// (enforced: llm_test js_numbers_print_as_javascript_does); smaller
/// non-integral values take serde's exponent form.
pub fn serialize_js_number<S: Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    if !x.is_finite() {
        s.serialize_unit()
    } else if x.fract() == 0.0 && x.abs() < 9.0e15 {
        s.serialize_i64(*x as i64)
    } else {
        s.serialize_f64(*x)
    }
}

fn serialize_opt_js_number<S: Serializer>(x: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match x {
        Some(x) => serialize_js_number(x, s),
        None => s.serialize_none(),
    }
}

/// An insertion-ordered `Record<string, string>` with JS object semantics
/// (a repeated key keeps its first position, index-like keys lead).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrMap(pub Vec<(String, String)>);

impl Serialize for StrMap {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize_entries(&self.0, s)
    }
}

impl<'de> Deserialize<'de> for StrMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsRecord::<String>::deserialize(deserializer).map(|r| StrMap(r.0))
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
    /// The previous answer's keys that belonged to NO asked identifier
    /// (`naming::answer_keys`, finding #85) — disclosed beside the
    /// missing ids so the model is told which keys to use. Absent from
    /// the key material when empty: every other retry keys as before.
    #[serde(default)]
    pub stray_keys: Vec<String>,
}

/// One previously rejected suggestion of a re-asked identifier (the
/// ACCUMULATED disclosure a collision re-ask carries, 2026-09-29): the
/// suggestion, and whether it was rejected as an invalid target rather
/// than a name collision — each disclosed with its own reason.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriorReject {
    pub name: String,
    #[serde(default)]
    pub invalid: bool,
    /// The program's minified name this suggestion borrowed as a word
    /// (`H6t` in `H6tClass`) when it was refused for that — the re-ask
    /// names the stem. Absent from the key material when None, so every
    /// other re-ask keys exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borrowed: Option<String>,
    /// WHO holds the rejected name, as the re-ask words it ("an inner
    /// function's parameter") — set by the wave barrier when the scopes
    /// know (2026-10-04). Absent from the key material when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held_by: Option<String>,
}

/// An insertion-ordered `Record<string, PriorReject[]>` (`StrMap`'s
/// sibling): every failed suggestion per identifier, oldest first. The
/// retry prompt renders one diagnostic line per entry and flattens the
/// names into the do-not-suggest block; [`BatchRenameRequest::prior_rejects`]
/// carries it as prompt material.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PriorRejects(pub Vec<(String, Vec<PriorReject>)>);

impl PriorRejects {
    /// `record[id]` for an own key.
    pub fn get(&self, id: &str) -> Option<&[PriorReject]> {
        self.0
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v.as_slice())
    }
}

impl Serialize for PriorRejects {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize_entries(&self.0, s)
    }
}

impl<'de> Deserialize<'de> for PriorRejects {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsRecord::<Vec<PriorReject>>::deserialize(deserializer).map(|r| PriorRejects(r.0))
    }
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
    /// Every prior rejected suggestion per identifier, oldest first (the
    /// accumulated do-not-suggest disclosure of a barrier/sweep re-ask;
    /// `previous_attempt` still carries the LAST suggestion). None on a
    /// lane round-2, which re-asks a rejected RESPONSE and knows one
    /// suggestion per identifier.
    #[serde(default)]
    pub prior_rejects: Option<PriorRejects>,
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
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeyParams {
    pub model: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_opt_js_number"
    )]
    pub temperature: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

/// The cache key's version. 1 keyed every request field (the TS
/// `cacheKeyOf`), including fields the model never sees — above all the
/// run's whole used-name set, of which a prompt shows at most the first
/// 50 or 200 — so a change that carried one name re-keyed nearly every
/// prompt and an A/B could not replay its control's answers (exp230: 2,535
/// of 4,806 new keys had a byte-identical prompt). 2 keys exactly what the
/// endpoint receives (exp233).
pub const CACHE_KEY_VERSION: u32 = 2;

/// The `response_format.type` every request sends: part of the wire body
/// (the client's) and so of the key (this module's) — one constant.
pub const RESPONSE_FORMAT_TYPE: &str = "json_object";

/// One message of the `messages` array every request sends.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct WireMessage<'a> {
    pub role: &'static str,
    pub content: &'a str,
}

/// The `messages` array every request sends — the system prompt, then the
/// user prompt. One builder for the wire body (the client's) and the key
/// (this module's), so the two cannot disagree on what the model sees.
pub fn wire_messages<'a>(system_prompt: &'a str, user_prompt: &'a str) -> [WireMessage<'a>; 2] {
    [
        WireMessage {
            role: "system",
            content: system_prompt,
        },
        WireMessage {
            role: "user",
            content: user_prompt,
        },
    ]
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyMaterial<'a> {
    cache_version: u32,
    messages: [WireMessage<'a>; 2],
    params: &'a CacheKeyParams,
    response_format: &'static str,
}

/// The canonical JSON the key hashes: the cache key version, the model
/// parameters, the response format and the two messages — everything the
/// endpoint receives that can shape the answer, and nothing else. Every
/// object's keys are sorted: the material goes through `serde_json::Value`,
/// whose maps sort by bytes, which for these ASCII keys is the order the
/// key was first defined in (enforced: llm_test pins the material).
/// Whatever the answer is checked against afterwards (used names,
/// identifiers, scopes) is applied by the caller to the recorded RAW
/// answer, after the lookup, so a replay re-runs that check on the live
/// state.
pub fn cache_key_material(
    system_prompt: &str,
    user_prompt: &str,
    params: &CacheKeyParams,
) -> String {
    let material = KeyMaterial {
        cache_version: CACHE_KEY_VERSION,
        messages: wire_messages(system_prompt, user_prompt),
        params,
        response_format: RESPONSE_FORMAT_TYPE,
    };
    let sorted = serde_json::to_value(&material).expect("the key material serializes");
    serde_json::to_string(&sorted).expect("a JSON value serializes")
}

/// The disk cache key: lowercase-hex sha256 over the UTF-8 bytes of
/// [`cache_key_material`].
pub fn cache_key_of(system_prompt: &str, user_prompt: &str, params: &CacheKeyParams) -> String {
    let digest = Sha256::digest(cache_key_material(system_prompt, user_prompt, params).as_bytes());
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
        Renames(entries.into_iter().collect::<JsRecord<_>>().0)
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

impl Serialize for Renames {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize_entries(&self.0, s)
    }
}

/// An object whose every value is a string or null; any other value fails
/// the read.
impl<'de> Deserialize<'de> for Renames {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        JsRecord::<Option<String>>::deserialize(deserializer).map(|r| Renames(r.0))
    }
}

/// Token usage as the pipeline records it (`BatchRenameResponse.usage`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

    /// From a JSON object of token counts named by `keys` (total, input,
    /// output); a count that is not a number is absent. None when `value`
    /// is not an object.
    pub fn from_value(value: &serde_json::Value, keys: [&str; 3]) -> Option<Self> {
        let obj = value.as_object()?;
        let num = |key: &str| {
            obj.get(key)
                .and_then(serde_json::Value::as_f64)
                .map(|n| n as u64)
        };
        Some(Usage {
            total_tokens: num(keys[0]),
            input_tokens: num(keys[1]),
            output_tokens: num(keys[2]),
        })
    }
}

/// One batch rename response (types.ts `BatchRenameResponse`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRenameResponse {
    pub renames: Renames,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

impl BatchRenameResponse {
    /// The response as compact JSON — field order renames, finishReason,
    /// usage; absent fields left out.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a response serializes")
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
/// keys on the rendered text (what the model sees), never on `request`
/// (exp233); `request` is what the caller checks the answer against.
#[derive(Clone, Debug, PartialEq)]
pub struct LlmCall {
    pub request: BatchRenameRequest,
    pub system_prompt: String,
    pub user_prompt: String,
    /// Which namer asked. Recording only: the call ledger
    /// ([`crate::call_ledger`]) files the call's fate under it. It is NOT
    /// part of the request, so it never reaches the cache key, the prompt
    /// or a decision.
    pub site: CallSite,
}

impl LlmCall {
    /// This call's cache key: its two prompts under `params` — what the
    /// endpoint receives ([`cache_key_of`]).
    pub fn cache_key(&self, params: &CacheKeyParams) -> String {
        cache_key_of(&self.system_prompt, &self.user_prompt, params)
    }
}

/// The namer an LLM call belongs to — the `providerErrors` block's five
/// namers, so the call ledger's failed counts reconcile with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CallSite {
    /// The function/identifier waves and the coverage sweeps.
    Naming,
    /// The module namer (`naming::module_names`).
    Modules,
    /// The vendor namer.
    Vendor,
    /// The split's file/folder namer.
    Split,
    /// The split's tree reviser.
    TreeReviser,
}

impl CallSite {
    /// Every site, in the stats block's order.
    pub const ALL: [CallSite; 5] = [
        CallSite::Naming,
        CallSite::Modules,
        CallSite::Vendor,
        CallSite::Split,
        CallSite::TreeReviser,
    ];

    /// The site's key — `providerErrors`' key for the same namer.
    pub fn as_str(self) -> &'static str {
        match self {
            CallSite::Naming => "naming",
            CallSite::Modules => "modules",
            CallSite::Vendor => "vendor",
            CallSite::Split => "split",
            CallSite::TreeReviser => "treeReviser",
        }
    }
}

/// The completion callback of [`NameProvider::run_pipelined`]: handed each
/// finished call's id and result, it returns the follow-up calls to start
/// (each with a caller-chosen id).
pub type OnCallDone<'a> =
    dyn FnMut(usize, Result<BatchRenameResponse, LlmError>) -> Vec<(usize, LlmCall)> + 'a;

/// The synchronous LLM seam (02 §7): one wave of calls in, one result per
/// call out, same order. Implementations bound their own concurrency.
pub trait NameProvider {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>>;

    /// Pipelined dispatch (the naming waves' schedule): start every
    /// `initial` call; as each
    /// one finishes, hand its result to `on_done` and start the follow-ups
    /// it returns, until nothing is in flight. COMPLETION ORDER IS NOT
    /// DETERMINISTIC — a caller may only use it for work whose outcome is
    /// independent of it (each follow-up depends on its own chain alone).
    ///
    /// The default is generational: every call of a generation runs as one
    /// [`NameProvider::run_wave`], results are handed over in id order, and
    /// the follow-ups form the next generation — the step barrier a
    /// provider without real concurrency has anyway.
    fn run_pipelined(&self, initial: Vec<(usize, LlmCall)>, on_done: &mut OnCallDone<'_>) {
        let mut generation = initial;
        while !generation.is_empty() {
            generation.sort_by_key(|(id, _)| *id);
            let (ids, calls): (Vec<usize>, Vec<LlmCall>) = generation.into_iter().unzip();
            let results = self.run_wave(calls);
            let mut next = Vec::new();
            for (id, result) in ids.into_iter().zip(results) {
                next.extend(on_done(id, result));
            }
            generation = next;
        }
    }
}

/// A borrowed provider is a provider — the pipeline holds `&dyn
/// NameProvider` and hands it to the generic stage drivers.
impl<T: NameProvider + ?Sized> NameProvider for &T {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        (**self).run_wave(calls)
    }

    fn run_pipelined(&self, initial: Vec<(usize, LlmCall)>, on_done: &mut OnCallDone<'_>) {
        (**self).run_pipelined(initial, on_done)
    }
}

/// Endpoint configuration (types.ts `LLMConfig`), resolved by the CLI.
#[derive(Clone, Debug, PartialEq)]
pub struct LlmConfig {
    /// Base URL, e.g. "http://<llm-host>:8000/v1".
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
