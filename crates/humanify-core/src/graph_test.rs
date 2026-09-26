//! Graph unit tests (WP1.4): the three passes over small programs.

use oxc_allocator::Allocator;

use crate::graph::build_function_graph;
use crate::ingest::Ingest;

fn graph_of(code: &str) -> (Allocator, crate::graph::FunctionGraph) {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let (graph, _symbols) = build_function_graph(ingest.semantic(), "input.js", &[]);
    (allocator, graph)
}

#[test]
fn graph_finds_declaration_and_assigned_functions() {
    // The real shape: calls live inside the wrapper IIFE, and the wrapper
    // is their caller (babel's analyzeCallees runs per function node).
    let (_a, g) = graph_of(
        "(function(){\nfunction declared() { return 1; }\nvar assigned = () => { return 2; };\ndeclared();\nassigned();\n})();\n",
    );
    assert_eq!(g.functions.len(), 3, "wrapper + declaration + arrow found");
    // The wrapper edges both callees.
    let wrapper = g
        .functions
        .iter()
        .find(|f| f.span.end > 100)
        .expect("the wrapper is the biggest span");
    assert_eq!(
        wrapper.internal_callees.len(),
        2,
        "wrapper edges both callees"
    );
}

#[test]
fn graph_records_member_calls_as_external() {
    let (_a, g) = graph_of("function f() { obj.method(); other[\"prop\"](); }\nf();\n");
    let f = &g.functions[0];
    assert!(
        f.external_callees.contains("method") && f.external_callees.contains("prop"),
        "external callees carry method names: {:?}",
        f.external_callees
    );
    assert!(f.internal_callees.is_empty());
}

#[test]
fn graph_scope_parents_nest() {
    let (_a, g) =
        graph_of("function outer() { function inner() { return 1; } inner(); }\nouter();\n");
    assert_eq!(g.functions.len(), 2);
    let inner = g
        .functions
        .iter()
        .find(|f| f.name == "inner")
        .expect("inner found");
    let outer = g
        .functions
        .iter()
        .find(|f| f.name == "outer")
        .expect("outer found");
    assert_eq!(inner.scope_parent, Some(outer.span), "nested -> parent");
    assert_eq!(outer.scope_parent, None);
}

#[test]
fn graph_hashes_are_rename_invariant() {
    let (_a, g1) = graph_of("function f(x) { return x + 1; }\nf(2);\n");
    let (_b, g2) = graph_of("function g(y) { return y + 1; }\ng(2);\n");
    assert_eq!(
        g1.functions[0].structural_hash, g2.functions[0].structural_hash,
        "renaming a function + its param does not move the hash"
    );
}

/// The third-party skip: functions inside a classified factory body never
/// enter the graph, and calls INTO them resolve to no edge (the oracle's
/// member set — the WP1.4 one-edge divergence this closes).
#[test]
fn graph_skips_factory_body_functions() {
    use crate::hash::serialize::SymbolTables;
    use crate::modules::{classify_bun_modules, wrapper::find_wrapper_function};

    let src = "var d=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports),tO8=d((q,m)=>{var helper=(x)=>x*2; return helper(3);}); var caller=()=>tO8(1);";
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, src, "input.js");
    assert!(ingest.errors.is_empty());
    let tables = SymbolTables::build(ingest.semantic());
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic());
    let classification = classify_bun_modules(
        src,
        ingest.program,
        ingest.semantic(),
        wrapper.as_ref().map(|w| w.body_span),
        &tables,
    )
    .expect("helper present");
    assert_eq!(classification.factories.len(), 1);

    let (graph, _symbols) =
        build_function_graph(ingest.semantic(), "input.js", &classification.factories);
    // Everything inside the factory body [body_span] is out: the factory
    // arrow itself and `helper`. The HELPER DEFINITION's two arrows (the
    // `var d=(I,A)=>()=>…` — outside any factory body) and `caller`
    // remain.
    let body = classification.factories[0].body_span;
    let inside: Vec<_> = graph
        .functions
        .iter()
        .filter(|f| f.span.start >= body.start && f.span.end <= body.end)
        .collect();
    assert!(inside.is_empty(), "factory-body functions must be skipped");
    assert_eq!(graph.functions.len(), 3);
    // And the call into tO8 resolved to NO edge: the factory arrow was
    // skipped, so the declarator's binding maps to nothing.
    let caller = graph
        .functions
        .iter()
        .find(|f| f.span.start == 129)
        .expect("caller present");
    assert!(caller.internal_callees.is_empty());
}

/// The module-binding half: rows, the three skips, and both edge kinds
/// (a function-holding module binding earns BOTH the mb→mb and the
/// mb→function edge — the TS runs edge builders 4a and 4b over the same
/// initializer subtree).
#[test]
fn module_bindings_rows_and_edges() {
    use crate::graph::build_unified_graph;
    let src = "var f = () => 1; var g = f; var h = () => f(); var obj = {}; var declared = function named() {}; function declFn() {} var skipnamed = function named2() {};";
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, src, "input.js");
    assert!(ingest.errors.is_empty());
    let graph = build_unified_graph(
        ingest.semantic(),
        ingest.program,
        "input.js",
        &[],
        None,
        None,
    );
    let mb: Vec<_> = graph
        .module_bindings
        .iter()
        .map(|m| (m.name.as_str(), m.internal_callees.len()))
        .collect();
    // f (arrow init, unnamed) IS an mb; g, h, obj are; `declared` (named
    // fn-expr init) is SKIPPED; declFn (function declaration) is SKIPPED;
    // skipnamed (named fn-expr init) is SKIPPED.
    assert_eq!(
        mb.iter().map(|x| x.0).collect::<Vec<_>>(),
        vec!["f", "g", "h", "obj"],
        "bindings: {mb:?}"
    );
    let by_name = |n: &str| graph.module_bindings.iter().find(|m| m.name == n).expect(n);
    let f = by_name("f");
    let g = by_name("g");
    let h = by_name("h");
    // g = f: the mb edge (f's identifier) + the fn edge (f's arrow).
    assert_eq!(
        g.internal_callees.len(),
        2,
        "g's deps: {:?}",
        g.internal_callees
    );
    // h's init subtree references f → same two edges.
    assert_eq!(h.internal_callees.len(), 2);
    // obj has an empty init — no edges.
    assert_eq!(by_name("obj").internal_callees.len(), 0);
    // f's own row: no refs in its own init besides nothing — 0 edges.
    assert_eq!(f.internal_callees.len(), 0);
    // sessionIds carry the minified name.
    assert_eq!(f.session_id, "module:f");
    // spans: the mb's key is the DECLARATOR ID's span, not the declarator's.
    let g_span = &src[g.span.start as usize..g.span.end as usize];
    assert_eq!(g_span, "g");
}

