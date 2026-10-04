//! The naming graph: what the TS `UnifiedGraph` carries for the LLM waves
//! beyond the rows the functions gate compares — TS original:
//! `src/analysis/function-graph.ts` (the `nodes` Map order, the
//! `dependencies` sets incl. the function → class edges, `scopeParentEdges`,
//! `internalCallees` in Set INSERTION order, `callSites`) and
//! `src/rename/plugin.ts` (a module binding's `declaration`,
//! `assignments`, `usages`, `declarationLine` — collectAssignmentContext,
//! collectUsageExamples, getDeclarationText).
//!
//! The structure here is computed at GRAPH-BUILD time over the ORIGINAL
//! (minified) names. The prompt TEXTS are not: the graph records WHICH
//! code a prompt shows (spans — [`super::prompt_text`]) and the prompt
//! prints it at ask time under the names current then (2026-10-04).
//!
//! ORDER is decision input (15-porting-lessons §4): the wave membership
//! iterates the node order, the prompt's "This function calls:" list is
//! the callee Set's insertion order (the Rust graph stores the callee SET
//! sorted by span), and call sites keep their first five distinct codes in
//! visit order.

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::Expression;
use oxc_semantic::{AstNodes, NodeId, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};

use humanify_model::js::{utf16_len, utf16_prefix};

use super::generate::TextView;
use super::prompt_text::{CallSite, Cap, Snippet};
use crate::babel_view::unparen;
use crate::graph::UnifiedGraph;

/// One `graph.nodes` entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeRef {
    /// A function row (index into `UnifiedGraph::functions`).
    Fn(usize),
    /// A module-binding row (index into `UnifiedGraph::module_bindings`).
    Mb(usize),
}

/// A module binding's prompt material (ModuleBindingNode's text fields)
/// — which code to show, printed at ask time ([`Snippet::render`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MbText {
    pub declaration: Snippet,
    /// `identifier.loc.start.line`.
    pub declaration_line: u32,
    pub assignments: Vec<Snippet>,
    pub usages: Vec<Snippet>,
}

/// The naming graph over one fresh text.
pub struct NamingGraph {
    /// `graph.nodes` key order: functions (traversal order), then module
    /// bindings (the target scope's table order).
    pub order: Vec<NodeRef>,
    pub node_of_fn: Vec<usize>,
    pub node_of_mb: Vec<usize>,
    /// `graph.dependencies`, per node, in Set insertion order.
    pub deps: Vec<Vec<usize>>,
    /// `scopeParentEdges`: (node, its scope-parent node).
    pub scope_parent_edges: HashSet<(usize, usize)>,
    /// `fn.scopeParent`, as a function row.
    pub fn_scope_parent: Vec<Option<usize>>,
    /// `fn.internalCallees` in Set insertion order.
    pub fn_callees: Vec<Vec<usize>>,
    /// `fn.callSites`: the first five distinct sites (distinct over the
    /// original text), printed at ask time ([`CallSite::render`]).
    pub fn_call_sites: Vec<Vec<CallSite>>,
    pub mb_text: Vec<MbText>,
}

impl NamingGraph {
    /// The node's session id.
    pub fn session_id<'g>(&self, graph: &'g UnifiedGraph, node: usize) -> &'g str {
        match self.order[node] {
            NodeRef::Fn(i) => &graph.functions[i].session_id,
            NodeRef::Mb(i) => &graph.module_bindings[i].session_id,
        }
    }
}

/// Cap on call sites recorded per function (MAX_CALL_SITES).
const MAX_CALL_SITES: usize = 5;
/// Cap on assignment + usage snippets per module binding.
const MAX_CONTEXT_SNIPPETS: usize = 10;

