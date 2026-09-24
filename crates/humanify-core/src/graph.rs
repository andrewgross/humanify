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
//! JSON — its subtree of the side's one parsed program JSON, which is the
//! node's own serialization (no per-type walk); the walk's placeholder
//! mapping carries symbol ids, so slots join to declaration spans by
//! identity (scoping.symbol_span), never by name.

use std::collections::{BTreeSet, HashMap};

use oxc_ast::AstKind;
use oxc_estree::{CompactSerializer, ESTree};
use oxc_semantic::{AstNode, AstNodes, NodeId, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};
use oxc_syntax::reference::ReferenceFlags;
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
    /// The fingerprint's structural features (the TS computes them at
    /// graph build — computeFingerprintAndPlaceholders), from the same row
    /// JSON the hash walks. NOT dumped.
    pub features: crate::matching::StructuralFeatures,
}

/// The built graph (the function half; module bindings arrive with their
/// pass).
#[derive(Debug)]
pub struct FunctionGraph {
    pub functions: Vec<GraphFunction>,
}

/// The binding's declaration shape — `isMatchableBinding`'s second half
/// (prior-version.ts:1444-1453). A declarator whose init is a function,
/// arrow or CLASS EXPRESSION is matched by the function cascade (with
/// var-name transfers); matching it here by init hash would compete with
/// the better-informed function matcher. Not dumped (the moduleBindingRow
/// carries internalCallees only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclaratorInit {
    /// The declaration is not a variable declarator — a class declaration
    /// hashes itself and stays matchable (:1446 `!isVariableDeclarator()
    /// → true`).
    NotDeclarator,
    /// A variable declarator with no initializer (:1448 `if (!init)
    /// return true`).
    NoInit,
    /// A variable declarator whose init is a function or arrow expression
    /// (:1450-1451) — NOT matchable.
    FunctionInit,
    /// A variable declarator whose init is a class expression (:1452) —
    /// NOT matchable.
    ClassInit,
    /// A variable declarator with any other initializer — matchable.
    OtherInit,
}

impl DeclaratorInit {
    /// TS `isMatchableBinding`'s init verdict (:1449-1453).
    pub fn is_matchable(self) -> bool {
        !matches!(
            self,
            DeclaratorInit::FunctionInit | DeclaratorInit::ClassInit
        )
    }
}

/// One module-level binding node (ModuleBindingNode's dumped fields plus
/// the alternation's identity inputs — the NOT-dumped fields are marked).
#[derive(Debug)]
pub struct ModuleBindingNode {
    /// `module:<name>` — the minified binding name.
    pub session_id: String,
    /// The binding identifier's span (the row key + nameBinding).
    pub span: Span,
    pub name: String,
    /// The binding's resolved symbol — the babel `Binding` OBJECT the TS
    /// keys its reference-identity maps by (prior-version.ts:1748-1807);
    /// oxc's SymbolId is the same resolved-identity standard the hash
    /// placeholders use (07 §1). NOT dumped.
    pub symbol: SymbolId,
    /// Spans of the dependency nodes — other module bindings' identifiers
    /// and functions (wireModuleBindingCallees copies the dependency set).
    pub internal_callees: Vec<Span>,
    /// The functions that REFERENCE the binding — edge builder 4d
    /// (addFunctionToBindingReferenceEdges, function-graph.ts:689): every
    /// resolved reference's nearest enclosing graph function's span.
    /// `callerFnIds` (prior-version.ts:1465) and the binding fingerprint's
    /// callerShapes both read it. NOT dumped (the moduleBindingRow carries
    /// internalCallees only).
    pub callers: Vec<Span>,
    /// The declaration shape (see [`DeclaratorInit`]) — NOT dumped.
    pub declarator_init: DeclaratorInit,
    /// The OTHER declarators of the same name in the container (the babel
    /// `constantViolations` redeclarations, minus the binding's own
    /// declaration, sorted + deduped) — the content resolver's
    /// first-violation rule reads them. NOT dumped.
    pub redeclared_spans: Vec<u32>,
    /// The binding-match fingerprint's hash (chunk 2 — the literal-preserving
    /// computeBindingFingerprint; None until that port lands).
    pub fingerprint_hash: Option<String>,
}

/// The unified graph: functions + module-level bindings (the TS
/// buildUnifiedGraph's two node kinds).
#[derive(Debug)]
pub struct UnifiedGraph {
    pub functions: Vec<GraphFunction>,
    pub module_bindings: Vec<ModuleBindingNode>,
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
        // Methods have NO id — babel's fnNameIdentifier reads `node.id`
        // only, so the row's name is "" and the nameBinding null, even when
        // the key is an identifier (the row's name is the POST-TRANSFER id
        // name, never the key). Object methods (ObjectProperty method=true
        // or Get/Set) are ObjectProperty — no id either.
        AstKind::MethodDefinition(_) => None,
        _ => None,
    }
}

/// The babel loc line:col for a byte offset (1-based line, 0-based column).
fn line_col_of(offset: u32, line_starts: &[u32]) -> (u32, u32) {
    // The LAST line start <= offset (the starts ascend; the first is 0),
    // by binary search — a linear scan per row was quadratic.
    let line = line_starts
        .partition_point(|start| *start <= offset)
        .saturating_sub(1);
    (line as u32 + 1, offset - line_starts[line])
}

/// One function's collected entry (pass 1's record).
struct FnEntry {
    node_id: NodeId,
    span: Span,
    name_binding: Option<(Span, String)>,
    hash: String,
    slots: Vec<(String, Span, String)>,
    features: crate::matching::StructuralFeatures,
}

/// Serialize one AST node to ESTree JSON via the public trait.
macro_rules! serialize_node_json {
    ($ser:expr, $node:expr) => {{
        $node.serialize(&mut $ser);
    }};
}

/// One graph-entry node's canonical serialization — the pass-1 code path
/// (ESTree JSON → canonical token stream) as a function, so the WP2.1 hash
/// probe reproduces row hashes and can diff the token stream itself
/// (the stream is the diagnostics surface: [`CanonicalOutput::parts`]).
pub(crate) fn hash_entry_subtree(
    nodes: &AstNodes<'_>,
    node_id: NodeId,
    tables: &SymbolTables,
) -> crate::hash::serialize::CanonicalOutput {
    hash_entry_json(&entry_subtree_json(nodes, node_id), tables)
}

/// A graph-entry node's ESTree JSON — the half of [`hash_entry_subtree`]
/// that reads the AST (so it runs on the thread that owns the arena).
pub(crate) fn entry_subtree_json(nodes: &AstNodes<'_>, node_id: NodeId) -> String {
    let node = nodes.get_node(node_id);
    let mut ser = CompactSerializer::new(false, false);
    match node.kind() {
        AstKind::Function(f) => serialize_node_json!(ser, *f),
        AstKind::ArrowFunctionExpression(a) => serialize_node_json!(ser, *a),
        AstKind::MethodDefinition(m) => serialize_node_json!(ser, *m),
        // Object methods: babel's ObjectMethod node — oxc's property
        // (key included in the span; the hash covers the property).
        AstKind::ObjectProperty(p) => serialize_node_json!(ser, *p),
        _ => {}
    }
    ser.into_string()
}

/// The pure half of [`hash_entry_subtree`]: parse the entry's JSON and
/// canonicalize it.
fn hash_entry_json(json: &str, tables: &SymbolTables) -> crate::hash::serialize::CanonicalOutput {
    canonical_serialize(
        &crate::ingest::parse_estree_json(json),
        tables,
        LiteralPolicy::Blurred,
    )
}

