//! Module bodies a bundler writes INLINE — not behind a module helper
//! (`var X = __commonJS(fn)`, which `classify_bun_modules` reads) but as
//! functions inside the bundle's own statements (exp105). One recogniser
//! per bundler grammar (`toolchain::ModuleWrapperGrammar`); the statement
//! cut (`cut::units`) asks the run's grammar and gives each body found a
//! lazy-module container whose statements are units of their own — the
//! same unit model on the index side and the run side.
//!
//! - **webpack 5**: the module TABLE (`var e = {123(e,t,n){…}, 456:(e)=>{…}}`
//!   — numeric module ids, each value the module's function) and the
//!   CommonJS wrapper `R.cw(function(e,t){…})` webpack 5 writes for a
//!   CommonJS module it inlines into a concatenated one. Each body runs
//!   once, on the module's first `require` — what a lazy module is. The
//!   IIFEs that hold the concatenated (scope-hoisted) entry modules
//!   (`(()=>{…})()`) are not modules but SCOPES
//!   ([`webpack_statement_scopes`]): their statements are cut like the
//!   bootstrap's own, as Bun's and rollup's hoisted statements are.
//! - **rollup** (`@rollup/plugin-commonjs`): the guarded require function
//!   of a CommonJS module (`function requireX(){if(h)return m;h=1;…}`), as
//!   a declaration or inlined by the minifier as an IIFE
//!   (`var a=function(){if(t)return r;t=1;…}()`, also as the one argument
//!   of a call: `getDefaultExportFromCjs(requireX())`).
//!
//! The Bun/esbuild grammar finds none of these: its modules are the
//! helper's factories and `__esm` inits.

use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, Expression, Function, ObjectPropertyKind, PropertyKey,
    Statement, VariableDeclarator,
};
use oxc_span::{GetSpan, Span};
use oxc_syntax::operator::{AssignmentOperator, UnaryOperator};

use crate::babel_view::unparen;

