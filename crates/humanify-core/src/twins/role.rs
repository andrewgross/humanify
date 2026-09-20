//! Binding role evidence — TS original: `src/prior-version/binding-role.ts`
//! (239 LOC), the role half of WP2.3.
//!
//! A module binding's role is compact plain data (no AST references) so the
//! prior side can be computed while the prior AST is alive and compared
//! against the fresh side's. Two consumers call [`binding_roles_agree`]:
//! the statement-twin gate's `declared_roles_agree`
//! (statement-twin.ts :339 — `allowContentFreeElimination` FALSE: a
//! consumer that compares many slots pairwise must not get blanket
//! agreement on bare declarators) and, later, the single-vote pin ladder
//! (WP3.3 — TRUE).
//!
//! The structural-hash half is the row's own `fingerprint_hash` (the
//! literal-preserving computeBindingFingerprint, computed at graph build —
//! graph.rs `binding_fingerprint_hash`); this module computes only the
//! CONTENT SHINGLES (the slot-blind k-gram sets) on demand, because they
//! are consulted only when the hashes differ.

use std::collections::{BTreeSet, HashMap};

use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::GetSpan;
use serde_json::Value;

use crate::graph::ModuleBindingNode;
use crate::hash::serialize::SymbolTables;

/// Tokens per shingle k-gram (binding-role.ts :37).
const SHINGLE_K: usize = 4;
/// Deterministic cap on shingles kept per binding, in walk order (:39).
const SHINGLE_CAP: usize = 2048;
/// Minimum shingle overlap for two roles to count as the same binding (:41).
pub const SINGLE_VOTE_CONTENT_FLOOR: f64 = 0.5;

/// TS `BindingRole` (:23).
#[derive(Debug, Clone, Default)]
pub struct BindingRole {
    /// Content hash from the binding's fingerprint; None when unhashable.
    pub structural_hash: Option<String>,
    /// Slot-blind, literal-preserving k-gram shingles of the binding's
    /// content; None when the binding has no init and no assignment.
    pub content_shingles: Option<BTreeSet<String>>,
    /// Session ids of FUNCTION callees referenced by the initializer.
    pub fn_callee_ids: Vec<String>,
    /// True when the initializer also references module bindings — the
    /// callee comparison is then inconclusive and must not veto.
    pub has_binding_callees: bool,
}

/// One side's evidence context for [`compute_binding_role`]: the semantic
/// (declaration nodes, symbol spans), the symbol tables the content walk
/// slots identifiers through, the container span (the wrapper body, else
/// the program — the redeclaration search's bounds, graph.rs's
/// `container_span`), and the span → session-id join for the callee ids.
pub struct RoleSide<'a, 's> {
    pub semantic: &'a Semantic<'s>,
    pub tables: &'a SymbolTables,
    /// The container span (the wrapper body's, else the program's).
    pub container_span: oxc_span::Span,
    /// Graph-row span → session id (functions AND module bindings), the
    /// TS `callee.sessionId` join (alternation.rs `session_join`).
    pub session_join: &'a HashMap<(u32, u32), String>,
}

/// Verdict with a log-friendly reason (TS `RoleAgreement` :126).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAgreement {
    pub agrees: bool,
    pub reason: &'static str,
}

impl RoleAgreement {
    fn agree(reason: &'static str) -> RoleAgreement {
        RoleAgreement {
            agrees: true,
            reason,
        }
    }
    fn refuse(reason: &'static str) -> RoleAgreement {
        RoleAgreement {
            agrees: false,
            reason,
        }
    }
}

/// TS `computeBindingRole` (:81) over a graph row.
pub fn compute_binding_role(row: &ModuleBindingNode, side: &RoleSide<'_, '_>) -> BindingRole {
    let (fn_callee_ids, has_binding_callees) = split_callees(&row.internal_callees, side);
    let content_shingles =
        binding_content_json(row, side).map(|json| compute_content_shingles(&json, side.tables));
    BindingRole {
        structural_hash: row.fingerprint_hash.clone(),
        content_shingles,
        fn_callee_ids,
        has_binding_callees,
    }
}

