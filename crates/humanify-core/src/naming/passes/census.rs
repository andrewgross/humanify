//! The minted-token census (exp021's meter) — TS: the binding walk and the
//! summary of `src/rename/minted-census.ts` (the name-shape predicates live
//! in `rename::floor`, their one owner).
//!
//! The walk reads Babel's scope tables through the rename state: scopes in
//! traversal (pre-)order, each scope's bindings in `Object.entries` order
//! under their CURRENT names (a renamed binding sits at the end of its
//! map), each binding once. The floor passes consume the same list, so the
//! order is a decision input (which candidate is attempted first).

use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::{AssignmentTarget, BindingPattern, PropertyKey};
use oxc_semantic::{NodeId, Semantic};

use humanify_model::js::cmp_utf16;

use crate::modules::known_globals::is_known_global;
use crate::naming::waves::render::Occurrences;
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::{is_bun_token, is_decorated_descriptive, is_wordless_mint_shape};
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BindingId, BindingKind};

/// `MintedFamily`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MintedFamily {
    ClassExprId,
    FnExprId,
    Param,
    FnDecl,
    VarOther,
}

impl MintedFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            MintedFamily::ClassExprId => "classExprId",
            MintedFamily::FnExprId => "fnExprId",
            MintedFamily::Param => "param",
            MintedFamily::FnDecl => "fnDecl",
            MintedFamily::VarOther => "varOther",
        }
    }
}

/// `MintedBinding`.
#[derive(Clone, Debug)]
pub struct MintedBinding {
    pub name: String,
    pub family: MintedFamily,
    /// Class/function-expression ids: the name derivation would use.
    pub derived_from: Option<String>,
    pub ref_count: usize,
    pub binding: BindingId,
}

/// `collectMintedBindings`' result.
pub struct MintedWalk {
    pub entries: Vec<MintedBinding>,
    pub total_bindings: usize,
}

/// The parent of `node`, skipping oxc's parenthesized expressions (Babel
/// has no paren nodes).
pub fn babel_parent(semantic: &Semantic<'_>, node: NodeId) -> Option<NodeId> {
    let nodes = semantic.nodes();
    let mut cur = node;
    loop {
        let p = nodes.parent_id(cur);
        if p == cur {
            return None;
        }
        if !matches!(nodes.kind(p), AstKind::ParenthesizedExpression(_)) {
            return Some(p);
        }
        cur = p;
    }
}

fn classify(semantic: &Semantic<'_>, state: &RenameState, binding: BindingId) -> MintedFamily {
    let b = state.view().binding(binding);
    match semantic.nodes().kind(b.path_node) {
        AstKind::Class(c) if c.is_expression() => return MintedFamily::ClassExprId,
        AstKind::Function(f) if f.is_expression() => return MintedFamily::FnExprId,
        _ => {}
    }
    if b.kind == BindingKind::Param {
        return MintedFamily::Param;
    }
    match semantic.nodes().kind(b.path_node) {
        AstKind::Function(f) if f.is_declaration() => MintedFamily::FnDecl,
        AstKind::Class(c) if c.is_declaration() => MintedFamily::FnDecl,
        _ => MintedFamily::VarOther,
    }
}

/// `derivationSource(exprPath)`: the name a class/function expression's
/// inner id would take — assignment target, declarator id, or object
/// property key (current names) — unless that name is itself minted.
pub fn derivation_source(
    semantic: &Semantic<'_>,
    state: &RenameState,
    occ: &Occurrences,
    expr: NodeId,
) -> Option<String> {
    let nodes = semantic.nodes();
    let expr_span = oxc_span::GetSpan::span(&nodes.kind(expr));
    let parent = babel_parent(semantic, expr)?;
    let current =
        |start: u32, original: &str| occ.current_at(state, start).unwrap_or(original).to_string();
    let is_expr = |e: &oxc_ast::ast::Expression<'_>| {
        oxc_span::GetSpan::span(crate::babel_view::unparen(e)) == expr_span
    };
    let candidate = match nodes.kind(parent) {
        AstKind::AssignmentExpression(a) if is_expr(&a.right) => match &a.left {
            AssignmentTarget::AssignmentTargetIdentifier(id) => {
                Some(current(id.span.start, &id.name))
            }
            AssignmentTarget::StaticMemberExpression(m) => Some(m.property.name.to_string()),
            _ => None,
        },
        AstKind::VariableDeclarator(d) if d.init.as_ref().is_some_and(is_expr) => match &d.id {
            BindingPattern::BindingIdentifier(id) => Some(current(id.span.start, &id.name)),
            _ => None,
        },
        AstKind::ObjectProperty(p) if !p.method && !p.computed && is_expr(&p.value) => {
            match &p.key {
                PropertyKey::StaticIdentifier(k) => Some(k.name.to_string()),
                _ => None,
            }
        }
        _ => None,
    };
    candidate.filter(|c| !is_bun_token(c))
}

