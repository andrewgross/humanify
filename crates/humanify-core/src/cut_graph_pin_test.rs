//! Behaviour pin for the CUTTER and the GRAPH (2026-10-13, the
//! matching-unification refactor, step 3: one cutter and one graph —
//! docs/matching-architecture.md §3.1, §3.3, §4, §7).
//!
//! Step 3 moves no byte. These digests were recorded on main @ 3cb9a9ec,
//! BEFORE the cutting and the edges moved to one owner, and each records
//! one consumer's view of the pieces and the edges over every committed e2e
//! fixture bundle (both versions), through the consumer's real entry point:
//!
//! - `graph`: the unified graph's function rows (span, name binding, scope
//!   parent, internal and external callees) and module-binding rows
//!   (dependency edges, callers, declaration shape, redeclarations), and
//!   the symbol → row map the naming graph shares;
//! - `statements`: the twins' statement inventory (spans, hashes, the
//!   rows' enclosing-statement assignment) and the naming plumbing's
//!   container statements (the lazy-init helpers read over them);
//! - `units`: the library units, with and without factory bodies;
//! - `references`: the statement reference graph (units, runtime marks,
//!   every edge with its kind, both adjacency lists).
//!
//! A digest change is never "update the constant": a consumer stopped
//! seeing the pieces or the edges it saw.

use std::fmt::Write as _;

use oxc_allocator::Allocator;
use oxc_span::Span;

use crate::ingest::Ingest;
use crate::matching_helpers_pin_test::{digest, fixture_pairs};

/// The run's toolchain for one bundle (detection, no overrides).
fn toolchain_of(code: &str) -> crate::toolchain::Toolchain {
    crate::toolchain::resolve_toolchain(&crate::detect::detect_bundle(code), None, None)
}

/// Run one section over every fixture side and assert its digest; a red
/// pin writes the section's lines to `<temp dir>/humanify-cutpin-<section>.txt`.
fn pin(section: &str, expected: &str, mut lines: impl FnMut(&str, &mut String)) {
    let mut out = String::new();
    for (name, a, b) in fixture_pairs() {
        let _ = writeln!(out, "# {name}");
        lines(&a, &mut out);
        lines(&b, &mut out);
    }
    let got = digest(&out);
    if got != expected {
        let _ = std::fs::write(
            std::env::temp_dir().join(format!("humanify-cutpin-{section}.txt")),
            &out,
        );
    }
    assert_eq!(got, expected, "{section}");
}

/// Parse loudly: a text that does not parse (or whose ESTree JSON does not
/// round-trip) would pin an empty view — the silent failure step 2 met on
/// the raw release bundle's lone surrogates.
fn parse_loud<'a>(alloc: &'a Allocator, text: &'a str) -> Ingest<'a> {
    let ingest = Ingest::parse(alloc, text, "input.js");
    assert!(
        ingest.errors.is_empty(),
        "pin input does not parse ({} diagnostics)",
        ingest.errors.len()
    );
    ingest
}

// ---------------------------------------------------------------------------
// The unified graph (function rows + module bindings)
// ---------------------------------------------------------------------------

fn graph_lines(text: &str, out: &mut String) {
    let tc = toolchain_of(text);
    let alloc = Allocator::default();
    let ingest = parse_loud(&alloc, text);
    let graph = crate::graph::build_unified_graph(
        ingest.semantic(),
        ingest.program,
        "input.js",
        crate::rename::eligibility::NeverRename::UNIVERSAL,
        tc.layout.piece,
    );
    for f in &graph.functions {
        let _ = writeln!(
            out,
            "F {} {:?} {:?} {} {:?} {:?} {:?}",
            f.session_id,
            f.span,
            f.name_binding,
            f.name,
            f.scope_parent,
            f.internal_callees,
            f.external_callees
        );
    }
    for b in &graph.module_bindings {
        let _ = writeln!(
            out,
            "B {} {:?} {:?} {:?} {:?} {:?}",
            b.session_id,
            b.span,
            b.internal_callees,
            b.callers,
            b.declarator_init,
            b.redeclared_spans
        );
    }
    let mut by_symbol: Vec<(usize, usize)> = graph
        .function_by_symbol
        .iter()
        .map(|(s, &i)| (s.index(), i))
        .collect();
    by_symbol.sort_unstable();
    let _ = writeln!(out, "S {by_symbol:?}");
}

