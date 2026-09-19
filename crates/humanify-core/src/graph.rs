//! The unified function graph (WP1.4) — TS original:
//! `src/analysis/function-graph.ts` (buildFunctionGraph +
//! buildUnifiedGraph's function half).
//!
//! Three passes carried over exactly:
//! 1. collect every function-like node (babel's `Function` visitor set =
//!    declarations, expressions, arrows, object/class/private methods)
//!    with its fingerprint hash (the canonical serializer's MatchKey) and
//!    its placeholder slots;
//! 2. analyze call expressions: identifier callees resolve through the
//!    symbol table to the callee function node (a function binding, or a
//!    var declarator whose init is the function); member callees record
//!    the method name as external; function-like callees (IIFEs) edge
//!    directly;
//! 3. scope nesting: a nested function depends on its immediate parent
//!    function even without call relationships (`scopeParent` — processing
//!    order, never fingerprint callee shapes).
//!
//! Identity: oxc spans ARE UTF-8 byte offsets, so a node's span indexes
//! the shipped text directly (07 §1's payoff). Session ids are
//! `input.js:LINE:COL` (babel's loc convention: 1-based lines, 0-based
//! columns), computed from the byte offset.
//!
//! Every function's hash runs the canonical serializer over ITS OWN ESTree
//! JSON (the `ESTree` trait is public on every node type — no per-type
//! walk); the walk's placeholder mapping carries symbol ids, so slots join
//! to declaration spans by identity (scoping.symbol_span), never by name.

use std::collections::{BTreeSet, HashMap};

use oxc_ast::AstKind;
use oxc_estree::{CompactSerializer, ESTree};
use oxc_semantic::{AstNode, AstNodes, NodeId, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};
use serde::Deserialize;
use serde_json::Value;

use crate::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize};

/// A function node in the graph (the FunctionNode's ported fields — the
/// matching/naming phases add the rest).
#[derive(Debug)]
pub struct GraphFunction {
    /// `input.js:LINE:COL` — babel's loc convention on the shipped text.
    pub session_id: String,
    /// The function node's span (UTF-8 byte offsets into the shipped text).
    pub span: Span,
    /// The NAME binding's span: the function's own id when it has one.
    pub name_binding: Option<Span>,
    pub name: String,
    /// The MatchKey hash (blurred literals) — the structuralHash.
    pub structural_hash: String,
    /// Spans of the internal callees, sorted (Set iteration must not leak).
    pub internal_callees: Vec<Span>,
    /// Names of library/builtin calls.
    pub external_callees: BTreeSet<String>,
    /// The scope parent's span, when nested.
    pub scope_parent: Option<Span>,
    /// The placeholder slots: (slot, decl span, name).
    pub placeholder_bindings: Vec<(String, Span, String)>,
}

/// The built graph (the function half; module bindings arrive with their
/// pass).
#[derive(Debug)]
pub struct FunctionGraph {
    pub functions: Vec<GraphFunction>,
}

/// Is this node kind one babel's `Function` visitor visits?
fn is_function_kind(kind: AstKind<'_>) -> bool {
    matches!(
        kind,
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::MethodDefinition(_)
    )
}

/// The function's NAME binding identifier: the id of declarations and
/// named expressions; a method's key. Arrows have none (their name comes
/// from the variable they're assigned to, which is the declarator's
/// binding — resolved via pass 2's symbol path, not here).
fn function_name_binding(kind: AstKind<'_>) -> Option<(Span, String)> {
    match kind {
        AstKind::Function(f) => {
            let id = f.id.as_ref()?;
            Some((id.span, id.name.to_string()))
        }
        AstKind::ArrowFunctionExpression(_) => None,
        // oxc 0.150 unifies object/class/private methods into
        // MethodDefinition; babel visits each — the visitor-set equivalence
        // is by kind, not name.
        AstKind::MethodDefinition(m) => {
            // A static identifier key: its span + name. String/number
            // keys are content (not bindings) — babel's Function visitor
            // covers the method either way; the NAME binding is only the
            // identifier form.
            if let oxc_ast::ast::PropertyKey::StaticIdentifier(ident) = &m.key {
                return Some((ident.span, ident.name.to_string()));
            }
            None
        }
        _ => None,
    }
}

/// The babel loc line:col for a byte offset (1-based line, 0-based column).
fn line_col_of(offset: u32, line_starts: &[u32]) -> (u32, u32) {
    let mut line = 0usize;
    for (i, start) in line_starts.iter().enumerate() {
        if *start <= offset {
            line = i;
        } else {
            break;
        }
    }
    (line as u32 + 1, offset - line_starts[line])
}