/// The ESTree `type` names a graph entry's own JSON node carries, per
/// entry kind (oxc's object methods serialize as "Property").
pub(crate) fn entry_json_types(kind: &AstKind<'_>) -> &'static [&'static str] {
    match kind {
        AstKind::Function(_) => &["FunctionDeclaration", "FunctionExpression"],
        AstKind::ArrowFunctionExpression(_) => &["ArrowFunctionExpression"],
        AstKind::MethodDefinition(_) => &["MethodDefinition"],
        AstKind::ObjectProperty(_) => &["Property"],
        _ => &[],
    }
}

/// The graph-entry JSON nodes under `root` by (start, end), in pre-order
/// (object keys in serde's BTreeMap order, arrays in order) — only nodes
/// whose type some entry kind carries ([`entry_json_types`]).
fn index_entry_json<'v>(root: &'v Value, out: &mut HashMap<(u32, u32), Vec<&'v Value>>) {
    match root {
        Value::Object(fields) => {
            if let (true, Some(s), Some(e)) = (
                ENTRY_JSON_TYPES.contains(&json_type_of(root)),
                fields.get("start").and_then(Value::as_u64),
                fields.get("end").and_then(Value::as_u64),
            ) {
                out.entry((s as u32, e as u32)).or_default().push(root);
            }
            for (_, v) in fields.iter() {
                index_entry_json(v, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|v| index_entry_json(v, out)),
        _ => {}
    }
}

/// The union of [`entry_json_types`].
const ENTRY_JSON_TYPES: [&str; 5] = [
    "FunctionDeclaration",
    "FunctionExpression",
    "ArrowFunctionExpression",
    "MethodDefinition",
    "Property",
];

fn json_type_of(value: &Value) -> &str {
    value.get("type").and_then(Value::as_str).unwrap_or("")
}

/// An entry's node in the program JSON: the first in pre-order with the
/// entry's span and one of its kind's types.
fn find_entry_json<'v>(
    index: &HashMap<(u32, u32), Vec<&'v Value>>,
    nodes: &AstNodes<'_>,
    entry: &FnEntry,
) -> Option<&'v Value> {
    let types = entry_json_types(&nodes.get_node(entry.node_id).kind());
    index
        .get(&(entry.span.start, entry.span.end))?
        .iter()
        .copied()
        .find(|v| types.contains(&json_type_of(v)))
}

/// One entry's row outputs: the canonical hash, the placeholder mapping
/// (slot, symbol, name) and the structural features.
type EntryOutput = (
    String,
    Vec<(String, Option<SymbolId>, String)>,
    crate::matching::StructuralFeatures,
);

/// Each entry's [`EntryOutput`], parallel to `entries`, from its row JSON.
///
/// The row JSON is the entry's subtree of the side's ONE program JSON
/// ([`crate::ingest::program_estree_json`]), found by span + type: a
/// node's ESTree serialization does not depend on its parent, so the
/// subtree IS the per-node JSON [`entry_subtree_json`] builds, plus the
/// `range` pairs both walks skip — without re-serializing every nested
/// function once per ancestor (the wrapper's JSON alone is the whole
/// bundle). An entry with no such node falls back to its own
/// serialization. The per-row walks are pure and run on the pool, their
/// outputs back in entry order.
fn entry_outputs(
    nodes: &AstNodes<'_>,
    entries: &[FnEntry],
    tables: &SymbolTables,
    program_json: &Value,
) -> Vec<EntryOutput> {
    let mut index = HashMap::new();
    index_entry_json(program_json, &mut index);
    let fallbacks: Vec<Option<Value>> = entries
        .iter()
        .map(|entry| {
            find_entry_json(&index, nodes, entry).is_none().then(|| {
                crate::ingest::parse_estree_json(&entry_subtree_json(nodes, entry.node_id))
            })
        })
        .collect();
    let rows: Vec<&Value> = entries
        .iter()
        .zip(&fallbacks)
        .map(|(entry, fallback)| {
            fallback
                .as_ref()
                .or_else(|| find_entry_json(&index, nodes, entry))
                .expect("an entry without a program node has a fallback")
        })
        .collect();
    crate::par::map_ordered(&rows, |row| {
        // The token stream (`parts`) is diagnostics only — dropped here,
        // on the pool (an outer function's stream contains every nested
        // one's; kept, they would total the bundle times its depth).
        let out = canonical_serialize(row, tables, LiteralPolicy::Blurred);
        (
            out.hash,
            out.mapping,
            crate::matching::features::structural_features_of(row, tables),
        )
    })
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
        if is_graph_entry_kind(&nodes.get_node(parent_id).kind()) {
            out.push(parent_id);
        }
        prev = parent_id;
        parent_id = nodes.parent_id(parent_id);
    }
    out
}

/// babel's `Function` alias — the node kinds whose parent-stop semantics
/// both the taint walk (getFunctionParent) and the edge attribution use:
/// FunctionDeclaration | FunctionExpression | ObjectMethod |
/// ArrowFunctionExpression | ClassMethod | ClassPrivateMethod. oxc's
/// object methods are ObjectProperty with method=true or a Get/Set kind.
fn is_graph_entry_kind(kind: &AstKind<'_>) -> bool {
    match kind {
        AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => true,
        AstKind::MethodDefinition(_) => true,
        AstKind::ObjectProperty(p) => p.method || p.kind != oxc_ast::ast::PropertyKind::Init,
        _ => false,
    }
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

/// Record an external callee name on every caller (a set: the name is
/// allocated only for a caller that does not hold it yet).
fn add_external_callee(functions: &mut [GraphFunction], callers: &[usize], name: &str) {
    for &caller_idx in callers {
        let set = &mut functions[caller_idx].external_callees;
        if !set.contains(name) {
            set.insert(name.to_string());
        }
    }
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
    visit_optional_calls: bool,
) {
    let entry_spans: std::collections::HashSet<(u32, u32)> =
        entries.iter().map(|e| (e.span.start, e.span.end)).collect();
    for node in nodes.iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        // Babel parses `x?.()` as an OptionalCallExpression — a node type
        // whose alias list does NOT include CallExpression — so the TS's
        // CallExpression-only visitor NEVER visits optional calls: no
        // internal edge AND no external name (the Pp9 one-edge divergence's
        // real mechanism; the factory-classification attribution was wrong).
        // oxc folds optional into CallExpression.optional. SKIPPING is the
        // shipped TS's blind spot, kept for parity; `visit_optional_calls`
        // is the SIZING probe for fixing it (how many matches the ~250
        // missing identifier-optional edges would earn).
        if call.optional && !visit_optional_calls {
            continue;
        }
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
                    None => add_external_callee(functions, &caller_indices, &id.name),
                }
            }
            // oxc 0.150 splits member access into three types (each
            // ESTree-renamed "MemberExpression"); the external-callee rule
            // reads the property name off the static/computed shapes.
            oxc_ast::ast::Expression::StaticMemberExpression(m) => {
                add_external_callee(functions, &caller_indices, &m.property.name);
            }
            oxc_ast::ast::Expression::ComputedMemberExpression(m) => {
                let name = match &m.expression {
                    oxc_ast::ast::Expression::StringLiteral(s) => Some(s.value.to_string()),
                    _ => None,
                };
                if let Some(n) = &name {
                    add_external_callee(functions, &caller_indices, n);
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
                // An entry with the callee's span edges it (the span IS the
                // entry's span — a set lookup, not a scan of the entries).
                if entry_spans.contains(&(callee_span.start, callee_span.end)) {
                    for &caller_idx in &caller_indices {
                        functions[caller_idx].internal_callees.push(callee_span);
                    }
                }
            }
        }
    }
}

