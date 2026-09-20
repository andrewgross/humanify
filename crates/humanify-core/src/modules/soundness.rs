//! Detection of constructs that make name-based renaming unsound (WP1.5) —
//! TS original: `src/analysis/soundness.ts`.
//!
//! A `with (obj) { ... }` block resolves bare identifiers against obj's
//! properties at RUNTIME; a direct `eval(...)` executes source that can
//! reference any binding visible at the call site by its ORIGINAL name.
//! Renaming any binding visible at such a site can change behavior while
//! still parsing cleanly — the output gates cannot catch it.
//!
//! The sound response is to freeze everything visible at the site: every
//! enclosing function's own bindings, and (since scope chains end there)
//! the module-level bindings too. Functions off the scope chain keep
//! renaming — eval'd code cannot see their locals.
//!
//! Span-based port: the TS taint set is `Set<t.Node>` over function NODES;
//! the Rust set is the functions' SPANS (the dump's identity). Runs once
//! per file at graph-build time; the rename passes (WP2+) freeze tainted
//! functions — and, when any site exists, module bindings — so neither the
//! LLM pass nor prior-version transfer renames them.

use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::{AstNodes, NodeId, Semantic};
use oxc_span::{GetSpan, Span};

/// The eval/with taint for an AST (EvalWithTaint).
pub struct EvalWithTaint {
    /// Functions (by span) on some taint site's scope chain.
    pub tainted_functions: Vec<Span>,
    /// True when any site exists — module-level bindings are always visible.
    pub module_tainted: bool,
    /// Number of with/direct-eval sites found.
    pub site_count: usize,
}

/// Collects eval/with taint. Runs once per file at graph-build time.
pub fn collect_eval_with_taint(semantic: &Semantic<'_>) -> EvalWithTaint {
    let nodes = semantic.nodes();
    let mut tainted: Vec<Span> = Vec::new();
    let mut site_count = 0usize;

    for node in nodes.iter() {
        let is_site = match node.kind() {
            AstKind::WithStatement(_) => true,
            AstKind::CallExpression(call) => is_direct_eval(call, node.id(), nodes, semantic),
            _ => false,
        };
        if !is_site {
            continue;
        }
        site_count += 1;
        // Walk up from the site, collecting every enclosing function's span
        // (the TS taintScopeChain's getFunctionParent chain) — via the one
        // owner of the enclosing-functions question.
        for id in crate::graph::enclosing_function_node_ids(node.id(), nodes) {
            let span = nodes.get_node(id).span();
            if !tainted.contains(&span) {
                tainted.push(span);
            }
        }
    }

    EvalWithTaint {
        tainted_functions: tainted,
        module_tainted: site_count > 0,
        site_count,
    }
}

/// True for `eval(...)` where eval is NOT a local binding (direct eval).
/// A locally bound eval is an ordinary function — renaming stays sound.
///
/// `!path.scope.getBinding("eval")` — getBinding walks UP the scope chain
/// from the site's scope; a binding is visible exactly when it is declared
/// in a scope whose NODE is on the site's ancestor chain (function scopes,
/// block scopes, the program scope — oxc's scope table models var-hoisting,
/// so the node-level walk is the same visibility question). Checked by
/// SYMBOL NAME: oxc's find_binding keys on an arena Ident.
fn is_direct_eval(
    call: &oxc_ast::ast::CallExpression<'_>,
    node_id: NodeId,
    nodes: &AstNodes<'_>,
    semantic: &Semantic<'_>,
) -> bool {
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    if callee.name != "eval" {
        return false;
    }
    // The ancestor chain: the call's node, every parent up to the program.
    let mut chain: Vec<NodeId> = vec![node_id];
    let mut prev = node_id;
    loop {
        let parent = nodes.parent_id(prev);
        if parent == prev {
            break;
        }
        chain.push(parent);
        prev = parent;
    }
    let scoping = semantic.scoping();
    for sid in 0..scoping.scopes_len() {
        let scope_id = oxc_semantic::ScopeId::new(sid);
        let scope_node = scoping.get_node_id(scope_id);
        if !chain.contains(&scope_node) {
            continue;
        }
        for symbol in scoping.iter_bindings_in(scope_id) {
            if scoping.symbol_name(symbol) == "eval" {
                return false; // locally bound eval — renaming stays sound
            }
        }
    }
    true
}

/// True when renaming a binding whose ENCLOSING FUNCTION's scope is
/// `enclosing_fn_span` (None = module level) is unsound because it is
/// visible at a with/direct-eval site. The one predicate every rename pass
/// consults so the freeze rule cannot drift between them (the graph pass
/// freezes these pre-emptively; passes that run afterward re-check here).
pub fn is_binding_eval_taint_frozen(
    enclosing_fn_span: Option<Span>,
    taint: &EvalWithTaint,
) -> bool {
    if taint.site_count == 0 {
        return false;
    }
    match enclosing_fn_span {
        // No enclosing function scope → module level → always visible.
        None => taint.module_tainted,
        Some(span) => taint.tainted_functions.contains(&span),
    }
}