// ---------------------------------------------------------------------------
// The top-level statements (twins' inventory, the naming plumbing's list)
// ---------------------------------------------------------------------------

fn statement_lines(text: &str, out: &mut String) {
    let tc = toolchain_of(text);
    let alloc = Allocator::default();
    let ingest = parse_loud(&alloc, text);
    let graph = crate::graph::build_unified_graph(
        ingest.semantic(),
        ingest.program,
        "input.js",
        crate::rename::eligibility::NeverRename::UNIVERSAL,
        tc.layout.piece,
    );
    let inventory = crate::twins::statement_inventory(text, "fresh", Some(&graph), tc.layout.piece)
        .expect("inventory");
    for s in &inventory.statements {
        let _ = writeln!(out, "T {:?} {}", s.span, s.hash);
    }
    let mut fns: Vec<_> = inventory.fns_by_statement.iter().collect();
    fns.sort_unstable();
    let mut bindings: Vec<_> = inventory.bindings_by_statement.iter().collect();
    bindings.sort_unstable();
    let _ = writeln!(
        out,
        "A {fns:?} {bindings:?} {} {}",
        inventory.unassigned_functions, inventory.unassigned_bindings
    );
    let json = crate::ingest::program_estree_json(ingest.program);
    let wrapper = tc
        .layout
        .piece
        .find_wrapper(ingest.program, ingest.semantic());
    let mut helpers: Vec<String> = crate::naming::plumbing::lazy_init_helpers_of(
        tc.module_wrappers.piece,
        &json,
        wrapper.map(|w| w.body_span),
    )
    .into_iter()
    .collect();
    helpers.sort();
    let _ = writeln!(out, "L {helpers:?}");
}

// ---------------------------------------------------------------------------
// The library units and the statement reference graph
// ---------------------------------------------------------------------------

