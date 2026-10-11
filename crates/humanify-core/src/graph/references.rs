//! The statement reference graph (exp095 library discovery,
//! docs/design/library-discovery.md §4): what each top-level statement of
//! the app code references, at STATEMENT granularity, over every resolved
//! binding — the second view of "what does this code reference", beside the
//! function graph's call-only, function-level view
//! ([`super::build_function_graph`]). Both resolve bindings through the
//! same oxc `Semantic`.
//!
//! **Units** (design §2): every statement of the container, except that a
//! multi-declarator `var a, b;` gives each declarator its own unit (a Bun
//! lazy module hoists its bindings that way); a bundled CommonJS module is
//! ONE unit; a lazy ES module's wrapper is a unit owning only its overhead,
//! and each statement of its init body is a unit of its own.
//!
//! **Edges** u → v, for every binding scope analysis resolves (design §4):
//!
//! - a READ in `u` points at the declaring unit AND at every unit that
//!   writes the binding (a lazy module's body statement `x = …` is the real
//!   definer of its hoisted `var x`);
//! - a WRITE links the writer and the declarer both ways;
//! - a DEFINITIONAL mutation that runs at module-init time (not inside a
//!   function of its unit) is a write of its root binding: `x.a = …`,
//!   `Object.defineProperty / defineProperties / assign(x, …)`, and a call
//!   to one of the bundler's namespace-filling helpers with `x` first. The
//!   same mutation inside a function is run-time configuration — a read;
//! - a lazy wrapper points at each of its body statements (calling the init
//!   runs them).
//!
//! An unresolved name (a global) makes no edge. Which helpers start a lazy
//! module, fill a namespace or belong to the runtime is the toolchain's
//! answer ([`BundlerFacts::recognise`]); this module never names a bundler.

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{
    Argument, ArrowFunctionExpression, BindingPattern, Expression, Statement, VariableDeclarator,
};
use oxc_semantic::{Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use crate::babel_view::unparen;
use crate::toolchain::{InteropHelpers, ModuleWrapperGrammar};

/// What the reference graph needs to know about the bundler (design §3a):
/// which top-level bindings are its runtime helpers (externals, never
/// candidates), which helpers start a lazy ES module, and which helpers
/// FILL a namespace object (a call defines its first argument). Read off
/// the run's toolchain pieces by [`BundlerFacts::recognise`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundlerFacts {
    /// The grammar whose factory-argument forms a lazy wrapper's init takes.
    pub grammar: ModuleWrapperGrammar,
    pub lazy_init: HashSet<String>,
    pub namespace_fillers: HashSet<String>,
    pub runtime: HashSet<String>,
}

impl Default for BundlerFacts {
    fn default() -> Self {
        BundlerFacts {
            grammar: ModuleWrapperGrammar::BunAndEsbuild,
            lazy_init: HashSet::new(),
            namespace_fillers: HashSet::new(),
            runtime: HashSet::new(),
        }
    }
}

/// React Compiler's cache sentinels (`var z = Symbol.for("react.memo_cache_sentinel")`):
/// compiler output every compiled component reads, never a library's code.
const COMPILER_SENTINELS: [&str; 2] = ["react.memo_cache_sentinel", "react.early_return_sentinel"];

impl BundlerFacts {
    /// The bundler facts of a container's statements, from the run's
    /// pieces: the module grammar's factory helper and lazy-init helpers,
    /// the interop helpers' shapes and namespace fillers, and React
    /// Compiler's sentinels. Every one of them is a runtime helper.
    pub fn recognise(
        code: &str,
        statements: &[Statement<'_>],
        grammar: ModuleWrapperGrammar,
        interop: InteropHelpers,
    ) -> BundlerFacts {
        let lazy_init = grammar.lazy_init_helpers_in(statements);
        let namespace_fillers = interop.namespace_fillers(code, statements);
        let mut runtime: HashSet<String> = lazy_init.union(&namespace_fillers).cloned().collect();
        if let Some(helper) = grammar.identify_factory_helper(code) {
            runtime.insert(helper.name);
        }
        for d in top_declarators(statements) {
            let (BindingPattern::BindingIdentifier(id), Some(init)) = (&d.id, &d.init) else {
                continue;
            };
            if interop.recognise(code, init).is_some() || is_compiler_sentinel(init) {
                runtime.insert(id.name.to_string());
            }
        }
        BundlerFacts {
            grammar,
            lazy_init,
            namespace_fillers,
            runtime,
        }
    }
}

fn top_declarators<'s, 'a>(
    statements: &'s [Statement<'a>],
) -> impl Iterator<Item = &'s VariableDeclarator<'a>> {
    statements.iter().flat_map(|s| match s {
        Statement::VariableDeclaration(d) => d.declarations.as_slice(),
        _ => &[],
    })
}

