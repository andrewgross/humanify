//! The debug wrapper (llm/debug-wrapper.ts) and the log seam.
//!
//! The TS layers call the process-wide `debug` logger directly. The logger
//! lives in humanify-cli (`log.rs`, the `-vv` surfaces), which this crate
//! cannot depend on — so every layer here emits [`LlmLogEvent`]s into an
//! optional [`LogSink`] the CLI supplies (humanify_cli::log formats them).
//! No sink = nothing logged, like the TS with debug disabled.

use std::sync::Arc;
use std::time::Instant;

use humanify_model::llm::{BatchRenameResponse, LlmCall, LlmError};

use crate::provider::AsyncProvider;

/// One `debug.llmRoundtrip` block (the fields the `-vv` log prints).
#[derive(Clone, Debug, PartialEq)]
pub struct Roundtrip {
    pub method: String,
    pub model: Option<String>,
    pub identifiers: Vec<String>,
    pub system_prompt: Option<String>,
    pub user_prompt: Option<String>,
    /// The raw model content, or the error message on failure.
    pub raw_response: Option<String>,
    pub duration_ms: u128,
    pub ok: bool,
}

/// What the LLM layers log.
#[derive(Clone, Debug, PartialEq)]
pub enum LlmLogEvent {
    /// `debug.log(category, message)`.
    Message {
        category: String,
        message: String,
    },
    Roundtrip(Roundtrip),
}

pub type LogSink = Arc<dyn Fn(LlmLogEvent) + Send + Sync>;

pub(crate) fn emit(sink: &Option<LogSink>, event: LlmLogEvent) {
    if let Some(sink) = sink {
        sink(event);
    }
}

/// The TS DebugLLMProvider: logs the identifiers before the call and one
/// roundtrip block after it (request + response together, so concurrent
/// calls never interleave).
pub struct DebugProvider<P> {
    inner: P,
    model: Option<String>,
    log: Option<LogSink>,
}

impl<P> DebugProvider<P> {
    pub fn new(inner: P, model: Option<String>, log: Option<LogSink>) -> Self {
        DebugProvider { inner, model, log }
    }
}

/// The wrapper's user-prompt summary when the request carries no explicit
/// user prompt (debug-wrapper.ts, byte for byte).
fn prompt_summary(call: &LlmCall) -> String {
    let request = &call.request;
    if let Some(user) = request.user_prompt.as_ref().filter(|u| !u.is_empty()) {
        return user.clone();
    }
    let used: Vec<&str> = request
        .used_names
        .iter()
        .take(30)
        .map(String::as_str)
        .collect();
    let is_retry = match request.is_retry {
        Some(true) => "true",
        Some(false) => "false",
        None => "undefined",
    };
    let failures = request
        .failures
        .as_ref()
        .map(|f| {
            format!(
                "Failures: duplicates={}, invalid={}",
                f.duplicates.join(","),
                f.invalid.join(",")
            )
        })
        .unwrap_or_default();
    format!(
        "Code:\n{}\n\nIdentifiers: {}\nUsed names: {}...\nIs retry: {is_retry}\n{failures}",
        request.code,
        request.identifiers.join(", "),
        used.join(", ")
    )
}

impl<P: AsyncProvider> AsyncProvider for DebugProvider<P> {
    async fn suggest_all_names(&self, call: &LlmCall) -> Result<BatchRenameResponse, LlmError> {
        let identifiers = call.request.identifiers.clone();
        emit(
            &self.log,
            LlmLogEvent::Message {
                category: "llm".to_string(),
                message: format!("suggestAllNames → identifiers: {}", identifiers.join(", ")),
            },
        );
        let start = Instant::now();
        let result = self.inner.suggest_all_names(call).await;
        let (ok, raw_response, user_prompt) = match &result {
            Ok(response) => (true, Some(response.to_json()), Some(prompt_summary(call))),
            Err(error) => (false, Some(error.message.clone()), None),
        };
        emit(
            &self.log,
            LlmLogEvent::Roundtrip(Roundtrip {
                method: "suggestAllNames".to_string(),
                model: self.model.clone(),
                identifiers,
                system_prompt: None,
                user_prompt,
                raw_response,
                duration_ms: start.elapsed().as_millis(),
                ok,
            }),
        );
        result
    }

    fn cache_stats(&self) -> Option<crate::cache::CacheStats> {
        self.inner.cache_stats()
    }
}
