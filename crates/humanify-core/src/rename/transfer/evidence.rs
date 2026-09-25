//! The RENAME half of `matchPriorVersion` (WP3.2) — TS original:
//! `src/prior-version/prior-version.ts` (applyExactMatches,
//! translatePriorNames, resolveNewPlaceholders, assertPlaceholderAlignment,
//! the close context's transfer fields, collectModuleScopeRefs,
//! collectFunctionVarNameTransfers / extractVarNameRename,
//! resolveBindingRenames / deriveBindingRenames, buildPriorBindingRoles /
//! buildPriorFunctionRoles). The match EVIDENCE half (cascades, close tier,
//! alternation) is `crate::prior::match_prior_version`; this module turns
//! its results into the inputs the transfer tiers apply — every list in
//! the TS iteration order the tiers consume it in.
//!
//! Placeholder slots are read through the babel-order token walk
//! (`statement_align::placeholder_table`, the TS `hashAndMapPath`): the
//! graph's own `placeholder_bindings` come from the canonical walk, whose
//! slot ORDER differs (alphabetical keys) — the pairing is the same, but
//! `translatePriorNames` iterates the prior table in first-occurrence
//! order and the exact-match tier applies the pairs in that order.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_semantic::{NodeId, Semantic, SymbolId};
use serde_json::Value;

use crate::graph::UnifiedGraph;
use crate::hash::serialize::SymbolTables;
use crate::matching::statement_align::{
    SideIndex, build_side_index, placeholder_table, row_json_in,
};
use crate::prior::MatchStage;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::twins::role::{BindingRole, RoleSide, compute_binding_role, compute_function_role};

use super::lifecycle::TransferPair;

/// One side's JSON + node lookups for its function rows.
pub struct SideRows<'a, 's> {
    pub graph: &'a UnifiedGraph,
    pub semantic: &'a Semantic<'s>,
    pub tables: &'a SymbolTables,
    index: SideIndex<'a>,
    row_nodes: HashMap<(u32, u32), (NodeId, AstKind<'a>)>,
    pub session_join: HashMap<(u32, u32), String>,
    fn_by_session: HashMap<String, usize>,
}

impl<'a, 's> SideRows<'a, 's> {
    pub fn build(
        graph: &'a UnifiedGraph,
        semantic: &'a Semantic<'s>,
        tables: &'a SymbolTables,
        program_json: &'a Value,
    ) -> SideRows<'a, 's> {
        SideRows {
            graph,
            semantic,
            tables,
            index: build_side_index(semantic, program_json),
            row_nodes: crate::matching::row_node_ids(&graph.functions, semantic.nodes()),
            session_join: crate::matching::alternation::session_join(graph),
            fn_by_session: graph
                .functions
                .iter()
                .enumerate()
                .map(|(i, f)| (f.session_id.clone(), i))
                .collect(),
        }
    }

    /// The function row's own ESTree node (`fn.path.node`).
    pub fn row_json(&self, fn_row: usize) -> Option<&'a Value> {
        let span = self.graph.functions[fn_row].span;
        let (_, kind) = self.row_nodes.get(&(span.start, span.end))?;
        row_json_in(&self.index, span, kind)
    }

    pub fn fn_of_session(&self, session_id: &str) -> Option<usize> {
        self.fn_by_session.get(session_id).copied()
    }

    /// The function row's babel node id.
    pub fn row_node(&self, fn_row: usize) -> Option<NodeId> {
        let span = self.graph.functions[fn_row].span;
        self.row_nodes
            .get(&(span.start, span.end))
            .map(|(id, _)| *id)
    }

    pub fn role_side(&self) -> RoleSide<'_, 's> {
        RoleSide {
            semantic: self.semantic,
            tables: self.tables,
            session_join: &self.session_join,
        }
    }

    /// TS `computeFunctionRole`.
    pub fn function_role(&self, fn_row: usize) -> Option<BindingRole> {
        let json = self.row_json(fn_row)?;
        Some(compute_function_role(
            &self.graph.functions[fn_row],
            json,
            &self.role_side(),
        ))
    }

    /// TS `computeBindingRole`.
    pub fn binding_role(&self, module_row: usize) -> BindingRole {
        compute_binding_role(&self.graph.module_bindings[module_row], &self.role_side())
    }
}

