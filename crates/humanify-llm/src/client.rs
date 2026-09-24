//! The OpenAI-compatible chat-completions client (llm/openai-compatible.ts
//! plus the slice of the `openai` npm SDK 4.90.0 it relies on).
//!
//! Request: `POST {endpoint}/chat/completions`, body
//! `{model, messages: [system, user], response_format: {type:
//! "json_object"}, temperature, max_tokens[, reasoning_effort]}` in that key
//! order, serialized as `JSON.stringify` would (temperature 0 is `0`).
//!
//! The SDK layer (core.js `makeRequest`/`retryRequest`), reproduced because
//! it is part of the retry/timeout envelope 02 §7 carries over:
//! - `maxRetries` (default 2) retries UNDER the rate limiter's own retries;
//! - the per-attempt timeout covers the request until response headers;
//! - a transport failure or timeout retries while retries remain, then
//!   fails as "Connection error." / "Request timed out." (no status);
//! - a non-2xx response retries when `x-should-retry` says so, else on
//!   408 / 409 / 429 / >= 500, then fails as `"{status} {message}"`;
//! - the retry delay honours `retry-after-ms` / `retry-after` when in
//!   [0, 60 s), else `min(0.5 * 2^n, 8) s` minus up to 25 % jitter.
//!
//! Response: `choices[0].message.content` — empty → no renames; valid JSON
//! → every string-valued entry of `Object.entries(parsed)` (so a bare JSON
//! string yields one entry per character, `null` throws into the
//! fallback); invalid JSON → the regex fallback
//! `/"([^"]+)"\s*:\s*"([^"]+)"/g`. Names pass through raw: validity is the
//! batch validator's call downstream.

use std::time::{Duration, Instant};

use humanify_model::js::{JsObject, JsValue, stringify};
use humanify_model::llm::{
    BatchRenameResponse, LlmCall, LlmConfig, LlmError, LlmErrorKind, Renames, Usage,
};

use crate::debug::{LlmLogEvent, LogSink, Roundtrip, emit};
use crate::provider::AsyncProvider;

pub struct OpenAiClient {
    http: reqwest::Client,
    config: LlmConfig,
    url: String,
    log: Option<LogSink>,
}

/// The SDK's `buildURL` for the path "/chat/completions".
pub fn completions_url(base: &str) -> String {
    if base.ends_with('/') {
        format!("{base}chat/completions")
    } else {
        format!("{base}/chat/completions")
    }
}

impl OpenAiClient {
    pub fn new(config: LlmConfig, log: Option<LogSink>) -> Self {
        OpenAiClient {
            http: reqwest::Client::new(),
            url: completions_url(&config.endpoint),
            config,
            log,
        }
    }

    /// `buildRequestBody` → the wire bytes.
    pub fn request_body(&self, system_prompt: &str, user_prompt: &str) -> String {
        let message = |role: &str, content: &str| {
            let mut m = JsObject::new();
            m.insert("role", JsValue::str(role));
            m.insert("content", JsValue::str(content));
            JsValue::Object(m)
        };
        let mut format = JsObject::new();
        format.insert("type", JsValue::str("json_object"));
        let mut body = JsObject::new();
        body.insert("model", JsValue::str(&self.config.model));
        body.insert(
            "messages",
            JsValue::Array(vec![
                message("system", system_prompt),
                message("user", user_prompt),
            ]),
        );
        body.insert("response_format", JsValue::Object(format));
        body.insert("temperature", JsValue::Number(self.config.temperature));
        body.insert("max_tokens", JsValue::Number(self.config.max_tokens as f64));
        body.insert_opt(
            "reasoning_effort",
            self.config.reasoning_effort.as_deref().map(JsValue::str),
        );
        stringify(&JsValue::Object(body))
    }

    /// One HTTP attempt, bounded by the per-attempt timeout (headers only,
    /// as the SDK's AbortController is cleared once fetch resolves).
    async fn attempt(&self, body: &str) -> Result<reqwest::Response, LlmErrorKind> {
        let send = self
            .http
            .post(&self.url)
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .header("authorization", format!("Bearer {}", self.config.api_key))
            .body(body.to_string())
            .send();
        match tokio::time::timeout(Duration::from_millis(self.config.timeout_ms), send).await {
            Err(_) => Err(LlmErrorKind::Timeout),
            Ok(Err(_)) => Err(LlmErrorKind::Connection),
            Ok(Ok(response)) => Ok(response),
        }
    }

