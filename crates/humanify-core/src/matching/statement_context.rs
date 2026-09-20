//! The enclosing-statement contexts (WP2.1 part 2) — the rung's per-node
//! identity evidence. TS originals: `src/analysis/enclosing-statement.ts`
//! (`MAX_ENCLOSING_STMT_LINES` :29, `statementUsability` :42,
//! `hashStatementPath` :58, `enclosingStatementHash` :85, `spanBucket` :92)
//! and `src/analysis/fingerprint-index.ts`'s `getEnclosingStmtHash` :349 +
//! `bindingNeighborContextHash` :380.
//!
//! THE HASH OWNER. The TS hashes the statement with
//! `hashPathWithMapping` (:977 structural-hash.ts) — the SAME
//! rename-invariant serializer the function structural hash runs under
//! (`hashAndMapPath(path, false)`: binding identifiers slotted, literals
//! blurred, property names and free identifiers verbatim). The Rust
//! equivalent is `hash::serialize::canonical_serialize` under
//! `LiteralPolicy::Blurred` over the statement's oxc ESTree JSON — NOT the
//! split's `statement_hash` (that walk masks property names too and keeps
//! literals verbatim, a different equivalence relation for a different
//! consumer). Digests are never compared across sides; only equality within
//! one side's buckets matters, so the WP1.4-gate-green class parity of the
//! canonical serializer carries over to statement roots.
//!
//! SHAPE vs the TS. The TS computes lazily and memoizes per statement NODE
//! on the index (`enclosingStmtHashCache`); the Rust precomputes eagerly per
//! row at build time with the memo keyed by statement SPAN — same values,
//! one pass. The TS reads `fn.path.getStatementParent()` and the node's
//! `loc` lines; the Rust climbs the oxc semantic parent chain and measures
//! lines from byte offsets over the SAME text the graph's session ids were
//! computed from (babel loc lines and newline counts agree by construction).

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_semantic::{AstNodes, NodeId, Semantic};
use oxc_span::{GetSpan, Span};
use serde_json::Value;

use crate::graph::UnifiedGraph;
use crate::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize};

/// TS `MAX_ENCLOSING_STMT_LINES` (:29): enclosing statements above this loc
/// span carry too much unrelated code (and cost too much to hash) to serve
/// as identity evidence. UNMEASURED WHEN CHOSEN (exp079) — see the TS doc.
pub const MAX_ENCLOSING_STMT_LINES: u32 = 50;

/// TS `STMT_SPAN_BUCKETS` (types.ts :681) — the reporting buckets, ordered;
/// the cap falls between the 3rd and 4th.
pub const STMT_SPAN_BUCKETS: [&str; 8] = [
    "1-9", "10-24", "25-49", "50-99", "100-199", "200-499", "500+", "unknown",
];

/// TS `spanBucket` (:92), as the index into [`STMT_SPAN_BUCKETS`].
pub fn span_bucket(span_lines: Option<u32>) -> usize {
    match span_lines {
        None => 7,
        Some(lines) if lines < 10 => 0,
        Some(lines) if lines < 25 => 1,
        Some(lines) if lines < 50 => 2,
        Some(lines) if lines < 100 => 3,
        Some(lines) if lines < 200 => 4,
        Some(lines) if lines < 500 => 5,
        Some(_) => 6,
    }
}

/// TS `statementUsability` (:42)'s verdict. The TS's `noLoc` reason cannot
/// occur here (oxc spans are always present), so it is folded away; `noNode`
/// covers the absent-statement case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StmtUsability {
    Ok { lines: u32 },
    TooLong { lines: u32 },
    NoNode,
}

impl StmtUsability {
    /// TS `.usable`.
    pub fn usable(&self) -> bool {
        matches!(self, StmtUsability::Ok { .. })
    }
}

/// TS `statementUsability` (:42): is a statement usable as identity
/// evidence, and how big is it? THE owner of the cap — the hash path and the
/// abstain diagnostics both ask this, so a counter reporting "excluded by
/// the cap" cannot drift from the rule that actually excluded it. The TS
/// takes the node and reads `loc`; the Rust takes the statement's span and
/// the line starts of the same text (same question, same answer).
pub fn statement_usability(stmt_span: Option<Span>, line_starts: &[u32]) -> StmtUsability {
    let Some(span) = stmt_span else {
        return StmtUsability::NoNode;
    };
    let lines = line_span(span, line_starts);
    if lines > MAX_ENCLOSING_STMT_LINES {
        return StmtUsability::TooLong { lines };
    }
    StmtUsability::Ok { lines }
}

