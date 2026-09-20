//! The function↔binding alternation — TS original:
//! `src/prior-version/prior-version.ts`'s module-binding matching block
//! (:1432-1652) plus the reference-identity evidence builder (:1655-1832).
//!
//! WHY IT EXISTS: binding matches give ambiguous same-hash function buckets
//! their only remaining identity signal — WHICH matched thing a member
//! references (Bun's export getters have no callees, callers, or matched
//! parents; the referenced value is a function or another binding, so callee
//! edges never see it). Each round's new function matches then strengthen
//! the next binding round (alias chains resolve through earlier rounds).
//!
//! Identity standard: the TS keys its reference-identity maps by the babel
//! `Binding` OBJECT (the resolved symbol, not the name — shadowing-safe);
//! the Rust equivalent is oxc's `SymbolId`, the same standard the hash
//! placeholders use (07 §1). Neighbor ids are session-id strings
//! (`input.js:L:C` / `module:<name>`), joined from the graph's spans.
//!
//! Babel-probed reference semantics (test/parity/wp22-ref-probe.mjs):
//! `isReferencedIdentifier` is FALSE for assignment-target writes
//! (`mb = 2`, `mb += 2`, `({x: mb} = o)`, `({mb} = o)`) and TRUE for `mb++`
//! and `for (mb of xs)` — exactly the `binding.referencePaths` model
//! [`crate::graph::babel_reference_node_ids`] implements, so evidence
//! collection reuses that helper wholesale.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use oxc_ast::{AstKind, ast::Expression};
use oxc_semantic::{AstNodes, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use crate::graph::{ModuleBindingNode, UnifiedGraph};
use crate::hash::serialize::SymbolTables;

use super::cascade::{MatchOptions, MatchResult, match_functions};
use super::statement_context::StatementContexts;
use super::{FingerprintIndex, IndexNode, build_binding_fingerprint_index};
use crate::propagation::ExternalRefEvidence;

// ---------------------------------------------------------------------------
// Matchability (isMatchableBinding, prior-version.ts:1437)
// ---------------------------------------------------------------------------

/// TS `isMatchableBinding` (:1437): a binding can participate in hash-based
/// cross-version matching when it HAS a fingerprint and its declaration is
/// not a function/arrow/class-EXPRESSION declarator (those go through the
/// function cascade with var-name transfers; matching them here by init
/// hash would compete with the better-informed function matcher). The
/// fingerprint half is the row's `fingerprint_hash` (the index builder
/// filters the same field, matching.rs `build_binding_fingerprint_index`).
pub fn is_matchable_binding(binding: &ModuleBindingNode) -> bool {
    binding.fingerprint_hash.is_some() && binding.declarator_init.is_matchable()
}

// ---------------------------------------------------------------------------
// Session-id joins
// ---------------------------------------------------------------------------

/// span → session id for every row of one graph (functions
/// `input.js:L:C`, module bindings `module:<name>`). The TS reads
/// `callee.sessionId` / `fn.sessionId` off the NODES it holds
/// (calleeNeighborIds :1460, callerFnIds :1465); the Rust graph carries
/// spans, so the join is built once per side. Every internal-callee /
/// caller span names a graph row by construction (the graph builds the
/// edges from its own rows), so a miss cannot happen — a miss is dropped.
fn session_join(graph: &UnifiedGraph) -> HashMap<(u32, u32), String> {
    let mut join: HashMap<(u32, u32), String> =
        HashMap::with_capacity(graph.functions.len() + graph.module_bindings.len());
    for f in &graph.functions {
        join.insert((f.span.start, f.span.end), f.session_id.clone());
    }
    for b in &graph.module_bindings {
        join.insert((b.span.start, b.span.end), b.session_id.clone());
    }
    join
}

/// The session ids of a span list through a side's join — TS
/// `[...binding.internalCallees].map((callee) => callee.sessionId)`
/// (calleeNeighborIds :1460) with `callers` for callerFnIds (:1465).
fn neighbor_ids(spans: &[Span], join: &HashMap<(u32, u32), String>) -> Vec<String> {
    spans
        .iter()
        .filter_map(|s| join.get(&(s.start, s.end)).cloned())
        .collect()
}

// ---------------------------------------------------------------------------
// The identity resolver (idsKey :1483, mapNeighborIds :1492,
// findUniqueByKey :1507, makeBindingIdentityResolver :1534)
// ---------------------------------------------------------------------------

/// TS `idsKey` (:1483): the canonical set key for a list of session ids —
/// deduped and sorted, `|`-joined. (The TS sorts with `.sort()`, the
/// default JS code-unit sort; session ids are ASCII, so a byte sort
/// agrees.)
fn ids_key(ids: &[String]) -> String {
    let unique: BTreeSet<&String> = ids.iter().collect();
    unique.into_iter().cloned().collect::<Vec<_>>().join("|")
}

/// TS `mapNeighborIds` (:1492): translate prior-side neighbor ids through
/// the matches so far and return the canonical key — or None when any
/// neighbor is unmatched or there are no neighbors (no identity evidence).
fn map_neighbor_ids(
    prior_ids: &[String],
    neighbor_matches: &HashMap<String, String>,
) -> Option<String> {
    if prior_ids.is_empty() {
        return None;
    }
    let mut mapped: Vec<String> = Vec::with_capacity(prior_ids.len());
    for id in prior_ids {
        let matched = neighbor_matches.get(id)?;
        mapped.push(matched.clone());
    }
    Some(ids_key(&mapped))
}

/// TS `findUniqueByKey` (:1507): the single candidate whose neighbor key
/// equals `expected_key` — None when none fits or MORE than one does
/// (strict by design, precision over recall).
fn find_unique_by_key(
    expected_key: &str,
    candidates: &[String],
    new_by_id: &BTreeMap<String, &ModuleBindingNode>,
    ids_of: impl Fn(&ModuleBindingNode) -> Vec<String>,
) -> Option<String> {
    let mut found: Option<&String> = None;
    for cand_id in candidates {
        let Some(candidate) = new_by_id.get(cand_id) else {
            continue;
        };
        if ids_key(&ids_of(candidate)) != expected_key {
            continue;
        }
        if found.is_some() {
            return None; // more than one candidate fits
        }
        found = Some(cand_id);
    }
    found.cloned()
}

// ---------------------------------------------------------------------------
// The prepared setup (BindingMatchSetup :1598, prepareBindingMatching :1609)
// ---------------------------------------------------------------------------

/// TS `BindingMatchSetup` (:1598): the binding cascade's inputs, shared
/// across alternation rounds. The TS holds the two filtered indexes and
/// the `Map<sessionId, ModuleBindingNode>` by-id maps; the Rust adds the
/// span→session-id joins (`neighbor_ids`' lookup) and keeps the by-id
/// values as graph-row references.
pub struct BindingMatchSetup<'g> {
    pub prior_index: FingerprintIndex<'g>,
    pub new_index: FingerprintIndex<'g>,
    pub prior_by_id: BTreeMap<String, &'g ModuleBindingNode>,
    pub new_by_id: BTreeMap<String, &'g ModuleBindingNode>,
    prior_join: HashMap<(u32, u32), String>,
    new_join: HashMap<(u32, u32), String>,
}