/// TS `splitCallees` (:110) over the row's callee spans through the
/// side's session join. A callee span that names no graph row is dropped
/// (the graph builds the edges from its own rows, so a miss cannot happen
/// — alternation.rs's `neighbor_ids` convention).
fn split_callees(spans: &[oxc_span::Span], side: &RoleSide<'_, '_>) -> (Vec<String>, bool) {
    let mut fn_callee_ids: Vec<String> = Vec::new();
    let mut has_binding_callees = false;
    for span in spans {
        let Some(session_id) = side.session_join.get(&(span.start, span.end)) else {
            continue;
        };
        if session_id.starts_with("module:") {
            has_binding_callees = true;
        } else {
            fn_callee_ids.push(session_id.clone());
        }
    }
    (fn_callee_ids, has_binding_callees)
}

/// The ESTree JSON of the node holding a module binding's hashable content
/// — the TS `resolveBindingContentPath` (function-graph.ts :411) + the
/// constantViolations[0] rule, mirroring graph.rs `binding_fingerprint_hash`
/// (the content's OWNER — this mirror exists because that fn is private;
/// keep the two in sync, the shingle stream must cover the SAME subtree the
/// fingerprint hashed). A class declaration hashes itself; a declarator
/// hashes its init; a bare declarator hashes the FIRST assignment's right
/// side; otherwise no content.
fn binding_content_json(row: &ModuleBindingNode, side: &RoleSide<'_, '_>) -> Option<Value> {
    use oxc_estree::ESTree;
    let nodes = side.semantic.nodes();
    let scoping = side.semantic.scoping();
    let symbol = row.symbol;
    let decl_node = nodes.get_node(scoping.symbol_declaration(symbol));
    let json: String = match decl_node.kind() {
        // A class declaration's own body IS the hashable content.
        AstKind::Class(c) => {
            let mut ser = oxc_estree::CompactSerializer::new(false, false);
            c.serialize(&mut ser);
            ser.into_string()
        }
        AstKind::VariableDeclarator(d) => match d.init.as_ref() {
            Some(init) => {
                let mut ser = oxc_estree::CompactSerializer::new(false, false);
                crate::babel_view::unparen(init).serialize(&mut ser);
                ser.into_string()
            }
            None => {
                // The TS constantViolations[0] — the redeclarations (the
                // OTHER declarators of the same name; oxc keeps ONE symbol
                // per name, so a redeclaration's identifier carries NO
                // symbol — graph.rs builds the same list) and the
                // assignments, in source order. Only an
                // AssignmentExpression violation yields content.
                let mut violations: Vec<oxc_span::Span> = Vec::new();
                for node in nodes.iter() {
                    let span = node.span();
                    if span.start < side.container_span.start || span.end > side.container_span.end
                    {
                        continue;
                    }
                    let AstKind::BindingIdentifier(id) = node.kind() else {
                        continue;
                    };
                    if id.name.as_str() != row.name || id.symbol_id.get().is_some() {
                        continue;
                    }
                    violations.push(oxc_span::Span::new(span.start, span.start));
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
                            violations.push(a.span());
                            break;
                        }
                        if parent == prev {
                            break;
                        }
                        prev = parent;
                    }
                }
                violations.sort_by_key(|s| s.start);
                let first = *violations.first()?;
                let node = nodes.iter().find(|n| {
                    n.span() == first && matches!(n.kind(), AstKind::AssignmentExpression(_))
                })?;
                let AstKind::AssignmentExpression(a) = node.kind() else {
                    unreachable!("filtered above");
                };
                let mut ser = oxc_estree::CompactSerializer::new(false, false);
                crate::babel_view::unparen(&a.right).serialize(&mut ser);
                ser.into_string()
            }
        },
        _ => return None,
    };
    let mut de = serde_json::Deserializer::from_str(&json);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).ok()
}

