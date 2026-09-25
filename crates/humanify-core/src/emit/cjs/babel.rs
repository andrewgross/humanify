//! The Babel `NodePath` questions cjs-emit.ts asks, answered over oxc's
//! node table. Babel has no `ParenthesizedExpression` (a path's parent is
//! the first non-paren ancestor) and models an object/class METHOD as one
//! function node whose scope includes its key; oxc hangs a `Function` off
//! a `MethodDefinition` / method `ObjectProperty` instead. Each predicate
//! below names the TS function it answers for.

use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, CallExpression, Expression, PropertyKind, UnaryOperator};
use oxc_semantic::{AstNodes, NodeId};
use oxc_span::{GetSpan, Span};

use crate::babel_view::unparen;

pub(crate) fn parent(nodes: &AstNodes<'_>, id: NodeId) -> Option<NodeId> {
    let p = nodes.parent_id(id);
    (p != id).then_some(p)
}

/// `path.parentPath` in Babel: the first non-paren ancestor.
pub(crate) fn babel_parent(nodes: &AstNodes<'_>, id: NodeId) -> Option<NodeId> {
    let mut p = parent(nodes, id)?;
    while matches!(nodes.kind(p), AstKind::ParenthesizedExpression(_)) {
        p = parent(nodes, p)?;
    }
    Some(p)
}

/// A link on an optional chain's spine (never through parens — a paren
/// ends a chain).
fn spine_optional(e: &Expression<'_>) -> bool {
    match e {
        Expression::StaticMemberExpression(m) => m.optional || spine_optional(&m.object),
        Expression::ComputedMemberExpression(m) => m.optional || spine_optional(&m.object),
        Expression::PrivateFieldExpression(m) => m.optional || spine_optional(&m.object),
        Expression::CallExpression(c) => c.optional || spine_optional(&c.callee),
        _ => false,
    }
}

/// Babel types this call `OptionalCallExpression` (lesson 14).
pub(crate) fn is_babel_optional_call(c: &CallExpression<'_>) -> bool {
    c.optional || spine_optional(&c.callee)
}

/// `isBareCalleePos(parent, node)`: the callee of a (possibly optional)
/// call, or a tagged template's tag.
pub(crate) fn is_bare_callee(nodes: &AstNodes<'_>, id: NodeId, span: Span) -> bool {
    match babel_parent(nodes, id).map(|p| nodes.kind(p)) {
        Some(AstKind::CallExpression(c)) => unparen(&c.callee).span() == span,
        Some(AstKind::TaggedTemplateExpression(t)) => unparen(&t.tag).span() == span,
        _ => false,
    }
}

/// `t.isObjectProperty(parent) && parent.shorthand && parent.value ===
/// node`: the value of a shorthand property (a literal's `{x}`, an
/// assignment pattern's `{x}` without default, a declaration's `{x}`).
pub(crate) fn is_shorthand_value(nodes: &AstNodes<'_>, id: NodeId, span: Span) -> bool {
    match parent(nodes, id).map(|p| nodes.kind(p)) {
        Some(AstKind::ObjectProperty(p)) => p.shorthand && p.value.span() == span,
        Some(AstKind::AssignmentTargetPropertyIdentifier(p)) => p.init.is_none(),
        Some(AstKind::BindingProperty(p)) => {
            p.shorthand && matches!(p.value, BindingPattern::BindingIdentifier(_))
        }
        _ => false,
    }
}

/// `delete x` on this identifier: the UnaryExpression's span.
pub(crate) fn delete_of(nodes: &AstNodes<'_>, id: NodeId, span: Span) -> Option<Span> {
    match babel_parent(nodes, id).map(|p| nodes.kind(p)) {
        Some(AstKind::UnaryExpression(u))
            if u.operator == UnaryOperator::Delete && unparen(&u.argument).span() == span =>
        {
            Some(u.span)
        }
        _ => None,
    }
}

/// `t.isUnaryExpression(parent) && parent.operator === "delete"` (the
/// argument check left out, as the TS's write-target test does).
pub(crate) fn parent_is_delete(nodes: &AstNodes<'_>, id: NodeId) -> bool {
    matches!(
        babel_parent(nodes, id).map(|p| nodes.kind(p)),
        Some(AstKind::UnaryExpression(u)) if u.operator == UnaryOperator::Delete
    )
}

/// A Babel ObjectMethod / ClassMethod node (oxc: the method's container).
fn is_method_container(kind: &AstKind<'_>) -> bool {
    match kind {
        AstKind::MethodDefinition(_) => true,
        AstKind::ObjectProperty(p) => p.method || p.kind != PropertyKind::Init,
        _ => false,
    }
}