/// One exact match: (fresh fn row, translated pairs — `None` when every
/// slot is already named alike).
pub type ExactTransfer = (usize, Option<Vec<TransferPair>>);

/// One corroborated-or-not close pair, as the close tier and the
/// suggestion step read it (TS `CloseMatchInfo`'s transfer fields).
#[derive(Debug, Clone)]
pub struct CloseInfo {
    pub fresh_fn: usize,
    pub prior_id: String,
    pub corroborated: bool,
    pub name_transfers: Vec<TransferPair>,
    /// TS `priorExternals` / `newExternals`: the module-scope names the
    /// function references (insertion order).
    pub prior_externals: Vec<String>,
    pub new_externals: Vec<String>,
}

/// TS `ModuleBindingRename`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingRename {
    pub old_name: String,
    pub new_name: String,
    pub scope: BScopeId,
    /// The binding the match evidence was collected on (the validated
    /// owner's `stale-binding` guard).
    pub binding: Option<BindingId>,
}

/// Everything the transfer tiers read from the match stage.
pub struct TransferEvidence {
    /// applyExactMatches: (fresh fn row, translated pairs) per match, in
    /// match order. `None` = every slot already named alike.
    pub exact: Vec<ExactTransfer>,
    /// buildCloseMatchContext's pairs, assignment order.
    pub close: Vec<CloseInfo>,
    /// deriveBindingRenames (cascade order) then the function var-name
    /// transfers (exact-match order, then close pairs).
    pub binding_renames: Vec<BindingRename>,
    /// How many of `binding_renames` came from the binding cascade (the
    /// twins' claimed set is the cascade's old names only).
    pub cascade_renames: usize,
    pub prior_binding_roles: HashMap<String, BindingRole>,
    pub prior_function_roles: HashMap<String, BindingRole>,
    /// prior fn session → fresh fn session (the roles' callee veto).
    pub fn_matches: HashMap<String, String>,
    /// The close-matched fresh ids (excluded from function-head pins).
    pub close_matched_ids: HashSet<String>,
}

/// Build the transfer inputs. `fresh_state` is the fresh text's rename
/// state BEFORE any tier ran (names = crawl-time names), `prior_state` a
/// state over the prior text (only its scope lookups are read).
pub fn collect_evidence(
    stage: &MatchStage<'_, '_>,
    fresh: &SideRows<'_, '_>,
    prior: &SideRows<'_, '_>,
    fresh_state: &RenameState,
    prior_state: &RenameState,
) -> Result<TransferEvidence, String> {
    let function_result = stage.function_result;
    let exact = exact_transfers(stage, fresh, prior, fresh_state)?;
    let close = close_infos(stage, fresh, prior, fresh_state, prior_state);
    let (binding_renames, cascade_renames, matched_prior_binding_ids) =
        binding_renames(stage, fresh, prior, fresh_state, &close)?;
    let consumed: HashSet<&str> = binding_renames
        .iter()
        .map(|r| r.new_name.as_str())
        .collect();
    let prior_binding_roles = prior_binding_roles(prior, &matched_prior_binding_ids, &consumed);
    let excluded_prior_ids: HashSet<&str> = function_result
        .matches
        .keys()
        .map(String::as_str)
        .chain(close.iter().map(|c| c.prior_id.as_str()))
        .collect();
    let prior_function_roles = prior_function_roles(prior, &excluded_prior_ids, &consumed);
    let close_matched_ids = close
        .iter()
        .map(|c| fresh.graph.functions[c.fresh_fn].session_id.clone())
        .collect();
    Ok(TransferEvidence {
        exact,
        close,
        binding_renames,
        cascade_renames,
        prior_binding_roles,
        prior_function_roles,
        fn_matches: function_result.matches.to_hash_map(),
        close_matched_ids,
    })
}