fn is_compiler_sentinel(init: &Expression<'_>) -> bool {
    let Expression::CallExpression(call) = unparen(init) else {
        return false;
    };
    let Some(m) = call.callee.as_member_expression() else {
        return false;
    };
    matches!(m.object(), Expression::Identifier(o) if o.name == "Symbol")
        && m.static_property_name() == Some("for")
        && matches!(
            call.arguments.first(),
            Some(Argument::StringLiteral(s)) if COMPILER_SENTINELS.contains(&s.value.as_str())
        )
}

/// What one unit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitKind {
    /// A top-level statement, or one declarator of a multi-declarator `var`.
    Statement,
    /// A lazy ES module's wrapper (`var I = init(() => { … })`): it owns
    /// only the wrapper overhead; its body statements are their own units.
    LazyModule,
    /// One statement of a lazy module's init body.
    LazyBody { module: u32 },
    /// A bundled CommonJS module: one unit, external.
    Factory,
}

/// One node of the graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub span: Span,
    pub kind: UnitKind,
    /// The bytes this unit OWNS (a lazy wrapper: its span minus its body's).
    pub bytes: u32,
    /// A bundler runtime helper, or code only they reference.
    pub runtime: bool,
}

/// Why an edge exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EdgeKind {
    Read,
    Write,
    Body,
}

/// The statement reference graph.
#[derive(Clone, Debug, Default)]
pub struct ReferenceGraph {
    pub units: Vec<Unit>,
    /// Every edge (from, to, kind), sorted and deduplicated.
    pub edges: Vec<(u32, u32, EdgeKind)>,
    /// Per unit, its successors (sorted, deduplicated over kinds).
    pub out: Vec<Vec<u32>>,
    /// Per unit, its predecessors.
    pub inn: Vec<Vec<u32>>,
    /// Top-level units, then body units, each in source order: the
    /// position lookup's two levels.
    top: Vec<u32>,
    inner: Vec<u32>,
}

impl ReferenceGraph {
    /// The innermost unit holding byte `pos`.
    pub fn unit_at(&self, pos: u32) -> Option<u32> {
        self.find(&self.inner, pos)
            .or_else(|| self.find(&self.top, pos))
    }

    fn find(&self, level: &[u32], pos: u32) -> Option<u32> {
        let i = level.partition_point(|&u| self.units[u as usize].span.start <= pos);
        let u = *level.get(i.checked_sub(1)?)?;
        (pos < self.units[u as usize].span.end).then_some(u)
    }

    /// The external base: runtime helpers and bundled CommonJS modules.
    pub fn is_base_external(&self, u: u32) -> bool {
        let unit = &self.units[u as usize];
        unit.runtime || unit.kind == UnitKind::Factory
    }
}

/// The units of a container, and which unit declares each top-level name.
struct UnitBuilder<'f> {
    facts: &'f BundlerFacts,
    factories: HashSet<(u32, u32)>,
    units: Vec<Unit>,
    declared: HashMap<String, u32>,
}

