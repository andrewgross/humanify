//! The bindings a function OWNS (WP3.2) — TS original:
//! `src/rename/function-bindings.ts`, whole: the single traversal behind
//! both entry points plus the shadowed-block second pass.
//!
//! - [`collect_owned_binding_infos`] — the LLM naming path (WP4.3's
//!   consumer). Excludes nested function DECLARATION names: each function
//!   names itself in its own pass.
//! - [`build_owned_binding_map`] — the prior-version transfer path.
//!   Includes them: exact-match pairs legitimately carry them.
//! - [`collect_shadowed_block_bindings`] — after the main pass, the
//!   block-scoped bindings skipped because they shared a name with a
//!   function-scope binding at collection time.
//!
//! Collection order, first name wins:
//! 1. the function scope's own bindings, in the scope map's CURRENT order;
//! 2. the body scope when it differs — never, in Babel 7: a function's body
//!    block is not a scope (`isScope` refuses a BlockStatement whose parent
//!    is a Function), so the Rust view has no such scope and the step is
//!    structurally empty;
//! 3. nested block scopes (Block/For/ForIn/ForOf/Switch/Catch) of THIS
//!    function in traversal (pre-)order — nested functions are skipped,
//!    static blocks and classes are not;
//! 4. the function's own name binding (declarations look it up from the
//!    parent scope, named expressions from their own scope), by its
//!    CURRENT name.
//!
//! Names are read at call time; the transfer tiers build the map lazily
//! once per transfer call and never refresh it (the TS's `ownedMap ??=`).

use std::collections::HashSet;

use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BScopeId, BindingId, ScopeType};

use super::rows::{FnKind, FnRow, Rows};

/// TS `BindingInfo`: a collected binding with the scope that owns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingInfo {
    pub name: String,
    pub binding: BindingId,
    pub scope: BScopeId,
}

/// TS `collectOwnedBindingInfos`: the LLM path's owned bindings.
pub fn collect_owned_binding_infos(state: &RenameState, f: &FnRow) -> Vec<BindingInfo> {
    collect_owned_bindings(state, f, false)
}

/// TS `buildOwnedBindingMap`: owned name → the scope that owns it, in
/// insertion order (first name wins).
pub fn build_owned_binding_map(state: &RenameState, f: &FnRow) -> Vec<(String, BScopeId)> {
    collect_owned_bindings(state, f, true)
        .into_iter()
        .map(|i| (i.name, i.scope))
        .collect()
}

fn collect_owned_bindings(
    state: &RenameState,
    f: &FnRow,
    include_nested_fn_decl_names: bool,
) -> Vec<BindingInfo> {
    let view = state.view();
    let include = |b: BindingId| {
        include_nested_fn_decl_names || !Rows::binding_is_function_declaration(view, b)
    };
    let mut out: Vec<BindingInfo> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut push = |out: &mut Vec<BindingInfo>, name: String, binding: BindingId, scope| {
        if seen.insert(name.clone()) {
            out.push(BindingInfo {
                name,
                binding,
                scope,
            });
        }
    };
    // 1. the function scope's own bindings.
    for (name, b) in state.bindings_in(f.scope) {
        if view.binding(b).owner == f.scope && include(b) {
            push(&mut out, name, b, f.scope);
        }
    }
    // 3. nested block scopes of this function, pre-order.
    for sid in own_scopes(state, f) {
        if !matches!(
            view.scope(sid).ty,
            ScopeType::BlockStatement
                | ScopeType::ForStatement
                | ScopeType::ForInStatement
                | ScopeType::ForOfStatement
                | ScopeType::SwitchStatement
                | ScopeType::CatchClause
        ) {
            continue;
        }
        for (name, b) in state.bindings_in(sid) {
            if view.binding(b).owner == sid && include(b) {
                push(&mut out, name, b, sid);
            }
        }
    }
    // 4. the function's own name binding.
    if matches!(f.kind, FnKind::Declaration | FnKind::Expression)
        && let Some(symbol) = f.id_symbol
        && let Some(id_binding) = view.binding_of_symbol(symbol)
    {
        let id_name = state.name_of(id_binding).to_string();
        let lookup_scope = if f.kind == FnKind::Declaration {
            view.scope(f.scope).parent
        } else {
            Some(f.scope)
        };
        if let Some(from) = lookup_scope
            && let Some(name_binding) = state.get_binding(from, &id_name)
        {
            let owner = view.binding(name_binding).owner;
            push(&mut out, id_name, name_binding, owner);
        }
    }
    out
}

/// TS `collectShadowedBlockBindings`: every scope inside the function
/// (nested functions skipped, the function's own scope excluded) and its
/// OWN bindings whose current name is still eligible — no name dedup
/// (sibling blocks reusing a name each report).
pub fn collect_shadowed_block_bindings(
    state: &RenameState,
    f: &FnRow,
    is_eligible: impl Fn(&str) -> bool,
) -> Vec<BindingInfo> {
    let view = state.view();
    let mut out = Vec::new();
    for sid in own_scopes(state, f) {
        if view.scope(sid).ty.is_function() {
            continue;
        }
        for (name, b) in state.bindings_in(sid) {
            if view.binding(b).owner == sid && is_eligible(&name) {
                out.push(BindingInfo {
                    name,
                    binding: b,
                    scope: sid,
                });
            }
        }
    }
    out
}

/// The scopes strictly inside the function whose nearest enclosing
/// FUNCTION scope is the function's own — the traversal's reach with
/// `Function(path) { path.skip() }` (StaticBlocks are traversed, so they do
/// not stop the climb) — in pre-order.
fn own_scopes(state: &RenameState, f: &FnRow) -> Vec<BScopeId> {
    let view = state.view();
    let mut out = Vec::new();
    for (i, s) in view.scopes.iter().enumerate() {
        let sid = BScopeId(i as u32);
        if sid == f.scope || s.span.start < f.span.start || s.span.end > f.span.end {
            continue;
        }
        if nearest_function_scope(state, s.parent) == Some(f.scope) {
            out.push(sid);
        }
    }
    out
}

/// The nearest ancestor-or-self scope whose block is a FUNCTION.
fn nearest_function_scope(state: &RenameState, from: Option<BScopeId>) -> Option<BScopeId> {
    let view = state.view();
    let mut cur = from;
    while let Some(id) = cur {
        if view.scope(id).ty.is_function() {
            return Some(id);
        }
        cur = view.scope(id).parent;
    }
    None
}

#[cfg(test)]
mod owned_test;
