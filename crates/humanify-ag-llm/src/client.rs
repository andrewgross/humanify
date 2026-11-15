//! The OpenAI-compatible chat-completions client (llm/openai-compatible.ts
//! plus the slice of the `openai` npm SDK 4.90.0 it relies on).
//!
//! Request: `POST {endpoint}/chat/completions`, body
//! `{model, messages: [system, user], response_format: {type:
//! "json_object"}, temperature, max_tokens[, reasoning_effort]}` in that key
//! order, temperature written as a JS number (0 is `0`).
//!
//! The SDK layer (core.js `makeRequest`/`retryRequest`), reproduced because
//! it is part of the retry/timeout envelope 02 §7 carries over:
//! - `maxRetries` (default 2) retries UNDER the rate limiter's own retries;
//! - the per-attempt timeout covers the whole exchange, body included (the
//!   SDK stopped at the response headers — audit 2026-10 W2);
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

use humanify_ag_model::llm::{
    BatchRenameResponse, LlmCall, LlmConfig, LlmError, LlmErrorKind, Renames, Usage,
};
use serde_json::Value;

use humanify_ag_model::call_ledger::{Attempt, AttemptRecord};

use crate::debug::{LlmLogEvent, LogSink, Roundtrip, emit};
use crate::provider::{AsyncProvider, CallTrace};

pub struct OpenAiClient {
    http: reqwest::Client,
    config: LlmConfig,
    url: String,
    log: Option<LogSink>,
}