/// Slot-blind k-gram shingles over the content's serialized token stream
/// (TS `computeContentShingles` :58). Streams shorter than k yield one
/// shingle of the whole stream, so tiny contents still compare.
pub fn compute_content_shingles(content_json: &Value, tables: &SymbolTables) -> BTreeSet<String> {
    let tokens: Vec<String> = verbatim_tokens(content_json, tables)
        .into_iter()
        .map(blind_slot_ordinal)
        .collect();
    let mut shingles = BTreeSet::new();
    if tokens.len() <= SHINGLE_K {
        shingles.insert(tokens.join("\0"));
        return shingles;
    }
    for window in tokens.windows(SHINGLE_K) {
        shingles.insert(window.join("\0"));
        if shingles.len() >= SHINGLE_CAP {
            break;
        }
    }
    shingles
}

/// Binding-slot (`$3`) and label-slot (`L1`) tokens, ordinal-blinded (:47).
fn blind_slot_ordinal(token: String) -> String {
    let bytes = token.as_bytes();
    if bytes.first() == Some(&b'$') && bytes.len() > 1 && bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return "$".to_string();
    }
    if bytes.first() == Some(&b'L') && bytes.len() > 1 && bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return "L".to_string();
    }
    token
}

/// TS `bindingRolesAgree` (:156). Content must positively corroborate
/// (equal non-null hashes, or shingle overlap at the floor) — missing
/// evidence is a refusal, not agreement. The callee veto then compares the
/// prior's function callees mapped through the function matches.
pub fn binding_roles_agree(
    prior: &BindingRole,
    next: &BindingRole,
    prior_to_new_fn_ids: &HashMap<String, String>,
    allow_content_free_elimination: bool,
) -> RoleAgreement {
    let content = content_agreement(prior, next, allow_content_free_elimination);
    if !content.agrees {
        return content;
    }
    match callee_veto(prior, next, prior_to_new_fn_ids) {
        Some(veto) => veto,
        None => content,
    }
}

/// Positive content corroboration: hash equality or shingle overlap (:176).
fn content_agreement(
    prior: &BindingRole,
    next: &BindingRole,
    allow_content_free_elimination: bool,
) -> RoleAgreement {
    if prior.structural_hash.is_some() && prior.structural_hash == next.structural_hash {
        return RoleAgreement::agree("hash-equal");
    }
    let prior_shingles = prior.content_shingles.as_ref().filter(|s| !s.is_empty());
    let next_shingles = next.content_shingles.as_ref().filter(|s| !s.is_empty());
    if let (Some(a), Some(b)) = (prior_shingles, next_shingles) {
        let similarity = crate::matching::jaccard_similarity(a, b);
        if similarity >= SINGLE_VOTE_CONTENT_FLOOR {
            return RoleAgreement::agree("shingle-overlap");
        }
    }
    // exp066: SYMMETRIC content absence agrees by elimination — OPT-IN,
    // the license is the CALLER's exclusivity gates, never the roles
    // themselves (the twin tier's pairwise comparison must NOT get it).
    if allow_content_free_elimination
        && prior.structural_hash.is_none()
        && next.structural_hash.is_none()
        && prior_shingles.is_none()
        && next_shingles.is_none()
    {
        return RoleAgreement::agree("content-free-elimination");
    }
    if prior_shingles.is_none() || next_shingles.is_none() {
        return RoleAgreement::refuse("no-content-evidence");
    }
    RoleAgreement::refuse("content-below-floor")
}

