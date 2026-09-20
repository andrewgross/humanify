//! The member-key extraction (WP2.1) — TS original:
//! `extractMemberKey` (src/analysis/function-fingerprint.ts :93) with
//! `keyName` :79, `memberKeyThroughVariable` :141 and
//! `variableHoldingFunction` :167.
//!
//! The TS compares PARENT IDENTITY (`parent.value === node`,
//! `parent.init === node`, `parent.right === node`) over babel paths, whose
//! parser never emits ParenthesizedExpression. oxc keeps the parens, so
//! every comparison unwraps them (`babel_view::unparen`) and every parent
//! lookup is the first non-paren ancestor — the babel-equivalent parent.
//! Binding identity is resolved through the symbol tables (the span-keyed
//! owner, 07 §1) in place of the TS's `scope.getBinding(name)`: the holder
//! position's own declaration or reference names the same binding the
//! function's scope chain resolves to, by construction of the two holder
//! forms (the declarator declares it; the assignment target is a resolved
//! reference to it).

use oxc_ast::AstKind;
use oxc_ast::ast::{AssignmentTarget, PropertyKey, PropertyKind};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use crate::hash::serialize::SymbolTables;

/// TS `extractMemberKey` (:93): the property key a function row is assigned
/// to. `row_node_id`/`row_kind`/`row_span` describe the row's own node.
pub(crate) fn extract_member_key(
    row_node_id: NodeId,
    row_kind: AstKind<'_>,
    row_span: Span,
    semantic: &Semantic<'_>,
    tables: &SymbolTables,
) -> Option<String> {
    let nodes = semantic.nodes();
    // babel's `fn.path.parent` — the first non-paren ancestor.
    let parent_id = parent_skipping_parens(nodes, row_node_id);
    let parent_kind = nodes.get_node(parent_id).kind();

    // (a) ObjectProperty: { getCount: function(){...} } (:97-100)
    if let AstKind::ObjectProperty(prop) = parent_kind
        && crate::babel_view::unparen(&prop.value).span() == row_span
    {
        return key_name(&prop.key, prop.computed);
    }

    // (b) the function IS the method (:102-109) — babel's ObjectMethod /
    // ClassMethod checks. ClassProperty (:105) can never be a function row
    // (a class field's arrow is the field's VALUE, a different node), so
    // that arm is dead there and omitted here.
    match row_kind {
        AstKind::MethodDefinition(method) => return key_name(&method.key, method.computed),
        AstKind::ObjectProperty(prop) if prop.method || prop.kind != PropertyKind::Init => {
            return key_name(&prop.key, prop.computed);
        }
        _ => {}
    }

    // (c) AssignmentExpression with a member LHS: obj.foo = function(){...}
    // (:111-121) — non-computed, identifier property (oxc's
    // StaticMemberExpression IS that shape; the computed form is
    // ComputedMemberExpression).
    if let AstKind::AssignmentExpression(assignment) = parent_kind
        && crate::babel_view::unparen(&assignment.right).span() == row_span
        && let AssignmentTarget::StaticMemberExpression(member) = &assignment.left
    {
        return Some(member.property.name.to_string());
    }

    // (d) one hop of indirection (:123 → :141)
    member_key_through_variable(row_span, parent_kind, semantic, tables)
}

/// TS `keyName` (:79): a non-computed identifier key or a string key.
fn key_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) if !computed => Some(id.name.to_string()),
        PropertyKey::StringLiteral(s) => Some(s.value.to_string()),
        // A computed identifier key reads a binding's CURRENT name — the TS
        // returns undefined for it (isIdentifier is true but the
        // !computed guard fails), so a computed key never contributes.
        PropertyKey::StaticIdentifier(_) => None,
        _ => None,
    }
}

/// TS `memberKeyThroughVariable` (:141): the property key a function
/// reaches through ONE hop of indirection —
/// `const getState = () => state; … { getState: getState }`.
///
/// zustand's known shortfall is exactly this shape (the TS doc :127-139):
/// the evidence that separates `getState` from `getInitialState` is the
/// property key, and it is read only when the key is ONE reference away.
/// Requires a UNIQUE key across all references — a variable used under two
/// keys is a contradiction, not weaker evidence, because the cascade trusts
/// a member key above shape.
fn member_key_through_variable(
    row_span: Span,
    parent_kind: AstKind<'_>,
    semantic: &Semantic<'_>,
    tables: &SymbolTables,
) -> Option<String> {
    let held = variable_holding_function(row_span, parent_kind)?;
    // TS `fn.path.scope.getBinding(held)` (:144) — the holder position's
    // own binding: the declarator's declaration, or the assignment target's
    // resolved reference (the `var h; h = () => 1` form declares nothing
    // at the target).
    let symbol = tables
        .decl_by_start
        .get(&held.0.start)
        .or_else(|| tables.ref_by_start.get(&held.0.start))
        .copied()?;
    let nodes = semantic.nodes();
    let mut keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for node_id in super::babel_reference_node_ids(semantic, symbol) {
        // `holder.value === ref.node` (:149) — the reference must BE the
        // property's value (unwrapped through the parens oxc keeps).
        let ref_parent = parent_skipping_parens(nodes, node_id);
        if let AstKind::ObjectProperty(prop) = nodes.get_node(ref_parent).kind()
            && crate::babel_view::unparen(&prop.value).span() == nodes.get_node(node_id).span()
            && let Some(key) = key_name(&prop.key, prop.computed)
        {
            keys.insert(key);
        }
    }
    if keys.len() == 1 {
        keys.into_iter().next()
    } else {
        None
    }
}

/// TS `variableHoldingFunction` (:167): the variable a function is held
/// in — DECLARED with it or ASSIGNED to it, `const f = () => 1` and
/// `let f; f = () => 1` alike (the assignment form is the shape the
/// bundle's own lazy-init emits, TS doc :157-166). Answers the holder's
/// identifier (span + name): the span resolves the binding through the
/// symbol tables, the name is what the TS passes to getBinding.
fn variable_holding_function(row_span: Span, parent_kind: AstKind<'_>) -> Option<(Span, String)> {
    match parent_kind {
        AstKind::VariableDeclarator(declarator) => {
            let init = crate::babel_view::unparen(declarator.init.as_ref()?);
            if init.span() != row_span {
                return None;
            }
            match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => {
                    Some((id.span, id.name.to_string()))
                }
                _ => None,
            }
        }
        AstKind::AssignmentExpression(assignment) => {
            let right = crate::babel_view::unparen(&assignment.right);
            if right.span() != row_span {
                return None;
            }
            match &assignment.left {
                AssignmentTarget::AssignmentTargetIdentifier(id) => {
                    Some((id.span, id.name.to_string()))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The babel-equivalent parent: the first non-paren ancestor (oxc keeps
/// ParenthesizedExpression where babel's parser drops them).
pub(crate) fn parent_skipping_parens(
    nodes: &oxc_semantic::AstNodes<'_>,
    node_id: NodeId,
) -> NodeId {
    let mut prev = node_id;
    let mut parent = nodes.parent_id(prev);
    while parent != prev
        && matches!(
            nodes.get_node(parent).kind(),
            AstKind::ParenthesizedExpression(_)
        )
    {
        prev = parent;
        parent = nodes.parent_id(prev);
    }
    parent
}
