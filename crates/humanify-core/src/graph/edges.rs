//! The ONE graph (docs/matching-architecture.md §3.3, §4): one edge
//! vocabulary — kind and region scope — and one structure, [`CodeGraph`],
//! whose nodes are the cutter's pieces ([`crate::cut::Cut`]) and whose
//! edges come from the two builders.
//!
//! **Two builders, kept as named views** (each would change bytes if
//! folded into the other):
//!
//! - the FUNCTION graph ([`super::build_unified_graph`]): call edges
//!   between function rows with babel's recursive-traverse attribution
//!   (EVERY ancestor function of a call accumulates its edge — the
//!   wrapper holds them all), plus the module bindings' TS-ported
//!   dependency edges (4a/4b) and caller edges (4d, over babel's
//!   `referencePaths`, which drops assignment targets). The cascade, the
//!   propagation rungs, the alternation and the naming graph read its rows
//!   (`internal_callees`, `callers`) in their stored order.
//! - the REFERENCE graph ([`super::references::build_reference_graph`]):
//!   statement-granularity read / write / init-mutation / lazy-wrapper
//!   edges over every resolved binding (oxc's reference flags, writes
//!   kept), for library discovery.
//!
//! Same reference facts, different attribution rules and different
//! consumers: unifying the builders would move the cascade's evidence or
//! discovery's fixpoint. Both now speak [`Edge`], and [`CodeGraph`] joins
//! them over one node set, so a rule reads one graph (step 4).
//!
//! **Region scope** (Andrew, 2026-10-13): an edge touching a region (a set
//! of pieces believed to be one library) is INSIDE it, INTO it or OUT OF
//! it. The scope is stored as the two ends' regions ([`EdgeScope`]) and
//! read RELATIVE to a region ([`EdgeScope::relative_to`]): an edge between
//! two regions is OUT OF one and INTO the other. `None` until regions are
//! known; library discovery is the producer. No rule reads it in step 3.

use oxc_span::Span;

use crate::cut::{Cut, PieceKind};

/// What an edge says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    /// A call: caller function → callee function (the function graph).
    Call,
    /// A read of a binding (both graphs): reader → declarer / definer.
    Read,
    /// A plain write: writer ↔ declarer.
    Write,
    /// A definitional mutation run at module-init time (`x.a = …`, an
    /// `Object.define*` / namespace-filler call on `x`): mutator ↔
    /// declarer. A write by mutation.
    InitMutation,
    /// A lazy module's wrapper → each statement of its init body.
    LazyWrapper,
}

impl EdgeKind {
    /// The kind's class: an init-time mutation is a write (the reference
    /// graph keeps one edge per pair and class).
    pub fn class(self) -> EdgeKind {
        match self {
            EdgeKind::InitMutation => EdgeKind::Write,
            k => k,
        }
    }
}

/// Where an edge sits relative to a region (docs/matching-architecture.md
/// §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    /// Both ends in the region.
    Inside,
    /// Caller outside, callee inside.
    Into,
    /// Caller inside, callee outside (another package, the runtime, app).
    OutOf,
}

/// The regions of an edge's two ends (`None`: not in any region).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeScope {
    pub from: Option<u32>,
    pub to: Option<u32>,
}

impl EdgeScope {
    /// The edge's scope relative to `region`; `None` when it does not touch
    /// the region.
    pub fn relative_to(&self, region: u32) -> Option<Scope> {
        match (self.from == Some(region), self.to == Some(region)) {
            (true, true) => Some(Scope::Inside),
            (false, true) => Some(Scope::Into),
            (true, false) => Some(Scope::OutOf),
            (false, false) => None,
        }
    }
}

/// One edge between two nodes of a graph (indices into its node list).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    pub kind: EdgeKind,
    /// The ends' regions; `None` until regions are known.
    pub scope: Option<EdgeScope>,
}

impl Edge {
    pub fn new(from: u32, to: u32, kind: EdgeKind) -> Edge {
        Edge {
            from,
            to,
            kind,
            scope: None,
        }
    }
}

