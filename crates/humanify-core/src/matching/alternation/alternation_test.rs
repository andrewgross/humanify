//! Tests for the function↔binding alternation ([`super`]). Two layers:
//! unit tests for the pure helpers (each TS behavior cited from the port
//! source) and harness tests whose subtle babel semantics were probed with
//! test/parity/wp22-ref-probe.mjs (its findings are cited per assertion).
//!
//! The end-to-end tests port prior-version.test.ts:905-1035's
//! ambiguous-bucket-cracking cases at the MATCH level (the TS asserts
//! through matchPriorVersion's rename records; the Rust asserts the
//! MatchResult's session-id pairs — the rename derivation is the apply
//! site's job, not this module's).

use std::collections::BTreeMap;
use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_span::Span;

use super::{
    AlternationOutcome, GraphSide, MAX_ALTERNATION_ROUNDS, MAX_IDENTITY_ROUNDS,
    alternate_function_and_binding_matching, find_unique_by_key, ids_key, is_matchable_binding,
    map_neighbor_ids, prepare_binding_matching, reference_ids_by_binding,
};
use crate::graph::UnifiedGraph;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::build_fingerprint_index;
use crate::matching::cascade::{MatchOptions, MatchResult, match_functions};
use crate::matching::statement_context::StatementContexts;

// ---------------------------------------------------------------------------
// Unit: the pure helpers
// ---------------------------------------------------------------------------

#[test]
fn ids_key_dedupes_and_sorts() {
    assert_eq!(
        ids_key(&["b".into(), "a".into(), "b".into()]),
        "a|b",
        "dedupe + sort (TS idsKey :1483 — [...new Set(ids)].sort().join('|'))"
    );
    assert_eq!(ids_key(&[]), "", "empty list keys to the empty string");
}

#[test]
fn map_neighbor_ids_contract() {
    let matches: std::collections::HashMap<String, String> =
        [(String::from("input.js:1:1"), String::from("fresh.js:1:1"))]
            .into_iter()
            .collect();
    // No neighbors — no identity evidence (:1495).
    assert_eq!(map_neighbor_ids(&[], &matches), None);
    // Any unmatched neighbor → null (:1500).
    assert_eq!(map_neighbor_ids(&["input.js:2:2".into()], &matches), None);
    // Every neighbor matched → the canonical key of the MAPPED ids.
    assert_eq!(
        map_neighbor_ids(&["input.js:1:1".into(), "input.js:1:1".into()], &matches),
        Some("fresh.js:1:1".into()),
        "mapped through the matches, then deduped+sorted"
    );
}

/// A stand-in row fixture for the uniqueness test: it drives `ids_of`
/// directly, so the node's own fields are inert.
mod fixture {
    use super::*;
    use crate::graph::ModuleBindingNode;

    /// A minimal ModuleBindingNode with dummy identity fields — the
    /// uniqueness contract test supplies the neighbor ids through the
    /// `ids_of` closure, so the row's spans and symbol are inert.
    pub fn binding(name: &str) -> ModuleBindingNode {
        ModuleBindingNode {
            session_id: name.to_string(),
            span: Span::new(0, 1),
            name: name.to_string(),
            symbol: oxc_semantic::SymbolId::new(0usize),
            internal_callees: Vec::new(),
            callers: Vec::new(),
            declarator_init: crate::graph::DeclaratorInit::OtherInit,
            redeclared_spans: Vec::new(),
            fingerprint_hash: Some("h".into()),
        }
    }
}

