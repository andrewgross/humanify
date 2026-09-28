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
//! - [`reask`] — what happens to a rejected rename suggestion: the ONE
//!   retry policy the wave barrier and the coverage sweep share
//!   (2026-09-28: the collision classes get one disclosed re-ask, the
//!   unrecoverable classes stay loud).
//! - [`js_record`] — the one owner of "what does `record[key]` read" for a
//!   TS `Record<string, string>` (an absent key falls through to
//!   Object.prototype — probed, see the module).

pub mod code_window;
pub mod context;
pub mod driver;
pub mod js_record;
pub mod passes;
pub mod prompts;
pub mod reask;
pub mod reconcile;
pub mod report;
pub mod snap;
pub mod validation;
pub mod waves;