/// Build the naming graph (`buildUnifiedGraph`'s wave-side extras).
pub fn build_naming_graph(
    semantic: &Semantic<'_>,
    graph: &UnifiedGraph,
    view: &TextView<'_>,
) -> NamingGraph {
    let nodes = semantic.nodes();
    let n_fns = graph.functions.len();
    let n_mbs = graph.module_bindings.len();
    let mut order = Vec::with_capacity(n_fns + n_mbs);
    order.extend((0..n_fns).map(NodeRef::Fn));
    order.extend((0..n_mbs).map(NodeRef::Mb));
    let node_of_fn: Vec<usize> = (0..n_fns).collect();
    let node_of_mb: Vec<usize> = (n_fns..n_fns + n_mbs).collect();

    let fn_by_span: HashMap<(u32, u32), usize> = graph
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.span.start, f.span.end), i))
        .collect();
    let mb_by_span: HashMap<(u32, u32), usize> = graph
        .module_bindings
        .iter()
        .enumerate()
        .map(|(i, b)| ((b.span.start, b.span.end), i))
        .collect();
    let fn_scope_parent: Vec<Option<usize>> = graph
        .functions
        .iter()
        .map(|f| {
            f.scope_parent
                .and_then(|s| fn_by_span.get(&(s.start, s.end)).copied())
        })
        .collect();

    let row_nodes = crate::matching::row_node_ids(&graph.functions, nodes);
    let fn_node: Vec<NodeId> = graph
        .functions
        .iter()
        .map(|f| row_nodes[&(f.span.start, f.span.end)].0)
        .collect();
    let calls = call_targets(semantic, graph, &fn_by_span);
    let fn_callees = callee_insertion_order(graph, &calls, &fn_by_span);
    let fn_call_sites = call_sites(semantic, graph, view, &calls, &fn_node);

    // graph.dependencies: callees, scope parent, then the class edges.
    let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n_fns + n_mbs];
    let mut scope_parent_edges = HashSet::new();
    for i in 0..n_fns {
        let d = &mut deps[node_of_fn[i]];
        for &c in &fn_callees[i] {
            push_unique(d, node_of_fn[c]);
        }
        if let Some(p) = fn_scope_parent[i] {
            push_unique(d, node_of_fn[p]);
            scope_parent_edges.insert((node_of_fn[i], node_of_fn[p]));
        }
    }
    for (j, b) in graph.module_bindings.iter().enumerate() {
        let d = &mut deps[node_of_mb[j]];
        for s in &b.internal_callees {
            if let Some(&f) = fn_by_span.get(&(s.start, s.end)) {
                push_unique(d, node_of_fn[f]);
            } else if let Some(&m) = mb_by_span.get(&(s.start, s.end)) {
                push_unique(d, node_of_mb[m]);
            }
        }
    }
    for (j, b) in graph.module_bindings.iter().enumerate() {
        if !is_class_binding(semantic, b.symbol) {
            continue;
        }
        for s in &b.callers {
            if let Some(&f) = fn_by_span.get(&(s.start, s.end)) {
                let target = node_of_mb[j];
                push_unique(&mut deps[node_of_fn[f]], target);
            }
        }
    }

    let mb_text = module_binding_texts(semantic, graph, view);
    NamingGraph {
        order,
        node_of_fn,
        node_of_mb,
        deps,
        scope_parent_edges,
        fn_scope_parent,
        fn_callees,
        fn_call_sites,
        mb_text,
    }
}

fn push_unique(v: &mut Vec<usize>, x: usize) {
    if !v.contains(&x) {
        v.push(x);
    }
}

/// One visited call expression: its node, span, and the function row its
/// callee resolves to (with whether the resolution was an IDENTIFIER
/// callee — only those record call sites).
struct CallTarget {
    node: NodeId,
    span: Span,
    target: usize,
    identifier_callee: bool,
}

/// Every call babel's `CallExpression` visitor sees inside a graph
/// function (optional calls included — finding #1), in pre-order, with an
/// internal target.
fn call_targets(
    semantic: &Semantic<'_>,
    graph: &UnifiedGraph,
    fn_by_span: &HashMap<(u32, u32), usize>,
) -> Vec<CallTarget> {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let mut out = Vec::new();
    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        let target = match &call.callee {
            Expression::Identifier(id) => id
                .reference_id
                .get()
                .and_then(|r| scoping.get_reference(r).symbol_id())
                .and_then(|s| graph.function_by_symbol.get(&s).copied())
                .map(|t| (t, true)),
            other => {
                let callee = unparen(other);
                match callee {
                    Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                        let s = callee.span();
                        fn_by_span.get(&(s.start, s.end)).map(|&t| (t, false))
                    }
                    _ => None,
                }
            }
        };
        let Some((target, identifier_callee)) = target else {
            continue;
        };
        out.push(CallTarget {
            node: node.id(),
            span: call.span,
            target,
            identifier_callee,
        });
    }
    out
}

