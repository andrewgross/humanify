//! The LLM naming stage (02 §2: `naming::{prompts,waves,reconcile,passes,
//! driver}`). WP4.2 lands its leaves — everything that turns frozen wave
//! context into the bytes the model sees, and the name-legality tables:
//!
//! - [`prompts`] — the prompt builders (src/llm/prompts.ts) and the
//!   provider's render selection (openai-compatible.ts
//!   `buildBatchUserPrompt`). Prompts are an external contract with the
//!   disk cache (02 §7), byte-exact.
//! - [`code_window`] — the code shown for an oversized function
//!   (src/rename/code-window.ts).
//! - [`context`] — the per-function naming context (callee signatures,
//!   used identifiers, parent-scope context vars;
//!   src/rename/context-builder.ts), over a read-only view of the scope
//!   state at wave time.
//! - [`validation`] — reserved words, global builtins, identifier syntax,
//!   the decoration ladder (src/llm/validation.ts — the DECORATION_WORDS
//!   owner).
//! - [`prompt_gate`] — the byte-identity gate against the oracle's
//!   prompts and the captured builder inputs (migration scaffolding).
//! - [`js_record`] — the one owner of "what does `record[key]` read" for a
//!   TS `Record<string, string>` (an absent key falls through to
//!   Object.prototype — probed, see the module).

pub mod code_window;
pub mod context;
pub mod driver;
pub mod js_record;
pub mod passes;
pub mod prompt_gate;
pub mod prompts;
pub mod reconcile;
pub mod report;
pub mod snap;
pub mod validation;
pub mod waves;

/// The WP4.2 probe vectors (test/parity/wp42-vectors.json), recorded from
/// the real TS functions by test/parity/wp42-probe.ts.
#[cfg(test)]
fn test_vectors_text() -> String {
    let path = format!(
        "{}/../../test/parity/wp42-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[cfg(test)]
pub(crate) fn test_vectors() -> serde_json::Value {
    serde_json::from_str(&test_vectors_text()).unwrap()
}

/// The same vectors through the JS-semantics parser — object key ORDER
/// preserved (serde_json::Value sorts keys).
#[cfg(test)]
pub(crate) fn test_vectors_js() -> humanify_model::js::JsValue {
    humanify_model::js::JsValue::parse(&test_vectors_text()).unwrap()
}