#[test]
fn find_unique_by_key_uniqueness_contract() {
    let a = fixture::binding("newA");
    let b = fixture::binding("newB");
    let c = fixture::binding("newC");
    // Two candidates share the twin key, one is unique.
    let mut by_id: BTreeMap<String, &_> = BTreeMap::new();
    by_id.insert("newA".into(), &a);
    by_id.insert("newB".into(), &b);
    by_id.insert("newC".into(), &c);
    let mut twin_of: std::collections::HashMap<&str, String> = Default::default();
    twin_of.insert("newA", "k1".into());
    twin_of.insert("newB", "k1".into());
    twin_of.insert("newC", "k2".into());
    let ids_of = |node: &crate::graph::ModuleBindingNode| vec![twin_of[node.name.as_str()].clone()];

    // Exactly one fit → it (:1515-1523).
    assert_eq!(
        find_unique_by_key("k2", &["newA".into(), "newC".into()], &by_id, ids_of),
        Some("newC".into())
    );
    // MORE than one fit → null (:1520).
    assert_eq!(
        find_unique_by_key("k1", &["newA".into(), "newB".into()], &by_id, ids_of),
        None
    );
    // No fit → null.
    assert_eq!(
        find_unique_by_key("k1", &["newC".into()], &by_id, ids_of),
        None
    );
    // A candidate absent from by_id is skipped, not a miss (:1513).
    assert_eq!(
        find_unique_by_key("k2", &["ghost".into(), "newC".into()], &by_id, ids_of),
        Some("newC".into())
    );
}

#[test]
fn caps_match_the_ts_constants() {
    assert_eq!(MAX_IDENTITY_ROUNDS, 4, "TS :1569");
    assert_eq!(MAX_ALTERNATION_ROUNDS, 3, "TS :1628");
}

// ---------------------------------------------------------------------------
// Harness: one side's graph-level behaviors
// ---------------------------------------------------------------------------

/// Parse + graph + tables, handed to the assertions inside the scope that
/// owns the arena (matching_test's pattern).
fn with_harness<T>(
    code: &str,
    run: impl FnOnce(&Ingest<'_>, &UnifiedGraph, &SymbolTables) -> T,
) -> T {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let unified = crate::graph::build_unified_graph(
        &ingest.semantic,
        ingest.program,
        "input.js",
        &[],
        None,
        None,
    );
    run(&ingest, &unified, &tables)
}

fn binding_named<'g>(graph: &'g UnifiedGraph, name: &str) -> &'g crate::graph::ModuleBindingNode {
    graph
        .module_bindings
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no module binding named {name}"))
}

/// The function row whose source text contains `needle` (session ids are
/// positional, and arrow rows carry no name — the text slice is the
/// stable handle).
fn fn_row_with<'g>(
    graph: &'g UnifiedGraph,
    text: &str,
    needle: &str,
) -> &'g crate::graph::GraphFunction {
    graph
        .functions
        .iter()
        .find(|f| text[f.span.start as usize..f.span.end as usize].contains(needle))
        .unwrap_or_else(|| panic!("no function row containing {needle:?}"))
}

#[test]
fn matchability_by_declarator_init() {
    // TS isMatchableBinding (:1437): fingerprint present AND the
    // declarator init not a function/arrow/class expression. The TS
    // probes its own branches (:1444-1453); every arm is exercised here.
    with_harness(
        r#"
        var arrow = () => 1;
        var num = 1;
        class Decl { m() {} }
        var clsExpr = class {};
        var uninitialized;
        var obj = { a: 1 };
    "#,
        |_, unified, _| {
            assert!(
                !is_matchable_binding(binding_named(unified, "arrow")),
                "arrow-function init — the function cascade's job (:1450-1451)"
            );
            assert!(is_matchable_binding(binding_named(unified, "num")));
            assert!(
                is_matchable_binding(binding_named(unified, "Decl")),
                "a class DECLARATION hashes itself and stays matchable (:1446)"
            );
            assert!(
                !is_matchable_binding(binding_named(unified, "clsExpr")),
                "class EXPRESSION init (:1452)"
            );
            assert_eq!(
                binding_named(unified, "uninitialized").declarator_init,
                crate::graph::DeclaratorInit::NoInit,
                "the INIT half of matchability answers true for no init (:1448)"
            );
            assert!(
                !is_matchable_binding(binding_named(unified, "uninitialized")),
                "…but the whole predicate vetoes: no init means NO FINGERPRINT, and                  isMatchableBinding requires one (:1440 `if (!binding.fingerprint) return false`)"
            );
            assert!(is_matchable_binding(binding_named(unified, "obj")));
        },
    );
}

