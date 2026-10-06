//! Names the PIPELINE chooses, never the model: the bundler's own runtime
//! plumbing, recognised by shape by the run's toolchain (2026-10-05,
//! Andrew's decision (a) on the "Once" pattern).
//!
//! Today that is one binding: the lazy-init helper (`__esm` in Bun's and
//! esbuild's runtimes, minified to a letter in a Bun build), recognised by
//! `toolchain::ModuleWrapperGrammar::lazy_init_helpers` — the ONE shape
//! owner the split's load order and the fossil layout already read. Every
//! lazy-init wrapper in the bundle is `var X = helper(() => { … })`, and
//! since the prompts show CURRENT names (finding #83) every wrapper's ask
//! shows the helper's name. When the model chose that name it echoed it:
//! in 2.1.197/198 it named the helper `once`, and 40% of the wrapper
//! answers that saw `once(() => …)` came back as `initFooOnce` (1,642
//! names in the scored 2.1.198 tree, against ~5 in runs where the helper
//! was named anything else). A replay of 150 recorded wrapper asks with
//! only the helper's name changed measured the echo per candidate name:
//! `once` 34%, `lazyInit` 31%, `esmInit` 18%, `__esm` 0%.
//!
//! So the helper gets `__esm` — the name its own bundler gives it, and a
//! name the never-rename rules already keep (a word-like `__` prefix,
//! `rename::eligibility`), so no later pass can rename it again. It is
//! applied as a deterministic decision before the first wave (after the
//! prior-version transfer, which it overrides: a prior tree may carry a
//! model's name for it), recorded on the trail under
//! [`Tier::ToolchainPlumbing`], and the binding is settled so no wave asks
//! for it. Nothing rewrites a model answer.
//!
//! Precision first: exactly ONE recognised helper is named; two (never
//! seen in a real bundle) are left to the model, as is a helper whose
//! rename the validated applier refuses (a binding already called `__esm`
//! in its scope). Both are recorded.

use std::collections::HashSet;

use crate::graph::UnifiedGraph;
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};

mod library_imports;
#[cfg(test)]
mod plumbing_test;

pub(crate) use library_imports::sole_write;
pub use library_imports::{
    conventional_import_name, is_library_specifier, name_library_imports, require_specifier,
};

/// The lazy-init helper's pipeline-chosen name: Bun's and esbuild's own.
pub const LAZY_INIT_HELPER_NAME: &str = "__esm";

/// What the plumbing pass did, for the run's record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlumbingNames {
    /// `(name before, plumbing name)` per binding renamed.
    pub named: Vec<(String, String)>,
    /// `(name, why)` per recognised helper left to the model.
    pub declined: Vec<(String, String)>,
}

/// The lazy-init helpers among the container's top-level statements: the
/// wrapper body when the layout found one, else the program body — the
/// statements the split's load order reads the same question over.
pub fn lazy_init_helpers_of(
    grammar: crate::toolchain::ModuleWrapperGrammar,
    program_json: &serde_json::Value,
    wrapper_body: Option<oxc_span::Span>,
) -> HashSet<String> {
    let body = match wrapper_body {
        Some(span) => crate::twins::block_body_by_span(program_json, span),
        None => program_json
            .get("body")
            .and_then(serde_json::Value::as_array),
    };
    body.map(|b| grammar.lazy_init_helpers(b))
        .unwrap_or_default()
}

/// Name the lazy-init helper among `graph`'s module bindings. `helpers`
/// holds the ORIGINAL (fresh-text) names the toolchain recognised.
pub fn name_lazy_init_helper(
    graph: &UnifiedGraph,
    helpers: &HashSet<String>,
    state: &mut RenameState,
    binding_state: &mut [Lifecycle],
) -> PlumbingNames {
    let mut out = PlumbingNames::default();
    let rows: Vec<usize> = graph
        .module_bindings
        .iter()
        .enumerate()
        .filter(|(_, b)| helpers.contains(&b.name))
        .map(|(i, _)| i)
        .collect();
    if rows.len() > 1 {
        for &row in &rows {
            let name = graph.module_bindings[row].name.clone();
            out.declined.push((name, "more-than-one-helper".into()));
        }
        return out;
    }
    let Some(&row) = rows.first() else {
        return out;
    };
    let b = &graph.module_bindings[row];
    let Some(binding) = state.view().binding_of_symbol(b.symbol) else {
        out.declined.push((b.name.clone(), "no-binding".into()));
        return out;
    };
    let current = state.name_of(binding).to_string();
    // A settled binding (the transfer carried a name) gets a post-pass row:
    // the plumbing name overrides it.
    let post_pass = binding_state[row].is_settled();
    if current != LAZY_INIT_HELPER_NAME {
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope: state.scope_of_binding(binding),
                old_name: &current,
                new_name: LAZY_INIT_HELPER_NAME,
                expected: Some(binding),
            },
            TrailSpec::CallerRecords {
                tier: Tier::ToolchainPlumbing,
            },
        );
        if !attempt.applied {
            let why = attempt.reason.map_or("unknown", |r| r.as_str());
            let row_attempt = Attempt::new(Tier::ToolchainPlumbing, Outcome::Rejected)
                .reason(why)
                .proposed(LAZY_INIT_HELPER_NAME);
            state.record(binding, &current, row_attempt, post_pass);
            out.declined.push((current, why.to_string()));
            return out;
        }
        let applied =
            Attempt::new(Tier::ToolchainPlumbing, Outcome::Applied).proposed(LAZY_INIT_HELPER_NAME);
        state.record(binding, &current, applied, post_pass);
        out.named
            .push((current.clone(), LAZY_INIT_HELPER_NAME.to_string()));
    }
    if binding_state[row].is_pending() {
        binding_state[row].mark_skipped("toolchain-plumbing", &b.session_id);
    }
    out
}