/// The container statements, factories and bundler facts of one side —
/// discovery's `bundle_graph` inputs, from the detected toolchain.
fn with_container(
    text: &str,
    run: impl FnOnce(
        &Ingest<'_>,
        &[oxc_ast::ast::Statement<'_>],
        &[Span],
        &crate::graph::references::BundlerFacts,
    ),
) {
    let tc = toolchain_of(text);
    let alloc = Allocator::default();
    let ingest = parse_loud(&alloc, text);
    let semantic = ingest.semantic();
    let wrapper = tc.layout.piece.find_wrapper(ingest.program, semantic);
    let body = wrapper.as_ref().map(|w| w.body_span);
    let statements = crate::modules::container_statements(ingest.program, semantic, body);
    let tables = crate::hash::serialize::SymbolTables::build(semantic);
    let factories: Vec<Span> = tc
        .module_wrappers
        .piece
        .classify_factories(text, ingest.program, semantic, body, &tables)
        .map(|c| c.factories.iter().map(|f| f.span).collect())
        .unwrap_or_default();
    let facts = crate::graph::references::BundlerFacts::recognise(
        text,
        statements,
        tc.module_wrappers.piece,
        tc.interop.piece,
    );
    run(&ingest, statements, &factories, &facts);
}

fn unit_lines(text: &str, out: &mut String) {
    with_container(text, |_, statements, factories, facts| {
        for bodies in [false, true] {
            let units =
                crate::graph::references::container_units(statements, factories, facts, bodies);
            for (u, node) in units.units.iter().zip(&units.nodes) {
                let node = match node {
                    crate::graph::references::UnitNode::Statement(s) => {
                        format!("S{:?}", oxc_span::GetSpan::span(*s))
                    }
                    crate::graph::references::UnitNode::Declarator(d) => format!("D{:?}", d.span),
                    crate::graph::references::UnitNode::Container => "C".to_string(),
                };
                let _ = writeln!(out, "U{} {u:?} {node}", u8::from(bodies));
            }
        }
    });
}

fn reference_lines(text: &str, out: &mut String) {
    with_container(text, |ingest, statements, factories, facts| {
        let g = crate::graph::references::build_reference_graph(
            text,
            ingest.semantic(),
            statements,
            factories,
            facts,
        );
        for u in &g.units {
            let _ = writeln!(out, "U {u:?}");
        }
        // The edge list as discovery reads it: (from, to, kind class). The
        // kind is printed as its CLASS (read / write / body) so a finer
        // kind vocabulary that keeps every class is the same view.
        for &(from, to, kind) in &g.edges {
            let _ = writeln!(out, "E {from} {to} {kind:?}");
        }
        let _ = writeln!(out, "O {:?}", g.out);
        let _ = writeln!(out, "I {:?}", g.inn);
        let probes: Vec<Option<u32>> = g
            .units
            .iter()
            .flat_map(|u| [u.span.start, u.span.end.saturating_sub(1), u.span.end])
            .map(|p| g.unit_at(p))
            .collect();
        let _ = writeln!(out, "P {probes:?}");
    });
}

#[test]
fn graph_rows_and_edges_are_pinned() {
    pin("graph", PIN_GRAPH, graph_lines);
}

#[test]
fn statement_cuts_are_pinned() {
    pin("statements", PIN_STATEMENTS, statement_lines);
}

#[test]
fn library_units_are_pinned() {
    pin("units", PIN_UNITS, unit_lines);
}

#[test]
fn reference_graph_is_pinned() {
    pin("references", PIN_REFERENCES, reference_lines);
}

type Section = fn(&str, &mut String);

/// The one-off FULL-corpus run (docs/matching-architecture.md §7): every
/// section over every `.js` file named (or under each directory named) one
/// per line in `<temp dir>/humanify-cutpin-corpus.txt` — formatted
/// release trees (the RAW bundle's lone-surrogate escapes do not
/// round-trip; [`parse_loud`] and the JSON check below refuse rather than
/// pin an empty view). One digest line per file and section, then one
/// total per section: run on the base commit and on the refactor, diff.
#[test]
#[ignore = "one-off: needs <temp dir>/humanify-cutpin-corpus.txt (real release trees)"]
fn full_corpus_digests() {
    let list = std::env::temp_dir().join("humanify-cutpin-corpus.txt");
    let corpus = std::fs::read_to_string(&list).expect("the corpus list file");
    let sections: [(&str, Section); 4] = [
        ("graph", graph_lines as Section),
        ("statements", statement_lines as Section),
        ("units", unit_lines as Section),
        ("references", reference_lines as Section),
    ];
    let mut files = Vec::new();
    for root in corpus.lines().filter(|l| !l.trim().is_empty()) {
        js_files(std::path::Path::new(root), &mut files);
    }
    files.sort();
    let mut totals: Vec<String> = vec![String::new(); sections.len()];
    for path in &files {
        let text = std::fs::read_to_string(path).expect("corpus file reads");
        {
            let alloc = Allocator::default();
            let ingest = parse_loud(&alloc, &text);
            assert!(
                !crate::ingest::program_estree_json(ingest.program).is_null(),
                "{}: the ESTree JSON does not round-trip (lone surrogates?)",
                path.display()
            );
        }
        for (i, (section, lines)) in sections.iter().enumerate() {
            let mut out = String::new();
            lines(&text, &mut out);
            let d = digest(&out);
            println!("CORPUS {} {section} {d} {}", path.display(), out.len());
            totals[i].push_str(&d);
        }
    }
    for ((section, _), all) in sections.iter().zip(&totals) {
        println!("TOTAL {section} {} over {} files", digest(all), files.len());
    }
}

fn js_files(path: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).into_iter().flatten().flatten() {
            js_files(&entry.path(), out);
        }
    } else if path.extension().is_some_and(|e| e == "js") {
        out.push(path.to_path_buf());
    }
}

const PIN_GRAPH: &str = "ab8d5cb67730e6bad86a481d";
const PIN_STATEMENTS: &str = "fa14dc596ae8ebf706faef73";
const PIN_UNITS: &str = "f9d1da3bb72236b147e6b337";
const PIN_REFERENCES: &str = "86d4340df35917ffceba5c7b";