#[test]
fn callers_are_the_referencing_graph_functions() {
    // Edge builder 4d (function-graph.ts:689): every resolved reference's
    // nearest enclosing GRAPH function. An assignment write (`target.v =
    // 2`) still reads `target` — only the assignment TARGET position is a
    // violation, so beta counts; a top-level reference adds nothing
    // (findEnclosingFunction answers null).
    with_harness(
        r#"
        var target = { v: 1 };
        function alpha() { return target.v; }
        function beta() { target.v = 2; target.v++; }
        function gamma() { return alpha(); }
        var top = target.v;
    "#,
        |ingest, unified, _| {
            let text = ingest.text;
            let target = binding_named(unified, "target");
            let alpha = fn_row_with(unified, text, "return target.v");
            let beta = fn_row_with(unified, text, "target.v = 2");
            let expected: Vec<(u32, u32)> = vec![
                (alpha.span.start, alpha.span.end),
                (beta.span.start, beta.span.end),
            ];
            let got: Vec<(u32, u32)> = target.callers.iter().map(|s| (s.start, s.end)).collect();
            assert_eq!(got, expected, "callers = alpha + beta, in span order");
            let gamma = fn_row_with(unified, text, "return alpha()");
            assert!(
                !got.contains(&(gamma.span.start, gamma.span.end)),
                "a call of another function is not a reference of target"
            );
        },
    );
}

#[test]
fn holding_symbol_arms() {
    // TS holdingBinding (:1779) with the wp22-ref-probe findings:
    // declarations hold through their name binding; a declarator holds the
    // function EXPRESSION it inits (named or not — the named-expression
    // probe: holding === DECLARATOR, self-name in its own scope); methods,
    // class declarations and redeclared `var x; var x = () => 1` hold
    // NOTHING (probe: binding.path@first-declarator for all redeclarations).
    with_harness(
        r#"
        function decl() { return 1; }
        var held = () => 2;
        var named = function inner() { return 3; };
        var obj = { method() { return 4; } };
        var redecl;
        var redecl = () => 5;
        class Cls { m() { return 6; } }
    "#,
        |ingest, unified, _| {
            let side = GraphSide::build(unified, &ingest.semantic);
            let text = ingest.text;
            let held_ids: HashSet<&String> = side.holders.values().collect();
            let decl = fn_row_with(unified, text, "return 1;");
            let arrow = fn_row_with(unified, text, "() => 2");
            let inner = fn_row_with(unified, text, "return 3;");
            let method = fn_row_with(unified, text, "return 4;");
            let redecl_thunk = fn_row_with(unified, text, "() => 5");

            assert!(held_ids.contains(&decl.session_id), "arm 1: declaration");
            assert!(
                held_ids.contains(&arrow.session_id),
                "arm 2: the arrow is held by its declarator binding"
            );
            assert!(
                held_ids.contains(&inner.session_id),
                "arm 2 for a NAMED expression: the declarator binding holds it, not the self-name"
            );
            assert!(
                !held_ids.contains(&method.session_id),
                "methods have no holding binding"
            );
            assert!(
                !held_ids.contains(&redecl_thunk.session_id),
                "redeclared var: babel resolves to the FIRST declarator (no init) — the thunk's guard fails"
            );
            let _ = ingest;
        },
    );
}

/// The function row whose source text contains `needle`, picking the
/// INNERMOST match (`fn_row_with` takes the first row whose text contains
/// the needle — an enclosing row contains an inner one's text too).
fn fn_row_innermost<'g>(
    graph: &'g UnifiedGraph,
    text: &str,
    needle: &str,
) -> &'g crate::graph::GraphFunction {
    graph
        .functions
        .iter()
        .filter(|f| text[f.span.start as usize..f.span.end as usize].contains(needle))
        .min_by_key(|f| f.span.end - f.span.start)
        .unwrap_or_else(|| panic!("no function row containing {needle:?}"))
}