impl BindingMatchSetup<'_> {
    /// TS `makeBindingIdentityResolver` (:1534): the same-hash bucket
    /// disambiguator — a prior and a new binding correspond when the
    /// prior's referenced (or referencing) neighbors map exactly onto the
    /// candidate's under `neighbor_matches` — matched functions plus
    /// bindings matched in earlier rounds. Callee neighbors first, caller
    /// functions second (`??` order, :1546-1552).
    pub fn make_identity_resolver(
        &self,
        neighbor_matches: HashMap<String, String>,
    ) -> impl Fn(&str, &[String]) -> Option<String> + '_ {
        move |old_id: &str, candidates: &[String]| -> Option<String> {
            let prior = self.prior_by_id.get(old_id)?;
            let resolve = |prior_spans: &[Span], cand_spans: fn(&ModuleBindingNode) -> &[Span]| {
                let prior_ids = neighbor_ids(prior_spans, &self.prior_join);
                let expected_key = map_neighbor_ids(&prior_ids, &neighbor_matches)?;
                find_unique_by_key(&expected_key, candidates, &self.new_by_id, |cand| {
                    neighbor_ids(cand_spans(cand), &self.new_join)
                })
            };
            resolve(&prior.internal_callees, |b| &b.internal_callees)
                .or_else(|| resolve(&prior.callers, |b| &b.callers))
        }
    }
}