/// Build the function graph over the semantic.
///
/// `factories` = the Bun CJS classification's factory records (empty when
/// the bundle has no CJS factory helper): functions inside any factory
/// body are THIRD-PARTY and are skipped (the TS buildFunctionGraph's
/// classification skip — the member set must match the oracle's).
pub fn build_function_graph(
    semantic: &Semantic<'_>,
    file_name: &str,
    factories: &[crate::modules::FactoryRecord],
) -> (FunctionGraph, HashMap<SymbolId, usize>) {
    build_function_graph_opts(semantic, file_name, factories, false)
}

/// The sizing-probe variant: `visit_optional_calls` = the FIX for babel's
/// optional-call blind spot (visit `x?.()` like a normal call). Parity
/// runs pass false (the shipped TS never visits them).
pub fn build_function_graph_opts(
    semantic: &Semantic<'_>,
    file_name: &str,
    factories: &[crate::modules::FactoryRecord],
    visit_optional_calls: bool,
) -> (FunctionGraph, HashMap<SymbolId, usize>) {
    let program_json = crate::ingest::program_estree_json(semantic.nodes().program());
    build_function_graph_with_json(
        semantic,
        &program_json,
        file_name,
        factories,
        visit_optional_calls,
    )
}

/// [`build_function_graph_opts`] over the side's already-parsed program
/// JSON ([`crate::ingest::program_estree_json`]).
fn build_function_graph_with_json(
    semantic: &Semantic<'_>,
    program_json: &Value,
    file_name: &str,
    factories: &[crate::modules::FactoryRecord],
    visit_optional_calls: bool,
) -> (FunctionGraph, HashMap<SymbolId, usize>) {
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
    let (mut entries, idx_by_node) = collect_function_entries(nodes, factories);

    // The per-function canonical hash + features, over each function's
    // subtree of the side's program JSON ([`entry_outputs`]).
    let tables = SymbolTables::build(semantic);
    let outputs = entry_outputs(nodes, &entries, &tables, program_json);
    for (entry, (hash, mapping, features)) in entries.iter_mut().zip(outputs) {
        entry.hash = hash;
        entry.features = features;
        // Slots: the mapping's symbol id -> the DECLARATION span via the
        // scoping (identity, never name).
        entry.slots = mapping
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

    // The row fields MOVE out of the entries (pass 2/3 read only the
    // entries' node ids, spans and names).
    let mut functions: Vec<GraphFunction> = Vec::with_capacity(entries.len());
    for entry in &mut entries {
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
            structural_hash: std::mem::take(&mut entry.hash),
            internal_callees: Vec::new(),
            external_callees: BTreeSet::new(),
            scope_parent: None,
            placeholder_bindings: std::mem::take(&mut entry.slots),
            features: std::mem::take(&mut entry.features),
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
        visit_optional_calls,
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

    (FunctionGraph { functions }, function_by_symbol)
}

/// Build the UNIFIED graph: the function half plus the module-level
/// bindings with their dependency edges (the TS buildUnifiedGraph's
/// module half — getModuleLevelBindings' node set, edge builders 4a/4b,
/// wireModuleBindingCallees).
///
/// `bundler`/`minifier` feed the eligibility skip-set (the pipeline
/// resolves them from RunConfig; the dump's meta.json flags carry them).
pub fn build_unified_graph(
    semantic: &Semantic<'_>,
    program: &oxc_ast::ast::Program<'_>,
    file_name: &str,
    factories: &[crate::modules::FactoryRecord],
    bundler: Option<&str>,
    minifier: Option<&str>,
) -> UnifiedGraph {
    build_unified_graph_with_eligibility(
        semantic,
        program,
        file_name,
        factories,
        Eligibility::SkipSet { bundler, minifier },
    )
}

/// The module-binding eligibility (the TS's IsEligibleFn): the skip-set
/// filter, or ALL-TRUE — the prior side's graph (prior-version.ts:284-288:
/// prior binding names are all humanified, so every binding is a valid
/// name source).
#[derive(Clone, Copy, Debug)]
pub enum Eligibility<'x> {
    SkipSet {
        bundler: Option<&'x str>,
        minifier: Option<&'x str>,
    },
    All,
}

pub fn build_unified_graph_with_eligibility(
    semantic: &Semantic<'_>,
    program: &oxc_ast::ast::Program<'_>,
    file_name: &str,
    factories: &[crate::modules::FactoryRecord],
    eligibility: Eligibility<'_>,
) -> UnifiedGraph {
    build_unified_graph_with_eligibility_opts(
        semantic,
        program,
        file_name,
        factories,
        eligibility,
        false,
    )
}

/// The sizing-probe variant (see build_function_graph_opts).
pub fn build_unified_graph_with_eligibility_opts(
    semantic: &Semantic<'_>,
    program: &oxc_ast::ast::Program<'_>,
    file_name: &str,
    factories: &[crate::modules::FactoryRecord],
    eligibility: Eligibility<'_>,
    visit_optional_calls: bool,
) -> UnifiedGraph {
    let program_json = crate::ingest::program_estree_json(program);
    build_unified_graph_with_json(
        semantic,
        program,
        &program_json,
        file_name,
        factories,
        eligibility,
        visit_optional_calls,
    )
}

/// [`build_unified_graph_with_eligibility_opts`] over the side's
/// already-parsed program JSON ([`crate::ingest::program_estree_json`]) —
/// the dump shares one per side across every consumer.
pub fn build_unified_graph_with_json(
    semantic: &Semantic<'_>,
    program: &oxc_ast::ast::Program<'_>,
    program_json: &Value,
    file_name: &str,
    factories: &[crate::modules::FactoryRecord],
    eligibility: Eligibility<'_>,
    visit_optional_calls: bool,
) -> UnifiedGraph {
    let (graph, function_by_symbol) = build_function_graph_with_json(
        semantic,
        program_json,
        file_name,
        factories,
        visit_optional_calls,
    );
    let module_bindings = build_module_bindings(
        semantic,
        program,
        factories,
        eligibility,
        &function_by_symbol,
        &graph.functions,
    );
    UnifiedGraph {
        functions: graph.functions,
        module_bindings,
    }
}

/// The module-binding half. The bindings = the WRAPPER scope's (or the
/// program's) direct bindings, eligibility-filtered, with the three
/// shouldSkipBinding skips. Edges: every RESOLVED identifier reference
/// inside a binding's initializer (a) to another module binding →
/// binding→binding, (b) to a function (findFnForBinding's shapes) →
/// binding→function; the row's internalCallees is the whole dependency
/// set (wireModuleBindingCallees).
fn excluded_ref_starts(
    nodes: &AstNodes<'_>,
    scoping: &oxc_semantic::Scoping,
) -> std::collections::HashSet<u32> {
    let mut write_ref_starts: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for symbol in scoping.symbol_ids() {
        for &reference_id in scoping.get_resolved_reference_ids(symbol) {
            if scoping
                .get_reference(reference_id)
                .flags()
                .contains(oxc_syntax::reference::ReferenceFlags::Write)
            {
                let node_id = scoping.get_reference(reference_id).node_id();
                write_ref_starts.insert(nodes.get_node(node_id).span().start);
            }
        }
    }
    for n in nodes.iter() {
        // Only a DIRECT identifier argument is babel's binding position
        // (`!y`, `y++`); `!z9_.call(...)`'s identifier sits inside a
        // member expression — not excluded.
        match n.kind() {
            AstKind::UnaryExpression(u) => {
                if let oxc_ast::ast::Expression::Identifier(id) =
                    crate::babel_view::unparen(&u.argument)
                {
                    write_ref_starts.insert(id.span().start);
                }
            }
            // oxc models the update target as a SimpleAssignmentTarget.
            AstKind::UpdateExpression(u) => {
                if let oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) =
                    &u.argument
                {
                    write_ref_starts.insert(id.span().start);
                }
            }
            _ => {}
        }
    }
    write_ref_starts
}