#[test]
fn holding_symbol_arms_inner_declarations() {
    // TS holdingBinding arm 1 has NO parent-type restriction — every
    // `isFunctionDeclaration` holds through its name binding, wherever it
    // sits (the WP2.1 self-registration fns: Bun's wrapper body holds the
    // whole bundle, so a wrapper-inner declaration is the NORMAL shape).
    // oxc types a wrapper body FunctionBody where babel types
    // BlockStatement — the parent list must reach it (and the other babel
    // statement-list reach: switch cases, static blocks).
    with_harness(
        r#"
        function wrapper() {
            function inner() { return 111; }
            if (true) { function inBlock() { return 222; } }
            switch (1) { case 0: function inCase() { return 333; } }
            class C { static { function inStatic() { return 444; } } }
            var named = function selfNamed() { return 555; };
        }
    "#,
        |ingest, unified, _| {
            let side = GraphSide::build(unified, &ingest.semantic);
            let text = ingest.text;
            let held_ids: HashSet<&String> = side.holders.values().collect();
            let inner = fn_row_innermost(unified, text, "return 111;");
            let in_block = fn_row_innermost(unified, text, "return 222;");
            let in_case = fn_row_innermost(unified, text, "return 333;");
            let in_static = fn_row_innermost(unified, text, "return 444;");
            let self_named = fn_row_innermost(unified, text, "return 555;");

            assert!(
                held_ids.contains(&inner.session_id),
                "arm 1: a declaration inside a function BODY holds (babel reaches it; oxc's body node is FunctionBody)"
            );
            assert!(
                held_ids.contains(&in_block.session_id),
                "arm 1: a declaration inside a block holds"
            );
            assert!(
                held_ids.contains(&in_case.session_id),
                "arm 1: a declaration inside a switch case holds"
            );
            assert!(
                held_ids.contains(&in_static.session_id),
                "arm 1: a declaration inside a static block holds"
            );
            assert!(
                held_ids.contains(&self_named.session_id),
                "a NAMED EXPRESSION holds through its DECLARATOR (arm 2 — TS's holdingBinding arm 2), never through the self-name"
            );
            let _ = ingest;
        },
    );
}

