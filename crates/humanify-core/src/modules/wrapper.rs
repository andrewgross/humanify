//! Wrapper IIFE detection (WP1.5) — TS original:
//! `src/analysis/wrapper-detection.ts` (extracted from plugin.ts to break
//! the plugin/prior-version cycle).
//!
//! Span-based port: the TS returns a Scope + NodePath; the Rust returns the
//! wrapper function's span, its body block's span (the classification's
//! container), and the binding count. The threshold guards against small
//! per-module IIFEs (Webpack style).
//!
//! The threshold reads the text it is GIVEN, so a caller handed a
//! post-extraction runtime must answer "is the run's input really a
//! bundled app?" through [`original_bundle_binding_count`] instead —
//! vendor extraction splicing wrapper-scope declarations out of the
//! runtime is expected, and being a bundled app is a property of the
//! INPUT.

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
/// is a single expression statement containing a function, whose scope
/// declares at least [`WRAPPER_IIFE_BINDING_THRESHOLD`] bindings (the TS
/// traverse + `Object.keys(path.scope.bindings).length`).
///
/// The threshold is measured on the text `program` came from. A caller
/// handed a POST-EXTRACTION runtime reads the threshold off the run's
/// input instead — [`original_bundle_binding_count`] — and slices the
/// wrapper body through [`recognize_wrapper_function`].
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
    let wrapper = recognize_wrapper_function(program, semantic)?;
    (wrapper.binding_count >= WRAPPER_IIFE_BINDING_THRESHOLD).then_some(wrapper)
}

/// The wrapper IIFE's SHAPE — the same grammar as
/// [`find_wrapper_function`], with the binding count reported but NO
/// threshold applied. The one caller that may bypass the threshold is the
/// split, which has already answered "is this really a bundled app?" on
/// the run's ORIGINAL input (the vendor extraction it ran downstream
/// removed wrapper-scope bindings from the very text it is splitting).
pub fn recognize_wrapper_function(
    program: &oxc_ast::ast::Program<'_>,
    semantic: &Semantic<'_>,
) -> Option<WrapperFunction> {
    // Skip a Directive Prologue — esbuild's `--format=iife` output (the
    // default bundle form) opens with `"use strict";` before the IIFE.
    // The wrapper must still be the only REAL statement after it.
    let mut body = program.body.as_slice();
    loop {
        match body.split_first() {
            Some((Statement::ExpressionStatement(stmt), rest))
                if is_directive(&stmt.expression) =>
            {
                body = rest
            }
            _ => break,
        }
    }
    // Must be a single expression statement.
    let [Statement::ExpressionStatement(stmt)] = body else {
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

/// A Directive Prologue member (spec): a leading ExpressionStatement whose
/// expression is a bare string literal (`"use strict"`). Parenthesized
/// strings are NOT directives (the ESTree `directive` field's rule), and a
/// template literal never is.
fn is_directive(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::StringLiteral(_))
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

/// The shape's binding count, no threshold (see
/// [`recognize_wrapper_function`]).
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

/// The run's input-bundle gate: is the ORIGINAL text — the input the
/// unpack stage saw, before its vendor extraction spliced the CJS
/// factories out — really a bundled app? The frozen WP1.5 grammar AND the
/// ≥50 wrapper-binding threshold, both measured on the ORIGINAL.
///
/// The split asks this instead of re-measuring the post-extraction
/// runtime it is handed (2026-10-02): being a bundled app is a property
/// of the INPUT, and a mid-size app whose vendor half dominates (one
/// wrapper-scope `var require_*` per vendored module removed by the
/// extraction) lands UNDER the threshold on the runtime — 32 wrapper
/// bindings on the esbuild lane's real test app — although its input
/// clears it comfortably. The GRAMMAR still reads the runtime (a plain
/// script never becomes splittable by its input's shape).
///
/// Returns the original wrapper's binding count — the caller's proof the
/// input is bundled, threaded to the split's statement slicing.
pub fn original_bundle_binding_count(original: &str) -> Result<usize, String> {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, original, "input bundle");
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc failed to parse the input bundle: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic()).ok_or(
        "the run's input bundle has no recognizable bundle wrapper (the frozen \
≥50-binding gate is measured on the input)",
    )?;
    Ok(wrapper.binding_count)
}
