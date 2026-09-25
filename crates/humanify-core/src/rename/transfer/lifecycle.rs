//! The graph-row lifecycle (WP3.2) — TS original: `src/rename/lifecycle.ts`.
//!
//! A node is handled exactly once, so every legal transition is
//! pending → settled; a second write is a double-handling bug and panics
//! (the TS throws). The Rust graph rows carry no state, so the transfer
//! stage keeps one [`Lifecycle`] per function row and per module-binding
//! row (parallel to `UnifiedGraph::functions` / `module_bindings`).

use crate::rename::validated::scopes::BindingId;

/// One prior-version name transfer (TS `TransferPair`): `old_name` is the
/// fresh (minified) name, `new_name` the prior name it inherits, `binding`
/// the exact binding the pair targets (resolved through the placeholder
/// slot at match time) — None for a POSITIONAL pair (close-match signature
/// transfers), which the applier resolves through the owned-binding map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferPair {
    pub old_name: String,
    pub new_name: String,
    pub binding: Option<BindingId>,
}

/// TS `LifecycleState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lifecycle {
    /// Not yet handled; still eligible for dispatch.
    Pending,
    /// A prior-version exact match: the pairs the transfer pass applies.
    Transferred(Vec<TransferPair>),
    /// Frozen without the LLM (library / wrapper / eval-with taint), or a
    /// prior-version-matched / propagated binding.
    Skipped(&'static str),
    /// Named by the LLM waves (`markLlmDone`).
    LlmDone,
    /// The wave task failed (`markFailed`).
    Failed,
}

impl Lifecycle {
    /// TS `isPending`.
    pub fn is_pending(&self) -> bool {
        matches!(self, Lifecycle::Pending)
    }

    /// TS `isSettled`.
    pub fn is_settled(&self) -> bool {
        !self.is_pending()
    }

    /// TS `transition`: only pending → settled is legal.
    pub fn transition(&mut self, to: Lifecycle, who: &str) {
        assert!(
            self.is_pending(),
            "illegal lifecycle transition for {who}: {self:?} -> {to:?} (only pending -> settled is allowed)"
        );
        *self = to;
    }

    /// TS `markSkipped`.
    pub fn mark_skipped(&mut self, reason: &'static str, who: &str) {
        self.transition(Lifecycle::Skipped(reason), who);
    }

    /// TS `markLlmDone`.
    pub fn mark_llm_done(&mut self, who: &str) {
        self.transition(Lifecycle::LlmDone, who);
    }

    /// TS `markFailed`.
    pub fn mark_failed(&mut self, who: &str) {
        self.transition(Lifecycle::Failed, who);
    }

    /// TS `markTransferred`.
    pub fn mark_transferred(&mut self, pairs: Vec<TransferPair>, who: &str) {
        self.transition(Lifecycle::Transferred(pairs), who);
    }
}
