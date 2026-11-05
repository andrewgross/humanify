//! THE cutter (docs/matching-architecture.md §3.1): the one owner of "what
//! are the pieces of this code?". Every consumer that cuts code asks here
//! for the granularity it uses:
//!
//! | piece | what it is | owner here | consumers |
//! | --- | --- | --- | --- |
//! | module | the container: the layout's wrapper body, else the program | [`container_statements`] (AST), [`container_statement_values`] (ESTree JSON) | the factory classification, discovery, the index units; twins / the split's inventory, the naming plumbing, the module-name step |
//! | statement | one statement of the container (or of a lazy-init / factory body) | [`units::container_units`] | the reference graph, the library units |
//! | declarator | one declarator of a multi-declarator `var` | [`units::container_units`] | the same |
//! | function | babel's `Function` alias (declarations, expressions, arrows, object / class methods) | [`function_pieces`], [`is_function_piece`], [`enclosing_function_piece`] | the function graph's rows and scope parents, the eval/with taint |
//! | binding | a binding declared directly in the container scope | [`container_scope`] | the function graph's module bindings |
//!
//! [`Cut`] lays every kind out as one list of [`Piece`]s, each with its
//! byte span and its PARENT (a body statement → its lazy wrapper or
//! module; a function → its enclosing function, else its statement; a
//! binding → its statement) — the node set of the one graph
//! ([`crate::graph::edges::CodeGraph`]). The consumers do not build it:
//! each reads its own granularity from the owner functions above, computed
//! once per consumer as before (the perf arc's rule: no eager
//! all-kinds materialisation on the pipeline's path).
//!
//! **Two substrates, one rule.** The statement level is answered over the
//! AST ([`container_statements`]) and over the side's ESTree JSON
//! ([`container_statement_values`]) — the twins' statement hash, the
//! split and the naming plumbing walk the JSON; the graphs walk the AST.
//! Both select the same list (the wrapper's body block, located by the
//! layout's body span, else the program body).

pub mod units;

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_ast::ast::{Program, Statement};
use oxc_semantic::{AstNodes, NodeId, ScopeId, Semantic};
use oxc_span::{GetSpan, Span};
use serde_json::Value;

use crate::modules::wrapper::WrapperFunction;
use units::{BundlerFacts, UnitKind, UnitLocator, UnitNode, container_units};

/// What a piece is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PieceKind {
    Module,
    Statement,
    Declarator,
    Function,
    Binding,
}

/// One piece of the cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceKind,
    pub span: Span,
    /// The enclosing piece (an index into [`Cut::pieces`]); `None` for the
    /// module and for code outside the container (the wrapper function).
    pub parent: Option<u32>,
}

/// What a [`Cut`] cuts: one parsed side, its wrapper (the layout's), the
/// bundled modules' declarator spans (the unpack's classification) and the
/// bundler facts; `factory_bodies` cuts inside the bundled modules (the
/// index side's unit model).
pub struct CutInput<'s, 'a> {
    pub semantic: &'s Semantic<'a>,
    pub program: &'a Program<'a>,
    pub wrapper: Option<&'s WrapperFunction>,
    pub factories: &'s [Span],
    pub facts: &'s BundlerFacts,
    pub factory_bodies: bool,
}

/// Every piece of one side: the module, its statement-level units (in unit
/// order), its functions (in node order), its container bindings (in
/// source order).
#[derive(Clone, Debug, Default)]
pub struct Cut {
    pub pieces: Vec<Piece>,
    by_span: HashMap<(PieceKind, u32, u32), u32>,
}