/// Callee-identity veto, or None when inconclusive/agreeing (:218).
fn callee_veto(
    prior: &BindingRole,
    next: &BindingRole,
    prior_to_new_fn_ids: &HashMap<String, String>,
) -> Option<RoleAgreement> {
    if prior.has_binding_callees || next.has_binding_callees {
        return None;
    }
    if prior.fn_callee_ids.is_empty() || next.fn_callee_ids.is_empty() {
        return None;
    }
    let mut mapped: Vec<&String> = Vec::with_capacity(prior.fn_callee_ids.len());
    for prior_id in &prior.fn_callee_ids {
        let new_id = prior_to_new_fn_ids.get(prior_id)?;
        mapped.push(new_id);
    }
    let expected = ids_key_strings(mapped.iter().copied().cloned());
    let actual = ids_key_strings(next.fn_callee_ids.iter().cloned());
    if expected != actual {
        return Some(RoleAgreement::refuse("callee-mismatch"));
    }
    None
}

/// TS `[...new Set(ids)].sort().join("|")` (:233) — session ids are ASCII,
/// so a byte sort agrees with the default JS sort.
fn ids_key_strings<I: IntoIterator<Item = String>>(ids: I) -> String {
    let unique: BTreeSet<String> = ids.into_iter().collect();
    unique.into_iter().collect::<Vec<_>>().join("|")
}

// ---------------------------------------------------------------------------
// The token walk
// ---------------------------------------------------------------------------

/// TS-Granularity token stream of an ESTree JSON subtree, LITERALS VERBATIM
/// (the shingle walk runs under `preserveLiterals: true` —
/// binding-role.ts :61 — so only the keep branches of the literal
/// classification are needed).
///
/// DUPLICATION NOTICE: this mirrors `crate::hash::serialize`'s walk
/// decision-for-decision (identifier roles, slot keys, label namespaces,
/// per-class private slots, block unwrapping, skip keys) — it exists
/// because the canonical walk emits one concatenated `parts` STRING and the
/// shingles need the TOKEN sequence, and serialize.rs cannot be extended
/// from here. The proof test (`tokens_join_to_the_canonical_parts`) pins
/// the mirror to the real walk: for any subtree, the tokens joined MUST
/// equal `canonical_serialize(.., Verbatim).parts` byte-for-byte — a
/// masking decision that drifts between the two walks fails that test.
fn verbatim_tokens(root: &Value, tables: &SymbolTables) -> Vec<String> {
    let mut state = TokenState {
        tables,
        slot_by_symbol: HashMap::new(),
        label_slots: HashMap::new(),
        counter: 0,
        private_slots: None,
        tokens: Vec::new(),
    };
    walk_value(root, None, "", &mut state);
    state.tokens
}

struct TokenState<'a> {
    tables: &'a SymbolTables,
    slot_by_symbol: HashMap<oxc_semantic::SymbolId, String>,
    label_slots: HashMap<String, String>,
    counter: u32,
    private_slots: Option<HashMap<String, String>>,
    tokens: Vec<String>,
}

/// The keys the token walks drop (pub because gates.rs's private-pair
/// walk drops the same keys — the serialize.rs walk's list).
pub const SKIP_KEYS: [&str; 8] = [
    "type",
    "loc",
    "start",
    "end",
    "range",
    "extra",
    "leadingComments",
    "trailingComments",
];

fn walk_value(value: &Value, parent: Option<&Value>, key: &str, state: &mut TokenState<'_>) {
    match value {
        Value::Null => state.tokens.push("null".to_string()),
        Value::Bool(b) => state.tokens.push(if *b {
            "true".to_string()
        } else {
            "false".to_string()
        }),
        Value::Number(n) => state.tokens.push(n.to_string()),
        Value::String(s) => state.tokens.push(json_escape(s)),
        Value::Array(items) => {
            state.tokens.push("[".to_string());
            for item in items {
                walk_value(item, parent, key, state);
                state.tokens.push(",".to_string());
            }
            state.tokens.push("]".to_string());
        }
        Value::Object(_) => walk_node(value, parent, key, state),
    }
}