/// `fn.internalCallees` in insertion order: the first call to each callee
/// in the function's subtree, pre-order (the traversal covers nested
/// functions). Restricted to the gated callee SET.
fn callee_insertion_order(
    graph: &UnifiedGraph,
    calls: &[CallTarget],
    fn_by_span: &HashMap<(u32, u32), usize>,
) -> Vec<Vec<usize>> {
    // Calls sorted by (start asc, end desc) — pre-order.
    let mut sorted: Vec<&CallTarget> = calls.iter().collect();
    sorted.sort_by_key(|c| (c.span.start, std::cmp::Reverse(c.span.end)));
    graph
        .functions
        .iter()
        .map(|f| {
            let set: HashSet<usize> = f
                .internal_callees
                .iter()
                .filter_map(|s| fn_by_span.get(&(s.start, s.end)).copied())
                .collect();
            let lo = sorted.partition_point(|c| c.span.start < f.span.start);
            let mut order = Vec::new();
            for c in &sorted[lo..] {
                if c.span.start >= f.span.end {
                    break;
                }
                if c.span.end <= f.span.end && set.contains(&c.target) && !order.contains(&c.target)
                {
                    order.push(c.target);
                }
            }
            // Anything the walk missed keeps the set's span order (never
            // expected — the probe pins it).
            let mut rest: Vec<usize> = f
                .internal_callees
                .iter()
                .filter_map(|s| fn_by_span.get(&(s.start, s.end)).copied())
                .filter(|t| !order.contains(t))
                .collect();
            order.append(&mut rest);
            order
        })
        .collect()
}

/// `recordCallSite` over every visited identifier-callee call, in visit
/// order (the first visit of a call is its outermost function's
/// traversal — document pre-order).
fn call_sites(
    semantic: &Semantic<'_>,
    graph: &UnifiedGraph,
    view: &TextView<'_>,
    calls: &[CallTarget],
    fn_node: &[NodeId],
) -> Vec<Vec<CallSite>> {
    let nodes = semantic.nodes();
    let fn_nodes: HashSet<NodeId> = fn_node.iter().copied().collect();
    let mut codes: Vec<Vec<String>> = vec![Vec::new(); graph.functions.len()];
    let mut sites: Vec<Vec<CallSite>> = vec![Vec::new(); graph.functions.len()];
    for c in calls {
        if !c.identifier_callee || sites[c.target].len() >= MAX_CALL_SITES {
            continue;
        }
        if !inside_graph_function(nodes, c.node, &fn_nodes) {
            continue;
        }
        let Some((code, site)) = gather_call_site(nodes, view, c) else {
            continue;
        };
        if !codes[c.target].contains(&code) {
            codes[c.target].push(code);
            sites[c.target].push(site);
        }
    }
    sites
}

fn inside_graph_function(nodes: &AstNodes<'_>, node: NodeId, fn_nodes: &HashSet<NodeId>) -> bool {
    nodes.ancestor_ids(node).any(|a| fn_nodes.contains(&a))
}

/// `gatherCallSiteCode`: the statement parent's compact code, expanded
/// with up to two preceding siblings when short, capped at 200 units —
/// over the ORIGINAL names (the distinctness key), with the site's spans
/// for the ask-time print.
fn gather_call_site(
    nodes: &AstNodes<'_>,
    view: &TextView<'_>,
    call: &CallTarget,
) -> Option<(String, CallSite)> {
    let stmt = statement_parent(nodes, call.node)?;
    let mut code = compact_statement(nodes, view, stmt);
    if utf16_len(&code) < 80
        && let Some(expanded) = expand_with_siblings(nodes, view, stmt)
    {
        code = expanded;
    }
    if utf16_len(&code) > 200 {
        code = format!("{}...", utf16_prefix(&code, 197));
    }
    let callee = match nodes.kind(call.node) {
        AstKind::CallExpression(c) => c.callee.span(),
        _ => call.span,
    };
    let site = CallSite {
        stmt: nodes.get_node(stmt).span(),
        semicolon: needs_standalone_semicolon(nodes, stmt),
        siblings: sibling_spans(nodes, stmt),
        callee,
    };
    Some((code, site))
}

