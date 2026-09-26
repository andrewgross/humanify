//! The per-function naming context — src/rename/context-builder.ts
//! `buildContext`: callee signatures, call sites, the identifiers already
//! in use, and (when the scope parent is still pending) the parent scope's
//! declarations as read-only context.
//!
//! The builder reads the scope state AT WAVE TIME — binding names after
//! every rename applied so far, in Babel's `Object.keys(scope.bindings)`
//! order (a rename moves a binding's key to the END of its scope's table),
//! and generated code for non-trivial params, callee bodies and parent
//! declarations. That state belongs to the rename overlay and the renderer
//! (WP4.3 / rename::validated), so this module takes it as a [`ContextView`]
//! and owns everything the TS decides FROM it: the callee name precedence,
//! the param forms, the 3-line snippet, the ordered union of used names,
//! and the context-var rules (first line, JS trim, ≤ 120 UTF-16 units,
//! stop after 30). Gated against every buildContext call of the four
//! oracle pairs with the TS's own view captured at call time
//! (`humanify prompt-gate`, context section).

use humanify_model::js::{trim, utf16_len};
use humanify_model::llm::CalleeSignature;

/// A callee parameter as the TS classifies it.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ParamView {
    /// `a` → `a`
    Identifier { name: String },
    /// `...a` (identifier argument) → `...a`
    Rest { name: String },
    /// `a = 1` (identifier left) → `a`
    Assign { name: String },
    /// Anything else: the generated code (`generate(param)` with
    /// `compact: false, comments: false`; "[code generation failed]" when
    /// generation throws).
    Other { code: String },
}

/// One internal callee, as `getCalleeSignatures` reads it.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalleeView {
    /// The Babel node type (`FunctionDeclaration`, `FunctionExpression`,
    /// `ArrowFunctionExpression`, `ObjectMethod`, …).
    pub node_type: String,
    /// The node's own `id` name, when it has one.
    pub id: Option<String>,
    /// The parent `VariableDeclarator`'s identifier name, when the parent
    /// is one.
    pub declarator_id: Option<String>,
    pub params: Vec<ParamView>,
    /// The generated body (`compact: false, comments: false`).
    pub body_code: String,
}

/// A parent-scope binding's declaration, as `getBindingDeclCode` reads it.
#[derive(Clone, Debug, PartialEq)]
pub enum DeclView {
    /// A function or class declaration — contributes nothing.
    FunctionOrClass,
    /// A variable declarator: the generated code of its whole declaration.
    Declarator { code: String },
    /// Anything else: the generated binding node ("" when generation threw).
    Other { code: String },
}

impl DeclView {
    fn code(&self) -> &str {
        match self {
            DeclView::FunctionOrClass => "",
            DeclView::Declarator { code } | DeclView::Other { code } => code,
        }
    }
}

/// One entry of the parent function scope's bindings, in table order.
#[derive(Clone, Debug, PartialEq)]
pub struct ParentBinding {
    pub name: String,
    pub decl: DeclView,
}

/// The scope state `buildContext` reads, at wave time.
#[derive(Clone, Debug, PartialEq)]
pub struct ContextView {
    /// `fn.internalCallees` in Set (insertion) order.
    pub callees: Vec<CalleeView>,
    /// Binding names of each non-program scope from the function's own
    /// scope outward, each in its table order.
    pub scope_chain: Vec<Vec<String>>,
    /// The program scope's binding names, in table order.
    pub program_bindings: Vec<String>,
    /// The program scope's free names (`scope.globals`), in order.
    pub program_globals: Vec<String>,
    /// The scope parent's bindings — `Some` only when the function HAS a
    /// scope parent that is still pending (deadlock-broken processing).
    pub parent_bindings: Option<Vec<ParentBinding>>,
}

/// `LLMContext`.
#[derive(Clone, Debug, PartialEq)]
pub struct LlmContext {
    pub callee_signatures: Vec<CalleeSignature>,
    pub callsites: Vec<String>,
    /// A Set in insertion order.
    pub used_identifiers: Vec<String>,
    pub context_vars: Option<Vec<String>>,
}

/// Lines of callee body kept as the signature's snippet.
const SNIPPET_LINES: usize = 3;
/// Cap on parent-scope context declarations.
const MAX_CONTEXT_VARS: usize = 30;
/// A context declaration longer than this (UTF-16 units) is dropped.
const MAX_CONTEXT_VAR_LENGTH: usize = 120;

/// The callee's display name: its own id for a function declaration or
/// expression, else the declarator it initializes, else "anonymous".
fn callee_name(c: &CalleeView) -> String {
    let own = matches!(
        c.node_type.as_str(),
        "FunctionDeclaration" | "FunctionExpression"
    );
    own.then(|| c.id.clone())
        .flatten()
        .or_else(|| c.declarator_id.clone())
        .unwrap_or_else(|| "anonymous".to_string())
}

fn param_text(p: &ParamView) -> String {
    match p {
        ParamView::Identifier { name } | ParamView::Assign { name } => name.clone(),
        ParamView::Rest { name } => format!("...{name}"),
        ParamView::Other { code } => code.clone(),
    }
}

fn callee_signature(c: &CalleeView) -> CalleeSignature {
    let snippet: Vec<&str> = c.body_code.split('\n').take(SNIPPET_LINES).collect();
    CalleeSignature {
        name: callee_name(c),
        params: c.params.iter().map(param_text).collect(),
        snippet: Some(snippet.join("\n")),
    }
}

/// Every binding name from the function's scope outward, then the file's
/// free names — an insertion-ordered set.
/// The Set's order has ONE owner, [`UsedSet`] (the wave processor holds
/// the same layers shared across contexts).
fn used_identifiers(view: &ContextView) -> Vec<String> {
    use crate::naming::waves::used_set::{NameLayer, UsedSet};
    let layer = |names: &Vec<String>| std::sync::Arc::new(NameLayer::new(names.iter().cloned()));
    let layers = view
        .scope_chain
        .iter()
        .chain([&view.program_bindings, &view.program_globals])
        .map(layer)
        .collect();
    UsedSet::new(layers).order().map(str::to_string).collect()
}

/// `getParentScopeContextVars`: eligible bindings' declaration first
/// lines, JS-trimmed, at most 120 UTF-16 units, stopping once 30 are kept.
fn parent_context_vars(
    bindings: &[ParentBinding],
    is_eligible: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut vars = Vec::new();
    for b in bindings {
        if vars.len() >= MAX_CONTEXT_VARS {
            break;
        }
        if !is_eligible(&b.name) {
            continue;
        }
        let code = b.decl.code();
        if code.is_empty() {
            continue;
        }
        let line = trim(code.split('\n').next().unwrap_or_default());
        if utf16_len(line) <= MAX_CONTEXT_VAR_LENGTH {
            vars.push(line.to_string());
        }
    }
    vars
}

/// Build the naming context for one function (`buildContext`). `callsites`
/// is the graph's `fn.callSites` code, passed through.
pub fn build_context(
    view: &ContextView,
    callsites: &[String],
    is_eligible: impl Fn(&str) -> bool,
) -> LlmContext {
    let context_vars = view
        .parent_bindings
        .as_deref()
        .map(|b| parent_context_vars(b, is_eligible))
        .filter(|v| !v.is_empty());
    LlmContext {
        callee_signatures: view.callees.iter().map(callee_signature).collect(),
        callsites: callsites.to_vec(),
        used_identifiers: used_identifiers(view),
        context_vars,
    }
}

#[cfg(test)]
mod context_test;
