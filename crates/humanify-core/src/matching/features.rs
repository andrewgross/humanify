//! Structural features (WP2.1) — TS originals: `extractStructuralFeatures`
//! (src/analysis/structural-hash.ts :278) with its collectors
//! (`collectControlFlow` :176, `collectLiterals` :204, `collectPropertyAccess`
//! :213, `collectCallPatterns` :225, `collectMemberCallee` :245), the CFG
//! shape (`buildCfgShapeString` :427 + its encoders :328-404), and the
//! KNOWN_GLOBALS set (:11-102).
//!
//! THE SET IS THE LOCAL ONE (:11), NOT the `src/analysis/known-globals.ts`
//! module — that module (five environment sets, ~190 names) drives REPORTING
//! of unreachable free references; the features walk consults the smaller
//! browser/Node/built-in/library list defined inside structural-hash.ts
//! (probed: `Bun.write('b')` is `*.write`, not `Bun.write` — `Bun` is in the
//! reporting module but not here).
//!
//! The walk INPUT is the row's oxc ESTree JSON, not the oxc AST: the TS
//! walks the babel AST via a generic `Object.keys` child walk (visitChildren
//! :159), and the ESTree JSON is the same tree with the same field names —
//! walking it reproduces the TS walk exactly, including nested function
//! bodies (the wrapper's features count everything). The type-name mappings
//! that matter (verified against the oxc serializer, WP2.1 scratch probes):
//!
//! - babel `StringLiteral`/`NumericLiteral` → oxc `"Literal"`, classified by
//!   the JSON `value` type;
//! - babel's `OptionalMemberExpression`/`OptionalCallExpression` do not
//!   alias the plain types, so every hop of an optional chain is EXCLUDED
//!   from propertyAccesses and externalCalls. oxc folds optional chains into
//!   plain `MemberExpression`/`CallExpression` with `optional` flags under a
//!   `ChainExpression`, so the exclusion is reproduced by the SPINE rule:
//!   a member/call is babel-Optional-typed iff it (or the spine below it,
//!   through `object`/`callee` only, stopping at any paren — `(a?.b).c` is a
//!   plain MemberExpression) carries `optional: true`. Chain args and nested
//!   function bodies are NOT on the spine (`x?.f(g().h)` counts `.h`;
//!   `x?.f().g` does not count `.g` — probed).
//! - directives are NOT counted as string literals: babel types a directive
//!   as its own node (`'use strict';` is not a visited StringLiteral), while
//!   oxc emits `ExpressionStatement{directive, expression: Literal}` — the
//!   Rust skips a string literal whose parent ExpressionStatement carries a
//!   `directive` field (probed: the TS's stringLiterals for
//!   `function f() { 'use strict'; return 1; }` is empty);
//! - oxc names an object property `"Property"` (babel `ObjectProperty`) and
//!   a class member `"MethodDefinition"` (babel `ClassMethod`) — neither
//!   name is consulted by this walk.
//!
//! `start` offsets ride on every JSON node, so the bound-identifier test
//! (the TS's `bindingByIdentifier` cache, fed by computeFingerprintAndPlaceholders
//! :120-128 so externalCalls stays rename-invariant) is answered by the
//! symbol tables: an identifier occurrence is bound iff its span start
//! holds a declaration or a resolved reference.

use oxc_ast::AstKind;
use oxc_ast::ast::PropertyKind;
use oxc_semantic::Semantic;
use serde_json::Value;

use super::StructuralFeatures;
use crate::hash::serialize::SymbolTables;

/// TS `KNOWN_GLOBALS` (structural-hash.ts :11-102), verbatim — grouped by
/// the source's comment banners. `$` is deliberately absent (:95-97): it is
/// the first name in minifier alphabets, so treating it as jQuery would make
/// features rename-variant across versions.
const KNOWN_GLOBALS: &[&str] = &[
    // Browser APIs
    "fetch",
    "setTimeout",
    "setInterval",
    "clearTimeout",
    "clearInterval",
    "requestAnimationFrame",
    "cancelAnimationFrame",
    "alert",
    "confirm",
    "prompt",
    "console",
    "document",
    "window",
    "navigator",
    "location",
    "history",
    "localStorage",
    "sessionStorage",
    "indexedDB",
    "XMLHttpRequest",
    "WebSocket",
    "Worker",
    "Blob",
    "File",
    "FileReader",
    "URL",
    "URLSearchParams",
    "FormData",
    "Headers",
    "Request",
    "Response",
    "AbortController",
    "CustomEvent",
    "MutationObserver",
    "IntersectionObserver",
    "ResizeObserver",
    "PerformanceObserver",
    // Node.js
    "require",
    "process",
    "Buffer",
    "global",
    "__dirname",
    "__filename",
    // Built-in constructors/objects
    "Array",
    "Object",
    "String",
    "Number",
    "Boolean",
    "Symbol",
    "BigInt",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Promise",
    "Proxy",
    "Reflect",
    "JSON",
    "Math",
    "Date",
    "RegExp",
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "URIError",
    "EvalError",
    "Function",
    "eval",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "encodeURI",
    "decodeURI",
    "encodeURIComponent",
    "decodeURIComponent",
    "atob",
    "btoa",
    // Common library patterns
    "jQuery",
    "React",
    "Vue",
    "angular",
];