/// The 4a key-position edge: a non-computed object KEY named like a module
/// binding edges to it (babel's Identifier visitor sees keys;
/// isBinding's ObjectExpression special case makes keys non-binding).
#[test]
fn mb_edge_from_object_key_position() {
    use crate::graph::build_unified_graph;
    let src = "var all = 1; var Pp = b(() => { var cfg = { all: 5, other: 6 }; });";
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, src, "input.js");
    assert!(ingest.errors.is_empty());
    let graph = build_unified_graph(
        ingest.semantic(),
        ingest.program,
        "input.js",
        &[],
        None,
        None,
    );
    let pp = graph
        .module_bindings
        .iter()
        .find(|m| m.name == "Pp")
        .expect("Pp row");
    assert_eq!(
        pp.internal_callees.len(),
        1,
        "edges: {:?}",
        pp.internal_callees
    );
    let all_row = graph
        .module_bindings
        .iter()
        .find(|m| m.name == "all")
        .expect("all row");
    assert_eq!(pp.internal_callees[0], all_row.span);
}

/// Finding #1: optional calls are call edges. `x?.()` (an optional call)
/// and `a?.b()` (a call inside an optional chain) are calls like any
/// other — dropping them left internalCallees (processing order AND the
/// fingerprint's callee inputs) blind to every `?.` call.
#[test]
fn optional_calls_are_call_edges() {
    let (_a, g) = graph_of(
        "(function(){\nfunction target() { return 1; }\nfunction caller() { target?.(); obj?.method(); obj?.a.deep?.(); }\ncaller();\n})();\n",
    );
    let caller = g
        .functions
        .iter()
        .find(|f| f.name == "caller")
        .expect("caller row");
    let target = g
        .functions
        .iter()
        .find(|f| f.name == "target")
        .expect("target row");
    assert_eq!(
        caller.internal_callees,
        vec![target.span],
        "`target?.()` edges target"
    );
    assert!(
        caller.external_callees.contains("method") && caller.external_callees.contains("deep"),
        "optional-chain member calls carry their names: {:?}",
        caller.external_callees
    );
}

/// Finding #17: which identifiers Babel counts as assignment TARGETS
/// (constant violations) vs references. Only the target slots of an
/// assignment's left are targets — a default value, a computed key, or a
/// member expression's object inside the left are ordinary expressions.
#[test]
fn babel_assignment_target_is_only_the_target_slot() {
    use crate::graph::is_babel_assignment_target;
    let cases: &[(&str, bool)] = &[
        ("var a, b, c; b = c;", true),
        ("var a, b, c; b += c;", true),
        ("var a, b, c; [a, b] = c;", true),
        ("var a, b, c; ({ x: b } = c);", true),
        ("var a, b, c; ({ b } = c);", true),
        ("var a, b, c; [a = 1, ...b] = c;", true),
        ("var a, b, c; [b = 1] = c;", true),
        ("var a, b, c; ({ b = 1 } = c);", true),
        ("var a, b, c; ({ x: b = 1 } = c);", true),
        ("var a, b, c; (b) = c;", true),
        // the default value is evaluated, never assigned (#17's probe)
        ("var a, b, c; [a = b++] = c;", false),
        ("var a, b, c; [a = b] = c;", false),
        ("var a, b, c; ({ a = b } = c);", false),
        ("var a, b, c; ({ x: a = b } = c);", false),
        // a computed key and a member object are read
        ("var a, b, c; ({ [b]: a } = c);", false),
        ("var a, b, c; b.x = c;", false),
        ("var a, b, c; a[b] = c;", false),
        ("var a, b, c; [b.x] = c;", false),
        // the right side, updates, for-of targets
        ("var a, b, c; a = b;", false),
        ("var a, b, c; b++;", false),
        ("var a, b, c; for (b of c);", false),
    ];
    let mut wrong = Vec::new();
    for (code, expected) in cases {
        let allocator = Allocator::default();
        let ingest = Ingest::parse(&allocator, code, "input.js");
        assert!(ingest.errors.is_empty(), "must parse: {code}");
        let nodes = ingest.semantic().nodes();
        let b_ref = nodes
            .iter()
            .find(|n| matches!(n.kind(), oxc_ast::AstKind::IdentifierReference(r) if r.name == "b"))
            .unwrap_or_else(|| panic!("a `b` reference in {code}"));
        if is_babel_assignment_target(nodes, b_ref.id()) != *expected {
            wrong.push(*code);
        }
    }
    assert!(wrong.is_empty(), "wrong verdict for `b` in: {wrong:#?}");
}