/// TS `prepareBindingMatching` (:1609): filters to matchable bindings and
/// builds the cascade inputs — None when either side has nothing matchable.
/// The TS filters the binding LIST then builds the index over the filter
/// (:1615-1617); the Rust builder runs over the whole graph (the WP2.1
/// probe pins it there), so the filter lands after the build via
/// [`FingerprintIndex::retain_entries`].
pub fn prepare_binding_matching<'g>(
    prior_graph: &'g UnifiedGraph,
    prior_semantic: &Semantic<'_>,
    prior_tables: &SymbolTables,
    new_graph: &'g UnifiedGraph,
    new_semantic: &Semantic<'_>,
    new_tables: &SymbolTables,
) -> Option<BindingMatchSetup<'g>> {
    // TS: `if (!newModuleBindings || newModuleBindings.length === 0)` — the
    // check is on the RAW list, before matchability.
    if new_graph.module_bindings.is_empty() {
        return None;
    }
    let matchable = |graph: &'g UnifiedGraph| -> Vec<&'g ModuleBindingNode> {
        graph
            .module_bindings
            .iter()
            .filter(|b| is_matchable_binding(b))
            .collect()
    };
    let prior_matchable = matchable(prior_graph);
    let new_matchable = matchable(new_graph);
    if prior_matchable.is_empty() || new_matchable.is_empty() {
        return None;
    }

    // The retain predicate: binding rows whose span is in the matchable
    // set (the builder already drops fingerprint-less rows; this adds the
    // declarator-init half).
    let keys = |rows: &[&'g ModuleBindingNode]| -> HashSet<(u32, u32)> {
        rows.iter().map(|b| (b.span.start, b.span.end)).collect()
    };
    let prior_keys = keys(&prior_matchable);
    let new_keys = keys(&new_matchable);
    let keep = |graph: &'g UnifiedGraph, allowed: HashSet<(u32, u32)>| {
        move |entry: &super::IndexEntry| match entry.node {
            IndexNode::Binding(row) => {
                let span = graph.module_bindings[row].span;
                allowed.contains(&(span.start, span.end))
            }
            IndexNode::Function(_) => false,
        }
    };
    let prior_index = build_binding_fingerprint_index(prior_graph, prior_semantic, prior_tables)
        .retain_entries(keep(prior_graph, prior_keys));
    let new_index = build_binding_fingerprint_index(new_graph, new_semantic, new_tables)
        .retain_entries(keep(new_graph, new_keys));

    let by_id = |rows: &[&'g ModuleBindingNode]| -> BTreeMap<String, &'g ModuleBindingNode> {
        rows.iter().map(|b| (b.session_id.clone(), *b)).collect()
    };
    Some(BindingMatchSetup {
        prior_index,
        new_index,
        prior_by_id: by_id(&prior_matchable),
        new_by_id: by_id(&new_matchable),
        prior_join: session_join(prior_graph),
        new_join: session_join(new_graph),
    })
}

// ---------------------------------------------------------------------------
// Binding identity rounds (runBindingMatchRounds, prior-version.ts:1566)
// ---------------------------------------------------------------------------

/// TS `MAX_IDENTITY_ROUNDS` (:1569): the binding cascade iterates identity
/// rounds to a fixpoint — bindings matched in one round become evidence
/// for their neighbors in the next (alias chains). Each round's resolver
/// is monotone (mappings never change), so previously resolved buckets
/// resolve identically and new evidence only adds matches.
pub const MAX_IDENTITY_ROUNDS: usize = 4;