/// TS `extractStructuralFeatures` (:278) over the ESTree JSON of a function
/// row's subtree. The graph's features always come bound-aware (the TS's
/// node-only callers keep the everything-free default for unit-test fakes;
/// the index path is the cache-fed one, :126-128).
pub fn extract_structural_features(estree_json: &str, tables: &SymbolTables) -> StructuralFeatures {
    use serde::Deserialize;

    let mut de = serde_json::Deserializer::from_str(estree_json);
    de.disable_recursion_limit();
    let root = match Value::deserialize(&mut de) {
        Ok(v) => v,
        Err(_) => return StructuralFeatures::default(),
    };

    let mut walk = FeatureWalk {
        tables,
        features: StructuralFeatures {
            complexity: 1,
            ..StructuralFeatures::default()
        },
    };

    // arity + hasRestParam (:283-284). Function/Arrow rows carry `params` at
    // the root; the method rows (MethodDefinition / object-method Property)
    // nest them under `value` (oxc keeps the Function node, babel does not).
    let params = root
        .get("value")
        .and_then(|v| v.get("params"))
        .or_else(|| root.get("params"));
    if let Some(Value::Array(items)) = params {
        walk.features.arity = items.len() as u32;
        walk.features.has_rest_param = items.iter().any(|p| type_of(p) == Some("RestElement"));
    }

    collect(&root, None, &mut walk);

    let mut features = walk.features;
    // Sort + dedupe (:313-319) — Set semantics then a deterministic order.
    // The string sorts are byte sorts: equal to the TS's default sort
    // (UTF-16 code units) for every well-formed Unicode string, and equal to
    // babel's `.sort()` on the shape/call alphabets (ASCII).
    features.string_literals.sort();
    features.string_literals.dedup();
    features
        .numeric_literals
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    features.numeric_literals.dedup_by(|a, b| a == b);
    features.external_calls.sort();
    features.external_calls.dedup();
    features.property_accesses.sort();
    features.property_accesses.dedup();

    features.cfg_shape = cfg_shape(&root);
    features
}

/// One walk's state.
struct FeatureWalk<'t> {
    tables: &'t SymbolTables,
    features: StructuralFeatures,
}

/// The function-row features table, parallel to `graph.functions` — the
/// Rust equivalent of the TS's `fn.fingerprint.features` (computed once per
/// function at graph build: computeFingerprintAndPlaceholders, :117-132).
/// A row whose arena node is missing (cannot happen for a graph row) or
/// whose kind is not a row kind answers the all-default features.
pub(crate) fn features_table(
    functions: &[crate::graph::GraphFunction],
    semantic: &Semantic<'_>,
    tables: &SymbolTables,
) -> Vec<StructuralFeatures> {
    let nodes = semantic.nodes();
    let row_ids = super::row_node_ids(functions, nodes);
    functions
        .iter()
        .map(|f| {
            let json = row_ids
                .get(&(f.span.start, f.span.end))
                .and_then(|(_, kind)| row_estree_json(*kind));
            match json {
                Some(json) => extract_structural_features(&json, tables),
                None => StructuralFeatures::default(),
            }
        })
        .collect()
}

