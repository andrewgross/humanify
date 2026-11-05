//! The one cutter: every piece kind with its span and its parent.

use oxc_allocator::Allocator;
use oxc_span::Span;

use crate::cut::units::BundlerFacts;
use crate::cut::{Cut, CutInput, PieceKind, enclosing_statement};
use crate::ingest::Ingest;

fn cut_of(code: &str, facts: &BundlerFacts, factories: &[&str], factory_bodies: bool) -> Cut {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let spans: Vec<Span> = factories
        .iter()
        .map(|f| {
            let at = code.find(f).expect("factory text") as u32;
            Span::new(at, at + f.len() as u32)
        })
        .collect();
    Cut::new(&CutInput {
        semantic: ingest.semantic(),
        program: ingest.program,
        wrapper: None,
        factories: &spans,
        facts,
        factory_bodies,
    })
}

/// The pieces as (kind, text, parent's text) rows, in cut order.
fn rows<'c>(cut: &Cut, code: &'c str) -> Vec<(PieceKind, &'c str, Option<&'c str>)> {
    let text = |s: Span| &code[s.start as usize..s.end as usize];
    cut.pieces
        .iter()
        .map(|p| {
            (
                p.kind,
                text(p.span),
                p.parent.map(|q| text(cut.pieces[q as usize].span)),
            )
        })
        .collect()
}

#[test]
fn every_piece_kind_has_a_span_and_a_parent() {
    let code = "var a = 1, b = function () { return a; };\nfunction f(x) { return b(x); }\nf(1);\n";
    let cut = cut_of(code, &BundlerFacts::default(), &[], false);
    let module = code;
    let fexpr = "function () { return a; }";
    let fdecl = "function f(x) { return b(x); }";
    assert_eq!(
        rows(&cut, code),
        vec![
            (PieceKind::Module, module, None),
            (PieceKind::Declarator, "a = 1", Some(module)),
            (
                PieceKind::Declarator,
                "b = function () { return a; }",
                Some(module)
            ),
            (PieceKind::Statement, fdecl, Some(module)),
            (PieceKind::Statement, "f(1);", Some(module)),
            (
                PieceKind::Function,
                fexpr,
                Some("b = function () { return a; }")
            ),
            (PieceKind::Function, fdecl, Some(fdecl)),
            (PieceKind::Binding, "a", Some("a = 1")),
            (
                PieceKind::Binding,
                "b",
                Some("b = function () { return a; }")
            ),
            (PieceKind::Binding, "f", Some(fdecl)),
        ]
    );
}

#[test]
fn a_nested_function_is_parented_by_its_enclosing_function() {
    let code = "function outer() { return () => 1; }\n";
    let cut = cut_of(code, &BundlerFacts::default(), &[], false);
    let r = rows(&cut, code);
    assert!(r.contains(&(PieceKind::Function, "() => 1", Some(code.trim_end()))));
}

#[test]
fn lazy_bodies_and_factory_bodies_are_children_of_their_container() {
    let code = "var I = lazy(() => { x = 1; });\nvar x;\nvar M = wrap((e, t) => { t.y = 2; });\n";
    let facts = BundlerFacts {
        lazy_init: ["lazy".to_string()].into(),
        ..BundlerFacts::default()
    };
    let factory = "M = wrap((e, t) => { t.y = 2; })";
    let with_bodies = rows(&cut_of(code, &facts, &[factory], true), code);
    let lazy = "var I = lazy(() => { x = 1; });";
    assert!(with_bodies.contains(&(PieceKind::Statement, "x = 1;", Some(lazy))));
    assert!(with_bodies.contains(&(
        PieceKind::Statement,
        "t.y = 2;",
        Some("var M = wrap((e, t) => { t.y = 2; });")
    )));
    // Without factory bodies a bundled module is one piece.
    let without = rows(&cut_of(code, &facts, &[factory], false), code);
    assert!(!without.iter().any(|r| r.1 == "t.y = 2;"));
}

#[test]
fn the_enclosing_statement_is_the_one_containing_the_span() {
    let spans = [Span::new(0, 10), Span::new(10, 20), Span::new(25, 30)];
    assert_eq!(enclosing_statement(&spans, Span::new(12, 15)), Some(1));
    assert_eq!(enclosing_statement(&spans, Span::new(10, 20)), Some(1));
    assert_eq!(enclosing_statement(&spans, Span::new(21, 22)), None);
    assert_eq!(enclosing_statement(&spans, Span::new(5, 12)), None);
}
