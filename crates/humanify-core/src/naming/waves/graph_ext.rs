//! The naming graph: what the TS `UnifiedGraph` carries for the LLM waves
//! beyond the rows the functions gate compares — TS original:
//! `src/analysis/function-graph.ts` (the `nodes` Map order, the
//! `dependencies` sets incl. the function → class edges, `scopeParentEdges`,
//! `internalCallees` in Set INSERTION order, `callSites`) and
//! `src/rename/plugin.ts` (a module binding's `declaration`,
//! `assignments`, `usages`, `declarationLine` — collectAssignmentContext,
//! collectUsageExamples, getDeclarationText).
//!
//! Everything here is computed at GRAPH-BUILD time in the TS, i.e. over
//! the ORIGINAL (minified) names — no rename overlay is read.
//!
//! ORDER is decision input (15-porting-lessons §4): the wave membership
//! iterates the node order, the prompt's "This function calls:" list is
//! the callee Set's insertion order (the Rust graph stores the callee SET
//! sorted by span), and call sites keep their first five distinct codes in
//! visit order.

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, PropertyKey};
use oxc_semantic::{AstNodes, NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use humanify_model::js::{trim, utf16_len, utf16_prefix};

use super::generate::TextView;
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

/// A module binding's prompt material (ModuleBindingNode's text fields).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MbText {
    pub declaration: String,
    /// `identifier.loc.start.line`.
    pub declaration_line: u32,
    pub assignments: Vec<String>,
    pub usages: Vec<String>,
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
    /// `fn.callSites[].code`.
    pub fn_call_sites: Vec<Vec<String>>,
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
const MAX_SNIPPET_CHARS: usize = 800;
const MAX_SNIPPET_LINES: usize = 10;
const MAX_DECLARATION_LINES: usize = 10;
const MAX_DECLARATION_CHARS: usize = 1000;

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
/// function (optional calls excluded — lesson 1), in pre-order, with an
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
        if call.optional {
            continue;
        }
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
) -> Vec<Vec<String>> {
    let nodes = semantic.nodes();
    let fn_nodes: HashSet<NodeId> = fn_node.iter().copied().collect();
    let mut sites: Vec<Vec<String>> = vec![Vec::new(); graph.functions.len()];
    for c in calls {
        if !c.identifier_callee || sites[c.target].len() >= MAX_CALL_SITES {
            continue;
        }
        if !inside_graph_function(nodes, c.node, &fn_nodes) {
            continue;
        }
        let Some(code) = gather_call_site_code(nodes, view, c.node) else {
            continue;
        };
        if !sites[c.target].contains(&code) {
            sites[c.target].push(code);
        }
    }
    sites
}

fn inside_graph_function(nodes: &AstNodes<'_>, node: NodeId, fn_nodes: &HashSet<NodeId>) -> bool {
    nodes.ancestor_ids(node).any(|a| fn_nodes.contains(&a))
}

/// `gatherCallSiteCode`: the statement parent's compact code, expanded
/// with up to two preceding siblings when short, capped at 200 units.
fn gather_call_site_code(
    nodes: &AstNodes<'_>,
    view: &TextView<'_>,
    call: NodeId,
) -> Option<String> {
    let stmt = statement_parent(nodes, call)?;
    let mut code = compact_statement(nodes, view, stmt);
    if utf16_len(&code) < 80
        && let Some(expanded) = expand_with_siblings(nodes, view, stmt)
    {
        code = expanded;
    }
    if utf16_len(&code) > 200 {
        code = format!("{}...", utf16_prefix(&code, 197));
    }
    Some(code)
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
    let parent = nodes.parent_node(stmt);
    let siblings: Vec<Span> = match parent.kind() {
        AstKind::BlockStatement(b) => b.body.iter().map(GetSpan::span).collect(),
        AstKind::FunctionBody(fb) => fb.statements.iter().map(GetSpan::span).collect(),
        _ => return None,
    };
    let own = nodes.get_node(stmt).span();
    let idx = siblings.iter().position(|s| *s == own)?;
    let lines: Vec<String> = siblings[idx.saturating_sub(2)..=idx]
        .iter()
        .map(|s| view.compact(*s))
        .collect();
    let combined = lines.join("\n");
    (utf16_len(&combined) <= 200).then_some(combined)
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
    let names: HashMap<&str, usize> = graph
        .module_bindings
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect();
    let mut texts: Vec<MbText> = graph
        .module_bindings
        .iter()
        .map(|b| MbText {
            declaration: declaration_text(nodes, view, scoping.symbol_declaration(b.symbol)),
            declaration_line: view.line_of(b.span.start),
            assignments: Vec::new(),
            usages: Vec::new(),
        })
        .collect();
    collect_assignment_context(nodes, view, &names, &mut texts);
    collect_usage_examples(semantic, view, &names, &mut texts);
    texts
}