/// The ESTree JSON of a graph row's subtree — the same serializer settings
/// graph.rs builds the row JSON with (`CompactSerializer::new(false, false)`
/// — no TS fields, no ranges; the macro there is the same call inline, a
/// candidate to unify when a third caller appears). The row kinds mirror
/// graph.rs's `is_graph_entry_kind`.
pub(crate) fn row_estree_json(kind: AstKind<'_>) -> Option<String> {
    use oxc_estree::{CompactSerializer, ESTree};
    let mut ser = CompactSerializer::new(false, false);
    match kind {
        AstKind::Function(f) => f.serialize(&mut ser),
        AstKind::ArrowFunctionExpression(a) => a.serialize(&mut ser),
        AstKind::MethodDefinition(m) => m.serialize(&mut ser),
        AstKind::ObjectProperty(p) if p.method || p.kind != PropertyKind::Init => {
            p.serialize(&mut ser)
        }
        _ => return None,
    }
    Some(ser.into_string())
}

/// The TS `visit` (:299-306): collect on the node, then every child — the
/// child iteration walks every field except type/loc/start/end
/// (`visitChildren` :159, `SKIP_KEYS` :138).
fn collect(value: &Value, parent: Option<&Value>, walk: &mut FeatureWalk<'_>) {
    let Some(map) = value.as_object() else {
        return;
    };
    let Some(ntype) = type_of(value) else {
        return;
    };
    collect_control_flow(ntype, map, walk);
    collect_literals(ntype, map, parent, walk);
    collect_property_access(ntype, map, walk);
    collect_call_patterns(ntype, map, walk);
    for (key, child) in map {
        if matches!(key.as_str(), "type" | "loc" | "start" | "end") {
            continue;
        }
        visit_value(child, Some(value), walk);
    }
}

/// TS `visitArrayValue` (:144): array items, nodes only.
fn visit_value(value: &Value, parent: Option<&Value>, walk: &mut FeatureWalk<'_>) {
    match value {
        Value::Array(items) => {
            for item in items {
                if item.is_object() {
                    collect(item, parent, walk);
                }
            }
        }
        Value::Object(_) => collect(value, parent, walk),
        _ => {}
    }
}

fn type_of(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str)
}

fn is_true(map: &serde_json::Map<String, Value>, key: &str) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// TS `collectControlFlow` (:176).
fn collect_control_flow(
    ntype: &str,
    map: &serde_json::Map<String, Value>,
    walk: &mut FeatureWalk<'_>,
) {
    match ntype {
        "ReturnStatement" => walk.features.return_count += 1,
        // IfStatement | ConditionalExpression
        "IfStatement" | "ConditionalExpression" => {
            walk.features.branch_count += 1;
            walk.features.complexity += 1;
        }
        // SwitchStatement: branchCount++ once, complexity += cases.length
        "SwitchStatement" => {
            walk.features.branch_count += 1;
            walk.features.complexity += switch_case_count(map) as u32;
        }
        // LogicalExpression: only && and || (?? does not count)
        "LogicalExpression" => {
            if matches!(
                map.get("operator").and_then(Value::as_str),
                Some("&&") | Some("||")
            ) {
                walk.features.complexity += 1;
            }
        }
        // For | While | DoWhile | ForIn | ForOf
        "ForStatement" | "WhileStatement" | "DoWhileStatement" | "ForInStatement"
        | "ForOfStatement" => {
            walk.features.loop_count += 1;
            walk.features.complexity += 1;
        }
        "TryStatement" => walk.features.try_count += 1,
        _ => {}
    }
}