/// TS `applyExactMatches`' translation half: `translatePriorNames` per
/// match, in match order.
fn exact_transfers(
    stage: &MatchStage<'_, '_>,
    fresh: &SideRows<'_, '_>,
    prior: &SideRows<'_, '_>,
    fresh_state: &RenameState,
) -> Result<Vec<ExactTransfer>, String> {
    let matches = &stage.function_result.matches;
    let mut exact = Vec::with_capacity(matches.len());
    for (prior_id, new_id) in matches.iter_ordered() {
        let (Some(prior_fn), Some(new_fn)) =
            (prior.fn_of_session(prior_id), fresh.fn_of_session(new_id))
        else {
            continue;
        };
        let translated = translate_prior_names(prior, prior_fn, fresh, new_fn, fresh_state)?;
        exact.push((new_fn, translated));
    }
    Ok(exact)
}

/// The close context's transfer fields, in assignment order.
fn close_infos(
    stage: &MatchStage<'_, '_>,
    fresh: &SideRows<'_, '_>,
    prior: &SideRows<'_, '_>,
    fresh_state: &RenameState,
    prior_state: &RenameState,
) -> Vec<CloseInfo> {
    let fresh_refs = RefSites::build(fresh_state);
    let prior_refs = RefSites::build(prior_state);
    let mut close = Vec::with_capacity(stage.close_pairs.len());
    for ctx in stage.close_pairs {
        let (Some(fresh_fn), Some(prior_fn)) = (
            fresh.fn_of_session(&ctx.fresh_id),
            prior.fn_of_session(&ctx.prior_id),
        ) else {
            continue;
        };
        let name_transfers = ctx
            .name_transfers
            .iter()
            .map(|t| TransferPair {
                old_name: t.old_name.clone(),
                new_name: t.new_name.clone(),
                binding: t
                    .binding
                    .and_then(|s| fresh_state.view().binding_of_symbol(s)),
            })
            .collect();
        close.push(CloseInfo {
            fresh_fn,
            prior_id: ctx.prior_id.clone(),
            corroborated: ctx.corroborated,
            name_transfers,
            prior_externals: module_scope_refs(prior_state, &prior_refs, prior, prior_fn),
            new_externals: module_scope_refs(fresh_state, &fresh_refs, fresh, fresh_fn),
        });
    }
    close
}

/// TS `resolveBindingRenames` (the cascade's matches, in match order) +
/// `collectFunctionVarNameTransfers` (exact matches, then corroborated
/// close pairs). Returns the list, the cascade's share, and the matched
/// prior binding ids.
fn binding_renames(
    stage: &MatchStage<'_, '_>,
    fresh: &SideRows<'_, '_>,
    prior: &SideRows<'_, '_>,
    fresh_state: &RenameState,
    close: &[CloseInfo],
) -> Result<(Vec<BindingRename>, usize, HashSet<String>), String> {
    let mut renames: Vec<BindingRename> = Vec::new();
    let mut matched_prior_ids: HashSet<String> = HashSet::new();
    if let (Some(result), Some(setup)) = (stage.binding_result, stage.binding_setup) {
        for (prior_id, new_id) in result.matches.iter_ordered() {
            matched_prior_ids.insert(prior_id.clone());
            let (Some(prior_b), Some(next_b)) =
                (setup.prior_by_id.get(prior_id), setup.new_by_id.get(new_id))
            else {
                continue;
            };
            let binding = fresh_state
                .view()
                .binding_of_symbol(next_b.symbol)
                .ok_or_else(|| format!("binding rename {}: no babel binding", next_b.name))?;
            let scope = fresh_state.scope_of_binding(binding);
            renames.push(BindingRename {
                old_name: next_b.name.clone(),
                new_name: prior_b.name.clone(),
                scope,
                binding: fresh_state.get_binding(scope, &next_b.name),
            });
        }
    }
    let cascade = renames.len();
    if !fresh.graph.module_bindings.is_empty() {
        for (prior_id, new_id) in stage.function_result.matches.iter_ordered() {
            if let (Some(p), Some(n)) = (prior.fn_of_session(prior_id), fresh.fn_of_session(new_id))
                && let Some(r) = extract_var_name_rename(prior, p, fresh, n, fresh_state)
            {
                renames.push(r);
            }
        }
        for info in close {
            if !info.corroborated || var_decl_name(fresh, info.fresh_fn).is_none() {
                continue;
            }
            if let Some(p) = prior.fn_of_session(&info.prior_id)
                && let Some(r) =
                    extract_var_name_rename(prior, p, fresh, info.fresh_fn, fresh_state)
            {
                renames.push(r);
            }
        }
    }
    Ok((renames, cascade, matched_prior_ids))
}