/// TS `runBindingMatchRounds` (:1566): the binding cascade's match rounds.
/// Round 0 resolves with the function matches alone; rounds 1.. add each
/// round's own binding matches to the evidence and stop at the first
/// non-growth.
pub fn run_binding_match_rounds(
    prior_index: &FingerprintIndex<'_>,
    new_index: &FingerprintIndex<'_>,
    prior_ctx: &StatementContexts,
    new_ctx: &StatementContexts,
    setup: &BindingMatchSetup<'_>,
    fn_matches: &HashMap<String, String>,
) -> MatchResult {
    // The evidence union is rebuilt per round from the function matches
    // plus THAT round's binding matches (TS `new Map([...fnMatches,
    // ...result.matches])`, :1577) — never accumulated across rounds, so
    // a hypothetical match loss cannot leak stale evidence.
    let resolver = setup.make_identity_resolver(fn_matches.clone());
    let mut result = match_functions(
        prior_index,
        new_index,
        prior_ctx,
        new_ctx,
        MatchOptions {
            resolve_ambiguous_candidate: Some(&resolver),
            ..MatchOptions::default()
        },
    );
    for _round in 1..MAX_IDENTITY_ROUNDS {
        let mut neighbor_matches = fn_matches.clone();
        neighbor_matches.extend(result.matches.iter().map(|(k, v)| (k.clone(), v.clone())));
        let resolver = setup.make_identity_resolver(neighbor_matches);
        let next = match_functions(
            prior_index,
            new_index,
            prior_ctx,
            new_ctx,
            MatchOptions {
                resolve_ambiguous_candidate: Some(&resolver),
                ..MatchOptions::default()
            },
        );
        let grew = next.matches.len() > result.matches.len();
        result = next;
        if !grew {
            break;
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Graph sides (the evidence builder's per-side inputs)
// ---------------------------------------------------------------------------

/// One side's reference-identity inputs: the TS reads `priorFnMap` /
/// `newFunctions` (Map<sessionId, FunctionNode>) plus each side's babel
/// AST for the reference walks; the Rust precomputes the holders and the
/// resolved-reference table once (`build`).
pub struct GraphSide<'g> {
    pub graph: &'g UnifiedGraph,
    /// session id → `graph.functions` row index (the TS map's key set).
    fn_by_session: HashMap<String, usize>,
    /// symbol → the fn session id whose value the binding holds
    /// (`functionIdsByBinding` :1767 over ALL of the side's functions).
    /// BTreeMap: the merge into the identity map reads it by iteration
    /// (holder overwrites binding ids), so the order must be defined.
    holders: BTreeMap<SymbolId, String>,
    /// Every resolved reference that survives babel's referencePaths
    /// predicate ([`crate::graph::babel_reference_node_ids`]), as
    /// (start, end, symbol), sorted by (start, end) — `fn.refs` is the
    /// per-FUNCTION view of the same walk (`collectReferencedBindingIds`
    /// :1814 traverses each function's subtree; a subtree IS the span
    /// range for well-nested spans).
    refs: Vec<(u32, u32, SymbolId)>,
}

impl<'g> GraphSide<'g> {
    pub fn build(graph: &'g UnifiedGraph, semantic: &Semantic<'_>) -> GraphSide<'g> {
        let fn_by_session: HashMap<String, usize> = graph
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.session_id.clone(), i))
            .collect();
        let holders = holding_session_ids(graph, semantic);
        let nodes = semantic.nodes();
        let mut refs: Vec<(u32, u32, SymbolId)> = Vec::new();
        let scoping = semantic.scoping();
        for i in 0..scoping.symbols_len() {
            let symbol = SymbolId::new(i);
            for node_id in crate::graph::babel_reference_node_ids(semantic, symbol) {
                let span = nodes.get_node(node_id).span();
                refs.push((span.start, span.end, symbol));
            }
        }
        refs.sort_unstable_by_key(|r| (r.0, r.1));
        GraphSide {
            graph,
            fn_by_session,
            holders,
            refs,
        }
    }

    /// The function row for a session id, when the side holds it.
    fn fn_row_of_session(&self, session_id: &str) -> Option<usize> {
        self.fn_by_session.get(session_id).copied()
    }

    /// TS `collectReferencedBindingIds` (:1814): the module-binding /
    /// function-holder session ids `fn_row` references, resolved per
    /// OCCURRENCE (a name lookup from the function root would mis-resolve
    /// shadowed occurrences). The inverse walk: the side's reference table
    /// is partitioned to the fn's span range (a reference inside the fn's
    /// subtree ⇔ its span is contained in the fn's span).
    fn collect_referenced_binding_ids(
        &self,
        fn_row: usize,
        ids_by_binding: &HashMap<SymbolId, String>,
    ) -> BTreeSet<String> {
        let span = self.graph.functions[fn_row].span;
        let lo = self.refs.partition_point(|r| r.0 < span.start);
        let mut refs = BTreeSet::new();
        for &(start, end, symbol) in &self.refs[lo..] {
            if start >= span.end {
                break;
            }
            if end > span.end {
                continue; // a container whose span straddles the fn — not a subtree member
            }
            if let Some(id) = ids_by_binding.get(&symbol) {
                refs.insert(id.clone());
            }
        }
        refs
    }
}

