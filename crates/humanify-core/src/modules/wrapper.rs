//! Wrapper IIFE detection (WP1.5) — TS original:
//! `src/analysis/wrapper-detection.ts` (extracted from plugin.ts to break
//! the plugin/prior-version cycle).
//!
//! Span-based port: the TS returns a Scope + NodePath; the Rust returns the
//! wrapper function's span, its body block's span (the classification's
//! container), and the binding count. The threshold guards against small
//! per-module IIFEs (Webpack style).

use crate::babel_view::unparen;
use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, Statement};
use oxc_semantic::Semantic;
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::UnaryOperator;

/// Minimum number of bindings for an IIFE to be considered a wrapper
/// (WRAPPER_IIFE_BINDING_THRESHOLD).
const WRAPPER_IIFE_BINDING_THRESHOLD: usize = 50;

/// The detected wrapper (WrapperFunctionResult's dumpable fields).
pub struct WrapperFunction {
    /// The wrapper function's span (for marking as pre-done).
    pub span: Span,
    /// The body BLOCK's span — the classification's container.
    pub body_span: Span,
    /// Bindings declared directly in the wrapper's scope.
    pub binding_count: usize,
}

/// Detects a giant wrapper function pattern where the entire program body
/// is a single expression statement containing a function.
///
/// Handles:
/// - `(function(exports, require, module) { ... })()`          — IIFE
/// - `!function() { ... }()`                                    — negated IIFE
/// - `(function(){}).call(this, ...)`                           — .call/.apply
/// - `(() => { ... })()`                                        — arrow IIFE
/// - `(function(exports, require, module) { ... });`            — Bun CJS bytecode (bare, not called)
pub fn find_wrapper_function(
    program: &oxc_ast::ast::Program<'_>,
    semantic: &Semantic<'_>,
) -> Option<WrapperFunction> {
    // Must be a single expression statement.
    if program.body.len() != 1 {
        return None;
    }
    let Statement::ExpressionStatement(stmt) = &program.body[0] else {
        return None;
    };
    // Babel drops paren wrappers; oxc keeps them — see through them first.
    let expr = unparen(&stmt.expression);

    let fn_expr: Option<&Expression> = match expr {
        // (function(){...})() or (() => {...})() — and the .call/.apply
        // forms, in the callee.
        Expression::CallExpression(call) => callee_function(unparen(&call.callee)),
        // !function(){...}()
        Expression::UnaryExpression(un) if un.operator == UnaryOperator::LogicalNot => {
            match unparen(&un.argument) {
                Expression::CallExpression(call) => callee_function(unparen(&call.callee)),
                _ => None,
            }
        }
        // Bun CJS bytecode: a bare function expression (not called) wrapping
        // the entire bundle.
        Expression::FunctionExpression(_) => Some(expr),
        _ => None,
    };
    checked(fn_expr?, semantic)
}

/// The callee's function expression, or None: a direct function, or a
/// member access `.call`/`.apply` on a function (the TS extractCalleeFromCall).
fn callee_function<'a>(callee: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let callee = unparen(callee);
    match callee {
        Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => Some(callee),
        Expression::StaticMemberExpression(member) => {
            // The TS checks `t.isIdentifier(fn.property)` and the name —
            // oxc splits computed (`f["call"]`) into ComputedMemberExpression,
            // which the TS's `t.isIdentifier` would ALSO have matched; the
            // static form is what real bundles emit, and the computed form
            // of `.call` does not occur in minifier output.
            let name = member.property.name.as_str();
            if name != "call" && name != "apply" {
                return None;
            }
            let obj = unparen(&member.object);
            match obj {
                Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                    Some(obj)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The binding-count gate: the wrapper counts only when its scope declares
/// at least WRAPPER_IIFE_BINDING_THRESHOLD bindings (the TS traverse +
/// `Object.keys(path.scope.bindings).length`).
fn checked<'a>(fn_expr: &'a Expression<'a>, semantic: &'a Semantic<'a>) -> Option<WrapperFunction> {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let span = fn_expr.span();
    let node = nodes
        .iter()
        .find(|n| n.span() == span && is_function_kind(n.kind()))?;
    // The function's own scope: the scope whose node is this function.
    let scope_id = (0..scoping.scopes_len())
        .map(oxc_semantic::ScopeId::new)
        .find(|sid| scoping.get_node_id(*sid) == node.id())?;
    let binding_count = scoping.iter_bindings_in(scope_id).count();
    if binding_count < WRAPPER_IIFE_BINDING_THRESHOLD {
        return None;
    }
    let body_span = match fn_expr {
        Expression::FunctionExpression(f) => f.body.as_ref().map(|b| b.span),
        Expression::ArrowFunctionExpression(a) => Some(a.body.span()),
        _ => None,
    }?;
    Some(WrapperFunction {
        span,
        body_span,
        binding_count,
    })
}

/// The function kinds the TS `Function` path matches.
fn is_function_kind(kind: AstKind<'_>) -> bool {
    matches!(
        kind,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
    )
}