/// TS `buildPriorBindingRoles`: role evidence for prior module bindings
/// whose name has no destination yet.
fn prior_binding_roles(
    prior: &SideRows<'_, '_>,
    matched_prior_ids: &HashSet<String>,
    consumed: &HashSet<&str>,
) -> HashMap<String, BindingRole> {
    let mut roles = HashMap::new();
    for (i, b) in prior.graph.module_bindings.iter().enumerate() {
        if matched_prior_ids.contains(&b.session_id) || consumed.contains(b.name.as_str()) {
            continue;
        }
        roles.insert(b.name.clone(), prior.binding_role(i));
    }
    roles
}

/// TS `buildPriorFunctionRoles`: unmatched prior function-declaration
/// heads; a name declared twice is excluded outright.
fn prior_function_roles(
    prior: &SideRows<'_, '_>,
    excluded_prior_ids: &HashSet<&str>,
    consumed: &HashSet<&str>,
) -> HashMap<String, BindingRole> {
    let mut roles = HashMap::new();
    let mut declared: HashSet<String> = HashSet::new();
    for (i, f) in prior.graph.functions.iter().enumerate() {
        let Some(name) = function_declaration_name(prior, i) else {
            continue;
        };
        if !declared.insert(name.clone()) {
            roles.remove(&name);
            continue;
        }
        if excluded_prior_ids.contains(f.session_id.as_str()) || consumed.contains(name.as_str()) {
            continue;
        }
        if let Some(role) = prior.function_role(i) {
            roles.insert(name, role);
        }
    }
    roles
}

/// TS `translatePriorNames`: bridge the prior function's humanified names
/// onto the new function's minified ones through the shared placeholder
/// slots, keyed by SLOT (each pair carries the new side's resolved
/// binding — two bindings can share a minified name). Fails when the two
/// slot sets differ (`assertPlaceholderAlignment` — equal hashes guarantee
/// equal slot sets, so a mismatch is a stale or corrupt mapping).
fn translate_prior_names(
    prior: &SideRows<'_, '_>,
    prior_fn: usize,
    fresh: &SideRows<'_, '_>,
    new_fn: usize,
    fresh_state: &RenameState,
) -> Result<Option<Vec<TransferPair>>, String> {
    let (Some(prior_json), Some(new_json)) = (prior.row_json(prior_fn), fresh.row_json(new_fn))
    else {
        return Err(format!(
            "matched pair {} / {}: row JSON unavailable",
            prior.graph.functions[prior_fn].session_id, fresh.graph.functions[new_fn].session_id
        ));
    };
    let prior_table = placeholder_table(prior_json, prior.tables);
    let new_table = placeholder_table(new_json, fresh.tables);
    let new_by_slot: HashMap<&str, (Option<SymbolId>, &str)> = new_table
        .iter()
        .map(|(slot, sym, name)| (slot.as_str(), (*sym, name.as_str())))
        .collect();
    let aligned = prior_table.len() == new_table.len()
        && prior_table
            .iter()
            .all(|(slot, _, _)| new_by_slot.contains_key(slot.as_str()));
    if !aligned {
        return Err(format!(
            "placeholder maps misaligned for matched pair at {}: prior has {} slots, new has {} — \
             equal hashes guarantee equal slot sets, so a mapping is stale or corrupt",
            fresh.graph.functions[new_fn].session_id,
            prior_table.len(),
            new_table.len()
        ));
    }
    let mut pairs = Vec::new();
    for (slot, _, prior_name) in &prior_table {
        let Some(&(symbol, new_name)) = new_by_slot.get(slot.as_str()) else {
            continue;
        };
        if new_name != prior_name {
            pairs.push(TransferPair {
                old_name: new_name.to_string(),
                new_name: prior_name.clone(),
                binding: symbol.and_then(|s| fresh_state.view().binding_of_symbol(s)),
            });
        }
    }
    Ok((!pairs.is_empty()).then_some(pairs))
}