impl UnitBuilder<'_> {
    fn push(&mut self, span: Span, kind: UnitKind) -> u32 {
        self.units.push(Unit {
            span,
            kind,
            bytes: span.size(),
            runtime: false,
        });
        self.units.len() as u32 - 1
    }

    fn statement<'a>(&mut self, stmt: &'a Statement<'a>) {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                let split = decl.declarations.len() > 1;
                for d in &decl.declarations {
                    let span = if split { d.span } else { decl.span };
                    let u = self.declarator(d, span);
                    if let BindingPattern::BindingIdentifier(id) = &d.id {
                        self.declared.insert(id.name.to_string(), u);
                    }
                }
            }
            Statement::FunctionDeclaration(f) => {
                let u = self.push(f.span, UnitKind::Statement);
                if let Some(id) = &f.id {
                    self.declared.insert(id.name.to_string(), u);
                }
            }
            Statement::ClassDeclaration(c) => {
                let u = self.push(c.span, UnitKind::Statement);
                if let Some(id) = &c.id {
                    self.declared.insert(id.name.to_string(), u);
                }
            }
            s => {
                self.push(s.span(), UnitKind::Statement);
            }
        }
    }

    fn declarator<'a>(&mut self, d: &'a VariableDeclarator<'a>, span: Span) -> u32 {
        if self.factories.contains(&(d.span.start, d.span.end)) {
            return self.push(span, UnitKind::Factory);
        }
        let Some(body) = lazy_init_body(d, self.facts) else {
            return self.push(span, UnitKind::Statement);
        };
        let module = self.push(span, UnitKind::LazyModule);
        let mut owned = span.size();
        for s in body {
            self.push(s.span(), UnitKind::LazyBody { module });
            owned -= s.span().size();
        }
        self.units[module as usize].bytes = owned;
        module
    }
}

/// A lazy wrapper's init body: `var I = init(<function>)` where `init` is
/// one of the bundle's lazy-init helpers and the argument takes one of the
/// grammar's factory forms with a block body.
fn lazy_init_body<'a>(
    d: &'a VariableDeclarator<'a>,
    facts: &BundlerFacts,
) -> Option<&'a [Statement<'a>]> {
    let Expression::CallExpression(call) = unparen(d.init.as_ref()?) else {
        return None;
    };
    let Expression::Identifier(callee) = unparen(&call.callee) else {
        return None;
    };
    if !facts.lazy_init.contains(callee.name.as_str()) || call.arguments.len() != 1 {
        return None;
    }
    let function = facts
        .grammar
        .factory_function(call.arguments[0].as_expression()?)?
        .function;
    match function {
        Expression::ArrowFunctionExpression(a) => arrow_block(a),
        Expression::FunctionExpression(f) => Some(f.body.as_ref()?.statements.as_slice()),
        _ => None,
    }
}

fn arrow_block<'a>(a: &'a ArrowFunctionExpression<'a>) -> Option<&'a [Statement<'a>]> {
    match &a.body {
        oxc_ast::ast::ArrowFunctionBody::FunctionBody(b) => Some(b.statements.as_slice()),
        _ => None,
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
) -> Vec<(u32, u32, EdgeKind)> {
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
        if let Some(m) = mutations.get(&symbol) {
            writers.extend(m.iter().copied());
        }
        writers.sort_unstable();
        writers.dedup();
        readers.sort_unstable();
        readers.dedup();
        for &w in writers.iter().filter(|&&w| w != decl) {
            edges.push((w, decl, EdgeKind::Write));
            edges.push((decl, w, EdgeKind::Write));
        }
        for &r in &readers {
            for &o in std::iter::once(&decl).chain(&writers) {
                if r != o {
                    edges.push((r, o, EdgeKind::Read));
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
    let mut builder = UnitBuilder {
        facts,
        factories: factories.iter().map(|s| (s.start, s.end)).collect(),
        units: Vec::new(),
        declared: HashMap::new(),
    };
    for stmt in statements {
        builder.statement(stmt);
    }
    let UnitBuilder {
        units, declared, ..
    } = builder;
    let (inner, top): (Vec<u32>, Vec<u32>) = (0..units.len() as u32)
        .partition(|&u| matches!(units[u as usize].kind, UnitKind::LazyBody { .. }));
    let mut graph = ReferenceGraph {
        units,
        top,
        inner,
        ..ReferenceGraph::default()
    };
    let mutations = init_time_mutations(semantic, &graph, facts);
    let mut edges = binding_edges(semantic, &graph, &mutations);
    for (u, unit) in graph.units.iter().enumerate() {
        if let UnitKind::LazyBody { module } = unit.kind {
            edges.push((module, u as u32, EdgeKind::Body));
        }
    }
    edges.sort_unstable();
    edges.dedup();
    let n = graph.units.len();
    graph.out = vec![Vec::new(); n];
    graph.inn = vec![Vec::new(); n];
    for &(a, b, _) in &edges {
        graph.out[a as usize].push(b);
        graph.inn[b as usize].push(a);
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
