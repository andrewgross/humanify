//! Binding role evidence — TS original: `src/prior-version/binding-role.ts`
//! (239 LOC), the role half of WP2.3.
//!
//! A module binding's role is compact plain data (no AST references) so the
//! prior side can be computed while the prior AST is alive and compared
//! against the fresh side's. Two consumers call [`binding_roles_agree`]:
//! the statement-twin gate's `declared_roles_agree`
//! (statement-twin.ts :339 — `allowContentFreeElimination` FALSE: a
//! consumer that compares many slots pairwise must not get blanket
//! agreement on bare declarators) and, later, the single-vote pin ladder
//! (WP3.3 — TRUE).
//!
//! The structural-hash half is the row's own `fingerprint_hash` (the
//! literal-preserving computeBindingFingerprint, computed at graph build —
//! graph.rs `binding_fingerprint_hash`); this module computes only the
//! CONTENT SHINGLES (the slot-blind k-gram sets) on demand, because they
//! are consulted only when the hashes differ.

use std::collections::{BTreeSet, HashMap};

use oxc_semantic::Semantic;
use serde_json::Value;

use crate::graph::ModuleBindingNode;
use crate::hash::serialize::SymbolTables;

/// Minimum shingle overlap for two roles to count as the same binding (:41).
pub const SINGLE_VOTE_CONTENT_FLOOR: f64 = 0.5;

/// TS `BindingRole` (:23).
#[derive(Debug, Clone, Default)]
pub struct BindingRole {
    /// Content hash from the binding's fingerprint; None when unhashable.
    pub structural_hash: Option<String>,
    /// Slot-blind, literal-preserving k-gram shingles of the binding's
    /// content; None when the binding has no init and no assignment.
    pub content_shingles: Option<BTreeSet<String>>,
    /// Session ids of FUNCTION callees referenced by the initializer.
    pub fn_callee_ids: Vec<String>,
    /// True when the initializer also references module bindings — the
    /// callee comparison is then inconclusive and must not veto.
    pub has_binding_callees: bool,
}

/// One side's evidence context for [`compute_binding_role`]: the semantic
/// (declaration nodes, symbol spans), the symbol tables the content walk
/// slots identifiers through, and the span → session-id join for the
/// callee ids.
pub struct RoleSide<'a, 's> {
    pub semantic: &'a Semantic<'s>,
    pub tables: &'a SymbolTables,
    /// Graph-row span → session id (functions AND module bindings), the
    /// TS `callee.sessionId` join (alternation.rs `session_join`).
    pub session_join: &'a HashMap<(u32, u32), String>,
}

/// Verdict with a log-friendly reason (TS `RoleAgreement` :126).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAgreement {
    pub agrees: bool,
    pub reason: &'static str,
}

impl RoleAgreement {
    fn agree(reason: &'static str) -> RoleAgreement {
        RoleAgreement {
            agrees: true,
            reason,
        }
    }
    fn refuse(reason: &'static str) -> RoleAgreement {
        RoleAgreement {
            agrees: false,
            reason,
        }
    }
}

/// TS `computeBindingRole` (:81) over a graph row.
pub fn compute_binding_role(row: &ModuleBindingNode, side: &RoleSide<'_, '_>) -> BindingRole {
    let (fn_callee_ids, has_binding_callees) = split_callees(&row.internal_callees, side);
    let content_shingles =
        binding_content_json(row, side).map(|json| compute_content_shingles(&json, side.tables));
    BindingRole {
        structural_hash: row.fingerprint_hash.clone(),
        content_shingles,
        fn_callee_ids,
        has_binding_callees,
    }
}

/// TS `computeFunctionRole` (:101): the role of a function declaration
/// head — same shape as a module binding's, so the single-vote ladder can
/// gate cold function heads. `row_json` is the row's own ESTree node (the
/// babel `fn.path` the TS shingles).
pub fn compute_function_role(
    row: &crate::graph::GraphFunction,
    row_json: &Value,
    side: &RoleSide<'_, '_>,
) -> BindingRole {
    let (fn_callee_ids, has_binding_callees) = split_callees(&row.internal_callees, side);
    BindingRole {
        structural_hash: Some(row.structural_hash.clone()),
        content_shingles: Some(compute_content_shingles(row_json, side.tables)),
        fn_callee_ids,
        has_binding_callees,
    }
}

/// TS `splitCallees` (:110) over the row's callee spans through the
/// side's session join. A callee span that names no graph row is dropped
/// (the graph builds the edges from its own rows, so a miss cannot happen
/// — alternation.rs's `neighbor_ids` convention).
fn split_callees(spans: &[oxc_span::Span], side: &RoleSide<'_, '_>) -> (Vec<String>, bool) {
    let mut fn_callee_ids: Vec<String> = Vec::new();
    let mut has_binding_callees = false;
    for span in spans {
        let Some(session_id) = side.session_join.get(&(span.start, span.end)) else {
            continue;
        };
        if session_id.starts_with("module:") {
            has_binding_callees = true;
        } else {
            fn_callee_ids.push(session_id.clone());
        }
    }
    (fn_callee_ids, has_binding_callees)
}