/// Every IDENTIFIER POSITION in the bundle, as (span start, owning
/// node id, name): reference nodes PLUS the positions babel's
/// Identifier visitor sees that oxc does not model as nodes — static
/// member properties (`Deno.build.os`), non-computed static keys
/// (`{os: 1}`) and private names (`#u`). (Assignment targets,
/// declarations and labels are filtered per builder below.)
fn identifier_positions(nodes: &AstNodes<'_>) -> Vec<(u32, NodeId, String)> {
    let mut identifier_refs: Vec<(u32, NodeId, String)> = Vec::new();
    for n in nodes.iter() {
        match n.kind() {
            AstKind::IdentifierReference(id) => {
                identifier_refs.push((n.span().start, n.id(), id.name.to_string()));
            }
            AstKind::StaticMemberExpression(m) => {
                identifier_refs.push((
                    m.property.span().start,
                    n.id(),
                    m.property.name.to_string(),
                ));
            }
            AstKind::ObjectProperty(p) => {
                if let oxc_ast::ast::PropertyKey::StaticIdentifier(idr) = &p.key {
                    identifier_refs.push((idr.span.start, n.id(), idr.name.to_string()));
                }
            }
            AstKind::MethodDefinition(m) => {
                if let oxc_ast::ast::PropertyKey::StaticIdentifier(idr) = &m.key {
                    identifier_refs.push((idr.span.start, n.id(), idr.name.to_string()));
                }
            }
            AstKind::PropertyDefinition(p) => {
                if let oxc_ast::ast::PropertyKey::StaticIdentifier(idr) = &p.key {
                    identifier_refs.push((idr.span.start, n.id(), idr.name.to_string()));
                }
            }
            // Private names (`this.#u = ...`): babel's Identifier visitor
            // sees the private name's identifier — a non-reference,
            // non-binding position — and 4a edges it when the NAME matches
            // a module binding (the jjH→u/d/h edges).
            AstKind::PrivateIdentifier(p) => {
                identifier_refs.push((n.span().start, n.id(), p.name.to_string()));
            }
            // Assignment-target property keys (`({all: aHu} = x)`): babel
            // sees the key — non-binding in a pattern — and 4a edges it
            // when the name matches (the Pp→all edge). The SHORTHAND form
            // (`({all} = x)`) is babel's ObjectProperty.value in a pattern
            // — isBinding TRUE — excluded; oxc's shorthand is
            // AssignmentTargetPropertyIdentifier — not matched here.
            AstKind::AssignmentTargetPropertyProperty(p) => {
                if let oxc_ast::ast::PropertyKey::StaticIdentifier(idn) = &p.name {
                    identifier_refs.push((idn.span.start, n.id(), idn.name.to_string()));
                }
            }
            // Destructuring DECLARATION keys (`var {all: x} = y`) — the
            // same babel position (key in a pattern, non-binding).
            AstKind::BindingProperty(p) => {
                if let oxc_ast::ast::PropertyKey::StaticIdentifier(idn) = &p.key {
                    identifier_refs.push((idn.span.start, n.id(), idn.name.to_string()));
                }
            }
            _ => {}
        }
    }
    identifier_refs
}

/// The two edge builders over one binding's declarator init (see the
/// module comment on `build_module_bindings` for the predicates).
#[allow(clippy::too_many_arguments)]
fn binding_edges(
    idx: usize,
    owner_symbol: SymbolId,
    owner_name: &str,
    bindings: &[(SymbolId, String, Span)],
    binding_symbols: &HashMap<SymbolId, usize>,
    rows: &mut [ModuleBindingNode],
    nodes: &AstNodes<'_>,
    scoping: &oxc_semantic::Scoping,
    scope_by_node: &HashMap<NodeId, oxc_semantic::ScopeId>,
    identifier_refs: &[(u32, NodeId, String)],
    mb_names: &HashMap<String, Vec<usize>>,
    write_ref_starts: &std::collections::HashSet<u32>,
    sorted_refs: &[(u32, SymbolId)],
    function_by_symbol: &HashMap<SymbolId, usize>,
    functions: &[GraphFunction],
) {
    // The declarator's init span (the TS: bindingPath.isVariableDeclarator()
    // → init) — other declaration shapes earn no edges.
    let init_span = {
        let decl_node = nodes.get_node(scoping.symbol_declaration(owner_symbol));
        let AstKind::VariableDeclarator(decl) = decl_node.kind() else {
            return;
        };
        match decl.init.as_ref() {
            Some(init) => crate::babel_view::unparen(init).span(),
            None => return,
        }
    };

    // 4a: every identifier position in the init whose name is a module
    // binding's name and whose scope-chain resolution is the container
    // binding (recordModuleRefDep: non-binding, non-owner positions).
    // `identifier_refs` is sorted by start: the init's positions are one
    // contiguous run (the pushes' order is irrelevant — the row's callees
    // are sorted + deduped after every builder ran).
    for (start, id_node, name) in in_span_run(identifier_refs, init_span, |r| r.0) {
        if name == owner_name {
            continue; // name === ownerName
        }
        // Assignment targets (`Lo9 ??= x`): babel's isBindingIdentifier is
        // TRUE for them — 4a's exclusion — and oxc flags the same
        // positions with the Write reference flag.
        if write_ref_starts.contains(start) {
            continue;
        }
        // Only names the module-binding set holds can edge at all.
        if !mb_names.contains_key(name) {
            continue;
        }
        // Resolve the name from the identifier's scope chain: walk the
        // parent chain to the nearest scope-owning node, then up the
        // scope chain by name.
        let Some(sym) = resolve_name_at(*id_node, scope_by_node, nodes, scoping, name) else {
            continue;
        };
        if let Some(&j) = binding_symbols.get(&sym)
            && bindings[j].1 == *name
        {
            rows[idx].internal_callees.push(bindings[j].2);
        }
    }

    // 4b: every REFERENCED identifier in the init (assignment targets
    // excluded — babel's isReferencedIdentifier) — the FUNCTION edge only;
    // the mb→mb edges come from 4a alone.
    // `sorted_refs` is the reference table sorted by start (starts are
    // unique keys): the init's references are one contiguous run, already
    // in the (start, symbol) order the edges were pushed in.
    for (start, sym) in in_span_run(sorted_refs, init_span, |r| r.0) {
        if write_ref_starts.contains(start) {
            continue;
        }
        if let Some(fn_idx) = function_node_for_symbol(*sym, function_by_symbol) {
            rows[idx].internal_callees.push(functions[fn_idx].span);
        }
    }
}

/// The contiguous run of `sorted` (ascending by `start_of`) whose start
/// lies in `[span.start, span.end)` — a range query by binary search.
fn in_span_run<T>(sorted: &[T], span: Span, start_of: impl Fn(&T) -> u32) -> &[T] {
    let lo = sorted.partition_point(|r| start_of(r) < span.start);
    let hi = sorted.partition_point(|r| start_of(r) < span.end);
    &sorted[lo..hi.max(lo)]
}