/// The compact print of a statement (a standalone `VariableDeclaration`
/// from a for-head gains babel's terminating semicolon).
fn compact_statement(nodes: &AstNodes<'_>, view: &TextView<'_>, stmt: NodeId) -> String {
    let mut code = view.compact(nodes.get_node(stmt).span());
    if needs_standalone_semicolon(nodes, stmt) {
        code.push(';');
    }
    code
}

/// A `VariableDeclaration` whose source has no `;` because it is a for
/// head — babel prints the semicolon when the node is printed alone.
fn needs_standalone_semicolon(nodes: &AstNodes<'_>, stmt: NodeId) -> bool {
    matches!(nodes.kind(stmt), AstKind::VariableDeclaration(_))
        && matches!(
            nodes.parent_kind(stmt),
            AstKind::ForStatement(_) | AstKind::ForInStatement(_) | AstKind::ForOfStatement(_)
        )
}

/// `tryExpandWithSiblings`: the statement plus up to two preceding
/// siblings of its BLOCK (a function body is babel's BlockStatement),
/// compact, newline-joined, when that fits 200 units.
fn expand_with_siblings(nodes: &AstNodes<'_>, view: &TextView<'_>, stmt: NodeId) -> Option<String> {
    let lines: Vec<String> = sibling_spans(nodes, stmt)?
        .iter()
        .map(|s| view.compact(*s))
        .collect();
    let combined = lines.join("\n");
    (utf16_len(&combined) <= 200).then_some(combined)
}

/// The statement and up to two preceding siblings of its BLOCK (a
/// function body is babel's BlockStatement); None outside a block.
fn sibling_spans(nodes: &AstNodes<'_>, stmt: NodeId) -> Option<Vec<Span>> {
    let parent = nodes.parent_node(stmt);
    let siblings: Vec<Span> = match parent.kind() {
        AstKind::BlockStatement(b) => b.body.iter().map(GetSpan::span).collect(),
        AstKind::FunctionBody(fb) => fb.statements.iter().map(GetSpan::span).collect(),
        _ => return None,
    };
    let own = nodes.get_node(stmt).span();
    let idx = siblings.iter().position(|s| *s == own)?;
    Some(siblings[idx.saturating_sub(2)..=idx].to_vec())
}

/// babel `isStatement` over an oxc node (the Statement alias). An
/// expression-bodied arrow has no FunctionBody in oxc 0.150 (its body is
/// the expression), so every FunctionBody is babel's BlockStatement.
fn is_babel_statement(nodes: &AstNodes<'_>, id: NodeId) -> bool {
    match nodes.kind(id) {
        AstKind::ExpressionStatement(_)
        | AstKind::BlockStatement(_)
        | AstKind::BreakStatement(_)
        | AstKind::ContinueStatement(_)
        | AstKind::DebuggerStatement(_)
        | AstKind::DoWhileStatement(_)
        | AstKind::EmptyStatement(_)
        | AstKind::ForInStatement(_)
        | AstKind::ForOfStatement(_)
        | AstKind::ForStatement(_)
        | AstKind::IfStatement(_)
        | AstKind::LabeledStatement(_)
        | AstKind::ReturnStatement(_)
        | AstKind::SwitchStatement(_)
        | AstKind::ThrowStatement(_)
        | AstKind::TryStatement(_)
        | AstKind::WhileStatement(_)
        | AstKind::WithStatement(_)
        | AstKind::VariableDeclaration(_)
        | AstKind::ImportDeclaration(_)
        | AstKind::ExportAllDeclaration(_)
        | AstKind::ExportDefaultDeclaration(_)
        | AstKind::ExportDeclaration(_)
        | AstKind::ExportFromDeclaration(_)
        | AstKind::ExportNamedDeclaration(_) => true,
        AstKind::Function(f) => f.is_declaration(),
        AstKind::Class(c) => c.is_declaration(),
        _ => false,
    }
}