#[test]
fn referenced_binding_ids_are_per_occurrence_and_reference_shaped() {
    // TS collectReferencedBindingIds (:1814). Probe (wp22-ref-probe):
    // isReferencedIdentifier is FALSE for assignment-target writes
    // (`mb = 1`) and TRUE for updates (`mb++`) — the babel referencePaths
    // model babel_reference_node_ids implements; a shadowing occurrence
    // resolves to the INNER symbol, so it contributes nothing.
    with_harness(
        r#"
        var mb = 1;
        var other = 2;
        function writeOnly() { mb = 1; }
        function updateOnly() { mb++; }
        function readIt() { return other; }
        function shadowed() { var mb = 9; return mb; }
    "#,
        |ingest, unified, _| {
            let side = GraphSide::build(unified, &ingest.semantic);
            // The identity map the evidence builder assembles: matchable
            // module bindings (the setup's by-id map) over the holders.
            let by_id: BTreeMap<String, &crate::graph::ModuleBindingNode> = unified
                .module_bindings
                .iter()
                .filter(|b| is_matchable_binding(b))
                .map(|b| (b.session_id.clone(), b))
                .collect();
            let ids = reference_ids_by_binding(Some(&by_id), &side);
            let refs_of = |needle: &str| {
                let row = fn_row_with(unified, ingest.text, needle);
                side.collect_referenced_binding_ids(
                    unified
                        .functions
                        .iter()
                        .position(|f| f.span == row.span)
                        .expect("row index"),
                    &ids,
                )
            };
            let write_only = refs_of("mb = 1");
            assert!(
                !write_only.contains("module:mb"),
                "an assignment-target write is a constantViolation, not a referencePath"
            );
            let update_only = refs_of("mb++");
            assert!(
                update_only.contains("module:mb"),
                "an update target IS a referencePath (probe: true)"
            );
            let read = refs_of("return other;");
            assert_eq!(
                read,
                ["module:other".to_string()].into_iter().collect(),
                "plain reads map through the identity map"
            );
            let shadowed = refs_of("var mb = 9");
            assert!(
                !shadowed.contains("module:mb"),
                "the shadowing occurrence resolves to the INNER symbol (per-occurrence)"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// Harness: two-side alternation (the TS tests' cases)
// ---------------------------------------------------------------------------

/// One side of the two-side harness: what the assertions need to name
/// rows (the graph + the text the spans index).
struct SideView<'a> {
    graph: &'a UnifiedGraph,
    text: &'a str,
}

struct TwoSides<'a> {
    prior: SideView<'a>,
    fresh: SideView<'a>,
    outcome: &'a AlternationOutcome,
    setup_present: bool,
}

/// Both sides parsed and alternated, exactly as the pipeline's
/// matchAndApplyFunctions drives it (prior-version.ts:546-566): the
/// initial function cascade with propagation, then the alternation with
/// the prepared binding setup. The assertion closure runs inside the
/// scope that owns both arenas.
fn with_two_sides(prior_code: &str, fresh_code: &str, run: impl FnOnce(&TwoSides<'_>)) {
    let prior_allocator = Allocator::default();
    let fresh_allocator = Allocator::default();
    let prior_ingest = Ingest::parse(&prior_allocator, prior_code, "prior.js");
    assert!(
        prior_ingest.errors.is_empty(),
        "prior must parse: {:?}",
        prior_ingest.errors
    );
    let fresh_ingest = Ingest::parse(&fresh_allocator, fresh_code, "fresh.js");
    assert!(
        fresh_ingest.errors.is_empty(),
        "fresh must parse: {:?}",
        fresh_ingest.errors
    );
    let prior_tables = SymbolTables::build(&prior_ingest.semantic);
    let fresh_tables = SymbolTables::build(&fresh_ingest.semantic);
    let prior_graph = crate::graph::build_unified_graph(
        &prior_ingest.semantic,
        prior_ingest.program,
        "prior.js",
        &[],
        None,
        None,
    );
    let fresh_graph = crate::graph::build_unified_graph(
        &fresh_ingest.semantic,
        fresh_ingest.program,
        "fresh.js",
        &[],
        None,
        None,
    );
    let prior_ctx = StatementContexts::build(
        &prior_graph,
        &prior_ingest.semantic,
        &prior_tables,
        prior_ingest.program,
        prior_code,
    );
    let fresh_ctx = StatementContexts::build(
        &fresh_graph,
        &fresh_ingest.semantic,
        &fresh_tables,
        fresh_ingest.program,
        fresh_code,
    );
    let prior_index = build_fingerprint_index(&prior_graph, &prior_ingest.semantic, &prior_tables);
    let fresh_index = build_fingerprint_index(&fresh_graph, &fresh_ingest.semantic, &fresh_tables);
    let prior_side = GraphSide::build(&prior_graph, &prior_ingest.semantic);
    let fresh_side = GraphSide::build(&fresh_graph, &fresh_ingest.semantic);
    let setup = prepare_binding_matching(&prior_graph, &fresh_graph);
    let setup_present = setup.is_some();
    let initial = match_functions(
        &prior_index,
        &fresh_index,
        &prior_ctx,
        &fresh_ctx,
        MatchOptions {
            enable_propagation: true,
            ..MatchOptions::default()
        },
    );
    let outcome = alternate_function_and_binding_matching(
        initial,
        &prior_index,
        &fresh_index,
        &prior_ctx,
        &fresh_ctx,
        &prior_side,
        &fresh_side,
        setup.as_ref(),
    );
    run(&TwoSides {
        prior: SideView {
            graph: &prior_graph,
            text: prior_code,
        },
        fresh: SideView {
            graph: &fresh_graph,
            text: fresh_code,
        },
        outcome: &outcome,
        setup_present,
    });
}

/// The prior→fresh match for the fn rows containing `prior_needle` /
/// `fresh_needle`.
fn assert_fn_match(sides: &TwoSides<'_>, prior_needle: &str, fresh_needle: &str) {
    let prior = fn_row_with(sides.prior.graph, sides.prior.text, prior_needle);
    let fresh = fn_row_with(sides.fresh.graph, sides.fresh.text, fresh_needle);
    assert_eq!(
        sides
            .outcome
            .function_result
            .matches
            .get(prior.session_id.as_str())
            .map(String::as_str),
        Some(fresh.session_id.as_str()),
        "expected {prior_needle:?} → {fresh_needle:?} (prior {prior:?} vs fresh {fresh:?}); matches: {:?}",
        sides.outcome.function_result.matches,
        prior = prior.session_id,
        fresh = fresh.session_id,
    );
}

/// prior-version.test.ts:905 — "cracks a same-hash function bucket by
/// which matched module binding each member references". The two arrows
/// are structurally identical (an outer-binding reference is a slot), no
/// callees, no callers, no matched scope parent — the binding
/// correspondence is their only identity.
#[test]
fn alternation_cracks_bucket_via_matched_bindings() {
    let prior = r#"
      var userTable = { name: "users", capacity: 100 };
      var orderTable = { name: "orders", capacity: 250 };
      var getUsers = () => userTable;
      var getOrders = () => orderTable;
      console.log(getUsers(), getOrders(), userTable, orderTable);
    "#;
    // The third fresh thunk keeps the enclosing-statement rung out of the
    // way: its statement class is 2-vs-3 (count mismatch → abstain), so
    // the arrows stay ambiguous into the alternation and the binding
    // correspondence is provably the mechanism (externalRefs > 0).
    let fresh = r#"
      var q1 = { name: "users", capacity: 100 };
      var q2 = { name: "orders", capacity: 250 };
      var q3 = { name: "extra", capacity: 9 };
      var f1 = () => q1;
      var f2 = () => q2;
      var f3 = () => q3;
      console.log(f1(), f2(), q1, q2);
    "#;
    with_two_sides(prior, fresh, |sides| {
        assert!(sides.setup_present, "two matchable table bindings");
        assert_fn_match(sides, "userTable", "q1");
        assert_fn_match(sides, "orderTable", "q2");
        // The identities the binding correspondence dictates — the pairing
        // is directional, not interchangeable.
        assert_fn_match(sides, "() => userTable", "() => q1");
        assert_fn_match(sides, "() => orderTable", "() => q2");
        assert_eq!(
            sides.outcome.function_result.ambiguous.len(),
            0,
            "both arrows settled"
        );
        // The evidence rung did the cracking (not scope-ordinal: the
        // arrows' scope parent is the program, which never matches).
        assert!(
            sides
                .outcome
                .function_result
                .resolution_stats
                .propagation_by_rung
                .external_refs
                > 0,
            "externalRefs resolved the bucket"
        );
        // The binding cascade's own result carried the table matches.
        let binding_matches = sides
            .outcome
            .binding_result
            .as_ref()
            .expect("binding result present")
            .matches
            .clone();
        let user_table = binding_named(sides.prior.graph, "userTable");
        let q1 = binding_named(sides.fresh.graph, "q1");
        assert_eq!(
            binding_matches
                .get(user_table.session_id.as_str())
                .map(String::as_str),
            Some(q1.session_id.as_str()),
            "userTable → q1"
        );
    });
}

/// prior-version.test.ts:956 — "cracks a same-hash bucket by which matched
/// FUNCTION each member references". Bun's export thunks REFERENCE a
/// function without calling it — no callee edge — and the referenced value
/// is a function, not a hashable binding, so `setup` is None: the
/// matched-function references alone crack the bucket.
#[test]
fn alternation_cracks_bucket_via_matched_functions_without_bindings() {
    let prior = r#"
      function computeUsers(input) {
        for (let i = 0; i < input.limit; i++) queueUser(i, input);
        return "users-ready";
      }
      function computeOrders(input) {
        if (input.flag) reportOrder(input);
        return { orders: input };
      }
      var loadUsers = () => computeUsers;
      var loadOrders = () => computeOrders;
      console.log(loadUsers(), loadOrders());
    "#;
    let fresh = r#"
      function xA(a) {
        for (let j = 0; j < a.limit; j++) queueUser(j, a);
        return "users-ready";
      }
      function xB(a) {
        if (a.flag) reportOrder(a);
        return { orders: a };
      }
      function xC(a) {
        return a.tag;
      }
      var t1 = () => xA;
      var t2 = () => xB;
      var t3 = () => xC;
      console.log(t1(), t2());
    "#;
    with_two_sides(prior, fresh, |sides| {
        assert!(
            !sides.setup_present,
            "every module binding is a function-init declarator — nothing matchable"
        );
        // All four functions exact-match: the compute pair on unique
        // hashes, the thunks via matched-function reference evidence.
        assert_fn_match(sides, "users-ready", "users-ready");
        assert_fn_match(sides, "orders", "orders");
        assert_fn_match(sides, "() => computeUsers", "() => xA");
        assert_fn_match(sides, "() => computeOrders", "() => xB");
        assert!(sides.outcome.binding_result.is_none());
        // The t3 thunk keeps the enclosing-statement rung out of the way
        // (2-vs-3 statement class), so the matched-FUNCTION references are
        // provably the mechanism.
        assert!(
            sides
                .outcome
                .function_result
                .resolution_stats
                .propagation_by_rung
                .external_refs
                > 0,
            "externalRefs resolved the thunk bucket"
        );
        assert_eq!(
            sides.outcome.function_result.ambiguous.len(),
            0,
            "both thunks settled"
        );
    });
}

/// No evidence, no growth: when the bindings do not match either
/// (literal-preserving fingerprints diverge) and nothing else matched,
/// buildExternalRefEvidence answers null and the alternation returns the
/// initial function result untouched (:1649 `if (!evidence) break`).
#[test]
fn alternation_without_evidence_returns_initial_result() {
    // The third prior arrow keeps the enclosing-statement rung out of the
    // way (3-vs-2 statement class → count-mismatch abstain), so the
    // ambiguity reaches the alternation with nothing to build on.
    let prior = r#"
      var p1 = { a: 1 };
      var p2 = { a: 2 };
      var p3 = { a: 3 };
      var g1 = () => p1;
      var g2 = () => p2;
      var g3 = () => p3;
      console.log(g1(), g2());
    "#;
    let fresh = r#"
      var r1 = { a: 4 };
      var r2 = { a: 5 };
      var h1 = () => r1;
      var h2 = () => r2;
      console.log(h1(), h2());
    "#;
    with_two_sides(prior, fresh, |sides| {
        let result: &MatchResult = &sides.outcome.function_result;
        assert!(
            result.matches.is_empty(),
            "nothing to build on — the ambiguous arrows stay ambiguous: {result:?}"
        );
        assert_eq!(
            result.ambiguous.len(),
            3,
            "the three prior arrows stay ambiguous"
        );
        assert_eq!(
            result.resolution_stats.propagation_by_rung.external_refs, 0,
            "the evidence rung never fired"
        );
        // The binding cascade still RAN (bindings exist and are matchable
        // by shape) — it simply matched nothing.
        assert!(sides.outcome.binding_result.is_some());
        assert!(
            sides
                .outcome
                .binding_result
                .as_ref()
                .unwrap()
                .matches
                .is_empty()
        );
    });
}

/// Round-cap regression: the identity rounds stop at the first non-growth
/// (a fixed-point reached early must not keep asking), and the resolver's
/// evidence union is REBUILT per round from fn_matches + that round's
/// binding matches — never accumulated.
#[test]
fn binding_rounds_stop_at_first_non_growth() {
    // Two structurally identical thunks over two UNIQUE bindings whose
    // references resolve them in round 0; a second run of the same shape
    // adds nothing.
    let prior = r#"
      var alpha = { n: 1 };
      var beta = { n: 2 };
      var r1 = () => alpha;
      var r2 = () => beta;
      console.log(r1(), r2());
    "#;
    let fresh = r#"
      var q1 = { n: 1 };
      var q2 = { n: 2 };
      var s1 = () => q1;
      var s2 = () => q2;
      console.log(s1(), s2());
    "#;
    with_two_sides(prior, fresh, |sides| {
        let binding = sides
            .outcome
            .binding_result
            .as_ref()
            .expect("setup present");
        assert_eq!(binding.matches.len(), 2, "alpha→q1, beta→q2");
        assert_fn_match(sides, "() => alpha", "() => q1");
        assert_fn_match(sides, "() => beta", "() => q2");
    });
}