/// The binding-match fingerprint (buildBindingMatchFingerprint): the
/// binding's CONTENT hashed LITERAL-PRESERVING (Verbatim — `var a = 4` and
/// `var a = 2` must differ when comparing binding content). Content:
/// a class declaration hashes ITSELF; a declarator's init; a bare declarator
/// hashes the FIRST assignment's right side (the TS constantViolations[0]);
/// otherwise no fingerprint (the row omits the hash, the family skips it).
/// The ESTree JSON of a module binding's hashable content — the TS
/// `resolveBindingContentPath` (function-graph.ts :411) + the
/// constantViolations[0] rule. ONE owner for the content-subtree decision:
/// [`binding_fingerprint_hash`] hashes it and `twins/role.rs` shingles it,
/// and the two consumers MUST walk the same subtree (role.rs's former
/// private copy re-scanned for redeclarations by `symbol_id.is_none()`,
/// which oxc's symbol-ful redeclaration identifiers defeat — the K5
/// zlib-counter case then minted content babel never has).
///
/// A class declaration hashes itself; a declarator hashes its (unparen'd)
/// init; a bare declarator hashes the first ASSIGNMENT among the
/// violations — the redeclarations (other declarators of the same name,
/// zero-width markers) and the write-reference assignments, in source
/// order. Any other declaration shape (or a redeclaration first) has no
/// content.
pub(crate) fn binding_content_estree(
    symbol: SymbolId,
    nodes: &AstNodes<'_>,
    scoping: &oxc_semantic::Scoping,
    redeclared_spans: &[u32],
) -> Option<String> {
    use oxc_estree::ESTree;
    let decl_node = nodes.get_node(scoping.symbol_declaration(symbol));
    match decl_node.kind() {
        // A class declaration's own body IS the hashable content.
        AstKind::Class(c) => {
            let mut ser = CompactSerializer::new(false, false);
            c.serialize(&mut ser);
            Some(ser.into_string())
        }
        AstKind::VariableDeclarator(d) => match d.init.as_ref() {
            Some(init) => {
                let mut ser = CompactSerializer::new(false, false);
                crate::babel_view::unparen(init).serialize(&mut ser);
                Some(ser.into_string())
            }
            None => {
                // babel's constantViolations[0] — the violations are the
                // REDECLARATIONS (the other declarators of the same name)
                // and the assignments, in source order. `var x, K5, K5`
                // puts a redeclaration FIRST, which is not an
                // AssignmentExpression — the TS's check aborts and the
                // fingerprint stays absent (the K5 zlib-counter case).
                // (start, the AssignmentExpression's node) — None for a
                // redeclaration marker, which never yields content.
                let mut violations: Vec<(u32, Option<NodeId>)> = Vec::new();
                for &start in redeclared_spans {
                    violations.push((start, None));
                }
                for &reference_id in scoping.get_resolved_reference_ids(symbol) {
                    let r = scoping.get_reference(reference_id);
                    if !r
                        .flags()
                        .contains(oxc_syntax::reference::ReferenceFlags::Write)
                    {
                        continue;
                    }
                    let mut prev = r.node_id();
                    loop {
                        let parent = nodes.parent_id(prev);
                        if let AstKind::AssignmentExpression(a) = nodes.get_node(parent).kind() {
                            violations.push((a.span().start, Some(parent)));
                            break;
                        }
                        if parent == prev {
                            break;
                        }
                        prev = parent;
                    }
                }
                violations.sort_by_key(|(start, _)| *start);
                // Only an AssignmentExpression violation yields content —
                // the node the walk above stopped at (an assignment's span
                // is unique among assignments, so this IS the node a
                // span-keyed search of the arena would find).
                let (_, first) = violations.first()?;
                let AstKind::AssignmentExpression(a) = nodes.get_node((*first)?).kind() else {
                    unreachable!("only assignment nodes are recorded");
                };
                let mut ser = CompactSerializer::new(false, false);
                crate::babel_view::unparen(&a.right).serialize(&mut ser);
                Some(ser.into_string())
            }
        },
        _ => None,
    }
}

/// The fingerprint hash of one binding's content JSON
/// ([`binding_content_estree`]) — the pure half, run on the pool.
fn binding_fingerprint_hash(
    content_json: &str,
    tables: &crate::hash::serialize::SymbolTables,
) -> String {
    let subtree = crate::ingest::parse_estree_json(content_json);
    crate::hash::serialize::canonical_serialize(
        &subtree,
        tables,
        crate::hash::serialize::LiteralPolicy::Verbatim,
    )
    .hash
}

/// Bindings per sequential-serialize / parallel-hash chunk (bounds the
/// content JSON alive at once).
const BINDING_HASH_CHUNK: usize = 4096;

/// One binding's eligibility under the run's setting.
fn is_eligible_under(eligibility: Eligibility<'_>, name: &str) -> bool {
    match eligibility {
        Eligibility::SkipSet { bundler, minifier } => {
            crate::rename::eligibility::is_eligible(name, bundler, minifier)
        }
        Eligibility::All => true,
    }
}

/// Each row's `redeclared_spans` (the OTHER declarators of the same name
/// in the container — the binding's OWN declaration is not a violation)
/// and the fingerprint hash that reads them, after the rows exist.
fn assign_redeclarations_and_hashes(
    rows: &mut [ModuleBindingNode],
    redeclarations: &HashMap<String, Vec<u32>>,
    scoping: &oxc_semantic::Scoping,
    nodes: &AstNodes<'_>,
    tables: &crate::hash::serialize::SymbolTables,
) {
    for row in rows.iter_mut() {
        let own = scoping.symbol_span(row.symbol).start;
        let mut redeclared: Vec<u32> = redeclarations
            .get(&row.name)
            .map(|v| v.iter().filter(|&&s| s != own).copied().collect())
            .unwrap_or_default();
        redeclared.sort_unstable();
        redeclared.dedup();
        row.redeclared_spans = redeclared;
    }
    // The content JSON reads the AST (serialized here, in row order); its
    // parse + hash is pure and runs on the pool, back in row order.
    let hashes = crate::par::produce_then_map(
        rows.len(),
        BINDING_HASH_CHUNK,
        |i| binding_content_estree(rows[i].symbol, nodes, scoping, &rows[i].redeclared_spans),
        |content| content.map(|json| binding_fingerprint_hash(&json, tables)),
    );
    for (row, hash) in rows.iter_mut().zip(hashes) {
        row.fingerprint_hash = hash;
    }
}