/// babel `isDeclaration` (the JS members of the alias).
fn is_babel_declaration(nodes: &AstNodes<'_>, id: NodeId) -> bool {
    match nodes.kind(id) {
        AstKind::VariableDeclaration(_)
        | AstKind::ImportDeclaration(_)
        | AstKind::ExportAllDeclaration(_)
        | AstKind::ExportDefaultDeclaration(_)
        | AstKind::ExportDeclaration(_)
        | AstKind::ExportFromDeclaration(_)
        | AstKind::ExportNamedDeclaration(_) => true,
        AstKind::Function(f) => f.is_declaration(),
        AstKind::Class(c) => c.is_declaration(),
        _ => false,
    }
}

/// `path.getStatementParent()`: the nearest ancestor-or-self statement
/// that sits in a statement LIST (babel: `Array.isArray(path.container)`).
/// None at the program (babel throws there; `recordCallSite` swallows it
/// and records nothing).
fn statement_parent(nodes: &AstNodes<'_>, from: NodeId) -> Option<NodeId> {
    let mut cur = from;
    loop {
        if is_babel_statement(nodes, cur) && in_statement_list(nodes, cur) {
            return Some(cur);
        }
        if matches!(nodes.kind(cur), AstKind::Program(_)) {
            return None;
        }
        cur = nodes.parent_id(cur);
    }
}

/// The node's container is a statement list.
fn in_statement_list(nodes: &AstNodes<'_>, id: NodeId) -> bool {
    matches!(
        nodes.parent_kind(id),
        AstKind::BlockStatement(_)
            | AstKind::Program(_)
            | AstKind::SwitchCase(_)
            | AstKind::StaticBlock(_)
            | AstKind::FunctionBody(_)
    )
}

/// `path.findParent(p => p.isStatement())` — strict ancestors.
fn find_statement_ancestor(nodes: &AstNodes<'_>, from: NodeId) -> Option<NodeId> {
    nodes
        .ancestor_ids(from)
        .find(|&a| is_babel_statement(nodes, a))
}