/// `capDeclarationText`.
fn cap_declaration_text(code: String) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    let mut text = if lines.len() > MAX_DECLARATION_LINES {
        format!("{}\n  // ...", lines[..MAX_DECLARATION_LINES].join("\n"))
    } else {
        code.clone()
    };
    if utf16_len(&text) > MAX_DECLARATION_CHARS {
        text = format!("{}…", utf16_prefix(&text, MAX_DECLARATION_CHARS));
    }
    text
}

/// `getDeclarationText` (graph time — original names).
fn declaration_text(nodes: &AstNodes<'_>, view: &TextView<'_>, decl: NodeId) -> String {
    match nodes.kind(decl) {
        AstKind::VariableDeclarator(_) => {
            cap_declaration_text(statement_code(nodes, view, nodes.parent_id(decl)))
        }
        _ => cap_declaration_text(view.pretty(nodes.get_node(decl).span(), &[], true)),
    }
}

/// `truncateSnippet`.
fn truncate_snippet(code: &str) -> Option<String> {
    let lines: Vec<&str> = code.split('\n').take(MAX_SNIPPET_LINES).collect();
    let joined = lines.join("\n");
    let mut snippet = trim(&joined).to_string();
    if utf16_len(&snippet) > MAX_SNIPPET_CHARS {
        snippet = format!("{}…", utf16_prefix(&snippet, MAX_SNIPPET_CHARS));
    }
    (!snippet.is_empty()).then_some(snippet)
}

/// A statement's pretty print as babel prints it alone. A for-head
/// `VariableDeclaration` has no parent then: babel adds the terminating
/// semicolon and, when a declarator has an init and there are several,
/// breaks the declarators onto indented lines (`commaSeparatorWithNewline`
/// — inline in the head because `isFor(parent)` suppressed it).
fn statement_code(nodes: &AstNodes<'_>, view: &TextView<'_>, stmt: NodeId) -> String {
    let span = nodes.get_node(stmt).span();
    if !needs_standalone_semicolon(nodes, stmt) {
        return view.pretty(span, &[], true);
    }
    let mut edits = Vec::new();
    if let AstKind::VariableDeclaration(d) = nodes.kind(stmt)
        && d.declarations.len() > 1
        && d.declarations.iter().any(|x| x.init.is_some())
    {
        for w in d.declarations.windows(2) {
            edits.push(super::generate::Replacement {
                span: Span::new(w[0].span.end, w[1].span.start),
                text: ",\n  ".to_string(),
            });
        }
    }
    let mut code = view.pretty(span, &edits, true);
    code.push(';');
    code
}

/// `extractAssignmentTargetName`: by NAME, never by binding.
fn assignment_target_name<'n>(
    left: &'n oxc_ast::ast::AssignmentTarget<'_>,
    names: &HashMap<&str, usize>,
) -> Option<&'n str> {
    use oxc_ast::ast::AssignmentTarget as T;
    let object = match left {
        T::AssignmentTargetIdentifier(id) => {
            return names
                .contains_key(id.name.as_str())
                .then_some(id.name.as_str());
        }
        T::StaticMemberExpression(m) => &m.object,
        T::ComputedMemberExpression(m) => &m.object,
        T::PrivateFieldExpression(m) => &m.object,
        _ => return None,
    };
    let object = unparen(object);
    // x.prototype.m — drill through one level.
    if let Some(inner) = member_object(object)
        && let Expression::Identifier(id) = unparen(inner)
        && names.contains_key(id.name.as_str())
    {
        return Some(id.name.as_str());
    }
    if let Expression::Identifier(id) = object
        && names.contains_key(id.name.as_str())
    {
        return Some(id.name.as_str());
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

/// `collectAssignmentContext`: every AssignmentExpression, pre-order.
fn collect_assignment_context(
    nodes: &AstNodes<'_>,
    view: &TextView<'_>,
    names: &HashMap<&str, usize>,
    texts: &mut [MbText],
) {
    for node in nodes.iter() {
        let AstKind::AssignmentExpression(a) = node.kind() else {
            continue;
        };
        let Some(name) = assignment_target_name(&a.left, names) else {
            continue;
        };
        let i = names[name];
        if texts[i].assignments.len() >= MAX_CONTEXT_SNIPPETS {
            continue;
        }
        let code = match find_statement_ancestor(nodes, node.id()) {
            Some(stmt) => statement_code(nodes, view, stmt),
            None => view.pretty(node.span(), &[], true),
        };
        if let Some(snippet) = truncate_snippet(&code)
            && !texts[i].assignments.contains(&snippet)
        {
            texts[i].assignments.push(snippet);
        }
    }
}

/// One babel `Identifier` visit position: its start, the node the
/// ancestor walk starts FROM (the identifier's parent, or the owner node
/// an embedded name lives in — inclusive), its name, and babel's
/// `isBindingIdentifier()` verdict.
struct IdentPos<'n> {
    start: u32,
    walk_from: NodeId,
    name: &'n str,
    is_binding: bool,
}

