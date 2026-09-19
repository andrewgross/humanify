//! Graph unit tests (WP1.4): the three passes over small programs.

use oxc_allocator::Allocator;

use crate::graph::build_function_graph;
use crate::ingest::Ingest;

fn graph_of(code: &str) -> (Allocator, crate::graph::FunctionGraph) {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let graph = build_function_graph(&ingest.semantic, "input.js", &[]);
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
    let tables = SymbolTables::build(&ingest.semantic);
    let wrapper = find_wrapper_function(ingest.program, &ingest.semantic);
    let classification = classify_bun_modules(
        src,
        ingest.program,
        &ingest.semantic,
        wrapper.as_ref().map(|w| w.body_span),
        &tables,
    )
    .expect("helper present");
    assert_eq!(classification.factories.len(), 1);

    let graph = build_function_graph(&ingest.semantic, "input.js", &classification.factories);
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