impl Cut {
    pub fn new(input: &CutInput<'_, '_>) -> Cut {
        let semantic = input.semantic;
        let nodes = semantic.nodes();
        let scoping = semantic.scoping();
        let container = input.wrapper.map_or(input.program.span, |w| w.body_span);
        let statements =
            container_statements(input.program, semantic, input.wrapper.map(|w| w.body_span));
        let units = container_units(
            statements,
            input.factories,
            input.facts,
            input.factory_bodies,
        );
        let mut cut = Cut::default();
        let module = cut.push(PieceKind::Module, container, None);

        // Statement level: a body unit's parent is its container unit.
        let unit_base = cut.pieces.len() as u32;
        for (i, unit) in units.units.iter().enumerate() {
            let kind = match units.nodes[i] {
                UnitNode::Declarator(_) => PieceKind::Declarator,
                _ if units.split_declarator[i] => PieceKind::Declarator,
                _ => PieceKind::Statement,
            };
            let parent = match unit.kind {
                UnitKind::LazyBody { module: m } | UnitKind::FactoryBody { module: m } => {
                    unit_base + m
                }
                _ => module,
            };
            cut.push(kind, unit.span, Some(parent));
        }
        let locator = UnitLocator::new(&units.units);
        let unit_at = |span: Span| {
            locator
                .unit_at(&units.units, span.start)
                .filter(|&u| span.end <= units.units[u as usize].span.end)
                .map(|u| unit_base + u)
        };
        let in_container = |span: Span| container.start <= span.start && span.end <= container.end;

        // Functions: the enclosing function, else the statement, else the
        // module (outside the container: none).
        let (functions, idx_by_node) = function_pieces(nodes);
        let fn_base = cut.pieces.len() as u32;
        for &(node_id, span) in &functions {
            let parent = enclosing_function_piece(node_id, nodes, &idx_by_node)
                .map(|f| fn_base + f as u32)
                .filter(|&p| in_container(cut.pieces[p as usize].span))
                .or_else(|| unit_at(span))
                .or_else(|| in_container(span).then_some(module));
            cut.push(PieceKind::Function, span, parent);
        }

        // Bindings of the container scope.
        if let Some(scope) = container_scope(semantic, input.wrapper) {
            let mut spans: Vec<Span> = scoping
                .iter_bindings_in(scope)
                .map(|symbol| scoping.symbol_span(symbol))
                .collect();
            spans.sort_by_key(|s| (s.start, s.end));
            for span in spans {
                let parent = unit_at(span).or(Some(module));
                cut.push(PieceKind::Binding, span, parent);
            }
        }
        cut
    }

    fn push(&mut self, kind: PieceKind, span: Span, parent: Option<u32>) -> u32 {
        let id = self.pieces.len() as u32;
        self.pieces.push(Piece { kind, span, parent });
        self.by_span
            .entry((kind, span.start, span.end))
            .or_insert(id);
        id
    }

    /// The (first) piece of `kind` with exactly `span`.
    pub fn piece_at(&self, kind: PieceKind, span: Span) -> Option<u32> {
        self.by_span.get(&(kind, span.start, span.end)).copied()
    }

    /// The statement-level piece (statement or declarator) with `span`.
    pub fn statement_piece_at(&self, span: Span) -> Option<u32> {
        self.piece_at(PieceKind::Statement, span)
            .or_else(|| self.piece_at(PieceKind::Declarator, span))
    }
}

// ---------------------------------------------------------------------------
// Module → statements
// ---------------------------------------------------------------------------