/// Every position babel's `Identifier` visitor sees, in traversal order,
/// with the positional `isBindingIdentifier` (lesson 3: TRUE for unary /
/// update arguments and every write target; FALSE for keys, member
/// properties, private names and shorthand-pattern KEYS).
fn identifier_positions<'n>(semantic: &'n Semantic<'_>) -> Vec<IdentPos<'n>> {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let mut out: Vec<IdentPos<'n>> = Vec::new();
    for node in nodes.iter() {
        let id = node.id();
        match node.kind() {
            AstKind::IdentifierReference(r) => {
                let mut parent = nodes.parent_id(id);
                while matches!(nodes.kind(parent), AstKind::ParenthesizedExpression(_)) {
                    parent = nodes.parent_id(parent);
                }
                let write = r
                    .reference_id
                    .get()
                    .is_some_and(|rid| scoping.get_reference(rid).is_write());
                let unary = matches!(
                    nodes.kind(parent),
                    AstKind::UnaryExpression(_) | AstKind::UpdateExpression(_)
                );
                out.push(IdentPos {
                    start: r.span.start,
                    walk_from: nodes.parent_id(id),
                    name: r.name.as_str(),
                    is_binding: write || unary,
                });
            }
            AstKind::BindingIdentifier(b) => out.push(IdentPos {
                start: b.span.start,
                walk_from: nodes.parent_id(id),
                name: b.name.as_str(),
                is_binding: true,
            }),
            AstKind::StaticMemberExpression(m) => out.push(IdentPos {
                start: m.property.span.start,
                walk_from: id,
                name: m.property.name.as_str(),
                is_binding: false,
            }),
            AstKind::ObjectProperty(p) => push_key(&mut out, &p.key, id),
            AstKind::MethodDefinition(m) => push_key(&mut out, &m.key, id),
            AstKind::PropertyDefinition(p) => push_key(&mut out, &p.key, id),
            AstKind::BindingProperty(p) => push_key(&mut out, &p.key, id),
            AstKind::AssignmentTargetPropertyProperty(p) => push_key(&mut out, &p.name, id),
            AstKind::AssignmentTargetPropertyIdentifier(p) => out.push(IdentPos {
                // The shorthand's KEY (babel clones it apart from the
                // value): a non-binding position.
                start: p.binding.span.start,
                walk_from: id,
                name: p.binding.name.as_str(),
                is_binding: false,
            }),
            AstKind::PrivateIdentifier(p) => out.push(IdentPos {
                start: p.span.start,
                walk_from: nodes.parent_id(id),
                name: p.name.as_str(),
                is_binding: false,
            }),
            AstKind::BreakStatement(b) => {
                if let Some(l) = &b.label {
                    out.push(IdentPos {
                        start: l.span.start,
                        walk_from: id,
                        name: l.name.as_str(),
                        is_binding: false,
                    });
                }
            }
            AstKind::ContinueStatement(c) => {
                if let Some(l) = &c.label {
                    out.push(IdentPos {
                        start: l.span.start,
                        walk_from: id,
                        name: l.name.as_str(),
                        is_binding: false,
                    });
                }
            }
            _ => {}
        }
    }
    // Traversal order: by start (a shorthand key precedes its value —
    // babel visits key then value; a stable sort keeps that).
    out.sort_by_key(|p| p.start);
    out
}

fn push_key<'n>(out: &mut Vec<IdentPos<'n>>, key: &'n PropertyKey<'_>, owner: NodeId) {
    if let PropertyKey::StaticIdentifier(k) = key {
        out.push(IdentPos {
            start: k.span.start,
            walk_from: owner,
            name: k.name.as_str(),
            is_binding: false,
        });
    }
}

/// `collectUsageExamples`: the statement (or declaration) around each
/// non-binding identifier position named like a module binding, capped so
/// assignments + usages stay within ten.
fn collect_usage_examples(
    semantic: &Semantic<'_>,
    view: &TextView<'_>,
    names: &HashMap<&str, usize>,
    texts: &mut [MbText],
) {
    let nodes = semantic.nodes();
    for pos in identifier_positions(semantic) {
        let Some(&i) = names.get(pos.name) else {
            continue;
        };
        let remaining = MAX_CONTEXT_SNIPPETS.saturating_sub(texts[i].assignments.len());
        if texts[i].usages.len() >= remaining || pos.is_binding {
            continue;
        }
        let stmt = std::iter::once(pos.walk_from)
            .chain(nodes.ancestor_ids(pos.walk_from))
            .find(|&a| is_babel_statement(nodes, a) || is_babel_declaration(nodes, a));
        let Some(stmt) = stmt else { continue };
        let code = statement_code(nodes, view, stmt);
        if code.is_empty() {
            continue;
        }
        if let Some(snippet) = truncate_snippet(&code)
            && !texts[i].usages.contains(&snippet)
        {
            texts[i].usages.push(snippet);
        }
    }
}