/// The ESTree JSON of the node holding a module binding's hashable content
/// — the TS `resolveBindingContentPath` (function-graph.ts :411) + the
/// constantViolations[0] rule. ONE owner for the decision: graph.rs's
/// `binding_content_estree` (the same subtree `binding_fingerprint_hash`
/// hashes — the shingle stream must cover what the fingerprint hashed).
/// The row's positional `redeclared_spans` carry the redeclarations: oxc
/// gives a redeclaration identifier the SAME symbol id, so a scan keyed on
/// `symbol_id.is_none()` sees nothing and the K5 zlib-counter case minted
/// content babel never has.
fn binding_content_json(row: &ModuleBindingNode, side: &RoleSide<'_, '_>) -> Option<Value> {
    let json = crate::graph::binding_content_estree(
        row.symbol,
        side.semantic.nodes(),
        side.semantic.scoping(),
        &row.redeclared_spans,
    )?;
    let mut de = serde_json::Deserializer::from_str(&json);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).ok()
}

/// Slot-blind k-gram shingles over the content's serialized token stream
/// (TS `computeContentShingles` :58) — ONE owner: the babel-order token
/// walk `matching::statement_align::content_shingles` (the TS walks
/// `Object.keys(babelNode)` and the k-grams are ORDER-sensitive —
/// 15-porting-lessons §12). This module used to carry its own mirror of
/// the canonical serializer's walk (alphabetical key order, oxc's field
/// set); the twins gate compared identical under both on all four pairs
/// (2026-09-25), and the single-vote pin ladder (WP3.3) needs the TS's
/// exact k-grams, so the mirror is gone.
pub fn compute_content_shingles(content_json: &Value, tables: &SymbolTables) -> BTreeSet<String> {
    crate::matching::statement_align::content_shingles(tables, content_json)
}

/// TS `bindingRolesAgree` (:156). Content must positively corroborate
/// (equal non-null hashes, or shingle overlap at the floor) — missing
/// evidence is a refusal, not agreement. The callee veto then compares the
/// prior's function callees mapped through the function matches.
pub fn binding_roles_agree(
    prior: &BindingRole,
    next: &BindingRole,
    prior_to_new_fn_ids: &HashMap<String, String>,
    allow_content_free_elimination: bool,
) -> RoleAgreement {
    let content = content_agreement(prior, next, allow_content_free_elimination);
    if !content.agrees {
        return content;
    }
    match callee_veto(prior, next, prior_to_new_fn_ids) {
        Some(veto) => veto,
        None => content,
    }
}

/// Positive content corroboration: hash equality or shingle overlap (:176).
fn content_agreement(
    prior: &BindingRole,
    next: &BindingRole,
    allow_content_free_elimination: bool,
) -> RoleAgreement {
    if prior.structural_hash.is_some() && prior.structural_hash == next.structural_hash {
        return RoleAgreement::agree("hash-equal");
    }
    let prior_shingles = prior.content_shingles.as_ref().filter(|s| !s.is_empty());
    let next_shingles = next.content_shingles.as_ref().filter(|s| !s.is_empty());
    if let (Some(a), Some(b)) = (prior_shingles, next_shingles) {
        let similarity = crate::matching::jaccard_similarity(a, b);
        if similarity >= SINGLE_VOTE_CONTENT_FLOOR {
            return RoleAgreement::agree("shingle-overlap");
        }
    }
    // exp066: SYMMETRIC content absence agrees by elimination — OPT-IN,
    // the license is the CALLER's exclusivity gates, never the roles
    // themselves (the twin tier's pairwise comparison must NOT get it).
    if allow_content_free_elimination
        && prior.structural_hash.is_none()
        && next.structural_hash.is_none()
        && prior_shingles.is_none()
        && next_shingles.is_none()
    {
        return RoleAgreement::agree("content-free-elimination");
    }
    if prior_shingles.is_none() || next_shingles.is_none() {
        return RoleAgreement::refuse("no-content-evidence");
    }
    RoleAgreement::refuse("content-below-floor")
}

/// Callee-identity veto, or None when inconclusive/agreeing (:218).
fn callee_veto(
    prior: &BindingRole,
    next: &BindingRole,
    prior_to_new_fn_ids: &HashMap<String, String>,
) -> Option<RoleAgreement> {
    if prior.has_binding_callees || next.has_binding_callees {
        return None;
    }
    if prior.fn_callee_ids.is_empty() || next.fn_callee_ids.is_empty() {
        return None;
    }
    let mut mapped: Vec<&String> = Vec::with_capacity(prior.fn_callee_ids.len());
    for prior_id in &prior.fn_callee_ids {
        let new_id = prior_to_new_fn_ids.get(prior_id)?;
        mapped.push(new_id);
    }
    let expected = ids_key_strings(mapped.iter().copied().cloned());
    let actual = ids_key_strings(next.fn_callee_ids.iter().cloned());
    if expected != actual {
        return Some(RoleAgreement::refuse("callee-mismatch"));
    }
    None
}

/// TS `[...new Set(ids)].sort().join("|")` (:233) — session ids are ASCII,
/// so a byte sort agrees with the default JS sort.
fn ids_key_strings<I: IntoIterator<Item = String>>(ids: I) -> String {
    let unique: BTreeSet<String> = ids.into_iter().collect();
    unique.into_iter().collect::<Vec<_>>().join("|")
}

/// The keys the token walks drop (pub because gates.rs's private-pair
/// walk drops the same keys — the serialize.rs walk's list).
pub const SKIP_KEYS: [&str; 8] = [
    "type",
    "loc",
    "start",
    "end",
    "range",
    "extra",
    "leadingComments",
    "trailingComments",
];

/// The union of shingle sets a role gate computes on — the tests need the
/// jaccard similarity of two `BindingRole`s (the TS test suite computes it
/// directly).
pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    crate::matching::jaccard_similarity(a, b)
}

#[cfg(test)]
mod role_test;