/// One attempt's response: status, headers, and the body text (None when
/// reading it failed).
struct Exchange {
    status: u16,
    headers: reqwest::header::HeaderMap,
    text: Option<String>,
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
    // concurrency: one pooled client; every attempt runs under ONE timeout_at deadline covering connect, send and body (audit W2, O12)
    #[allow(clippy::disallowed_methods)]
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
        #[derive(serde::Serialize)]
        struct Format {
            r#type: &'static str,
        }
        #[derive(serde::Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: [humanify_ag_model::llm::WireMessage<'a>; 2],
            response_format: Format,
            #[serde(serialize_with = "humanify_ag_model::llm::serialize_js_number")]
            temperature: f64,
            max_tokens: u64,
            #[serde(skip_serializing_if = "Option::is_none")]
            reasoning_effort: Option<&'a str>,
        }
        serde_json::to_string(&Body {
            model: &self.config.model,
            messages: humanify_ag_model::llm::wire_messages(system_prompt, user_prompt),
            response_format: Format {
                r#type: humanify_ag_model::llm::RESPONSE_FORMAT_TYPE,
            },
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            reasoning_effort: self.config.reasoning_effort.as_deref(),
        })
        .expect("a request body serializes")
    }

    /// One HTTP attempt: the response's status, headers and body text, the
    /// WHOLE exchange bounded by the per-attempt timeout. (The SDK cleared
    /// its AbortController once the headers arrived, so a body that stalled
    /// after them hung the call — and its rate-limiter slot — forever;
    /// concurrency audit 2026-10, W2. A healthy server sends the body with
    /// the headers, so this changes nothing a live run can observe short of
    /// a stall.)
    async fn attempt(&self, body: &str) -> Result<Exchange, LlmErrorKind> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(self.config.timeout_ms);
        let send = self
            .http
            .post(&self.url)
            .header("content-type", "application/json")
            .header("accept", "application/json")
            .header("authorization", format!("Bearer {}", self.config.api_key))
            .body(body.to_string())
            .send();
        let response = match tokio::time::timeout_at(deadline, send).await {
            Err(_) => return Err(LlmErrorKind::Timeout),
            Ok(Err(_)) => return Err(LlmErrorKind::Connection),
            Ok(Ok(response)) => response,
        };
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let text = match tokio::time::timeout_at(deadline, response.text()).await {
            Err(_) => return Err(LlmErrorKind::Timeout),
            Ok(text) => text.ok(),
        };
        Ok(Exchange {
            status,
            headers,
            text,
        })
    }

    /// The SDK's `makeRequest` loop: returns the 2xx response's body text.
    /// Every attempt's result and duration land in `trace` (recording only).
    async fn post_with_sdk_retries(
        &self,
        body: &str,
        trace: &CallTrace,
    ) -> Result<String, LlmError> {
        let max = self.config.sdk_max_retries;
        let mut remaining = max;
        loop {
            let retry_count = max - remaining;
            let started = Instant::now();
            let exchange = self.attempt(body).await;
            trace.push(AttemptRecord {
                result: attempt_result(&exchange),
                ms: started.elapsed().as_millis() as u64,
            });
            match exchange {
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
                Ok(exchange) => {
                    let status = exchange.status;
                    if (200..300).contains(&status) {
                        return exchange.text.ok_or_else(|| {
                            LlmError::new(LlmErrorKind::Connection, "Connection error.")
                        });
                    }
                    if remaining > 0 && should_retry(status, &exchange.headers) {
                        let delay = retry_after(&exchange.headers)
                            .unwrap_or_else(|| default_retry_delay(retry_count));
                        tokio::time::sleep(delay).await;
                        remaining -= 1;
                        continue;
                    }
                    return Err(status_error(status, &exchange.text.unwrap_or_default()));
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

/// What one attempt came to, for the call ledger (a 2xx whose body could
/// not be read is the connection error the loop turns it into).
fn attempt_result(exchange: &Result<Exchange, LlmErrorKind>) -> Attempt {
    match exchange {
        Err(LlmErrorKind::Timeout) => Attempt::Timeout,
        Err(_) => Attempt::Connection,
        Ok(e) if (200..300).contains(&e.status) && e.text.is_none() => Attempt::Connection,
        Ok(e) if (200..300).contains(&e.status) => Attempt::Ok,
        Ok(e) => Attempt::Status(e.status),
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
    let json: Option<Value> = serde_json::from_str(body).ok();
    // `errJSON ? undefined : errText` — a falsy parse keeps the raw text.
    let raw_message = match &json {
        Some(j) if js_truthy(j) => None,
        _ => Some(body),
    };
    // `errJSON?.['error']` — only an object has an `error` property.
    let error = json
        .as_ref()
        .and_then(Value::as_object)
        .and_then(|o| o.get("error"))
        .filter(|e| js_truthy(e));
    let message = match error {
        Some(error) => {
            let inner = error
                .as_object()
                .and_then(|o| o.get("message"))
                .filter(|m| js_truthy(m));
            Some(match inner {
                Some(Value::String(m)) => m.clone(),
                Some(m) => m.to_string(),
                None => error.to_string(),
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
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
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
            .is_some_and(|c| humanify_ag_model::js::is_js_whitespace(*c))
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
    serde_json::from_str::<AnswerEntries>(content).ok()?.0
}

/// A parsed answer's string-valued entries in `Object.entries` order (an
/// object in JS record order, an array by index, a string by its UTF-16
/// units); None for `null`.
struct AnswerEntries(Option<Renames>);

impl<'de> serde::Deserialize<'de> for AnswerEntries {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        fn some(entries: Vec<(String, Option<String>)>) -> AnswerEntries {
            AnswerEntries(Some(Renames::from_entries(entries)))
        }
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = AnswerEntries;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_unit<E>(self) -> Result<AnswerEntries, E> {
                Ok(AnswerEntries(None))
            }
            fn visit_bool<E>(self, _: bool) -> Result<AnswerEntries, E> {
                Ok(some(Vec::new()))
            }
            fn visit_i64<E>(self, _: i64) -> Result<AnswerEntries, E> {
                Ok(some(Vec::new()))
            }
            fn visit_u64<E>(self, _: u64) -> Result<AnswerEntries, E> {
                Ok(some(Vec::new()))
            }
            fn visit_f64<E>(self, _: f64) -> Result<AnswerEntries, E> {
                Ok(some(Vec::new()))
            }
            // A string's entries are its UTF-16 code units; an astral
            // character's halves cannot be Rust strings and become U+FFFD.
            fn visit_str<E>(self, s: &str) -> Result<AnswerEntries, E> {
                Ok(some(
                    s.encode_utf16()
                        .enumerate()
                        .map(|(i, unit)| {
                            let c = char::from_u32(u32::from(unit)).unwrap_or('\u{FFFD}');
                            (i.to_string(), Some(c.to_string()))
                        })
                        .collect(),
                ))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<AnswerEntries, A::Error> {
                let mut entries = Vec::new();
                let mut i = 0usize;
                while let Some(v) = seq.next_element::<Value>()? {
                    if let Value::String(s) = v {
                        entries.push((i.to_string(), Some(s)));
                    }
                    i += 1;
                }
                Ok(some(entries))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<AnswerEntries, A::Error> {
                let record =
                    <humanify_ag_model::llm::JsRecord<Value> as serde::Deserialize>::deserialize(
                        serde::de::value::MapAccessDeserializer::new(map),
                    )?;
                Ok(some(
                    record
                        .0
                        .into_iter()
                        .filter_map(|(k, v)| match v {
                            Value::String(s) => Some((k, Some(s))),
                            _ => None,
                        })
                        .collect(),
                ))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// The renames a completion's content carries.
pub fn renames_from_content(content: &str) -> Renames {
    json_renames(content).unwrap_or_else(|| parse_renames_from_content(content))
}

/// `extractUsage` → `usageToResult`: prompt → input, completion → output.
fn usage_of(completion: &serde_json::Map<String, Value>) -> Option<Usage> {
    Usage::from_value(
        completion.get("usage")?,
        ["total_tokens", "prompt_tokens", "completion_tokens"],
    )
}

/// A completion body → the response (content, finish reason, usage).
pub fn response_from_completion(
    body: &str,
) -> Result<(BatchRenameResponse, Option<String>), LlmError> {
    let json: Value = serde_json::from_str(body)
        .map_err(|e| LlmError::new(LlmErrorKind::Other, format!("invalid completion body: {e}")))?;
    let completion = json
        .as_object()
        .ok_or_else(|| LlmError::new(LlmErrorKind::Other, "completion body is not an object"))?;
    // `response.choices[0]?.…` — a missing `choices` throws in the TS.
    let first = match completion.get("choices") {
        Some(Value::Array(choices)) => choices.first().and_then(Value::as_object),
        _ => {
            return Err(LlmError::new(
                LlmErrorKind::Other,
                "Cannot read properties of undefined (reading '0')",
            ));
        }
    };
    let finish_reason = first
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let content = first
        .and_then(|c| c.get("message"))
        .and_then(Value::as_object)
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
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
        self.suggest_traced(call, &CallTrace::default()).await
    }

    async fn suggest_traced(
        &self,
        call: &LlmCall,
        trace: &CallTrace,
    ) -> Result<BatchRenameResponse, LlmError> {
        let started = Instant::now();
        let body = self.request_body(&call.system_prompt, &call.user_prompt);
        let result = match self.post_with_sdk_retries(&body, trace).await {
            Ok(text) => response_from_completion(&text).inspect_err(|_| {
                trace.amend_last(Attempt::Malformed);
            }),
            Err(error) => Err(error),
        };
        match &result {
            Ok((_, content)) => self.log_roundtrip(call, started, content.clone(), true),
            Err(error) => self.log_roundtrip(call, started, Some(error.message.clone()), false),
        }
        result.map(|(response, _)| response)
    }
}