/// One function's collected entry (pass 1's record).
struct FnEntry {
    node_id: NodeId,
    span: Span,
    name_binding: Option<(Span, String)>,
    hash: String,
    slots: Vec<(String, Span, String)>,
}

/// Serialize one AST node to ESTree JSON via the public trait.
macro_rules! serialize_node_json {
    ($ser:expr, $node:expr) => {{
        $node.serialize(&mut $ser);
    }};
}

/// Every ancestor function of a node, as NODE IDS — the ONE owner of the
/// "which functions enclose this node" question (docs/responsibility.md).
/// Consumers map ids to whatever identity they need (the graph maps to
/// entry indices; the eval/with taint records the spans).
pub(crate) fn enclosing_function_node_ids(node_id: NodeId, nodes: &AstNodes<'_>) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut prev = node_id;
    let mut parent_id = nodes.parent_id(prev);
    while parent_id != prev {
        if is_function_kind(nodes.get_node(parent_id).kind()) {
            out.push(parent_id);
        }
        prev = parent_id;
        parent_id = nodes.parent_id(parent_id);
    }
    out
}

/// Every ancestor function of a node, as entry indices (the recursive-
/// traverse edge semantics: the wrapper contains the whole bundle, so it
/// accumulates every call's edge).
fn function_ancestors(
    node: &AstNode<'_>,
    nodes: &AstNodes<'_>,
    idx_by_node: &HashMap<NodeId, usize>,
) -> Vec<usize> {
    enclosing_function_node_ids(node.id(), nodes)
        .into_iter()
        .filter_map(|id| idx_by_node.get(&id).copied())
        .collect()
}

/// Pass 2: attribute every call's edge to every ancestor function (babel's
/// recursive-traverse semantics).
#[allow(clippy::too_many_arguments)]
fn analyze_call_edges(
    nodes: &AstNodes<'_>,
    entries: &[FnEntry],
    functions: &mut [GraphFunction],
    function_by_symbol: &HashMap<SymbolId, usize>,
    tables: &crate::hash::serialize::SymbolTables,
    idx_by_node: &HashMap<NodeId, usize>,
) {
    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        // THE EDGE SEMANTICS (babel's analyzeCallees is a RECURSIVE
        // traverse of each function's subtree): every ANCESTOR function of
        // a call accumulates the edge — the wrapper (containing the whole
        // bundle) accumulates all of them. Attribute to the whole chain.
        let caller_indices = function_ancestors(node, nodes, idx_by_node);
        if caller_indices.is_empty() {
            continue;
        }
        match &call.callee {
            oxc_ast::ast::Expression::Identifier(id) => {
                let start = id.span().start;
                let symbol = tables
                    .ref_by_start
                    .get(&start)
                    .or_else(|| tables.decl_by_start.get(&start))
                    .copied();
                match symbol.and_then(|s| function_by_symbol.get(&s).copied()) {
                    Some(target_idx) => {
                        for &caller_idx in &caller_indices {
                            functions[caller_idx]
                                .internal_callees
                                .push(entries[target_idx].span);
                        }
                    }
                    None => {
                        for &caller_idx in &caller_indices {
                            functions[caller_idx]
                                .external_callees
                                .insert(id.name.to_string());
                        }
                    }
                }
            }
            // oxc 0.150 splits member access into three types (each
            // ESTree-renamed "MemberExpression"); the external-callee rule
            // reads the property name off the static/computed shapes.
            oxc_ast::ast::Expression::StaticMemberExpression(m) => {
                for &caller_idx in &caller_indices {
                    functions[caller_idx]
                        .external_callees
                        .insert(m.property.name.to_string());
                }
            }
            oxc_ast::ast::Expression::ComputedMemberExpression(m) => {
                let name = match &m.expression {
                    oxc_ast::ast::Expression::StringLiteral(s) => Some(s.value.to_string()),
                    _ => None,
                };
                if let Some(n) = &name {
                    for &caller_idx in &caller_indices {
                        functions[caller_idx].external_callees.insert(n.clone());
                    }
                }
            }
            oxc_ast::ast::Expression::PrivateFieldExpression(_) => {
                // A private method call is never an external callee name.
            }
            _ => {
                // An IIFE or other callee shape: edge the callee's function
                // node when it is one of ours. The callee may be
                // PARENTHESIZED (oxc keeps parens as nodes; babel's callee
                // is the function node directly) — unwrap first.
                let mut callee = &call.callee;
                // (the shared paren view — Babel drops the wrappers)
                callee = crate::babel_view::unparen(callee);
                let callee_span = callee.span();
                if let Some(target_idx) = entries.iter().position(|f| f.span == callee_span) {
                    for &caller_idx in &caller_indices {
                        functions[caller_idx]
                            .internal_callees
                            .push(entries[target_idx].span);
                    }
                }
            }
        }
    }
}