/// TS `functionIdsByBinding` (:1767) + `holdingBinding` (:1779): the
/// binding that holds each function's value, identity-guarded — the
/// resolved binding's declaration site must be this exact function or its
/// declarator. Arms (babel-probed, test/parity/wp22-ref-probe.mjs):
///
/// 1. FUNCTION DECLARATION (`function g(){}`): the name symbol whose
///    declaration is this Function node. The declaration parents are the
///    program/export/block wrappers — a named expression's self-name
///    registers under a declarator/method parent and is excluded here
///    (its row is held by arm 2 through the DECLARATOR's binding).
///    Redeclared declarations (`function g(){} function g(){}`) keep only
///    the first — babel's `binding.path` points at the first declaration,
///    so the second fails the `binding.path.node === path.node` guard;
///    oxc's `symbol_declaration` is the first declaration, so the second
///    is simply never this symbol's declaration node.
/// 2. `var f = <function-like>`: the DECLARATOR id's symbol, guarded by
///    the symbol's declaration being this declarator (babel's
///    `binding.path.node === parent.node`) and the id being a plain
///    identifier. Redeclared `var x; var x = () => 1` contributes NOTHING:
///    babel resolves `x` to the FIRST declarator (probe: binding.path@4
///    for all three), which has no init — so the thunk's guard fails; oxc
///    keys the same first declarator, whose init is None, so arm 2 skips.
///
/// Methods, class declarations and assignments (`x = () => 1`) have no
/// holding binding (babel answers null in every arm).
fn holding_session_ids(
    graph: &UnifiedGraph,
    semantic: &Semantic<'_>,
) -> BTreeMap<SymbolId, String> {
    let nodes: &AstNodes<'_> = semantic.nodes();
    let scoping = semantic.scoping();
    let fn_idx_by_span: HashMap<(u32, u32), usize> = graph
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.span.start, f.span.end), i))
        .collect();
    let mut out: BTreeMap<SymbolId, String> = BTreeMap::new();
    for i in 0..scoping.symbols_len() {
        let symbol = SymbolId::new(i);
        let decl_id = scoping.symbol_declaration(symbol);
        match nodes.get_node(decl_id).kind() {
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = decl.init.as_ref().map(crate::babel_view::unparen) else {
                    continue;
                };
                let fn_span = match init {
                    Expression::FunctionExpression(f) => f.span,
                    Expression::ArrowFunctionExpression(a) => a.span,
                    _ => continue,
                };
                // `t.isIdentifier(parent.node.id)` + the identity guard: a
                // destructured declarator's sub-symbol's declaration is not
                // this identifier's span.
                let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id else {
                    continue;
                };
                if id.span != scoping.symbol_span(symbol) {
                    continue;
                }
                if let Some(&row) = fn_idx_by_span.get(&(fn_span.start, fn_span.end)) {
                    out.insert(symbol, graph.functions[row].session_id.clone());
                }
            }
            AstKind::Function(f) => {
                let Some(id) = f.id.as_ref() else {
                    continue;
                };
                if id.span != scoping.symbol_span(symbol) {
                    continue;
                }
                // The declaration parents (babel isFunctionDeclaration's
                // reach — program, export wrappers, and the sloppy-mode
                // block; a declarator/method parent is an EXPRESSION).
                let parent_id = nodes.parent_id(decl_id);
                let is_declaration = parent_id != decl_id
                    && matches!(
                        nodes.get_node(parent_id).kind(),
                        AstKind::Program(_)
                            | AstKind::ExportNamedDeclaration(_)
                            | AstKind::ExportDefaultDeclaration(_)
                            | AstKind::BlockStatement(_)
                    );
                if !is_declaration {
                    continue;
                }
                if let Some(&row) = fn_idx_by_span.get(&(f.span.start, f.span.end)) {
                    out.insert(symbol, graph.functions[row].session_id.clone());
                }
            }
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Reference-identity evidence (buildExternalRefEvidence :1655)
// ---------------------------------------------------------------------------

/// TS `referenceIdsByBinding` (:1748): one side's reference-identity map —
/// module-binding ids first (the MATCHABLE by-id map, `setup?.priorById`),
/// function-holder ids second, the holder OVERWRITING on collision (a
/// binding that is both — `var t = () => x` — resolves to the function
/// id, whose match set grows through alternation). Same merge order on
/// both sides.
fn reference_ids_by_binding(
    by_id: Option<&BTreeMap<String, &ModuleBindingNode>>,
    side: &GraphSide<'_>,
) -> HashMap<SymbolId, String> {
    let mut ids: HashMap<SymbolId, String> = HashMap::new();
    if let Some(by_id) = by_id {
        for (session_id, node) in by_id {
            ids.insert(node.symbol, session_id.clone());
        }
    }
    for (symbol, session_id) in &side.holders {
        ids.insert(*symbol, session_id.clone());
    }
    ids
}

/// TS `buildExternalRefEvidence` (:1655): reference-identity evidence for
/// the ambiguous functions and their candidates — None when there is
/// nothing to build on (no ambiguity, no confirmed matches yet, or no
/// identity map on either side). A reference can hit a matchable module
/// binding OR a binding that holds a graph function (Bun's export thunks
/// reference functions without calling them, so callee edges never see
/// them). Refs are collected per binding IDENTITY (the resolved symbol),
/// the same precision standard vote propagation uses.
pub fn build_external_ref_evidence(
    ambiguous: &HashMap<String, Vec<String>>,
    prior: &GraphSide<'_>,
    new: &GraphSide<'_>,
    setup: Option<&BindingMatchSetup<'_>>,
    binding_matches: &HashMap<String, String>,
    fn_matches: &HashMap<String, String>,
) -> Option<ExternalRefEvidence> {
    if ambiguous.is_empty() {
        return None;
    }
    if binding_matches.is_empty() && fn_matches.is_empty() {
        return None;
    }
    let prior_ids = reference_ids_by_binding(setup.map(|s| &s.prior_by_id), prior);
    let new_ids = reference_ids_by_binding(setup.map(|s| &s.new_by_id), new);
    if prior_ids.is_empty() || new_ids.is_empty() {
        return None;
    }

    // The confirmed-match union: bindings then functions (TS
    // `[...bindingMatches, ...fnMatches]` — disjoint key namespaces, so
    // the later insert's precedence is unobservable). BTreeMap: keyed
    // lookups only.
    let mut ref_matches: BTreeMap<String, String> = binding_matches
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    ref_matches.extend(fn_matches.iter().map(|(k, v)| (k.clone(), v.clone())));

    let mut old_refs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut new_refs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut candidate_ids: BTreeSet<String> = BTreeSet::new();
    // Sorted key snapshot: the loop only fills keyed maps, so iteration
    // order cannot reach a decision — but hash-map iteration is forbidden
    // outright (house rule).
    let mut ambiguous_ids: Vec<&String> = ambiguous.keys().collect();
    ambiguous_ids.sort();
    for old_id in ambiguous_ids {
        let candidates = &ambiguous[old_id];
        let Some(row) = prior.fn_row_of_session(old_id) else {
            continue;
        };
        old_refs.insert(
            old_id.clone(),
            prior.collect_referenced_binding_ids(row, &prior_ids),
        );
        candidate_ids.extend(candidates.iter().cloned());
    }
    for cand_id in candidate_ids {
        if let Some(row) = new.fn_row_of_session(&cand_id) {
            let refs = new.collect_referenced_binding_ids(row, &new_ids);
            new_refs.insert(cand_id, refs);
        }
    }
    Some(ExternalRefEvidence {
        old_refs,
        new_refs,
        ref_matches,
    })
}

// ---------------------------------------------------------------------------
// The alternation loop (alternateFunctionAndBindingMatching :1639)
// ---------------------------------------------------------------------------

/// TS `MAX_ALTERNATION_ROUNDS` (:1628): cap on function↔binding
/// alternation rounds (first round included).
pub const MAX_ALTERNATION_ROUNDS: usize = 3;

/// TS's return shape `{ functionResult, bindingResult }`.
#[derive(Debug, Clone)]
pub struct AlternationOutcome {
    pub function_result: MatchResult,
    pub binding_result: Option<MatchResult>,
}

/// TS `alternateFunctionAndBindingMatching` (:1639): alternates the
/// function and binding cascades to a capped fixpoint. Runs even without
/// matchable bindings — matched-FUNCTION references alone crack
/// export-thunk buckets (the evidence builder's `setup` is optional).
/// Growth is checked on the FUNCTION side; the binding result always
/// reflects the final function matches.
#[allow(clippy::too_many_arguments)]
pub fn alternate_function_and_binding_matching(
    initial_function_result: MatchResult,
    prior_index: &FingerprintIndex<'_>,
    new_index: &FingerprintIndex<'_>,
    prior_ctx: &StatementContexts,
    new_ctx: &StatementContexts,
    prior: &GraphSide<'_>,
    new: &GraphSide<'_>,
    setup: Option<&BindingMatchSetup<'_>>,
) -> AlternationOutcome {
    let mut function_result = initial_function_result;
    let mut binding_result = setup.map(|s| {
        run_binding_match_rounds(
            &s.prior_index,
            &s.new_index,
            prior_ctx,
            new_ctx,
            s,
            &function_result.matches,
        )
    });

    for _round in 1..MAX_ALTERNATION_ROUNDS {
        if function_result.ambiguous.is_empty() {
            break;
        }
        let evidence = build_external_ref_evidence(
            &function_result.ambiguous,
            prior,
            new,
            setup,
            binding_result
                .as_ref()
                .map_or(&HashMap::new(), |r| &r.matches),
            &function_result.matches,
        );
        let Some(evidence) = evidence else {
            break;
        };
        let next = match_functions(
            prior_index,
            new_index,
            prior_ctx,
            new_ctx,
            MatchOptions {
                enable_propagation: true,
                external_ref_evidence: Some(evidence),
                ..MatchOptions::default()
            },
        );
        if next.matches.len() <= function_result.matches.len() {
            break;
        }
        function_result = next;
        if let Some(s) = setup {
            binding_result = Some(run_binding_match_rounds(
                &s.prior_index,
                &s.new_index,
                prior_ctx,
                new_ctx,
                s,
                &function_result.matches,
            ));
        }
    }

    AlternationOutcome {
        function_result,
        binding_result,
    }
}

#[cfg(test)]
mod alternation_test;