/// `collectMintedBindings`: every eligible minted binding, walking each
/// scope once in traversal order.
pub fn collect_minted_bindings(
    semantic: &Semantic<'_>,
    state: &RenameState,
    eligible: &Eligibility,
) -> MintedWalk {
    let view = state.view();
    let occ = Occurrences::build(semantic, state);
    let mut seen: HashSet<BindingId> = HashSet::new();
    let mut entries = Vec::new();
    for i in 0..view.scopes.len() {
        let scope = crate::rename::validated::scopes::BScopeId(i as u32);
        if view.scope(scope).ty.is_pattern() {
            continue;
        }
        for (name, binding) in state.bindings_in(scope) {
            if !seen.insert(binding) {
                continue;
            }
            if !eligible.is_eligible(&name) || !is_bun_token(&name) {
                continue;
            }
            let family = classify(semantic, state, binding);
            let is_expr_id = matches!(family, MintedFamily::ClassExprId | MintedFamily::FnExprId);
            let derived_from = if is_expr_id {
                derivation_source(semantic, state, &occ, view.binding(binding).path_node)
            } else {
                None
            };
            entries.push(MintedBinding {
                name,
                family,
                derived_from,
                ref_count: view.binding(binding).refs.len(),
                binding,
            });
        }
    }
    MintedWalk {
        entries,
        total_bindings: seen.len(),
    }
}

/// `collectFreeReferences`: minted-looking free names (never renamed —
/// reported so "unreachable" is never also invisible), sorted.
pub fn collect_free_references(state: &RenameState) -> Vec<String> {
    let mut found: Vec<String> = state
        .view()
        .globals
        .iter()
        .filter(|n| !is_known_global(n) && is_wordless_mint_shape(n))
        .cloned()
        .collect();
    // JS `.sort()`: UTF-16 code-unit order.
    found.sort_by(|a, b| cmp_utf16(a, b));
    found
}

/// `MintedCensus`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct MintedCensus {
    pub total: usize,
    pub decorated: usize,
    pub total_bindings: usize,
    pub by_family: [usize; 5],
    pub derivable_expr_ids: usize,
    pub zero_ref_expr_ids: usize,
    pub free_references: Vec<String>,
    pub names: Vec<String>,
    pub decorated_names: Vec<String>,
}

/// `summarizeCensus`.
pub fn summarize_census(
    bindings: &[MintedBinding],
    total_bindings: usize,
    free_references: Vec<String>,
) -> MintedCensus {
    let mut c = MintedCensus {
        total_bindings,
        free_references,
        ..MintedCensus::default()
    };
    for entry in bindings {
        if is_decorated_descriptive(&entry.name) {
            c.decorated += 1;
            c.decorated_names.push(entry.name.clone());
            continue;
        }
        c.names.push(entry.name.clone());
        c.by_family[entry.family as usize] += 1;
        if matches!(
            entry.family,
            MintedFamily::ClassExprId | MintedFamily::FnExprId
        ) {
            if entry.derived_from.is_some() {
                c.derivable_expr_ids += 1;
            }
            if entry.ref_count == 0 {
                c.zero_ref_expr_ids += 1;
            }
        }
    }
    c.total = bindings.len() - c.decorated;
    c
}