fn walk_node(node: &Value, parent: Option<&Value>, key: &str, state: &mut TokenState<'_>) {
    let Some(map) = node.as_object() else {
        walk_value(node, parent, key, state);
        return;
    };
    let node_type = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("<no-type>")
        .to_string();

    if node_type == "Identifier" {
        walk_identifier(map, parent, key, state);
        return;
    }
    if node_type == "PrivateIdentifier" {
        let name = map.get("name").and_then(Value::as_str).unwrap_or("");
        let token = private_name_token(name, state);
        state.tokens.push(token);
        return;
    }
    if let Some(token) = literal_token(map, &node_type) {
        state.tokens.push(token);
        return;
    }
    // Single-statement blocks at bare-statement positions unwrap.
    if node_type == "BlockStatement"
        && let Some(inner) = unwrappable_block(map, parent, key)
    {
        walk_node(inner, parent, key, state);
        return;
    }
    let outer_private_slots = state.private_slots.take();
    if node_type == "ClassExpression" || node_type == "ClassDeclaration" {
        state.private_slots = Some(HashMap::new());
    }

    state.tokens.push(format!("{node_type}{{"));
    for (k, v) in map.iter() {
        if SKIP_KEYS.contains(&k.as_str()) || k == "innerComments" || k == "shorthand" {
            continue;
        }
        if v.is_null() {
            continue;
        }
        state.tokens.push(format!("{k}:"));
        walk_value(v, Some(node), k, state);
        state.tokens.push(";".to_string());
    }
    state.tokens.push("}".to_string());

    state.private_slots = outer_private_slots;
}

fn walk_identifier(
    node: &serde_json::Map<String, Value>,
    parent: Option<&Value>,
    key: &str,
    state: &mut TokenState<'_>,
) {
    let name = node.get("name").and_then(Value::as_str).unwrap_or("");
    match identifier_role(parent, key) {
        "verbatim" => state.tokens.push(format!("I={name}")),
        "label" => {
            let size = state.label_slots.len();
            let slot = state
                .label_slots
                .entry(name.to_string())
                .or_insert_with(|| format!("L{size}"));
            state.tokens.push(slot.clone());
        }
        _ => {
            let start = node
                .get("start")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX) as u32;
            let symbol = state
                .tables
                .decl_by_start
                .get(&start)
                .or_else(|| state.tables.ref_by_start.get(&start))
                .copied();
            match symbol {
                Some(symbol_id) => {
                    let counter = state.counter;
                    let slot = state
                        .slot_by_symbol
                        .entry(symbol_id)
                        .or_insert_with(|| format!("${counter}"))
                        .clone();
                    if slot == format!("${counter}") {
                        state.counter += 1;
                    }
                    state.tokens.push(slot);
                }
                None => state.tokens.push(format!("I={name}")),
            }
        }
    }
}

/// The identifier-role rules — the serialize.rs walk's parent/key context.
fn identifier_role(parent: Option<&Value>, key: &str) -> &'static str {
    let Some(map) = parent.and_then(Value::as_object) else {
        return "slot";
    };
    let ptype = map.get("type").and_then(Value::as_str).unwrap_or("");
    let computed = map
        .get("computed")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let positional = matches!(
        (ptype, key),
        ("MemberExpression", "property")
            | ("OptionalMemberExpression", "property")
            | ("Property", "key")
            | ("MethodDefinition", "key")
            | ("PropertyDefinition", "key")
    );
    if positional && !computed {
        return "verbatim";
    }
    if ptype == "MetaProperty" {
        return "verbatim";
    }
    if matches!(
        (ptype, key),
        ("ExportSpecifier", "exported") | ("ImportSpecifier", "imported")
    ) {
        return "verbatim";
    }
    if matches!(
        (ptype, key),
        ("LabeledStatement", "label")
            | ("BreakStatement", "label")
            | ("ContinueStatement", "label")
    ) {
        return "label";
    }
    "slot"
}