fn build_module_bindings(
    semantic: &Semantic<'_>,
    program: &oxc_ast::ast::Program<'_>,
    factories: &[crate::modules::FactoryRecord],
    eligibility: Eligibility<'_>,
    function_by_symbol: &HashMap<SymbolId, usize>,
    functions: &[GraphFunction],
) -> Vec<ModuleBindingNode> {
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();

    // The container scope: the wrapper's own scope when a wrapper exists,
    // else the program's (the TS targetScope).
    let wrapper = crate::modules::wrapper::find_wrapper_function(program, semantic);
    let container_scope = wrapper.as_ref().map_or_else(
        || container_scope_of_program(scoping, nodes),
        |w| {
            (0..scoping.scopes_len())
                .map(oxc_semantic::ScopeId::new)
                .find(|sid| nodes.get_node(scoping.get_node_id(*sid)).span() == w.span)
        },
    );
    let Some(container_scope) = container_scope else {
        return Vec::new();
    };

    // The eligible bindings, in symbol order (the TS's Object.entries is
    // insertion order; the rows sort by key at write time anyway).
    let mut bindings: Vec<(SymbolId, String, Span)> = Vec::new();
    for symbol in scoping.iter_bindings_in(container_scope) {
        let name = scoping.symbol_name(symbol).to_string();
        if !is_eligible_under(eligibility, &name) {
            continue;
        }
        let decl_node_id = scoping.symbol_declaration(symbol);
        let decl_node = nodes.get_node(decl_node_id);
        if should_skip_binding(&decl_node.kind(), decl_node.span(), factories) {
            continue;
        }
        // The row key = the SYMBOL's declaration span — the binding
        // identifier's own span. For destructured declarators
        // (`var { a: Ro9 } = obj`) the declaration NODE's span is the whole
        // pattern; four bindings would share one key. The symbol's span is
        // the sub-identifier's — babel's binding.identifier.
        let span = scoping.symbol_span(symbol);
        bindings.push((symbol, name, span));
    }

    // The module-binding name set (the TS moduleBindingSet).
    let binding_symbols: HashMap<SymbolId, usize> = bindings
        .iter()
        .enumerate()
        .map(|(i, (sym, _, _))| (*sym, i))
        .collect();

    // The resolved-reference table, span-start -> symbol (the same table
    // the hashes use). References inside an initializer are exactly the
    // identifiers the TS's init.traverse visits that are NOT binding
    // identifiers (declarations are never references).
    let tables = crate::hash::serialize::SymbolTables::build(semantic);
    let write_ref_starts = excluded_ref_starts(nodes, scoping);
    // The redeclarations: oxc's bindings map holds ONE symbol per name —
    // `var ..., K5, K5, K5` declares the others with NO symbol. The
    // violation positions are the container's own BindingIdentifier
    // declarators: every BindingIdentifier in the container body whose
    // nearest function ancestor IS the container (a nested function's own
    // `var K5` is that function's binding, not a violation).
    let mut scope_by_node: HashMap<NodeId, oxc_semantic::ScopeId> = HashMap::new();
    for i in 0..scoping.scopes_len() {
        let sid = oxc_semantic::ScopeId::new(i);
        scope_by_node.insert(scoping.get_node_id(sid), sid);
    }
    let container_span = if let Some(w) = &wrapper {
        w.body_span
    } else {
        program.span
    };
    let mut redeclarations: HashMap<String, Vec<u32>> = HashMap::new();
    for n in nodes.iter() {
        let span = n.span();
        if span.start < container_span.start || span.end > container_span.end {
            continue;
        }
        let AstKind::BindingIdentifier(id) = n.kind() else {
            continue;
        };
        // The nearest function ancestor's scope (blocks don't stop the
        // walk — a block's `var` hoists to the container).
        let mut prev = n.id();
        let mut fn_scope = None;
        loop {
            if let Some(&sid) = scope_by_node.get(&prev)
                && matches!(
                    nodes.get_node(scoping.get_node_id(sid)).kind(),
                    AstKind::Function(_)
                        | AstKind::ArrowFunctionExpression(_)
                        | AstKind::Program(_)
                )
            {
                fn_scope = Some(sid);
                break;
            }
            let parent = nodes.parent_id(prev);
            if parent == prev {
                break;
            }
            prev = parent;
        }
        if fn_scope == Some(container_scope) {
            redeclarations
                .entry(id.name.to_string())
                .or_default()
                .push(span.start);
        }
    }
    let mut rows: Vec<ModuleBindingNode> = bindings
        .iter()
        .map(|(sym, name, span)| ModuleBindingNode {
            session_id: format!("module:{name}"),
            span: *span,
            name: name.clone(),
            symbol: *sym,
            internal_callees: Vec::new(),
            callers: Vec::new(),
            declarator_init: declarator_init_of(nodes, scoping, *sym),
            redeclared_spans: Vec::new(),
            fingerprint_hash: None,
        })
        .collect();
    // The redeclaration lists and the fingerprint hashes that read them —
    // per row, after the vec-of-rows exists (the map closure above cannot
    // borrow `scoping.symbol_span` twice cleanly).
    assign_redeclarations_and_hashes(&mut rows, &redeclarations, scoping, nodes, &tables);
    // Edges: for each binding, the declarator's init span; TWO builders
    // run over the same subtree, with DIFFERENT identifier predicates:
    //
    // 4a (mb→mb, recordModuleRefDep): EVERY IdentifierReference node in
    // the init — member properties included (`Deno.build.os` edges to
    // module:os when a module binding named os exists; only DECLARATION
    // positions are excluded, and oxc's BindingIdentifier is a separate
    // node kind). The target must resolve — by NAME from the identifier's
    // scope — to the CONTAINER-scope binding.
    //
    // 4b (mb→function, addModuleToFunctionEdges): only REFERENCED
    // identifiers (babel's isReferencedIdentifier — assignment targets
    // excluded), resolved through the reference table.
    let mut mb_names: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, (_, name, _)) in bindings.iter().enumerate() {
        mb_names.entry(name.clone()).or_default().push(i);
    }
    let mut identifier_refs = identifier_positions(nodes);
    identifier_refs.sort_by_key(|r| r.0);
    let mut sorted_refs: Vec<(u32, SymbolId)> = tables
        .ref_by_start
        .iter()
        .map(|(start, sym)| (*start, *sym))
        .collect();
    sorted_refs.sort();
    for (idx, (owner_symbol, _, _binding_span)) in bindings.iter().enumerate() {
        binding_edges(
            idx,
            *owner_symbol,
            &bindings[idx].1,
            &bindings,
            &binding_symbols,
            &mut rows,
            nodes,
            scoping,
            &scope_by_node,
            &identifier_refs,
            &mb_names,
            &write_ref_starts,
            &sorted_refs,
            function_by_symbol,
            functions,
        );
    }
    // Dedupe + sort (no Set iteration may leak); the rows themselves sort
    // by span — oxc's symbol registration order is hoisting order, not
    // the TS's insertion order, and the dump keys on spans anyway.
    for r in &mut rows {
        r.internal_callees.sort_by_key(|s| (s.start, s.end));
        r.internal_callees.dedup();
    }
    apply_binding_caller_edges(&mut rows, functions, semantic, nodes);
    rows.sort_by_key(|r| (r.span.start, r.span.end));
    rows
}

/// Edge builder 4d (addFunctionToBindingReferenceEdges,
/// function-graph.ts:689): binding.callers — for each module binding,
/// every resolved reference's nearest enclosing GRAPH function. The
/// babel `referencePaths` model is the oxc resolved references minus the
/// positions babel records as constantViolations (see
/// [`babel_reference_node_ids`]); `findEnclosingFunction` (:671) walks
/// the parent chain to the first Function ancestor — one that is not a
/// graph row answers null (the factory-skip semantics), so a top-level
/// reference adds nothing.
fn apply_binding_caller_edges(
    rows: &mut [ModuleBindingNode],
    functions: &[GraphFunction],
    semantic: &Semantic<'_>,
    nodes: &AstNodes<'_>,
) {
    let fn_idx_by_span: HashMap<(u32, u32), usize> = functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.span.start, f.span.end), i))
        .collect();
    for row in rows.iter_mut() {
        let mut callers: BTreeSet<(u32, u32)> = BTreeSet::new();
        for node_id in babel_reference_node_ids(semantic, row.symbol) {
            if let Some(idx) = nearest_row_function_from_ref(nodes, node_id, &fn_idx_by_span) {
                let span = functions[idx].span;
                callers.insert((span.start, span.end));
            }
        }
        row.callers = callers
            .into_iter()
            .map(|(start, end)| Span::new(start, end))
            .collect();
    }
}