    /// The SDK's `makeRequest` loop: returns the 2xx response's body text.
    async fn post_with_sdk_retries(&self, body: &str) -> Result<String, LlmError> {
        let max = self.config.sdk_max_retries;
        let mut remaining = max;
        loop {
            let retry_count = max - remaining;
            match self.attempt(body).await {
                Err(kind) => {
                    if remaining > 0 {
                        tokio::time::sleep(default_retry_delay(retry_count)).await;
                        remaining -= 1;
                        continue;
                    }
                    let message = match kind {
                        LlmErrorKind::Timeout => "Request timed out.",
                        _ => "Connection error.",
                    };
                    return Err(LlmError::new(kind, message));
                }
                Ok(response) => {
                    let status = response.status().as_u16();
                    if response.status().is_success() {
                        return response.text().await.map_err(|_| {
                            LlmError::new(LlmErrorKind::Connection, "Connection error.")
                        });
                    }
                    let headers = response.headers().clone();
                    if remaining > 0 && should_retry(status, &headers) {
                        let delay = retry_after(&headers)
                            .unwrap_or_else(|| default_retry_delay(retry_count));
                        tokio::time::sleep(delay).await;
                        remaining -= 1;
                        continue;
                    }
                    let text = response.text().await.unwrap_or_default();
                    return Err(status_error(status, &text));
                }
            }
        }
    }

    fn log_roundtrip(&self, call: &LlmCall, started: Instant, raw: Option<String>, ok: bool) {
        emit(
            &self.log,
            LlmLogEvent::Roundtrip(Roundtrip {
                method: "suggestAllNames".to_string(),
                model: Some(self.config.model.clone()),
                identifiers: call.request.identifiers.clone(),
                system_prompt: Some(call.system_prompt.clone()),
                user_prompt: Some(call.user_prompt.clone()),
                raw_response: raw,
                duration_ms: started.elapsed().as_millis(),
                ok,
            }),
        );
    }
}

/// The SDK's `shouldRetry` for a non-2xx response.
fn should_retry(status: u16, headers: &reqwest::header::HeaderMap) -> bool {
    match headers.get("x-should-retry").and_then(|v| v.to_str().ok()) {
        Some("true") => return true,
        Some("false") => return false,
        _ => {}
    }
    matches!(status, 408 | 409 | 429) || status >= 500
}

/// `retry-after-ms`, else `retry-after` (seconds, or an HTTP date), when
/// the result is a sane wait in [0, 60 s) — the SDK's rule.
fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let ms = header("retry-after-ms")
        .and_then(|v| v.trim().parse::<f64>().ok())
        .or_else(|| {
            header("retry-after")
                .and_then(|v| v.trim().parse::<f64>().ok())
                .map(|s| s * 1000.0)
        });
    // (An HTTP-date retry-after falls through to the default backoff here;
    // the SDK would honour it — no OpenAI-compatible server this project
    // talks to sends one.)
    ms.filter(|ms| (0.0..60_000.0).contains(ms) && *ms != 0.0)
        .map(|ms| Duration::from_micros((ms * 1000.0) as u64))
}

/// `calculateDefaultRetryTimeoutMillis`: min(0.5 * 2^n, 8) s, jittered down
/// by up to 25 %.
fn default_retry_delay(retry_count: u32) -> Duration {
    let seconds = (0.5 * 2f64.powi(retry_count as i32)).min(8.0);
    let jitter = 1.0 - jitter_fraction() * 0.25;
    Duration::from_secs_f64(seconds * jitter)
}

/// A cheap uniform draw in [0, 1) for the jitter (timing only — no
/// decision depends on it).
fn jitter_fraction() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    f64::from(nanos % 1_000_000) / 1_000_000.0
}

/// `APIError.generate` + `makeMessage` for a non-2xx response: the body's
/// `error.message` (string, else JSON), else the whole `error` as JSON;
/// a JSON body without `error` gives "status code (no body)"; a non-JSON
/// body is the message verbatim.
pub fn status_error(status: u16, body: &str) -> LlmError {
    // safeJSON: the parsed body, or undefined when it does not parse.
    let json = JsValue::parse(body).ok();
    // `errJSON ? undefined : errText` — a falsy parse keeps the raw text.
    let raw_message = match &json {
        Some(j) if js_truthy(j) => None,
        _ => Some(body),
    };
    // `errJSON?.['error']` — only an object has an `error` property.
    let error = json
        .as_ref()
        .and_then(JsValue::as_object)
        .and_then(|o| o.get("error"))
        .filter(|e| js_truthy(e));
    let message = match error {
        Some(error) => {
            let inner = error
                .as_object()
                .and_then(|o| o.get("message"))
                .filter(|m| js_truthy(m));
            Some(match inner {
                Some(JsValue::String(m)) => m.clone(),
                Some(m) => stringify(m),
                None => stringify(error),
            })
        }
        None => raw_message.filter(|m| !m.is_empty()).map(str::to_string),
    };
    let message = match message {
        Some(m) => format!("{status} {m}"),
        None => format!("{status} status code (no body)"),
    };
    LlmError {
        kind: LlmErrorKind::Status,
        message,
        status: Some(status),
    }
}

/// JS truthiness of a JSON value.
fn js_truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Null => false,
        JsValue::Bool(b) => *b,
        JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
        JsValue::String(s) => !s.is_empty(),
        JsValue::Array(_) | JsValue::Object(_) => true,
    }
}