/// The literal token for one literal node, verbatim policy only (the
/// blurred classes never reach the shingle stream).
fn literal_token(map: &serde_json::Map<String, Value>, node_type: &str) -> Option<String> {
    match node_type {
        "Literal" => {
            if let Some(pattern) = map
                .get("regex")
                .and_then(|r| r.get("pattern"))
                .and_then(Value::as_str)
            {
                let flags = map
                    .get("regex")
                    .and_then(|r| r.get("flags"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                return Some(format!("R={pattern}/{flags}"));
            }
            if let Some(bigint) = map.get("bigint").and_then(Value::as_str) {
                return Some(format!("B={bigint}"));
            }
            match map.get("value") {
                Some(Value::String(v)) => Some(format!("S={}", json_escape(v))),
                Some(Value::Number(n)) => Some(format!("N={n}")),
                // Booleans and null are NOT literal-classed (they fall to
                // the generic walk, parity with the TS).
                _ => None,
            }
        }
        "StringLiteral" | "DirectiveLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(format!("S={}", json_escape(value)))
        }
        "NumericLiteral" => {
            let value = map.get("value")?.as_f64()?;
            Some(format!("N={value}"))
        }
        "BigIntLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(format!("B={value}"))
        }
        "RegExpLiteral" => {
            let pattern = map.get("pattern")?.as_str()?;
            let flags = map.get("flags")?.as_str()?;
            Some(format!("R={pattern}/{flags}"))
        }
        "TemplateElement" => {
            let raw = map
                .get("value")
                .and_then(|v| v.get("raw"))
                .and_then(Value::as_str)?;
            let tail = map.get("tail").and_then(Value::as_bool).unwrap_or(false);
            Some(format!("Q={},tail={tail}", json_escape(raw)))
        }
        _ => None,
    }
}

fn private_name_token(name: &str, state: &mut TokenState<'_>) -> String {
    match &mut state.private_slots {
        None => format!("P=#{name}"),
        Some(slots) => {
            let size = slots.len();
            let slot = slots
                .entry(name.to_string())
                .or_insert_with(|| format!("P=${}", size + 1));
            slot.clone()
        }
    }
}

fn is_bare_statement_position(parent: Option<&Value>, key: &str) -> bool {
    let Some(map) = parent.and_then(Value::as_object) else {
        return false;
    };
    let ptype = map.get("type").and_then(Value::as_str).unwrap_or("");
    matches!(
        (ptype, key),
        ("IfStatement", "consequent")
            | ("IfStatement", "alternate")
            | ("ForStatement", "body")
            | ("ForInStatement", "body")
            | ("ForOfStatement", "body")
            | ("WhileStatement", "body")
            | ("DoWhileStatement", "body")
            | ("LabeledStatement", "body")
            | ("WithStatement", "body")
    )
}

fn unwrappable_block<'a>(
    node: &'a serde_json::Map<String, Value>,
    parent: Option<&Value>,
    key: &str,
) -> Option<&'a Value> {
    if !is_bare_statement_position(parent, key) {
        return None;
    }
    let body = node.get("body")?.as_array()?;
    if body.len() != 1 {
        return None;
    }
    let only = body.first()?;
    let otype = only.get("type").and_then(Value::as_str)?;
    if matches!(
        otype,
        "VariableDeclaration" | "FunctionDeclaration" | "ClassDeclaration"
    ) {
        return None;
    }
    Some(only)
}

/// JSON-safe escaping, matching Node's `JSON.stringify` semantics for the
/// BMP (the serialize.rs walk's rule).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The union of shingle sets a role gate computes on — the tests need the
/// jaccard similarity of two `BindingRole`s (the TS test suite computes it
/// directly).
pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    crate::matching::jaccard_similarity(a, b)
}

#[cfg(test)]
mod role_test;