/// 1-based line of a byte offset (the babel loc convention).
fn line_of(offset: u32, line_starts: &[u32]) -> u32 {
    match line_starts.binary_search(&offset) {
        Ok(i) => i as u32 + 1,
        Err(i) => i as u32, // line_starts[i] is the last start <= offset
    }
}

/// `loc.end.line - loc.start.line + 1` from byte offsets.
fn line_span(span: Span, line_starts: &[u32]) -> u32 {
    line_of(span.end, line_starts) - line_of(span.start, line_starts) + 1
}

/// The contexts of one function row (parallel to `graph.functions`).
#[derive(Debug)]
pub struct FnStmtContext {
    /// The statement node (babel's `getStatementParent`): the FIRST
    /// babel-Statement node on the parent chain, STARTING at the row node
    /// itself — a FunctionDeclaration/ClassDeclaration row IS its own
    /// statement (the probe pinned babel's Statement alias: declarations and
    /// export/import declarations are Statements; function/method
    /// expressions and `Program` are not, and `Program` is the climb's
    /// no-parent fallback).
    pub stmt_node_id: Option<NodeId>,
    /// The statement node's span (the usability question's input).
    pub stmt_span: Option<Span>,
    /// TS `stmt.node === fn.path.node`.
    pub is_own_statement: bool,
    /// TS `statementUsability(stmt?.node)` — computed on the STATEMENT, so
    /// an own-statement function measures its own lines (the bucket ignores
    /// it, the reason does not).
    pub usability: StmtUsability,
    /// TS `enclosingStatementHash(fn)` (:85): null when the function IS its
    /// own statement, the statement exceeds the cap, or (TS-only) hashing
    /// threw.
    pub hash: Option<String>,
}

/// The contexts of one binding row (parallel to `graph.module_bindings`).
#[derive(Debug)]
pub struct BindingStmtContext {
    /// The declaration statement of the binding's identifier.
    pub stmt_node_id: Option<NodeId>,
    /// The declaration statement's span.
    pub stmt_span: Option<Span>,
    /// The previous sibling statement's span (TS `getPrevSibling`).
    pub prev_sibling: Option<Span>,
    /// The next sibling statement's span (TS `getNextSibling`).
    pub next_sibling: Option<Span>,
    /// TS `bindingNeighborContextHash` (:380): the neighboring statements'
    /// hashes (`prev|next`, `^`/`$` for absent sides), null when neither
    /// side hashes.
    pub hash: Option<String>,
}

/// The per-side statement contexts the cascade's rung reads — the TS's
/// `enclosingStmtHashCache` plus the statement bookkeeping
/// (`recordArrival`/`distinctStatements` need node identities the hash cache
/// alone cannot answer).
///
/// Built eagerly per side (per parsed text); the TS fills the same values
/// lazily on first touch of a bucket. Pure memoization of deterministic
/// questions — eagerness changes nothing observable. The build's working
/// state (the program JSON, the span paths, the hash memo) is consumed at
/// construction and not kept.
pub struct StatementContexts {
    functions: Vec<FnStmtContext>,
    bindings: Vec<BindingStmtContext>,
}

/// One step on the path from the program root to a JSON node.
#[derive(Debug, Clone)]
enum Step {
    Key(String),
    Index(usize),
}