/// Build the function graph over the semantic.
pub fn build_function_graph(semantic: &Semantic<'_>, file_name: &str) -> FunctionGraph {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let text = semantic.source_text();

    // Byte offset -> line starts, for the session-id convention.
    let mut line_starts: Vec<u32> = vec![0];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            line_starts.push(i as u32 + 1);
        }
    }

    // --- pass 1: collect functions -------------------------------------
    let mut entries: Vec<FnEntry> = Vec::new();
    let mut idx_by_node: HashMap<NodeId, usize> = HashMap::new();
    for node in nodes.iter() {
        if !is_function_kind(node.kind()) {
            continue;
        }
        let entry = FnEntry {
            node_id: node.id(),
            span: node.span(),
            name_binding: function_name_binding(node.kind()),
            hash: String::new(),
            slots: Vec::new(),
        };
        idx_by_node.insert(node.id(), entries.len());
        entries.push(entry);
    }

    // The per-function ESTree JSON + canonical hash. The ESTree trait is
    // public on every node type: serialize each function's own subtree.
    let tables = SymbolTables::build(semantic);
    for entry in &mut entries {
        let node = nodes.get_node(entry.node_id);
        let mut ser = CompactSerializer::new(false, false);
        match node.kind() {
            AstKind::Function(f) => serialize_node_json!(ser, *f),
            AstKind::ArrowFunctionExpression(a) => serialize_node_json!(ser, *a),
            AstKind::MethodDefinition(m) => serialize_node_json!(ser, *m),
            _ => {}
        }
        let json = ser.into_string();
        let mut de = serde_json::Deserializer::from_str(&json);
        de.disable_recursion_limit();
        let subtree: Value = Deserialize::deserialize(&mut de).unwrap_or(Value::Null);
        let out = canonical_serialize(&subtree, &tables, LiteralPolicy::Blurred);
        entry.hash = out.hash;
        // Slots: the mapping's symbol id -> the DECLARATION span via the
        // scoping (identity, never name).
        entry.slots = out
            .mapping
            .into_iter()
            .map(|(slot, symbol, name)| {
                let span = symbol
                    .map(|sid| {
                        let decl_node = scoping.symbol_declaration(sid);
                        nodes.get_node(decl_node).span()
                    })
                    .unwrap_or_default();
                (slot, span, name)
            })
            .collect();
    }

    let mut functions: Vec<GraphFunction> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let (line, col) = line_col_of(entry.span.start, &line_starts);
        functions.push(GraphFunction {
            session_id: format!("{file_name}:{line}:{col}"),
            span: entry.span,
            name_binding: entry.name_binding.as_ref().map(|(s, _)| *s),
            name: entry
                .name_binding
                .as_ref()
                .map(|(_, n)| n.clone())
                .unwrap_or_default(),
            structural_hash: entry.hash.clone(),
            internal_callees: Vec::new(),
            external_callees: BTreeSet::new(),
            scope_parent: None,
            placeholder_bindings: entry.slots.clone(),
        });
    }

    // --- pass 2: call edges --------------------------------------------
    // The symbol -> function entry map: the symbol's DECLARATION resolves
    // to a function (its own node), or to a var declarator whose INIT is
    // the function (the TS isFunctionBinding + declarator-init path).
    let mut function_by_symbol: HashMap<SymbolId, usize> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        let node = nodes.get_node(entry.node_id);
        // The function's own binding symbol: the id's symbol for
        // declarations / named expressions; for arrows and anonymous
        // expressions, the DECLARATOR's binding (resolved below from the
        // parent chain).
        if let Some((span, _)) = &entry.name_binding
            && let Some(symbol) = tables.decl_by_start.get(&span.start)
        {
            function_by_symbol.insert(*symbol, i);
        }
        // The declarator-init path: the IMMEDIATE parent declarator (if
        // any) registers its binding's symbol -> this function. Only
        // function bindings + declarator inits register (babel's
        // handleIdentifierCallee edges those, nothing else).
        let pid = nodes.parent_id(node.id());
        if let AstKind::VariableDeclarator(d) = nodes.get_node(pid).kind()
            && let Some(symbol) = tables.decl_by_start.get(&d.id.span().start)
        {
            function_by_symbol.entry(*symbol).or_insert(i);
        }
    }
    analyze_call_edges(
        nodes,
        &entries,
        &mut functions,
        &function_by_symbol,
        &tables,
        &idx_by_node,
    );

    // --- pass 3: scope nesting -----------------------------------------
    for (i, entry) in entries.iter().enumerate() {
        let node = nodes.get_node(entry.node_id);
        if let Some(parent_idx) = nearest_function_ancestor(node, nodes, &idx_by_node)
            && parent_idx != i
        {
            functions[i].scope_parent = Some(entries[parent_idx].span);
        }
    }

    // Dedupe + sort the callee spans (no Set iteration may leak).
    for f in &mut functions {
        f.internal_callees.sort_by_key(|s| (s.start, s.end));
        f.internal_callees.dedup();
    }

    FunctionGraph { functions }
}