/// `isClassBinding`: a class declaration, a declarator initialized by a
/// class expression, or any reference that is a `new` callee.
fn is_class_binding(semantic: &Semantic<'_>, symbol: oxc_semantic::SymbolId) -> bool {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let decl = scoping.symbol_declaration(symbol);
    match nodes.kind(decl) {
        AstKind::Class(c) if c.is_declaration() => return true,
        AstKind::VariableDeclarator(d) => {
            if let Some(init) = &d.init
                && matches!(unparen(init), Expression::ClassExpression(_))
            {
                return true;
            }
        }
        _ => {}
    }
    for &r in scoping.get_resolved_reference_ids(symbol) {
        let reference = scoping.get_reference(r);
        let node = reference.node_id();
        let mut parent = nodes.parent_id(node);
        while matches!(nodes.kind(parent), AstKind::ParenthesizedExpression(_)) {
            parent = nodes.parent_id(parent);
        }
        if let AstKind::NewExpression(n) = nodes.kind(parent)
            && unparen(&n.callee).span() == nodes.get_node(node).span()
        {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// module-binding texts
// ---------------------------------------------------------------------------

fn module_binding_texts(
    semantic: &Semantic<'_>,
    graph: &UnifiedGraph,
    view: &TextView<'_>,
) -> Vec<MbText> {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let by_symbol: HashMap<SymbolId, usize> = graph
        .module_bindings
        .iter()
        .enumerate()
        .map(|(i, b)| (b.symbol, i))
        .collect();
    let mut texts: Vec<MbText> = graph
        .module_bindings
        .iter()
        .map(|b| {
            let id = scoping.symbol_span(b.symbol);
            MbText {
                declaration: declaration_snippet(
                    nodes,
                    scoping.symbol_declaration(b.symbol),
                    (id.start, id.size()),
                ),
                declaration_line: view.line_of(b.span.start),
                assignments: Vec::new(),
                usages: Vec::new(),
            }
        })
        .collect();
    collect_assignment_context(semantic, view, &by_symbol, &mut texts);
    collect_usage_examples(semantic, view, graph, &mut texts);
    texts
}

/// `getDeclarationText`'s node: a declarator prints its whole declaration
/// (capped), an import specifier its whole import declaration (UNCAPPED),
/// anything else itself (capped).
fn declaration_snippet(nodes: &AstNodes<'_>, decl: NodeId, id: (u32, u32)) -> Snippet {
    match nodes.kind(decl) {
        AstKind::VariableDeclarator(_) => {
            statement_snippet(nodes, nodes.parent_id(decl), Some(id), Cap::Declaration)
        }
        AstKind::ImportSpecifier(_)
        | AstKind::ImportDefaultSpecifier(_)
        | AstKind::ImportNamespaceSpecifier(_) => {
            let mut cur = decl;
            while !matches!(nodes.kind(cur), AstKind::ImportDeclaration(_)) {
                cur = nodes.parent_id(cur);
            }
            statement_snippet(nodes, cur, Some(id), Cap::Whole)
        }
        _ => Snippet {
            span: nodes.get_node(decl).span(),
            layout: Vec::new(),
            semicolon: false,
            mention: Some(id),
            cap: Cap::Declaration,
        },
    }
}

/// A statement as babel prints it alone. A for-head `VariableDeclaration`
/// has no parent then: babel adds the terminating semicolon and, when a
/// declarator has an init and there are several, breaks the declarators
/// onto indented lines (`commaSeparatorWithNewline` — inline in the head
/// because `isFor(parent)` suppressed it).
fn statement_snippet(
    nodes: &AstNodes<'_>,
    stmt: NodeId,
    mention: Option<(u32, u32)>,
    cap: Cap,
) -> Snippet {
    let span = nodes.get_node(stmt).span();
    let semicolon = needs_standalone_semicolon(nodes, stmt);
    let mut layout = Vec::new();
    if semicolon
        && let AstKind::VariableDeclaration(d) = nodes.kind(stmt)
        && d.declarations.len() > 1
        && d.declarations.iter().any(|x| x.init.is_some())
    {
        for w in d.declarations.windows(2) {
            layout.push(super::generate::Replacement {
                span: Span::new(w[0].span.end, w[1].span.start),
                text: ",\n  ".to_string(),
            });
        }
    }
    Snippet {
        span,
        layout,
        semicolon,
        mention,
        cap,
    }
}

/// A snippet's text over the ORIGINAL names — the graph-time distinctness
/// key (two sites printing alike are one).
fn original_text(view: &TextView<'_>, s: &Snippet) -> String {
    let mut code = view.pretty(s.span, &s.layout, true);
    if s.semicolon {
        code.push(';');
    }
    code
}

/// The module binding an identifier reference resolves to.
fn module_binding_of(
    scoping: &oxc_semantic::Scoping,
    reference: Option<oxc_semantic::ReferenceId>,
    by_symbol: &HashMap<SymbolId, usize>,
) -> Option<usize> {
    let symbol = scoping.get_reference(reference?).symbol_id()?;
    by_symbol.get(&symbol).copied()
}

/// The module binding an assignment WRITES — the binding itself, or the
/// object of a member write (`x.p = …`, drilling through one level for
/// `x.prototype.m = …`). Resolved through the scope (2026-10-04: by NAME
/// it also caught every same-named local of an inner scope).
fn assignment_target(
    scoping: &oxc_semantic::Scoping,
    left: &oxc_ast::ast::AssignmentTarget<'_>,
    by_symbol: &HashMap<SymbolId, usize>,
) -> Option<usize> {
    use oxc_ast::ast::AssignmentTarget as T;
    let object = match left {
        T::AssignmentTargetIdentifier(id) => {
            return module_binding_of(scoping, id.reference_id.get(), by_symbol);
        }
        T::StaticMemberExpression(m) => &m.object,
        T::ComputedMemberExpression(m) => &m.object,
        T::PrivateFieldExpression(m) => &m.object,
        _ => return None,
    };
    let object = unparen(object);
    if let Some(inner) = member_object(object)
        && let Expression::Identifier(id) = unparen(inner)
        && let Some(j) = module_binding_of(scoping, id.reference_id.get(), by_symbol)
    {
        return Some(j);
    }
    if let Expression::Identifier(id) = object {
        return module_binding_of(scoping, id.reference_id.get(), by_symbol);
    }
    None
}

/// A babel MemberExpression's object (static, computed or private).
fn member_object<'n>(e: &'n Expression<'_>) -> Option<&'n Expression<'n>> {
    match e {
        Expression::StaticMemberExpression(m) => Some(&m.object),
        Expression::ComputedMemberExpression(m) => Some(&m.object),
        Expression::PrivateFieldExpression(m) => Some(&m.object),
        _ => None,
    }
}

/// `collectAssignmentContext`: every AssignmentExpression, pre-order, the
/// statement around it — distinct over the original text, ten at most.
fn collect_assignment_context(
    semantic: &Semantic<'_>,
    view: &TextView<'_>,
    by_symbol: &HashMap<SymbolId, usize>,
    texts: &mut [MbText],
) {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let mut seen: Vec<Vec<String>> = vec![Vec::new(); texts.len()];
    for node in nodes.iter() {
        let AstKind::AssignmentExpression(a) = node.kind() else {
            continue;
        };
        let Some(i) = assignment_target(scoping, &a.left, by_symbol) else {
            continue;
        };
        if texts[i].assignments.len() >= MAX_CONTEXT_SNIPPETS {
            continue;
        }
        let mention = Some((a.span.start, a.left.span().size()));
        let snippet = match find_statement_ancestor(nodes, node.id()) {
            Some(stmt) => statement_snippet(nodes, stmt, mention, Cap::Snippet),
            None => Snippet {
                span: a.span,
                layout: Vec::new(),
                semicolon: false,
                mention,
                cap: Cap::Snippet,
            },
        };
        push_distinct(view, &mut seen[i], &mut texts[i].assignments, snippet);
    }
}

/// Keep `s` when its original text is new and non-blank.
fn push_distinct(view: &TextView<'_>, seen: &mut Vec<String>, out: &mut Vec<Snippet>, s: Snippet) {
    let code = original_text(view, &s);
    let key = humanify_model::js::trim(&code);
    if key.is_empty() || seen.iter().any(|k| k == key) {
        return;
    }
    seen.push(key.to_string());
    out.push(s);
}

/// `collectUsageExamples`: the statement (or declaration) around each READ
/// of a module binding, in source order, distinct, capped so assignments +
/// usages stay within ten. Reads are the binding's resolved references
/// (2026-10-04): by NAME the walk also took property keys, member names
/// and same-named inner locals — lines that never mention the binding.
fn collect_usage_examples(
    semantic: &Semantic<'_>,
    view: &TextView<'_>,
    graph: &UnifiedGraph,
    texts: &mut [MbText],
) {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    for (i, b) in graph.module_bindings.iter().enumerate() {
        let mut reads: Vec<(u32, u32, NodeId)> = scoping
            .get_resolved_reference_ids(b.symbol)
            .iter()
            .map(|&r| scoping.get_reference(r))
            .filter(|r| !r.is_write())
            .map(|r| {
                let span = nodes.get_node(r.node_id()).span();
                (span.start, span.size(), r.node_id())
            })
            .collect();
        reads.sort_unstable();
        let remaining = MAX_CONTEXT_SNIPPETS.saturating_sub(texts[i].assignments.len());
        let mut seen = Vec::new();
        for (start, len, node) in reads {
            if texts[i].usages.len() >= remaining {
                break;
            }
            let from = nodes.parent_id(node);
            let stmt = std::iter::once(from)
                .chain(nodes.ancestor_ids(from))
                .find(|&a| is_babel_statement(nodes, a) || is_babel_declaration(nodes, a));
            let Some(stmt) = stmt else { continue };
            let snippet = statement_snippet(nodes, stmt, Some((start, len)), Cap::Snippet);
            push_distinct(view, &mut seen, &mut texts[i].usages, snippet);
        }
    }
}