/// The container's statements over the AST: the wrapper body's (when the
/// body is a block — the TS bodyPath.isBlockStatement() gate) or the
/// program's. A wrapper body that is not found (or not a block) has none.
pub fn container_statements<'a>(
    program: &'a Program<'a>,
    semantic: &Semantic<'a>,
    wrapper_body: Option<Span>,
) -> &'a [Statement<'a>] {
    let Some(body) = wrapper_body else {
        return program.body.as_slice();
    };
    semantic
        .nodes()
        .iter()
        .find(|n| {
            n.span() == body
                && matches!(
                    n.kind(),
                    AstKind::FunctionBody(_) | AstKind::BlockStatement(_)
                )
        })
        .and_then(|n| match n.kind() {
            // oxc stores a function's body as FunctionBody (Babel: the
            // body IS a BlockStatement) — the wrapper container is the
            // function body's statement list.
            AstKind::FunctionBody(b) => Some(b.statements.as_slice()),
            AstKind::BlockStatement(b) => Some(b.body.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

/// The container's statements over a side's ESTree JSON
/// ([`crate::ingest::program_estree_json`]): the wrapper body block's
/// (located by span), else the program body. `None` when the wrapper's
/// block is absent from the JSON or the program has no body.
pub fn container_statement_values(
    program_json: &Value,
    wrapper_body: Option<Span>,
) -> Option<&Vec<Value>> {
    match wrapper_body {
        Some(span) => block_body_by_span(program_json, span),
        None => program_json.get("body").and_then(Value::as_array),
    }
}

/// The block statement whose span is `target`, as its body array — the
/// wrapper-gate's body block, located in the serialization by the span the
/// gate already computed. Iterative (the AST's depth).
fn block_body_by_span(root: &Value, target: Span) -> Option<&Vec<Value>> {
    let mut stack: Vec<&Value> = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(map) => {
                if map.get("type").and_then(Value::as_str) == Some("BlockStatement")
                    && map.get("start").and_then(Value::as_u64) == Some(u64::from(target.start))
                    && map.get("end").and_then(Value::as_u64) == Some(u64::from(target.end))
                {
                    match map.get("body") {
                        Some(body @ Value::Array(_)) => return body.as_array(),
                        _ => return None,
                    }
                }
                stack.extend(map.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    None
}

/// The index of the statement (of `spans`: sorted, non-overlapping
/// siblings) containing `row` — the last statement starting at or before
/// the row (`partition_point`), when it holds the row's whole span.
pub fn enclosing_statement(spans: &[Span], row: Span) -> Option<usize> {
    let idx = spans.partition_point(|s| s.start <= row.start);
    if idx == 0 {
        return None;
    }
    let candidate = spans[idx - 1];
    (candidate.start <= row.start && row.end <= candidate.end).then_some(idx - 1)
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/// babel's `Function` alias — FunctionDeclaration | FunctionExpression |
/// ObjectMethod | ArrowFunctionExpression | ClassMethod |
/// ClassPrivateMethod — the node kinds whose parent-stop semantics the
/// taint walk (getFunctionParent) and the call-edge attribution use. oxc's
/// object methods are ObjectProperty with method=true or a Get/Set kind.
pub(crate) fn is_function_piece(kind: &AstKind<'_>) -> bool {
    match kind {
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => true,
        AstKind::MethodDefinition(_) => true,
        AstKind::ObjectProperty(p) => p.method || p.kind != oxc_ast::ast::PropertyKind::Init,
        _ => false,
    }
}

/// Is `node` a method's inner function (oxc's Function under a
/// MethodDefinition or a method ObjectProperty)?
fn is_method_inner(nodes: &AstNodes<'_>, node: NodeId) -> bool {
    match nodes.get_node(nodes.parent_id(node)).kind() {
        AstKind::MethodDefinition(_) => true,
        AstKind::ObjectProperty(p) => p.method || p.kind != oxc_ast::ast::PropertyKind::Init,
        _ => false,
    }
}

/// Every function piece, in node order, as (node, span), and the node →
/// index map. oxc splits the method forms: class methods are
/// MethodDefinition; OBJECT methods (and getters/setters) are
/// ObjectProperty with method=true or a Get/Set kind, whose value is a
/// plain Function node — the METHOD node is the piece (babel's span starts
/// at the key; the inner Function node would be a duplicate).
pub(crate) fn function_pieces(
    nodes: &AstNodes<'_>,
) -> (Vec<(NodeId, Span)>, HashMap<NodeId, usize>) {
    let mut pieces: Vec<(NodeId, Span)> = Vec::new();
    let mut idx_by_node: HashMap<NodeId, usize> = HashMap::new();
    for node in nodes.iter() {
        let is_piece = match node.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                !is_method_inner(nodes, node.id())
            }
            AstKind::MethodDefinition(_) => true,
            AstKind::ObjectProperty(p) => p.method || p.kind != oxc_ast::ast::PropertyKind::Init,
            _ => false,
        };
        if is_piece {
            idx_by_node.insert(node.id(), pieces.len());
            pieces.push((node.id(), node.span()));
        }
    }
    (pieces, idx_by_node)
}

/// The nearest enclosing function piece of a node, by parent walk. A
/// method's INNER function is the same scope as its MethodDefinition but
/// is not a piece (the method node is) — the walk continues to the piece.
pub(crate) fn enclosing_function_piece(
    node: NodeId,
    nodes: &AstNodes<'_>,
    idx_by_node: &HashMap<NodeId, usize>,
) -> Option<usize> {
    // parent_id is a plain NodeId (the root self-parents, which
    // terminates the walk).
    let mut prev = node;
    let mut parent_id = nodes.parent_id(prev);
    while parent_id != prev {
        if is_function_piece(&nodes.get_node(parent_id).kind())
            && let Some(&idx) = idx_by_node.get(&parent_id)
        {
            return Some(idx);
        }
        prev = parent_id;
        parent_id = nodes.parent_id(parent_id);
    }
    None
}

/// Every ancestor function of a node, as NODE IDS, innermost first — the
/// ONE owner of the "which functions enclose this node" question
/// (docs/responsibility.md). Consumers map ids to whatever identity they
/// need (the graph maps to row indices; the eval/with taint records the
/// spans).
pub(crate) fn enclosing_function_node_ids(node_id: NodeId, nodes: &AstNodes<'_>) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut prev = node_id;
    let mut parent_id = nodes.parent_id(prev);
    while parent_id != prev {
        if is_function_piece(&nodes.get_node(parent_id).kind()) {
            out.push(parent_id);
        }
        prev = parent_id;
        parent_id = nodes.parent_id(parent_id);
    }
    out
}

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

/// The container scope (the TS targetScope): the wrapper function's own
/// scope when a wrapper exists, else the program's. Its bindings are the
/// binding pieces.
pub fn container_scope(
    semantic: &Semantic<'_>,
    wrapper: Option<&WrapperFunction>,
) -> Option<ScopeId> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    match wrapper {
        Some(w) => (0..scoping.scopes_len())
            .map(ScopeId::new)
            .find(|sid| nodes.get_node(scoping.get_node_id(*sid)).span() == w.span),
        None => {
            let program_id = nodes.iter().next()?.id();
            (0..scoping.scopes_len())
                .map(ScopeId::new)
                .find(|sid| scoping.get_node_id(*sid) == program_id)
        }
    }
}