/// Regions as spans (laminar: nested or disjoint, as AST pieces are), each
/// with its region id. A span belongs to the INNERMOST region span that
/// contains it.
#[derive(Clone, Debug, Default)]
pub struct Regions {
    /// Sorted by (start, end descending): an enclosing span first.
    spans: Vec<(Span, u32)>,
    /// Per span, the nearest span that contains it.
    enclosing: Vec<Option<usize>>,
}

impl Regions {
    pub fn new(mut spans: Vec<(Span, u32)>) -> Regions {
        spans.sort_by_key(|(s, r)| (s.start, std::cmp::Reverse(s.end), *r));
        let mut enclosing = Vec::with_capacity(spans.len());
        let mut open: Vec<usize> = Vec::new();
        for (i, (s, _)) in spans.iter().enumerate() {
            while let Some(&top) = open.last() {
                if spans[top].0.end >= s.end && spans[top].0.start <= s.start {
                    break;
                }
                open.pop();
            }
            enclosing.push(open.last().copied());
            open.push(i);
        }
        Regions { spans, enclosing }
    }

    /// The region of the innermost region span containing `span`.
    pub fn region_of(&self, span: Span) -> Option<u32> {
        let mut i = self
            .spans
            .partition_point(|(s, _)| s.start <= span.start)
            .checked_sub(1);
        while let Some(at) = i {
            let (s, region) = self.spans[at];
            if s.start <= span.start && span.end <= s.end {
                return Some(region);
            }
            // An earlier sibling ends before this one starts, so only
            // what encloses this span can still contain the query.
            i = self.enclosing[at];
        }
        None
    }

    /// The scope of an edge between two spans.
    pub fn scope(&self, from: Span, to: Span) -> EdgeScope {
        EdgeScope {
            from: self.region_of(from),
            to: self.region_of(to),
        }
    }
}

/// Give every edge its scope; `span_of` maps a node index to its span.
pub fn assign_scopes(edges: &mut [Edge], span_of: impl Fn(u32) -> Span, regions: &Regions) {
    for e in edges {
        e.scope = Some(regions.scope(span_of(e.from), span_of(e.to)));
    }
}

/// The one graph: the cutter's pieces and every edge both builders know,
/// in the one vocabulary (sorted, deduplicated).
#[derive(Clone, Debug, Default)]
pub struct CodeGraph {
    pub pieces: Vec<crate::cut::Piece>,
    pub edges: Vec<Edge>,
}

impl CodeGraph {
    /// Join the builders' edges over the cut's pieces: function rows and
    /// module bindings map to Function / Binding pieces, reference-graph
    /// units to Statement / Declarator pieces, by span. An end the cut does
    /// not hold (a function outside the container) is skipped.
    pub fn assemble(
        cut: Cut,
        unified: Option<&super::UnifiedGraph>,
        references: Option<&super::references::ReferenceGraph>,
    ) -> CodeGraph {
        let mut edges = Vec::new();
        if let Some(g) = unified {
            let fns: Vec<Option<u32>> = g
                .functions
                .iter()
                .map(|f| cut.piece_at(PieceKind::Function, f.span))
                .collect();
            let bindings: Vec<Option<u32>> = g
                .module_bindings
                .iter()
                .map(|b| cut.piece_at(PieceKind::Binding, b.span))
                .collect();
            let node = |i: u32| {
                let i = i as usize;
                if i < fns.len() {
                    fns[i]
                } else {
                    bindings[i - fns.len()]
                }
            };
            for e in g.edges() {
                if let (Some(from), Some(to)) = (node(e.from), node(e.to)) {
                    edges.push(Edge::new(from, to, e.kind));
                }
            }
        }
        if let Some(g) = references {
            let units: Vec<Option<u32>> = g
                .units
                .iter()
                .map(|u| cut.statement_piece_at(u.span))
                .collect();
            for e in &g.edges {
                if let (Some(from), Some(to)) = (units[e.from as usize], units[e.to as usize]) {
                    edges.push(Edge::new(from, to, e.kind));
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        CodeGraph {
            pieces: cut.pieces,
            edges,
        }
    }

    /// Give every edge its scope.
    pub fn assign_scopes(&mut self, regions: &Regions) {
        let pieces = &self.pieces;
        assign_scopes(&mut self.edges, |p| pieces[p as usize].span, regions);
    }
}