/// One attempt of `/"([^"]+)"\s*:\s*"([^"]+)"/` anchored at `start` (a
/// quote). The pattern cannot backtrack into a different match — each
/// `[^"]+` must run to the next quote, and `\s*` cannot swallow `:` or `"`
/// — so the greedy scan IS the regex. Returns (key, value, end).
fn match_pair_at(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let quoted = |from: usize| -> Option<(String, usize)> {
        if chars.get(from) != Some(&'"') {
            return None;
        }
        let end = (from + 1..chars.len()).find(|&i| chars[i] == '"')?;
        (end > from + 1).then(|| (chars[from + 1..end].iter().collect(), end + 1))
    };
    let skip_space = |mut i: usize| {
        while chars
            .get(i)
            .is_some_and(|c| humanify_model::js::is_js_whitespace(*c))
        {
            i += 1;
        }
        i
    };
    let (key, after_key) = quoted(start)?;
    let colon = skip_space(after_key);
    if chars.get(colon) != Some(&':') {
        return None;
    }
    let (value, end) = quoted(skip_space(colon + 1))?;
    Some((key, value, end))
}

/// `parseRenamesFromContent`: every `"key": "value"` pair, left to right
/// (a failed attempt resumes one character later, as a global regex does).
pub fn parse_renames_from_content(content: &str) -> Renames {
    let chars: Vec<char> = content.chars().collect();
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"'
            && let Some((key, value, end)) = match_pair_at(&chars, i)
        {
            pairs.push((key, Some(value)));
            i = end;
            continue;
        }
        i += 1;
    }
    Renames::from_entries(pairs)
}

/// `Object.entries(JSON.parse(content))` keeping string values; None when
/// the content is not JSON or is `null` (Object.entries throws → fallback).
fn json_renames(content: &str) -> Option<Renames> {
    let entries: Vec<(String, Option<String>)> = match JsValue::parse(content).ok()? {
        JsValue::Null => return None,
        JsValue::Object(obj) => obj
            .entries()
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), Some(s.to_string()))))
            .collect(),
        JsValue::Array(items) => items
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_str().map(|s| (i.to_string(), Some(s.to_string()))))
            .collect(),
        // A string's entries are its UTF-16 code units; an astral
        // character's halves cannot be Rust strings and become U+FFFD.
        JsValue::String(s) => s
            .encode_utf16()
            .enumerate()
            .map(|(i, unit)| {
                let c = char::from_u32(u32::from(unit)).unwrap_or('\u{FFFD}');
                (i.to_string(), Some(c.to_string()))
            })
            .collect(),
        JsValue::Number(_) | JsValue::Bool(_) => Vec::new(),
    };
    Some(Renames::from_entries(entries))
}

/// The renames a completion's content carries.
pub fn renames_from_content(content: &str) -> Renames {
    json_renames(content).unwrap_or_else(|| parse_renames_from_content(content))
}

/// `extractUsage` → `usageToResult`: prompt → input, completion → output.
fn usage_of(completion: &JsObject) -> Option<Usage> {
    let usage = completion.get("usage")?.as_object()?;
    let num = |key: &str| match usage.get(key) {
        Some(JsValue::Number(n)) => Some(*n as u64),
        _ => None,
    };
    Some(Usage {
        total_tokens: num("total_tokens"),
        input_tokens: num("prompt_tokens"),
        output_tokens: num("completion_tokens"),
    })
}

/// A completion body → the response (content, finish reason, usage).
pub fn response_from_completion(
    body: &str,
) -> Result<(BatchRenameResponse, Option<String>), LlmError> {
    let json = JsValue::parse(body)
        .map_err(|e| LlmError::new(LlmErrorKind::Other, format!("invalid completion body: {e}")))?;
    let completion = json
        .as_object()
        .ok_or_else(|| LlmError::new(LlmErrorKind::Other, "completion body is not an object"))?;
    // `response.choices[0]?.…` — a missing `choices` throws in the TS.
    let first = match completion.get("choices") {
        Some(JsValue::Array(choices)) => choices.first().and_then(JsValue::as_object),
        _ => {
            return Err(LlmError::new(
                LlmErrorKind::Other,
                "Cannot read properties of undefined (reading '0')",
            ));
        }
    };
    let finish_reason = first
        .and_then(|c| c.get("finish_reason"))
        .and_then(JsValue::as_str)
        .map(str::to_string);
    let content = first
        .and_then(|c| c.get("message"))
        .and_then(JsValue::as_object)
        .and_then(|m| m.get("content"))
        .and_then(JsValue::as_str)
        .filter(|c| !c.is_empty())
        .map(str::to_string);
    let renames = content
        .as_deref()
        .map(renames_from_content)
        .unwrap_or_default();
    Ok((
        BatchRenameResponse {
            renames,
            finish_reason,
            usage: usage_of(completion),
        },
        content,
    ))
}

impl AsyncProvider for OpenAiClient {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let started = Instant::now();
        let body = self.request_body(&call.system_prompt, &call.user_prompt);
        let result = match self.post_with_sdk_retries(&body).await {
            Ok(text) => response_from_completion(&text),
            Err(error) => Err(error),
        };
        match &result {
            Ok((_, content)) => self.log_roundtrip(call, started, content.clone(), true),
            Err(error) => self.log_roundtrip(call, started, Some(error.message.clone()), false),
        }
        result.map(|(response, _)| response)
    }
}
