//! The statement reference graph (exp095 library discovery,
//! docs/design/library-discovery.md §4): what each top-level statement of
//! the app code references, at STATEMENT granularity, over every resolved
//! binding — one of the two builders of the ONE graph
//! ([`super::edges::CodeGraph`]), beside the function graph's call-level
//! view ([`super::build_function_graph`]). Both resolve bindings through
//! the same oxc `Semantic`, and both emit the one edge vocabulary
//! ([`super::edges::Edge`]).
//!
//! **Nodes** are the cutter's statement-level units
//! ([`crate::cut::units::container_units`], without factory bodies: a
//! bundled module is one external unit here).
//!
//! **Edges** u → v, for every binding scope analysis resolves (design §4):
//!
//! - a READ in `u` points at the declaring unit AND at every unit that
//!   writes the binding (a lazy module's body statement `x = …` is the real
//!   definer of its hoisted `var x`);
//! - a WRITE links the writer and the declarer both ways;
//! - a DEFINITIONAL mutation that runs at module-init time (not inside a
//!   function of its unit) is a write of its root binding — kind
//!   [`EdgeKind::InitMutation`]: `x.a = …`, `Object.defineProperty /
//!   defineProperties / assign(x, …)`, and a call to one of the bundler's
//!   namespace-filling helpers with `x` first. The same mutation inside a
//!   function is run-time configuration — a read. A pair a plain write
//!   already links stays [`EdgeKind::Write`] (one edge per pair and class:
//!   the edge count discovery reports is the write class's);
//! - a lazy wrapper points at each of its body statements (calling the init
//!   runs them) — [`EdgeKind::LazyWrapper`].
//!
//! An unresolved name (a global) makes no edge. Every edge's region scope
//! is unset until regions are known ([`ReferenceGraph::assign_scopes`];
//! library discovery is the producer).

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, Statement};
use oxc_semantic::{Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use super::edges::{Edge, EdgeKind, Regions};
use crate::babel_view::unparen;
use crate::cut::units::{
    BundlerFacts, ContainerUnits, Unit, UnitKind, UnitLocator, container_units,
};

/// The statement reference graph.
#[derive(Clone, Debug, Default)]
pub struct ReferenceGraph {
    pub units: Vec<Unit>,
    /// Every edge, sorted by (from, to, kind) and deduplicated.
    pub edges: Vec<Edge>,
    /// Per unit, its successors (sorted, deduplicated over kinds).
    pub out: Vec<Vec<u32>>,
    /// Per unit, its predecessors.
    pub inn: Vec<Vec<u32>>,
    locator: UnitLocator,
}

impl ReferenceGraph {
    /// The innermost unit holding byte `pos`.
    pub fn unit_at(&self, pos: u32) -> Option<u32> {
        self.locator.unit_at(&self.units, pos)
    }

    /// The external base: runtime helpers and bundled CommonJS modules.
    pub fn is_base_external(&self, u: u32) -> bool {
        let unit = &self.units[u as usize];
        unit.runtime || unit.kind == UnitKind::Factory
    }

    /// Give every edge its region scope (docs/matching-architecture.md
    /// §4), from the regions' spans. Nothing reads the scope yet: step 4's
    /// library rules will (never an INTO edge as identity evidence).
    pub fn assign_scopes(&mut self, regions: &Regions) {
        let units = &self.units;
        super::edges::assign_scopes(&mut self.edges, |u| units[u as usize].span, regions);
    }
}

/// The identifier at the root of a member chain (`x` in `x.a.b`).
fn chain_root<'a>(mut e: &'a Expression<'a>) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
    loop {
        match unparen(e) {
            Expression::Identifier(id) => return Some(id),
            other => e = other.as_member_expression()?.object(),
        }
    }
}

/// `Object.defineProperty / defineProperties / assign` on the global `Object`.
fn is_object_definer(semantic: &Semantic<'_>, callee: &Expression<'_>) -> bool {
    let Some(m) = unparen(callee).as_member_expression() else {
        return false;
    };
    let Expression::Identifier(object) = unparen(m.object()) else {
        return false;
    };
    object.name == "Object"
        && resolved(semantic, object).is_none()
        && matches!(
            m.static_property_name(),
            Some("defineProperty" | "defineProperties" | "assign")
        )
}

fn resolved(
    semantic: &Semantic<'_>,
    id: &oxc_ast::ast::IdentifierReference<'_>,
) -> Option<SymbolId> {
    let r = id.reference_id.get()?;
    semantic.scoping().get_reference(r).symbol_id()
}

/// The binding a node DEFINES by mutation, when it is a definitional
/// mutation: a member assignment's root, or the first argument of a
/// namespace filler / an `Object` definer.
fn mutated_binding<'a>(
    semantic: &Semantic<'a>,
    kind: &AstKind<'a>,
    facts: &BundlerFacts,
) -> Option<SymbolId> {
    let target = match kind {
        AstKind::AssignmentExpression(a) => chain_root(a.left.as_member_expression()?.object())?,
        AstKind::CallExpression(c) => {
            let filler = matches!(unparen(&c.callee), Expression::Identifier(id)
                if facts.namespace_fillers.contains(id.name.as_str()));
            if !filler && !is_object_definer(semantic, &c.callee) {
                return None;
            }
            match c.arguments.first()?.as_expression().map(unparen)? {
                Expression::Identifier(id) => id,
                _ => return None,
            }
        }
        _ => return None,
    };
    resolved(semantic, target)
}