/// The nearest enclosing function of a node, by parent walk.
fn nearest_function_ancestor(
    node: &AstNode<'_>,
    nodes: &AstNodes<'_>,
    idx_by_node: &HashMap<NodeId, usize>,
) -> Option<usize> {
    // parent_id is a plain NodeId (the root self-parents, which
    // terminates the walk).
    let mut prev = node.id();
    let mut parent_id = nodes.parent_id(prev);
    while parent_id != prev {
        let parent = nodes.get_node(parent_id);
        if is_function_kind(parent.kind()) {
            return idx_by_node.get(&parent_id).copied();
        }
        prev = parent_id;
        parent_id = nodes.parent_id(parent_id);
    }
    None
}

/// The WP1.4 gate's Rust-side functions dump: rebuild `functions.json`'s
/// kind=function rows (the graph edges + scope parents + hashes) into a
/// Rust-side dump dir the differ can compare. The module-binding rows
/// land with WP1.5 (the bun classification owns their member set).
pub mod functions_dump {
    use std::fs;
    use std::path::Path;

    use oxc_allocator::Allocator;
    use serde_json::{Value, json};

    use crate::graph::build_function_graph;

    pub fn dump_functions(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
        let meta_text = fs::read_to_string(ts_dump_dir.join("meta.json"))
            .map_err(|e| format!("meta.json: {e}"))?;
        let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;
        // functions.json's rows anchor the FRESH text (the beautified
        // input the rename-era decisions consumed) — NOT the shipped text
        // the split-era sections anchor.
        let fresh = fs::read_to_string(ts_dump_dir.join("text").join("fresh.js"))
            .map_err(|e| format!("fresh: {e}"))?;

        let allocator = Allocator::default();
        let ingest = crate::ingest::Ingest::parse(&allocator, &fresh, "fresh.js");
        if !ingest.errors.is_empty() {
            return Err(format!("oxc: {} diagnostic(s)", ingest.errors.len()));
        }
        let graph = build_function_graph(&ingest.semantic, "input.js");

        let rows: Vec<Value> = graph
            .functions
            .iter()
            .map(|f| {
                json!({
                    "key": {"text": "fresh", "start": f.span.start, "end": f.span.end},
                    "sessionId": f.session_id,
                    "kind": "function",
                    "name": f.name,
                    "nameBinding": f.name_binding.map(|s| json!({"text": "fresh", "start": s.start, "end": s.end})),
                    "structuralHash": f.structural_hash,
                    "internalCallees": f.internal_callees.iter()
                        .map(|s| json!({"text": "fresh", "start": s.start, "end": s.end}))
                        .collect::<Vec<_>>(),
                    "scopeParent": f.scope_parent.map(|s| json!({"text": "fresh", "start": s.start, "end": s.end})),
                    "bindings": f.placeholder_bindings.iter()
                        .map(|(slot, span, name)| json!({
                            "slot": slot,
                            "span": {"text": "fresh", "start": span.start, "end": span.end},
                            "name": name
                        }))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut rows = rows;
        rows.sort_by(|a, b| {
            let ak = (
                a["key"]["start"].as_u64().unwrap_or(0),
                a["key"]["end"].as_u64().unwrap_or(0),
            );
            let bk = (
                b["key"]["start"].as_u64().unwrap_or(0),
                b["key"]["end"].as_u64().unwrap_or(0),
            );
            ak.cmp(&bk)
        });

        fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
        fs::write(
            out_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .map_err(|e| format!("write meta: {e}"))?;
        fs::write(
            out_dir.join("functions.json"),
            serde_json::to_string(&json!({"schemaVersion": 1, "functions": rows})).unwrap(),
        )
        .map_err(|e| format!("write functions: {e}"))?;
        Ok(rows.len())
    }
}
