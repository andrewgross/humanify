//! The one graph's edge vocabulary: kinds, region scope, and the code
//! graph assembled from both builders over the cutter's pieces.

use oxc_allocator::Allocator;
use oxc_span::Span;

use crate::cut::units::BundlerFacts;
use crate::cut::{Cut, CutInput, PieceKind};
use crate::graph::edges::{CodeGraph, Edge, EdgeKind, EdgeScope, Regions, Scope};
use crate::ingest::Ingest;

#[test]
fn an_edge_scope_reads_relative_to_a_region() {
    let regions = Regions::new(vec![(Span::new(10, 20), 0), (Span::new(30, 40), 1)]);
    let inside = regions.scope(Span::new(11, 12), Span::new(15, 16));
    assert_eq!(inside.relative_to(0), Some(Scope::Inside));
    assert_eq!(inside.relative_to(1), None);
    let into = regions.scope(Span::new(0, 5), Span::new(15, 16));
    assert_eq!(into.relative_to(0), Some(Scope::Into));
    let across = regions.scope(Span::new(11, 12), Span::new(31, 32));
    assert_eq!(across.relative_to(0), Some(Scope::OutOf));
    assert_eq!(across.relative_to(1), Some(Scope::Into));
    let app = regions.scope(Span::new(0, 5), Span::new(50, 60));
    assert_eq!(
        app,
        EdgeScope {
            from: None,
            to: None
        }
    );
}

#[test]
fn the_innermost_region_holds_a_span() {
    let regions = Regions::new(vec![(Span::new(0, 100), 7), (Span::new(10, 20), 8)]);
    assert_eq!(regions.region_of(Span::new(12, 13)), Some(8));
    assert_eq!(regions.region_of(Span::new(30, 31)), Some(7));
    assert_eq!(regions.region_of(Span::new(15, 25)), Some(7));
    assert_eq!(regions.region_of(Span::new(100, 101)), None);
}

#[test]
fn an_init_time_mutation_is_its_own_kind() {
    let code = "var N = {};\nN.extra = 1;\nfunction g() { N.later = 2; }\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    let g = crate::graph::references::build_reference_graph(
        code,
        ingest.semantic(),
        &ingest.program.body,
        &[],
        &BundlerFacts::default(),
    );
    let kinds: Vec<(u32, u32, EdgeKind)> = g.edges.iter().map(|e| (e.from, e.to, e.kind)).collect();
    assert!(kinds.contains(&(1, 0, EdgeKind::InitMutation)));
    assert!(kinds.contains(&(0, 1, EdgeKind::InitMutation)));
    assert!(
        kinds.contains(&(2, 0, EdgeKind::Read)),
        "run-time configuration reads"
    );
    assert!(g.edges.iter().all(|e| e.scope.is_none()), "no regions yet");
}

#[test]
fn a_reference_graph_takes_scopes_once_regions_are_known() {
    let code = "var a = 1;\nvar b = a;\nvar c = b;\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    let mut g = crate::graph::references::build_reference_graph(
        code,
        ingest.semantic(),
        &ingest.program.body,
        &[],
        &BundlerFacts::default(),
    );
    // a and b are one library; c is app code.
    g.assign_scopes(&Regions::new(vec![
        (g.units[0].span, 0),
        (g.units[1].span, 0),
    ]));
    let scope_of = |from: u32, to: u32| {
        g.edges
            .iter()
            .find(|e| e.from == from && e.to == to)
            .and_then(|e| e.scope)
            .and_then(|s| s.relative_to(0))
    };
    assert_eq!(scope_of(1, 0), Some(Scope::Inside));
    assert_eq!(scope_of(2, 1), Some(Scope::Into));
}

#[test]
fn the_code_graph_carries_both_builders_edges_over_the_cut() {
    let code = "var t = 1;\nfunction g() { return t; }\nfunction f() { return g(); }\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    let semantic = ingest.semantic();
    let facts = BundlerFacts::default();
    let cut = Cut::new(&CutInput {
        semantic,
        program: ingest.program,
        wrapper: None,
        factories: &[],
        facts: &facts,
        factory_bodies: false,
    });
    let unified = crate::graph::build_unified_graph(
        semantic,
        ingest.program,
        "input.js",
        crate::rename::eligibility::NeverRename::UNIVERSAL,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    );
    let references = crate::graph::references::build_reference_graph(
        code,
        semantic,
        &ingest.program.body,
        &[],
        &facts,
    );
    let graph = CodeGraph::assemble(cut, Some(&unified), Some(&references));
    let text = |i: u32| {
        let s = graph.pieces[i as usize].span;
        &code[s.start as usize..s.end as usize]
    };
    let rows: Vec<(PieceKind, &str, PieceKind, &str, EdgeKind)> = graph
        .edges
        .iter()
        .map(|e: &Edge| {
            (
                graph.pieces[e.from as usize].kind,
                text(e.from),
                graph.pieces[e.to as usize].kind,
                text(e.to),
                e.kind,
            )
        })
        .collect();
    let f = "function f() { return g(); }";
    let g = "function g() { return t; }";
    // The function graph's call edge, between function pieces.
    assert!(rows.contains(&(
        PieceKind::Function,
        f,
        PieceKind::Function,
        g,
        EdgeKind::Call
    )));
    // The binding's caller edge: g reads t.
    assert!(rows.contains(&(
        PieceKind::Function,
        g,
        PieceKind::Binding,
        "t",
        EdgeKind::Read
    )));
    // The reference graph's statement edge.
    assert!(rows.contains(&(
        PieceKind::Statement,
        f,
        PieceKind::Statement,
        g,
        EdgeKind::Read
    )));
}