/// The function row's parent declarator (skipping parentheses — babel has
/// no ParenthesizedExpression node) when its id is a plain identifier:
/// (declarator node, id symbol, id name). TS `getVarDeclName`.
fn var_declarator_of(side: &SideRows<'_, '_>, fn_row: usize) -> Option<(NodeId, SymbolId, String)> {
    let nodes = side.semantic.nodes();
    let mut cur = side.row_node(fn_row)?;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return None;
        }
        match nodes.kind(parent) {
            AstKind::ParenthesizedExpression(_) => cur = parent,
            AstKind::VariableDeclarator(d) => {
                let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &d.id else {
                    return None;
                };
                // The function must be the INIT (not inside the id).
                return Some((parent, id.symbol_id.get()?, id.name.to_string()));
            }
            _ => return None,
        }
    }
}

fn var_decl_name(side: &SideRows<'_, '_>, fn_row: usize) -> Option<String> {
    var_declarator_of(side, fn_row).map(|(_, _, name)| name)
}

/// TS `extractVarNameRename`: both functions are declarator inits — the
/// holding var's name transfers (same-name pairs too, exp066). The scope is
/// the one that OWNS the binding (a block `var` hoists past the
/// declarator's own scope).
fn extract_var_name_rename(
    prior: &SideRows<'_, '_>,
    prior_fn: usize,
    fresh: &SideRows<'_, '_>,
    new_fn: usize,
    fresh_state: &RenameState,
) -> Option<BindingRename> {
    let prior_name = var_decl_name(prior, prior_fn)?;
    let (declarator, _, new_name) = var_declarator_of(fresh, new_fn)?;
    let from = fresh_state.view().scope_of_node(declarator);
    let binding = fresh_state.get_binding(from, &new_name)?;
    Some(BindingRename {
        old_name: new_name,
        new_name: prior_name,
        scope: fresh_state.scope_of_binding(binding),
        binding: Some(binding),
    })
}

/// A FunctionDeclaration row's id name (TS `t.isFunctionDeclaration(node)
/// && node.id`).
fn function_declaration_name(side: &SideRows<'_, '_>, fn_row: usize) -> Option<String> {
    let span = side.graph.functions[fn_row].span;
    match side.row_nodes.get(&(span.start, span.end))?.1 {
        AstKind::Function(f) if f.r#type == oxc_ast::ast::FunctionType::FunctionDeclaration => {
            f.id.as_ref().map(|id| id.name.to_string())
        }
        _ => None,
    }
}

/// Every Babel reference site of one text, sorted by start: (start, end,
/// the referenced binding's crawl-time name).
pub struct RefSites {
    sites: Vec<(u32, u32, String)>,
}

impl RefSites {
    pub fn build(state: &RenameState) -> RefSites {
        let view = state.view();
        let mut sites: Vec<(u32, u32, String)> = Vec::new();
        for b in &view.bindings {
            for site in &b.refs {
                sites.push((site.span.start, site.span.end, b.name.clone()));
            }
        }
        sites.sort();
        RefSites { sites }
    }

    /// The names of the reference sites inside `[start, end]`.
    fn names_within(&self, start: u32, end: u32) -> impl Iterator<Item = &str> {
        let from = self.sites.partition_point(|s| s.0 < start);
        self.sites[from..]
            .iter()
            .take_while(move |s| s.0 <= end)
            .filter(move |s| s.1 <= end)
            .map(|s| s.2.as_str())
    }
}

/// TS `collectModuleScopeRefs`: the names referenced inside the function
/// that resolve — by NAME, from the function's own scope — to a binding
/// declared in an ANCESTOR scope. Only resolved references can qualify
/// (an unresolved one has no binding up the function's chain), so the
/// Babel reference sites are the whole candidate set.
fn module_scope_refs(
    state: &RenameState,
    refs: &RefSites,
    side: &SideRows<'_, '_>,
    fn_row: usize,
) -> Vec<String> {
    let span = side.graph.functions[fn_row].span;
    let Some(node) = side.row_node(fn_row) else {
        return Vec::new();
    };
    let fn_scope = state.view().scope_of_node(node);
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for name in refs.names_within(span.start, span.end) {
        if !seen.insert(name) {
            continue;
        }
        if let Some(b) = state.get_binding(fn_scope, name)
            && state.scope_of_binding(b) != fn_scope
        {
            out.push(name.to_string());
        }
    }
    out
}