/// The binding's declaration shape ([`DeclaratorInit`]) from the symbol's
/// declaration node (the TS reads `binding.scope.bindings[binding.name]`
/// .path — prior-version.ts:1444). The init is unparenthesized first:
/// babel has no paren nodes, so `var x = (function(){})`'s init IS the
/// function expression for the TS.
fn declarator_init_of(
    nodes: &AstNodes<'_>,
    scoping: &oxc_semantic::Scoping,
    symbol: SymbolId,
) -> DeclaratorInit {
    let decl_node = nodes.get_node(scoping.symbol_declaration(symbol));
    let AstKind::VariableDeclarator(decl) = decl_node.kind() else {
        return DeclaratorInit::NotDeclarator;
    };
    match decl.init.as_ref().map(crate::babel_view::unparen) {
        None => DeclaratorInit::NoInit,
        Some(oxc_ast::ast::Expression::FunctionExpression(_))
        | Some(oxc_ast::ast::Expression::ArrowFunctionExpression(_)) => {
            DeclaratorInit::FunctionInit
        }
        Some(oxc_ast::ast::Expression::ClassExpression(_)) => DeclaratorInit::ClassInit,
        Some(_) => DeclaratorInit::OtherInit,
    }
}

// ---------------------------------------------------------------------------
// babel referencePaths (edge builder 4d + the member-key through-variable
// walk both read them)
// ---------------------------------------------------------------------------

/// The oxc resolved references of `symbol` that correspond to babel's
/// `binding.referencePaths` (see `binding_caller_indices` for the probe).
/// Used by the binding-caller walk AND the member-key through-variable walk
/// (both TS walks read `binding.referencePaths`).
pub(crate) fn babel_reference_node_ids(semantic: &Semantic<'_>, symbol: SymbolId) -> Vec<NodeId> {
    let scoping = semantic.scoping();
    scoping
        .get_resolved_reference_ids(symbol)
        .iter()
        .filter_map(|&reference_id| {
            let reference = scoping.get_reference(reference_id);
            let node_id = reference.node_id();
            if reference.flags().contains(ReferenceFlags::Write)
                && is_babel_assignment_target(semantic.nodes(), node_id)
            {
                None
            } else {
                Some(node_id)
            }
        })
        .collect()
}

/// Whether this WRITE reference sits inside an AssignmentExpression's LEFT
/// target — simple (`mb = x`), compound (`mb += x`) or destructuring
/// (`({x: mb} = o)`) — the positions babel records as constantViolations
/// instead of referencePaths. Update targets (`mb++`) and for-of/for-in
/// targets stay references, so the walk stops at the first statement or
/// function boundary instead of climbing out of them.
fn is_babel_assignment_target(nodes: &oxc_semantic::AstNodes<'_>, node_id: NodeId) -> bool {
    let span = nodes.get_node(node_id).span();
    let mut prev = node_id;
    let mut parent = nodes.parent_id(prev);
    while parent != prev {
        let kind = nodes.get_node(parent).kind();
        if let AstKind::AssignmentExpression(assignment) = kind {
            return assignment.left.span().contains_inclusive(span);
        }
        if kind.is_statement()
            || matches!(
                kind,
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) | AstKind::Program(_)
            )
        {
            return false;
        }
        prev = parent;
        parent = nodes.parent_id(prev);
    }
    false
}

/// TS `findEnclosingFunction` (:671) from a reference NODE: walk the parent
/// chain; the first babel-Function ancestor decides. The oxc wrinkle is
/// that a method's body sits under an inner Function node BELOW the
/// MethodDefinition row (babel has ONE ClassMethod node) — so an unrowed
/// Function whose parent is a method row continues through it, and any
/// other unrowed Function/Arrow answers None (the factory-skip stop,
/// babel's `fnByNode.get(...) ?? null`).
pub(crate) fn nearest_row_function_from_ref(
    nodes: &oxc_semantic::AstNodes<'_>,
    reference: NodeId,
    fn_idx_by_span: &HashMap<(u32, u32), usize>,
) -> Option<usize> {
    let mut prev = reference;
    let mut parent = nodes.parent_id(prev);
    while parent != prev {
        let node = nodes.get_node(parent);
        let span = node.span();
        if let Some(&idx) = fn_idx_by_span.get(&(span.start, span.end)) {
            return Some(idx);
        }
        match node.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                let grand = nodes.parent_id(parent);
                let through_method = grand != parent
                    && match nodes.get_node(grand).kind() {
                        AstKind::MethodDefinition(_) => true,
                        AstKind::ObjectProperty(p) => {
                            p.method || p.kind != oxc_ast::ast::PropertyKind::Init
                        }
                        _ => false,
                    };
                if !through_method {
                    return None;
                }
                prev = parent;
                parent = grand;
            }
            // An unrowed method (factory-skipped) — babel's isFunction()
            // fires on it and fnByNode misses → null. A plain ObjectProperty
            // is only a container and does not stop the walk.
            AstKind::MethodDefinition(_) => return None,
            AstKind::ObjectProperty(p)
                if p.method || p.kind != oxc_ast::ast::PropertyKind::Init =>
            {
                return None;
            }
            _ => {
                prev = parent;
                parent = nodes.parent_id(prev);
            }
        }
    }
    None
}

/// The program's own scope (the TS programScope): the scope whose node is
/// the program node (the node iterator's root).
fn container_scope_of_program(
    scoping: &oxc_semantic::Scoping,
    nodes: &AstNodes<'_>,
) -> Option<oxc_semantic::ScopeId> {
    let program_id = nodes.iter().next()?.id();
    (0..scoping.scopes_len())
        .map(oxc_semantic::ScopeId::new)
        .find(|sid| scoping.get_node_id(*sid) == program_id)
}

/// The three shouldSkipBinding skips.
fn should_skip_binding(
    kind: &AstKind<'_>,
    span: Span,
    factories: &[crate::modules::FactoryRecord],
) -> bool {
    // Skip bindings inside any third-party CJS factory body.
    if crate::modules::is_inside_factory_body(span, factories) {
        return true;
    }
    // Function declarations are processed as FunctionNodes.
    if matches!(kind, AstKind::Function(_)) {
        return true;
    }
    // A declarator whose init is a NAMED function expression: its
    // FunctionNode covers the name. A named CLASS expression has no node —
    // the declarator binding stays nameable here.
    if let AstKind::VariableDeclarator(d) = kind
        && let Some(oxc_ast::ast::Expression::FunctionExpression(f)) = &d.init
        && f.id.is_some()
    {
        return true;
    }
    false
}

/// The symbol a NAME resolves to from the position of an identifier at
/// `start` — babel's `p.scope.getBinding(name)` for identifiers that are
/// NOT references (member properties, non-computed keys): walk the parent
/// chain to the nearest scope-owning node, then up the scope chain.
fn resolve_name_at(
    id_node: NodeId,
    scope_by_node: &HashMap<NodeId, oxc_semantic::ScopeId>,
    nodes: &AstNodes<'_>,
    scoping: &oxc_semantic::Scoping,
    name: &str,
) -> Option<SymbolId> {
    // The identifier is NOT a reference (member property, non-computed
    // key) — no symbol resolution exists; babel's getBinding walks the
    // scope chain BY NAME from the identifier's scope. The identifier's
    // scope = the nearest scope-owning ancestor node, then
    // scope_ancestors + a name comparison per scope's bindings.
    let mut prev = id_node;
    let mut scope = scope_by_node.get(&prev).copied();
    while scope.is_none() {
        let parent = nodes.parent_id(prev);
        if parent == prev {
            break;
        }
        scope = scope_by_node.get(&parent).copied();
        prev = parent;
    }
    let sid = scope?;
    // A scope's bindings are keyed by name (one symbol per name), so the
    // keyed lookup IS the first same-named binding a scan would find.
    scoping
        .scope_ancestors(sid)
        .find_map(|ancestor| scoping.get_binding(ancestor, oxc_ast::ast::Ident::from(name)))
}