impl StatementContexts {
    /// Build the contexts for one side: `graph` + `semantic` + `tables` are
    /// that side's, `program` its parsed program, `text` the same text the
    /// graph's session ids were computed from (the line measure).
    pub fn build(
        graph: &UnifiedGraph,
        semantic: &Semantic<'_>,
        tables: &SymbolTables,
        program: &oxc_ast::ast::Program<'_>,
        text: &str,
    ) -> StatementContexts {
        let nodes = semantic.nodes();
        let line_starts = line_starts_of(text);
        let row_ids = super::row_node_ids(&graph.functions, nodes);

        // The binding identifiers' arena nodes — one sweep over the semantic
        // (a span no longer names a node; row_node_ids' pattern).
        let binding_spans: HashSet<(u32, u32)> = graph
            .module_bindings
            .iter()
            .map(|b| (b.span.start, b.span.end))
            .collect();
        let mut binding_ids: HashMap<(u32, u32), NodeId> = HashMap::new();
        for node in nodes.iter() {
            let span = node.span();
            let key = (span.start, span.end);
            if binding_spans.contains(&key) {
                binding_ids.entry(key).or_insert(node.id());
            }
        }

        // ── pass 1: the statement bookkeeping, per row ───────────────────
        let mut functions: Vec<FnStmtContext> = Vec::with_capacity(graph.functions.len());
        let mut wanted: HashSet<(u32, u32)> = HashSet::new();
        // The statement span each function row will hash (None when it is
        // its own statement — TS :85 — or has no row node); hashing lands in
        // pass 2 so the program JSON is serialized once.
        let mut fn_hash_span: Vec<Option<Span>> = Vec::with_capacity(graph.functions.len());
        for f in &graph.functions {
            let row_node_id = row_ids.get(&(f.span.start, f.span.end)).map(|(id, _)| *id);
            let stmt_node_id = row_node_id.and_then(|id| statement_parent_of(nodes, id));
            let stmt_span = stmt_node_id.map(|id| nodes.get_node(id).span());
            let is_own_statement = stmt_node_id.is_some() && stmt_node_id == row_node_id;
            let usability = statement_usability(stmt_span, &line_starts);
            // TS `enclosingStatementHash` (:85): own statement → null; the
            // cap and the hash land in pass 2.
            let hash_span = if is_own_statement { None } else { stmt_span };
            if let Some(span) = hash_span {
                wanted.insert((span.start, span.end));
            }
            fn_hash_span.push(hash_span);
            functions.push(FnStmtContext {
                stmt_node_id,
                stmt_span,
                is_own_statement,
                usability,
                hash: None, // filled below
            });
        }

        let mut bindings: Vec<BindingStmtContext> = Vec::with_capacity(graph.module_bindings.len());
        // (stmt span, prev sibling span, next sibling span) per binding row.
        let mut binding_stmts: Vec<(Option<Span>, Option<Span>, Option<Span>)> =
            Vec::with_capacity(graph.module_bindings.len());
        for b in &graph.module_bindings {
            let id_node = binding_ids
                .get(&(b.span.start, b.span.end))
                .copied()
                .unwrap_or(NodeId::DUMMY);
            let stmt_node_id = statement_parent_of(nodes, id_node);
            let stmt_span = stmt_node_id.map(|id| nodes.get_node(id).span());
            if let Some(span) = stmt_span {
                wanted.insert((span.start, span.end));
            }
            let (prev, next) = stmt_node_id
                .map(|id| sibling_spans(nodes, id))
                .unwrap_or((None, None));
            for s in [prev, next].into_iter().flatten() {
                wanted.insert((s.start, s.end));
            }
            binding_stmts.push((stmt_span, prev, next));
            bindings.push(BindingStmtContext {
                stmt_node_id,
                stmt_span,
                prev_sibling: prev,
                next_sibling: next,
                hash: None, // filled below
            });
        }

        // ── pass 2: the program JSON, the span paths, the hashes ─────────
        let estree_text = program.to_estree_json(false, true);
        let estree = parse_json_unbounded(&estree_text);
        let mut paths: HashMap<(u32, u32), Vec<Step>> = HashMap::new();
        collect_paths(&estree, &[], &wanted, &mut paths);

        let mut memo: HashMap<(u32, u32), Option<String>> = HashMap::new();
        // Function rows: TS `enclosingStatementHash` — null when the
        // statement is unusable, else the canonical hash of the statement.
        for (ctx, hash_span) in functions.iter_mut().zip(fn_hash_span) {
            ctx.hash = hash_span.and_then(|s| {
                Self::hash_statement(s, &estree, &paths, &mut memo, tables, &line_starts)
            });
        }
        // Binding rows: TS `bindingNeighborContextHash` — the neighbors, not
        // the declaration (the declaration is the clone; the statements
        // around it carry the identity).
        for (ctx, (_, prev, next)) in bindings.iter_mut().zip(binding_stmts) {
            let prev_hash = prev.and_then(|s| {
                Self::hash_statement(s, &estree, &paths, &mut memo, tables, &line_starts)
            });
            let next_hash = next.and_then(|s| {
                Self::hash_statement(s, &estree, &paths, &mut memo, tables, &line_starts)
            });
            ctx.hash = match (prev_hash, next_hash) {
                (None, None) => None,
                (prev, next) => Some(format!(
                    "{}|{}",
                    prev.unwrap_or_else(|| "^".to_string()),
                    next.unwrap_or_else(|| "$".to_string())
                )),
            };
        }

        StatementContexts {
            functions,
            bindings,
        }
    }