fn switch_case_count(map: &serde_json::Map<String, Value>) -> usize {
    map.get("cases")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

/// TS `collectLiterals` (:204) over the oxc `"Literal"` nodes, with the
/// directive exclusion from the module doc.
fn collect_literals(
    ntype: &str,
    map: &serde_json::Map<String, Value>,
    parent: Option<&Value>,
    walk: &mut FeatureWalk<'_>,
) {
    if ntype != "Literal" {
        return;
    }
    match map.get("value") {
        Some(Value::String(s)) => {
            let parent_is_directive = parent
                .map(|p| type_of(p) == Some("ExpressionStatement") && p.get("directive").is_some())
                .unwrap_or(false);
            if !parent_is_directive {
                walk.features.string_literals.push(s.clone());
            }
        }
        Some(Value::Number(n)) => {
            if let Some(f) = n.as_f64() {
                walk.features.numeric_literals.push(f);
            }
        }
        _ => {}
    }
}

/// TS `collectPropertyAccess` (:213): non-computed member accesses with an
/// identifier property. babel's OptionalMemberExpression is a different
/// node type, so optional-chain hops are excluded — the spine rule (module
/// doc) reproduces the type split on the folded oxc form.
fn collect_property_access(
    ntype: &str,
    map: &serde_json::Map<String, Value>,
    walk: &mut FeatureWalk<'_>,
) {
    if ntype != "MemberExpression" || is_true(map, "computed") {
        return;
    }
    if spine_optional(map) {
        return;
    }
    if let Some(name) = map.get("property").and_then(identifier_name) {
        walk.features.property_accesses.push(format!(".{name}"));
    }
}

/// TS `collectCallPatterns` (:225). The CALL itself must not be
/// optional-typed (`a?.()`'s callee is a bare Identifier in the folded form,
/// but babel's OptionalCallExpression is a different node type — probed
/// divergence without the check).
fn collect_call_patterns(
    ntype: &str,
    map: &serde_json::Map<String, Value>,
    walk: &mut FeatureWalk<'_>,
) {
    if ntype != "CallExpression" {
        return;
    }
    if spine_optional(map) {
        return;
    }
    let callee = json_unparen(map.get("callee").unwrap_or(&Value::Null));
    match type_of(callee) {
        // A bound callee whose name collides with a known global must not
        // leak its current name into the feature (:233-236).
        Some("Identifier") => {
            if let Some(name) = identifier_name(callee)
                && KNOWN_GLOBALS.contains(&name)
                && !is_bound(walk, callee)
            {
                walk.features.external_calls.push(name.to_string());
            }
        }
        Some("MemberExpression") => collect_member_callee(callee, walk),
        _ => {}
    }
}

/// TS `collectMemberCallee` (:245).
fn collect_member_callee(callee: &Value, walk: &mut FeatureWalk<'_>) {
    let Some(map) = callee.as_object() else {
        return;
    };
    // Computed access (x[cb]()) references a BINDING — excluded (:250-252).
    if is_true(map, "computed") {
        return;
    }
    let Some(prop_name) = map.get("property").and_then(identifier_name) else {
        return;
    };
    let object = json_unparen(map.get("object").unwrap_or(&Value::Null));
    if type_of(object) == Some("Identifier")
        && let Some(obj_name) = identifier_name(object)
        && KNOWN_GLOBALS.contains(&obj_name)
        && !is_bound(walk, object)
    {
        walk.features
            .external_calls
            .push(format!("{obj_name}.{prop_name}"));
        return;
    }
    // Generic method call like arr.map, str.split — and any call whose
    // object is a binding, whatever it is currently named (:261-265).
    walk.features.external_calls.push(format!("*.{prop_name}"));
}

/// Whether an identifier occurrence resolves to a binding — the TS's
/// `isBoundIdentifier` (the bindingByIdentifier cache, :126-128), answered
/// by the symbol tables.
fn is_bound(walk: &FeatureWalk<'_>, identifier: &Value) -> bool {
    let Some(start) = identifier.get("start").and_then(Value::as_u64) else {
        return false;
    };
    let start = u32::try_from(start).unwrap_or(u32::MAX);
    walk.tables.decl_by_start.contains_key(&start) || walk.tables.ref_by_start.contains_key(&start)
}

fn identifier_name(value: &Value) -> Option<&str> {
    if type_of(value) == Some("Identifier") {
        value.get("name").and_then(Value::as_str)
    } else {
        None
    }
}

/// The JSON twin of `babel_view::unparen`: strip paren wrappers the way
/// babel's parser never produces them.
fn json_unparen(value: &Value) -> &Value {
    let mut cur = value;
    while type_of(cur) == Some("ParenthesizedExpression") {
        match cur.get("expression") {
            Some(e) => cur = e,
            None => break,
        }
    }
    cur
}

/// Whether this member/call node is babel-Optional-typed: its own
/// `optional` flag, or the spine below it (through `object`/`callee` only,
/// stopping at any paren — `(a?.b).c` is a plain MemberExpression) carries
/// one. Chain args and nested function bodies are not on the spine, which
/// the recursion's shape guarantees: only `object`/`callee` children are
/// descended for the spine, so an arg's or a body's own optional-free
/// members answer false.
fn spine_optional(map: &serde_json::Map<String, Value>) -> bool {
    match map.get("type").and_then(Value::as_str) {
        Some("MemberExpression") => is_true(map, "optional") || spine_child(map.get("object")),
        Some("CallExpression") => is_true(map, "optional") || spine_child(map.get("callee")),
        _ => false,
    }
}

fn spine_child(child: Option<&Value>) -> bool {
    match child {
        // A paren breaks the optional chain.
        Some(c) if type_of(c) == Some("ParenthesizedExpression") => false,
        Some(c) => c.as_object().map(spine_optional).unwrap_or(false),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// buildCfgShapeString (structural-hash.ts :427)
// ---------------------------------------------------------------------------

/// TS `buildCfgShapeString` (:427): the statement-level control-flow shape.
/// The row's body: Function/Arrow rows carry `body` at the root, the method
/// rows under `value`.
pub fn cfg_shape(root: &Value) -> String {
    let body = root
        .get("value")
        .and_then(|v| v.get("body"))
        .unwrap_or_else(|| root.get("body").unwrap_or(&Value::Null));
    let mut shapes: Vec<&'static str> = Vec::new();
    if type_of(body) == Some("BlockStatement") {
        walk_statements(body.get("body"), &mut shapes);
    } else if body.is_object() {
        // Arrow function with expression body (:456-459).
        shapes.push("expr");
    }
    if shapes.is_empty() {
        "empty".to_string()
    } else {
        shapes.join("-")
    }
}

/// TS `walkStatements` (:430).
fn walk_statements(statements: Option<&Value>, shapes: &mut Vec<&'static str>) {
    let Some(Value::Array(items)) = statements else {
        return;
    };
    for stmt in items {
        walk_statement(stmt, shapes);
    }
}

/// One statement of `walkStatements`' loop body — factored so `walk_block`
/// can hand a SINGLE non-block statement through the same dispatch (the TS
/// wraps it: `walkStatements([node])`, :448-450).
fn walk_statement(stmt: &Value, shapes: &mut Vec<&'static str>) {
    {
        let Some(map) = stmt.as_object() else {
            return;
        };
        match type_of(stmt) {
            Some("IfStatement") => {
                shapes.push("if");
                walk_block(map.get("consequent"), shapes);
                // alternate is `null` when absent.
                if map.get("alternate").map(|a| a.is_object()).unwrap_or(false) {
                    shapes.push("else");
                    walk_block(map.get("alternate"), shapes);
                }
            }
            Some("DoWhileStatement") => {
                shapes.push("do");
                walk_block(map.get("body"), shapes);
            }
            Some("ForStatement" | "WhileStatement" | "ForOfStatement" | "ForInStatement") => {
                shapes.push("loop");
                walk_block(map.get("body"), shapes);
            }
            Some("TryStatement") => {
                shapes.push("try");
                walk_block(map.get("block"), shapes);
                if map.get("handler").map(|h| h.is_object()).unwrap_or(false) {
                    shapes.push("catch");
                    walk_block(map.get("handler").and_then(|h| h.get("body")), shapes);
                }
                if map.get("finalizer").map(|f| f.is_object()).unwrap_or(false) {
                    shapes.push("finally");
                    walk_block(map.get("finalizer"), shapes);
                }
            }
            Some("SwitchStatement") => {
                shapes.push("switch");
                if let Some(Value::Array(cases)) = map.get("cases") {
                    for case in cases {
                        let has_test = case.get("test").map(|t| t.is_object()).unwrap_or(false);
                        shapes.push(if has_test { "case" } else { "default" });
                        if let Some(Value::Array(consequent)) = case.get("consequent")
                            && !consequent.is_empty()
                        {
                            walk_statements(case.get("consequent"), shapes);
                        }
                    }
                }
            }
            _ => encode_simple_statement(type_of(stmt), shapes),
        }
    }
}

/// TS `walkBlock` (:446): a block's statements, or the single statement
/// wrapped in a one-element list.
fn walk_block(node: Option<&Value>, shapes: &mut Vec<&'static str>) {
    match node {
        Some(block) if type_of(block) == Some("BlockStatement") => {
            walk_statements(block.get("body"), shapes);
        }
        Some(stmt) if stmt.is_object() => walk_statement(stmt, shapes),
        _ => {}
    }
}

/// TS `encodeSimpleStatement` (:386): only the four abrupt-completion
/// statements contribute; everything else (expression statements, labels,
/// declarations, class/function declarations) contributes nothing.
fn encode_simple_statement(ntype: Option<&str>, shapes: &mut Vec<&'static str>) {
    match ntype {
        Some("ReturnStatement") => shapes.push("ret"),
        Some("ThrowStatement") => shapes.push("throw"),
        Some("BreakStatement") => shapes.push("break"),
        Some("ContinueStatement") => shapes.push("cont"),
        _ => {}
    }
}