/// findFnForBinding: the symbol's declaration resolves to a graph function
/// when it IS one (declaration/expression/arrow) or its declarator init is.
fn function_node_for_symbol(
    symbol: SymbolId,
    function_by_symbol: &HashMap<SymbolId, usize>,
) -> Option<usize> {
    // The graph registered exactly the shapes findFnForBinding edges —
    // but the TS's findFnForBinding ALSO walks referencePaths for
    // assignments (`x = function(){}`); the declarator/declaration shapes
    // are the ones the wrapper scope's module bindings hit. Registered
    // map first (declarator-inits + named functions).
    // The graph registered exactly the shapes findFnForBinding edges:
    // the function's own name binding (declarations / named expressions)
    // and the declarator's binding when the init is the function. Any
    // other symbol (e.g. `var x = someOtherFn`) finds no function in the
    // TS either — findFnForBinding returns null there.
    function_by_symbol.get(&symbol).copied()
}

/// Pass 1: collect every graph-entry function.
///
/// babel's `Function` alias = FunctionDeclaration | FunctionExpression |
/// ObjectMethod | ArrowFunctionExpression | ClassMethod |
/// ClassPrivateMethod. oxc splits the method forms: class methods are
/// MethodDefinition; OBJECT methods (and getters/setters) are
/// ObjectProperty with method=true or a Get/Set kind, whose value is a
/// plain Function node — the METHOD node is the row (babel's span starts
/// at the key; the inner Function node would be a duplicate).
fn collect_function_entries(
    nodes: &AstNodes<'_>,
    factories: &[crate::modules::FactoryRecord],
) -> (Vec<FnEntry>, HashMap<NodeId, usize>) {
    let mut entries: Vec<FnEntry> = Vec::new();
    let mut idx_by_node: HashMap<NodeId, usize> = HashMap::new();
    for node in nodes.iter() {
        let entry_kind: Option<Span> = match node.kind() {
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                // A method's inner function is NOT a graph node: babel sees
                // ONE ClassMethod/ObjectMethod node (span from the key); its
                // oxc Function node starts at the params and would be a
                // duplicate row.
                let parent_id = nodes.parent_id(node.id());
                let is_method_inner = match nodes.get_node(parent_id).kind() {
                    AstKind::MethodDefinition(_) => true,
                    AstKind::ObjectProperty(p) => {
                        p.method || p.kind != oxc_ast::ast::PropertyKind::Init
                    }
                    _ => false,
                };
                if is_method_inner {
                    continue;
                }
                Some(node.span())
            }
            AstKind::MethodDefinition(_) => Some(node.span()),
            AstKind::ObjectProperty(p)
                if p.method || p.kind != oxc_ast::ast::PropertyKind::Init =>
            {
                Some(node.span())
            }
            _ => None,
        };
        let Some(span) = entry_kind else {
            continue;
        };
        // Third-party factory bodies: the TS skips the whole subtree
        // (path.skip()) — no node inside a factory body enters the graph.
        if crate::modules::is_inside_factory_body(span, factories) {
            continue;
        }
        let entry = FnEntry {
            node_id: node.id(),
            span,
            name_binding: function_name_binding(node.kind()),
            hash: String::new(),
            slots: Vec::new(),
            features: Default::default(),
        };
        idx_by_node.insert(node.id(), entries.len());
        entries.push(entry);
    }
    (entries, idx_by_node)
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
        // The same entry-kind rule as pass 1 (methods are function nodes
        // in babel's scope chain too). A method's INNER function is the
        // same scope as its MethodDefinition but was not collected (the
        // method node is the row) — keep walking to the collected entry.
        if is_graph_entry_kind(&parent.kind()) {
            match idx_by_node.get(&parent_id).copied() {
                Some(idx) => return Some(idx),
                None => {
                    prev = parent_id;
                    parent_id = nodes.parent_id(parent_id);
                    continue;
                }
            }
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

    use crate::graph::build_unified_graph;

    pub fn dump_functions(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
        let meta_text = fs::read_to_string(ts_dump_dir.join("meta.json"))
            .map_err(|e| format!("meta.json: {e}"))?;
        let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;
        // functions.json's rows anchor the FRESH text (the beautified
        // input the rename-era decisions consumed) — NOT the shipped text
        // the split-era sections anchor.
        let fresh = fs::read_to_string(ts_dump_dir.join("text").join("fresh.js"))
            .map_err(|e| format!("fresh: {e}"))?;
        // The eligibility skip-set resolves from the run's own flags.
        let bundler = meta["flags"]["bundler"].as_str();
        let minifier = meta["flags"]["minifier"].as_str();

        let allocator = Allocator::default();
        let ingest = crate::ingest::Ingest::parse(&allocator, &fresh, "fresh.js");
        if !ingest.errors.is_empty() {
            return Err(format!("oxc: {} diagnostic(s)", ingest.errors.len()));
        }
        // The graph's classification: computed here on the fresh text —
        // the same pure function the TS graph build runs (WP1.5).
        let wrapper =
            crate::modules::wrapper::find_wrapper_function(ingest.program, &ingest.semantic);
        let tables = crate::hash::serialize::SymbolTables::build(&ingest.semantic);
        let classification = crate::modules::classify_bun_modules(
            &fresh,
            ingest.program,
            &ingest.semantic,
            wrapper.as_ref().map(|w| w.body_span),
            &tables,
        );
        let factories = classification.map(|c| c.factories).unwrap_or_default();
        let graph = build_unified_graph(
            &ingest.semantic,
            ingest.program,
            "input.js",
            &factories,
            bundler,
            minifier,
        );

        let key =
            |span: oxc_span::Span| json!({"text": "fresh", "start": span.start, "end": span.end});
        let mut rows: Vec<Value> = Vec::new();
        for f in &graph.functions {
            rows.push(json!({
                "key": key(f.span),
                "sessionId": f.session_id,
                "kind": "function",
                "name": f.name,
                "nameBinding": f.name_binding.map(&key),
                "structuralHash": f.structural_hash,
                "internalCallees": f.internal_callees.iter().map(|s| key(*s)).collect::<Vec<_>>(),
                "scopeParent": f.scope_parent.map(&key),
                "bindings": f.placeholder_bindings.iter()
                    .map(|(slot, span, name)| json!({
                        "slot": slot,
                        "span": key(*span),
                        "name": name
                    }))
                    .collect::<Vec<_>>()
            }));
        }
        for mb in &graph.module_bindings {
            let mut row = json!({
                "key": key(mb.span),
                "sessionId": mb.session_id,
                "kind": "module-binding",
                "name": mb.name,
                "nameBinding": key(mb.span),
                "structuralHash": mb.fingerprint_hash.clone().unwrap_or_default(),
                "internalCallees": mb.internal_callees.iter().map(|s| key(*s)).collect::<Vec<_>>(),
                "scopeParent": Value::Null,
                "bindings": []
            });
            let obj = row.as_object_mut().expect("row object");
            if mb.fingerprint_hash.is_none() {
                // The TS writes the hash only when a fingerprint exists
                // (JSON.stringify drops undefined) — mirror the omission
                // until the fingerprint port lands.
                obj.remove("structuralHash");
            }
            rows.push(row);
        }
        rows.sort_by(|a, b| {
            let key = |v: &Value| {
                (
                    v["key"]["start"].as_u64().unwrap_or(0),
                    v["key"]["end"].as_u64().unwrap_or(0),
                )
            };
            key(a).cmp(&key(b))
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