    /// TS `hashStatementPath` (:58) for one statement span: the usability
    /// cap, then the canonical hash of the statement's JSON subtree.
    fn hash_statement(
        span: Span,
        estree: &Value,
        paths: &HashMap<(u32, u32), Vec<Step>>,
        memo: &mut HashMap<(u32, u32), Option<String>>,
        tables: &SymbolTables,
        line_starts: &[u32],
    ) -> Option<String> {
        let key = (span.start, span.end);
        if let Some(known) = memo.get(&key) {
            return known.clone();
        }
        let usability = statement_usability(Some(span), line_starts);
        let hash = if !usability.usable() {
            None
        } else {
            let subtree = node_at(estree, paths.get(&key)?)?;
            Some(canonical_serialize(subtree, tables, LiteralPolicy::Blurred).hash)
        };
        memo.insert(key, hash.clone());
        hash
    }

    /// TS `getEnclosingStmtHash` (:349) for one index node: functions read
    /// their enclosing statement, bindings their neighbor context.
    pub fn context_hash(&self, node: super::IndexNode) -> Option<&str> {
        match node {
            super::IndexNode::Function(i) => self.functions[i].hash.as_deref(),
            super::IndexNode::Binding(j) => self.bindings[j].hash.as_deref(),
        }
    }

    /// The function row's statement bookkeeping (the rung's arrival
    /// counters; bindings answer `NoNode` — the TS's `fnNode` is undefined
    /// on a binding index).
    pub fn fn_context(&self, row: usize) -> Option<&FnStmtContext> {
        self.functions.get(row)
    }

    /// The binding row's context.
    pub fn binding_context(&self, row: usize) -> Option<&BindingStmtContext> {
        self.bindings.get(row)
    }

    /// All function rows in graph order (the parity probe's dump).
    pub fn function_rows(&self) -> &[FnStmtContext] {
        &self.functions
    }

    /// All binding rows in graph order (the parity probe's dump).
    pub fn binding_rows(&self) -> &[BindingStmtContext] {
        &self.bindings
    }
}

/// Line starts of `text` (offset 0 first; every position after a newline).
fn line_starts_of(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}

fn parse_json_unbounded(text: &str) -> Value {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    // The AST nests hundreds deep; unbounded depth is safe: the input is
    // oxc's own serialization of a program that parsed.
    serde::Deserialize::deserialize(&mut de).unwrap_or(Value::Null)
}

/// TS `getStatementParent` (babel): walk STARTING at the given node, breaking
/// on the first babel-Statement (the alias includes the declaration forms and
/// the export/import declarations — see `is_babel_statement`), or on the
/// node without a parent (the `Program` fallback the TS also returns).
fn statement_parent_of(nodes: &AstNodes<'_>, start: NodeId) -> Option<NodeId> {
    let mut cur = start;
    loop {
        if is_babel_statement(nodes.get_node(cur).kind()) {
            return Some(cur);
        }
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return Some(cur);
        }
        cur = parent;
    }
}