/// Definitional member mutations: binding → the units whose INIT-TIME code
/// mutates it (inside a function of the unit it is run-time use, a read).
fn init_time_mutations(
    semantic: &Semantic<'_>,
    graph: &ReferenceGraph,
    facts: &BundlerFacts,
) -> HashMap<SymbolId, Vec<u32>> {
    let nodes = semantic.nodes();
    let mut out: HashMap<SymbolId, Vec<u32>> = HashMap::new();
    for node in nodes.iter() {
        let Some(symbol) = mutated_binding(semantic, &node.kind(), facts) else {
            continue;
        };
        let Some(u) = graph.unit_at(node.span().start) else {
            continue;
        };
        let start = graph.units[u as usize].span.start;
        let nested = nodes
            .ancestors(node.id())
            .take_while(|a| a.span().start >= start)
            .any(|a| {
                matches!(
                    a.kind(),
                    AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
                )
            });
        if !nested {
            out.entry(symbol).or_default().push(u);
        }
    }
    out
}

/// Every resolved binding's edges (module doc).
fn binding_edges(
    semantic: &Semantic<'_>,
    graph: &ReferenceGraph,
    mutations: &HashMap<SymbolId, Vec<u32>>,
) -> Vec<Edge> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut edges = Vec::new();
    for symbol in scoping.symbol_ids() {
        let Some(decl) = graph.unit_at(scoping.symbol_span(symbol).start) else {
            continue;
        };
        let mut readers = Vec::new();
        let mut writers = Vec::new();
        for &rid in scoping.get_resolved_reference_ids(symbol) {
            let reference = scoping.get_reference(rid);
            let Some(at) = graph.unit_at(nodes.get_node(reference.node_id()).span().start) else {
                continue;
            };
            if reference.flags().is_write() {
                writers.push(at);
            }
            if reference.flags().is_read() {
                readers.push(at);
            }
        }
        writers.sort_unstable();
        writers.dedup();
        // A unit that only mutates at init time defines by mutation.
        let plain = writers.clone();
        if let Some(m) = mutations.get(&symbol) {
            writers.extend(m.iter().copied());
        }
        writers.sort_unstable();
        writers.dedup();
        readers.sort_unstable();
        readers.dedup();
        for &w in writers.iter().filter(|&&w| w != decl) {
            let kind = if plain.binary_search(&w).is_ok() {
                EdgeKind::Write
            } else {
                EdgeKind::InitMutation
            };
            edges.push(Edge::new(w, decl, kind));
            edges.push(Edge::new(decl, w, kind));
        }
        for &r in &readers {
            for &o in std::iter::once(&decl).chain(&writers) {
                if r != o {
                    edges.push(Edge::new(r, o, EdgeKind::Read));
                }
            }
        }
    }
    edges
}

/// Mark the runtime: the units declaring a runtime helper, and everything
/// they reach through plain statements.
fn mark_runtime(graph: &mut ReferenceGraph, seeds: impl Iterator<Item = u32>) {
    let mut stack: Vec<u32> = seeds.collect();
    while let Some(u) = stack.pop() {
        let unit = &mut graph.units[u as usize];
        if unit.runtime || unit.kind != UnitKind::Statement {
            continue;
        }
        unit.runtime = true;
        stack.extend(graph.out[u as usize].iter().copied());
    }
}

/// Build the graph over a container's statements. `factories` are the
/// declarator spans of the bundled CommonJS modules (the unpack's
/// classification).
pub fn build_reference_graph<'a>(
    _code: &str,
    semantic: &Semantic<'a>,
    statements: &'a [Statement<'a>],
    factories: &[Span],
    facts: &BundlerFacts,
) -> ReferenceGraph {
    let ContainerUnits {
        units, declared, ..
    } = container_units(statements, factories, facts, false);
    let mut graph = ReferenceGraph {
        locator: UnitLocator::new(&units),
        units,
        ..ReferenceGraph::default()
    };
    let mutations = init_time_mutations(semantic, &graph, facts);
    let mut edges = binding_edges(semantic, &graph, &mutations);
    for (u, unit) in graph.units.iter().enumerate() {
        if let UnitKind::LazyBody { module } = unit.kind {
            edges.push(Edge::new(module, u as u32, EdgeKind::LazyWrapper));
        }
    }
    edges.sort_unstable();
    // One edge per pair and class: a definition by mutation the plain
    // write already links is that write (Write sorts first).
    edges.dedup_by(|b, a| a.from == b.from && a.to == b.to && a.kind.class() == b.kind.class());
    let n = graph.units.len();
    graph.out = vec![Vec::new(); n];
    graph.inn = vec![Vec::new(); n];
    for e in &edges {
        graph.out[e.from as usize].push(e.to);
        graph.inn[e.to as usize].push(e.from);
    }
    for list in graph.out.iter_mut().chain(graph.inn.iter_mut()) {
        list.sort_unstable();
        list.dedup();
    }
    graph.edges = edges;
    let seeds: Vec<u32> = facts
        .runtime
        .iter()
        .filter_map(|name| declared.get(name).copied())
        .collect();
    mark_runtime(&mut graph, seeds.into_iter());
    graph
}