/// One inline module body: the span of the node that wraps it (a table
/// entry, a function) and its statements.
pub type InlineModule<'a> = (Span, &'a [Statement<'a>]);

/// The property webpack 5 writes for its CommonJS wrapper
/// (`__webpack_require__.cw`): the runtime's own spelling, as fixed as
/// `.d` and `.o`.
const WEBPACK_COMMONJS_WRAPPER: &str = "cw";

// ---- webpack ------------------------------------------------------------------

/// webpack's inline modules in one declarator: the entries of a module
/// table, or the function of a CommonJS wrapper call.
pub fn webpack_declarator_modules<'a>(d: &'a VariableDeclarator<'a>) -> Vec<InlineModule<'a>> {
    let Some(init) = d.init.as_ref().map(unparen) else {
        return Vec::new();
    };
    match init {
        Expression::ObjectExpression(o) => module_table(&o.properties),
        Expression::CallExpression(call) => commonjs_wrapper(call).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Every entry of a module table, or none: a table is an object whose keys
/// are ALL module ids (numbers) and whose values are ALL module functions
/// with a block body.
fn module_table<'a>(properties: &'a [ObjectPropertyKind<'a>]) -> Vec<InlineModule<'a>> {
    let mut out = Vec::new();
    for p in properties {
        let ObjectPropertyKind::ObjectProperty(p) = p else {
            return Vec::new();
        };
        if p.computed || !matches!(p.key, PropertyKey::NumericLiteral(_)) {
            return Vec::new();
        }
        let Some(body) = function_body(&p.value) else {
            return Vec::new();
        };
        out.push((p.span, body));
    }
    out
}

/// `R.cw(function(module, exports){…})`: webpack 5's lazy CommonJS module.
fn commonjs_wrapper<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<InlineModule<'a>> {
    let callee = unparen(&call.callee).as_member_expression()?;
    if callee.static_property_name() != Some(WEBPACK_COMMONJS_WRAPPER)
        || !matches!(unparen(callee.object()), Expression::Identifier(_))
    {
        return None;
    }
    let [Argument::FunctionExpression(f)] = call.arguments.as_slice() else {
        return None;
    };
    Some((f.span, f.body.as_ref()?.statements.as_slice()))
}

/// webpack's inline SCOPES in one statement: the IIFEs of an expression
/// statement (alone or in a sequence: `(()=>{…})(),module.exports=i`) —
/// each holds concatenated modules' statements, scope-hoisted.
pub fn webpack_statement_scopes<'a>(stmt: &'a Statement<'a>) -> Vec<InlineModule<'a>> {
    let Statement::ExpressionStatement(s) = stmt else {
        return Vec::new();
    };
    match unparen(&s.expression) {
        Expression::SequenceExpression(seq) => {
            seq.expressions.iter().filter_map(iife_module).collect()
        }
        e => iife_module(e).into_iter().collect(),
    }
}

/// `(()=>{…})()`, `(function(){…})()`, `!function(){…}()`.
fn iife_module<'a>(e: &'a Expression<'a>) -> Option<InlineModule<'a>> {
    let call = match unparen(e) {
        Expression::CallExpression(c) => c,
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::LogicalNot => {
            match unparen(&u.argument) {
                Expression::CallExpression(c) => c,
                _ => return None,
            }
        }
        _ => return None,
    };
    let callee = unparen(&call.callee);
    Some((callee.span(), function_body(callee)?))
}

/// The statements of a function or block-bodied arrow.
fn function_body<'a>(e: &'a Expression<'a>) -> Option<&'a [Statement<'a>]> {
    match unparen(e) {
        Expression::FunctionExpression(f) => Some(f.body.as_ref()?.statements.as_slice()),
        Expression::ArrowFunctionExpression(a) => arrow_body(a),
        _ => None,
    }
}

fn arrow_body<'a>(a: &'a ArrowFunctionExpression<'a>) -> Option<&'a [Statement<'a>]> {
    match &a.body {
        oxc_ast::ast::ArrowFunctionBody::FunctionBody(b) => Some(b.statements.as_slice()),
        _ => None,
    }
}

/// The module-id literals of a webpack bootstrap's container: the first
/// argument of every call of a REQUIRE binding with a number — the
/// bootstrap's require function (a top-level function that hands itself to
/// a module call, `M[id](m, m.exports, r)`) and each table module's third
/// parameter. webpack's ids hash the module's path into a range sized by
/// the build's module count, so the same module has another id in an app
/// than in an index build of its package (exp105): the unit prints read
/// these as one number (`libindex::units::unit_prints`).
pub fn webpack_module_id_literals(
    statements: &[Statement<'_>],
    semantic: &oxc_semantic::Semantic<'_>,
) -> std::collections::HashSet<u32> {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    // a reference's call: (the call, the reference's argument position)
    let call_of = |rid: oxc_syntax::reference::ReferenceId| {
        let node = nodes.get_node(scoping.get_reference(rid).node_id());
        match nodes.parent_kind(node.id()) {
            AstKind::CallExpression(c) => Some((c, node.span())),
            _ => None,
        }
    };
    let mut requires = Vec::new();
    for s in statements {
        match s {
            Statement::FunctionDeclaration(f) if f.params.items.len() == 1 => {
                let Some(symbol) = f.id.as_ref().and_then(|id| id.symbol_id.get()) else {
                    continue;
                };
                let hands_itself = scoping.get_resolved_reference_ids(symbol).iter().any(|&r| {
                    call_of(r).is_some_and(|(c, span)| {
                        c.arguments.len() == 3 && c.arguments[2].span() == span
                    })
                });
                if hands_itself {
                    requires.push(symbol);
                }
            }
            Statement::VariableDeclaration(v) => {
                for d in &v.declarations {
                    let Some(Expression::ObjectExpression(o)) = d.init.as_ref().map(unparen) else {
                        continue;
                    };
                    if module_table(&o.properties).is_empty() {
                        continue;
                    }
                    for p in &o.properties {
                        let ObjectPropertyKind::ObjectProperty(p) = p else {
                            continue;
                        };
                        let params = match unparen(&p.value) {
                            Expression::FunctionExpression(f) => &f.params,
                            Expression::ArrowFunctionExpression(a) => &a.params,
                            _ => continue,
                        };
                        if let Some(oxc_ast::ast::BindingPattern::BindingIdentifier(id)) =
                            params.items.get(2).map(|p| &p.pattern)
                            && let Some(symbol) = id.symbol_id.get()
                        {
                            requires.push(symbol);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = std::collections::HashSet::new();
    for symbol in requires {
        for &r in scoping.get_resolved_reference_ids(symbol) {
            if let Some((c, span)) = call_of(r)
                && unparen(&c.callee).span() == span
                && let Some(Argument::NumericLiteral(n)) = c.arguments.first()
            {
                out.insert(n.span.start);
            }
        }
    }
    out
}

// ---- rollup -------------------------------------------------------------------

/// A guarded require function's body: no parameters, and it opens with
/// `if (h) return m;` then `h = 1` (alone or first in a sequence).
fn guarded_body<'a>(f: &'a Function<'a>) -> Option<&'a [Statement<'a>]> {
    if !f.params.items.is_empty() || f.params.rest.is_some() {
        return None;
    }
    let body = f.body.as_ref()?.statements.as_slice();
    let [
        Statement::IfStatement(guard),
        Statement::ExpressionStatement(set),
        ..,
    ] = body
    else {
        return None;
    };
    let Expression::Identifier(flag) = unparen(&guard.test) else {
        return None;
    };
    let returns = match &guard.consequent {
        Statement::ReturnStatement(r) => r.argument.as_ref(),
        _ => None,
    };
    if guard.alternate.is_some() || !matches!(returns.map(unparen), Some(Expression::Identifier(_)))
    {
        return None;
    }
    let first = match unparen(&set.expression) {
        Expression::SequenceExpression(seq) => seq.expressions.first()?,
        e => e,
    };
    let Expression::AssignmentExpression(a) = unparen(first) else {
        return None;
    };
    let sets_flag = a.operator == AssignmentOperator::Assign
        && a.left.get_identifier_name() == Some(flag.name.as_str())
        && matches!(unparen(&a.right), Expression::NumericLiteral(n) if n.value == 1.0);
    sets_flag.then_some(body)
}

/// rollup's inline module in one statement: a guarded require function
/// declaration.
pub fn rollup_statement_modules<'a>(stmt: &'a Statement<'a>) -> Vec<InlineModule<'a>> {
    match stmt {
        Statement::FunctionDeclaration(f) => guarded_body(f)
            .map(|body| (f.span, body))
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

/// rollup's inline module in one declarator: a guarded require function
/// called at once (`a = function(){…}()`), also as a call's one argument
/// (`c = e(function(){…}())`).
pub fn rollup_declarator_modules<'a>(d: &'a VariableDeclarator<'a>) -> Vec<InlineModule<'a>> {
    let Some(init) = d.init.as_ref() else {
        return Vec::new();
    };
    guarded_iife(init, 2).into_iter().collect()
}

fn guarded_iife<'a>(e: &'a Expression<'a>, depth: u32) -> Option<InlineModule<'a>> {
    let Expression::CallExpression(call) = unparen(e) else {
        return None;
    };
    if let Expression::FunctionExpression(f) = unparen(&call.callee)
        && call.arguments.is_empty()
    {
        return Some((f.span, guarded_body(f)?));
    }
    match call.arguments.as_slice() {
        [arg] if depth > 1 => guarded_iife(arg.as_expression()?, depth - 1),
        _ => None,
    }
}

#[cfg(test)]
mod inline_modules_test;