/// `isIifeCallee(f)`: `f.parentPath.isCallExpression({callee: f.node})`.
fn is_iife_callee(nodes: &AstNodes<'_>, f: NodeId) -> bool {
    let span = nodes.get_node(f).span();
    match babel_parent(nodes, f).map(|p| nodes.kind(p)) {
        Some(AstKind::CallExpression(c)) => {
            !is_babel_optional_call(c) && unparen(&c.callee).span() == span
        }
        _ => false,
    }
}

/// `isLoadTimeSite(site, wrapperNode)`: the site executes while its module
/// LOADS (top level, or a top-level IIFE body).
pub(crate) fn is_load_time_site(nodes: &AstNodes<'_>, id: NodeId, wrapper: NodeId) -> bool {
    let mut cur = parent(nodes, id);
    while let Some(p) = cur {
        if p == wrapper {
            return true;
        }
        let kind = nodes.kind(p);
        match kind {
            AstKind::PropertyDefinition(d) if !d.r#static => return false,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                if !is_iife_callee(nodes, p) =>
            {
                return false;
            }
            _ if is_method_container(&kind) => return false,
            _ => {}
        }
        cur = parent(nodes, p);
    }
    false
}

/// `thisBelongsToWrapper(p, wrapperNode)`: the first `this`-rebinding
/// boundary above decides (non-arrow functions, methods, class fields,
/// static blocks rebind it).
pub(crate) fn this_belongs_to_wrapper(nodes: &AstNodes<'_>, id: NodeId, wrapper: NodeId) -> bool {
    let mut cur = parent(nodes, id);
    while let Some(p) = cur {
        if p == wrapper {
            return true;
        }
        let kind = nodes.kind(p);
        if matches!(
            kind,
            AstKind::Function(_) | AstKind::PropertyDefinition(_) | AstKind::StaticBlock(_)
        ) || is_method_container(&kind)
        {
            return false;
        }
        cur = parent(nodes, p);
    }
    false
}

/// Babel's `Statement` alias (a function body is a BlockStatement there).
fn is_babel_statement(kind: &AstKind<'_>) -> bool {
    match kind {
        AstKind::Function(f) => f.is_declaration(),
        AstKind::Class(c) => c.is_declaration(),
        AstKind::BlockStatement(_)
        | AstKind::FunctionBody(_)
        | AstKind::BreakStatement(_)
        | AstKind::ContinueStatement(_)
        | AstKind::DebuggerStatement(_)
        | AstKind::DoWhileStatement(_)
        | AstKind::EmptyStatement(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::ForInStatement(_)
        | AstKind::ForOfStatement(_)
        | AstKind::ForStatement(_)
        | AstKind::IfStatement(_)
        | AstKind::LabeledStatement(_)
        | AstKind::ReturnStatement(_)
        | AstKind::SwitchStatement(_)
        | AstKind::ThrowStatement(_)
        | AstKind::TryStatement(_)
        | AstKind::WhileStatement(_)
        | AstKind::WithStatement(_)
        | AstKind::VariableDeclaration(_) => true,
        _ => false,
    }
}

/// `classifyWrite`'s answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WriteKind {
    Write,
    VarRedecl,
    FnRedecl,
    PatternRedecl,
}

/// `classifyWrite(idPath)`.
pub(crate) fn classify_write(nodes: &AstNodes<'_>, id: NodeId, span: Span) -> WriteKind {
    let Some(p) = parent(nodes, id) else {
        return WriteKind::Write;
    };
    match nodes.kind(p) {
        AstKind::VariableDeclarator(d) if matches!(&d.id, BindingPattern::BindingIdentifier(b) if b.span == span) =>
        {
            return WriteKind::VarRedecl;
        }
        AstKind::Function(f)
            if f.is_declaration() && f.id.as_ref().is_some_and(|i| i.span == span) =>
        {
            return WriteKind::FnRedecl;
        }
        _ => {}
    }
    // `insideDeclaratorPattern`: below the nearest statement, a declarator
    // whose id contains the identifier.
    let mut cur = Some(p);
    while let Some(n) = cur {
        let kind = nodes.kind(n);
        if is_babel_statement(&kind) {
            break;
        }
        if let AstKind::VariableDeclarator(d) = kind {
            let id_span = d.id.span();
            if span.start >= id_span.start && span.end <= id_span.end {
                return WriteKind::PatternRedecl;
            }
            return WriteKind::Write;
        }
        cur = parent(nodes, n);
    }
    WriteKind::Write
}

/// `idPath.parentPath?.isVariableDeclarator()`.
pub(crate) fn parent_is_declarator(nodes: &AstNodes<'_>, id: NodeId) -> bool {
    matches!(
        parent(nodes, id).map(|p| nodes.kind(p)),
        Some(AstKind::VariableDeclarator(_))
    )
}