/// babel's `Statement` alias (probed against @babel/types, 2026-09-20): all
/// the plain statements PLUS VariableDeclaration, FunctionDeclaration,
/// ClassDeclaration, ExportNamedDeclaration/Default/All, ImportDeclaration.
/// NOT statements: function/method expressions, `Program`, `StaticBlock`,
/// class bodies. oxc carries the declaration forms on `Function`/`Class`
/// nodes via `is_declaration()`.
fn is_babel_statement(kind: AstKind<'_>) -> bool {
    match kind {
        AstKind::BlockStatement(_)
        | AstKind::BreakStatement(_)
        | AstKind::ContinueStatement(_)
        | AstKind::DebuggerStatement(_)
        | AstKind::EmptyStatement(_)
        | AstKind::ExpressionStatement(_)
        | AstKind::ForStatement(_)
        | AstKind::ForInStatement(_)
        | AstKind::ForOfStatement(_)
        | AstKind::WhileStatement(_)
        | AstKind::DoWhileStatement(_)
        | AstKind::LabeledStatement(_)
        | AstKind::ReturnStatement(_)
        | AstKind::SwitchStatement(_)
        | AstKind::ThrowStatement(_)
        | AstKind::TryStatement(_)
        | AstKind::WithStatement(_)
        | AstKind::IfStatement(_)
        | AstKind::VariableDeclaration(_)
        | AstKind::ExportDefaultDeclaration(_)
        | AstKind::ExportNamedDeclaration(_)
        | AstKind::ExportAllDeclaration(_)
        // oxc-only export kinds for `export ... from` — babel types them
        // ExportNamedDeclaration (a Statement), so the oxc spellings stay
        // statements for parity.
        | AstKind::ExportDeclaration(_)
        | AstKind::ExportFromDeclaration(_)
        | AstKind::ImportDeclaration(_) => true,
        AstKind::Function(f) => f.is_declaration(),
        AstKind::Class(c) => c.is_declaration(),
        _ => false,
    }
}

/// The previous/next SIBLING statements of `stmt` — babel's
/// `getPrevSibling`/`getNextSibling` (the neighbors in the parent's statement
/// list; a non-list parent has none). Covers babel's five `[[Statement]]`
/// list containers: `Program.body`, `BlockStatement.body` (which in oxc
/// splits by context — a function/arrow body is oxc's own `FunctionBody`
/// node, whose statements it holds; babel types that body BlockStatement),
/// `StaticBlock.body`, `SwitchCase.consequent`. Module bindings live in
/// wrapper function bodies, so the FunctionBody arm is the common case.
fn sibling_spans(nodes: &AstNodes<'_>, stmt: NodeId) -> (Option<Span>, Option<Span>) {
    let stmt_span = nodes.get_node(stmt).span();
    let parent_id = nodes.parent_id(stmt);
    if parent_id == stmt {
        return (None, None);
    }
    let list: &[oxc_ast::ast::Statement] = match nodes.get_node(parent_id).kind() {
        AstKind::Program(p) => &p.body,
        AstKind::FunctionBody(b) => &b.statements,
        AstKind::BlockStatement(b) => &b.body,
        AstKind::StaticBlock(b) => &b.body,
        AstKind::SwitchCase(c) => &c.consequent,
        _ => return (None, None),
    };
    let Some(pos) = list.iter().position(|s| s.span() == stmt_span) else {
        return (None, None);
    };
    let prev = if pos > 0 {
        Some(list[pos - 1].span())
    } else {
        None
    };
    let next = list.get(pos + 1).map(|s| s.span());
    (prev, next)
}

/// One DFS over the program JSON, recording the path of every WANTED node
/// span (first match wins; statement spans are unique among wanted nodes —
/// parens cannot wrap statements).
fn collect_paths(
    value: &Value,
    path: &[Step],
    wanted: &HashSet<(u32, u32)>,
    out: &mut HashMap<(u32, u32), Vec<Step>>,
) {
    match value {
        Value::Object(map) => {
            if let (Some(t), Some(start), Some(end)) = (
                map.get("type").and_then(Value::as_str),
                map.get("start").and_then(Value::as_u64),
                map.get("end").and_then(Value::as_u64),
            ) {
                let _ = t;
                let key = (start as u32, end as u32);
                if wanted.contains(&key) && !out.contains_key(&key) {
                    out.insert(key, path.to_vec());
                }
            }
            for (k, child) in map {
                let mut p = path.to_vec();
                p.push(Step::Key(k.clone()));
                collect_paths(child, &p, wanted, out);
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                let mut p = path.to_vec();
                p.push(Step::Index(i));
                collect_paths(item, &p, wanted, out);
            }
        }
        _ => {}
    }
}

/// Navigate a recorded path from the program root to its JSON node.
fn node_at<'v>(root: &'v Value, path: &[Step]) -> Option<&'v Value> {
    let mut cur = root;
    for step in path {
        cur = match step {
            Step::Key(k) => cur.get(k)?,
            Step::Index(i) => cur.get(*i)?,
        };
    }
    Some(cur)
}

#[cfg(test)]
mod statement_context_test;
