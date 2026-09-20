//! The canonical serialization (02 §4a): the deterministic token stream the
//! hash families consume.
//!
//! TS original: `src/analysis/structural-hash.ts`'s rename-invariant
//! serialization walk (binding-keyed placeholders over beautified text).
//! The Rust serializer does NOT reproduce those bytes — it can't (oxc's AST
//! shape and field order differ from Babel's by design); the PHASE-1 GATE is
//! decision parity: the equivalence classes the hashes define must match
//! (07 §4's partition comparison), not the digest strings.
//!
//! Substrate: oxc's own ESTree JSON (`Program::to_estree_json`) — generic
//! over every node type (no per-type walk to fall out of date), field order
//! = oxc's own; the walk emits keys in serde_json's BTreeMap order (a fixed
//! total order, never insertion order — 02 §5). The masking rules:
//!
//! - identifiers are classified by structural position (verbatim member
//!   properties / object keys / meta / export-import specifier EXTERNAL
//!   sides; labels in their own namespace; everything else a SLOT);
//! - slots are keyed by the resolved SYMBOL (oxc's native identity — the
//!   TS scheme's binding-keyed placeholder, made native);
//! - free identifiers are verbatim (version-stable content);
//! - literals per key family: blurred (volatile semver/ISO/hex-digest
//!   classes, string length markers, numeric magnitudes) or verbatim;
//! - single-statement blocks at bare-statement positions unwrap;
//! - class-private names get per-class order-keyed slots or stay verbatim.
//!
//! One walk produces hash + placeholder mapping; slot ordinals are assigned
//! by first occurrence in the walk, so structurally identical functions get
//! aligned ordinals regardless of binding names (the invariant
//! `translatePriorNames` relies on).

use std::collections::HashMap;

use oxc_semantic::{Semantic, SymbolId};
use oxc_span::GetSpan;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Which literal policy the walk runs under: blurred (`MatchKey`) or
/// verbatim (`IdentityKey` / the declaration-body hash).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiteralPolicy {
    Blurred,
    Verbatim,
}

/// The symbol tables one walk consults: span start -> symbol for
/// declarations and resolved references. Built once per semantic and
/// shared by every hash call in the run.
#[derive(Clone, Debug, Default)]
pub struct SymbolTables {
    pub decl_by_start: HashMap<u32, SymbolId>,
    pub ref_by_start: HashMap<u32, SymbolId>,
}

impl SymbolTables {
    /// Build from the semantic: declarations from each symbol's declaration
    /// span; references from the resolved-reference table (unresolved = the
    /// free identifiers, which stay verbatim by absence).
    pub fn build(semantic: &Semantic<'_>) -> SymbolTables {
        let scoping = semantic.scoping();
        let mut tables = SymbolTables {
            decl_by_start: HashMap::with_capacity(scoping.symbol_ids().len()),
            ref_by_start: HashMap::new(),
        };
        for symbol_id in scoping.symbol_ids() {
            let span = scoping.symbol_span(symbol_id);
            tables.decl_by_start.insert(span.start, symbol_id);
            for &reference_id in scoping.get_resolved_reference_ids(symbol_id) {
                let reference = scoping.get_reference(reference_id);
                let node_id = reference.node_id();
                let node = semantic.nodes().get_node(node_id);
                tables.ref_by_start.insert(node.span().start, symbol_id);
            }
        }
        // REDECLARED `var` positions: oxc keeps ONE symbol per redeclared
        // name and `symbol_span` holds a single span, so every OTHER
        // declarator's identifier is in NEITHER table — the walk then
        // serialized the redeclaration's id as a free identifier
        // (`I=<name>`), which differs across versions, while babel resolves
        // every occurrence (declarator ids included) to the same Binding
        // and slots them. Each declaring identifier carries its symbol id
        // (the binder sets it on every declaration position), so map them
        // all.
        for node in semantic.nodes().iter() {
            if let oxc_ast::AstKind::BindingIdentifier(ident) = node.kind()
                && let Some(symbol_id) = ident.symbol_id.get()
            {
                tables.decl_by_start.insert(ident.span.start, symbol_id);
            }
        }
        tables
    }
}

/// One walk's output: the 16-hex hash + the placeholder table (slot ->
/// (symbol id, original name), binding slots only — the symbol id lets
/// consumers join slots to declarations by identity, never by name).
pub struct CanonicalOutput {
    pub hash: String,
    pub mapping: Vec<(String, Option<SymbolId>, String)>,
    /// The token stream itself — diagnostics only (the WP1.5 gate's class
    /// divergences were bisected through it).
    pub parts: String,
}

/// Serialize one subtree to the canonical token stream and hash it.
/// `root` is the ESTree JSON of the subtree.
pub fn canonical_serialize(
    root: &Value,
    tables: &SymbolTables,
    policy: LiteralPolicy,
) -> CanonicalOutput {
    let mut state = State {
        tables,
        slot_by_symbol: HashMap::new(),
        label_slots: HashMap::new(),
        mapping: Vec::new(),
        counter: 0,
        preserve_literals: policy == LiteralPolicy::Verbatim,
        private_slots: None,
        parts: String::with_capacity(4096),
    };
    serialize_value(root, None, "", &mut state);
    CanonicalOutput {
        hash: sha256_16(state.parts.as_bytes()),
        mapping: state.mapping,
        parts: state.parts,
    }
}

struct State<'a> {
    tables: &'a SymbolTables,
    slot_by_symbol: HashMap<SymbolId, String>,
    label_slots: HashMap<String, String>,
    mapping: Vec<(String, Option<SymbolId>, String)>,
    counter: u32,
    preserve_literals: bool,
    private_slots: Option<HashMap<String, String>>,
    parts: String,
}

fn sha256_16(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

// --- the walk ---------------------------------------------------------------

/// Keys skipped from the stream: identity and rename artifacts (the TS
/// side's SERIALIZE_SKIP_KEYS, plus oxc's location aliases).
const SKIP_KEYS: [&str; 8] = [
    "type",
    "loc",
    "start",
    "end",
    "range",
    "extra",
    "leadingComments",
    "trailingComments",
];

fn serialize_value(value: &Value, parent: Option<&Value>, key: &str, state: &mut State<'_>) {
    match value {
        Value::Null => state.parts.push_str("null"),
        Value::Bool(b) => state.parts.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => state.parts.push_str(&n.to_string()),
        Value::String(s) => state.parts.push_str(&json_escape(s)),
        Value::Array(items) => {
            state.parts.push('[');
            for item in items {
                serialize_value(item, parent, key, state);
                state.parts.push(',');
            }
            state.parts.push(']');
        }
        Value::Object(_) => serialize_node(value, parent, key, state),
    }
}

fn serialize_node(node: &Value, parent: Option<&Value>, key: &str, state: &mut State<'_>) {
    let Some(map) = node.as_object() else {
        // Not an object in node position: serialize as a scalar.
        serialize_value(node, parent, key, state);
        return;
    };
    let node_type = map
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("<no-type>")
        .to_string();

    if node_type == "Identifier" {
        serialize_identifier(map, parent, key, state);
        return;
    }

    // Private names: member keys, not scope bindings.
    if node_type == "PrivateIdentifier" {
        let name = map.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let token = private_name_token(name, state);
        state.parts.push_str(&token);
        return;
    }

    // Literals: the policy classes.
    if let Some(token) = literal_token(map, &node_type, state.preserve_literals) {
        state.parts.push_str(&token);
        return;
    }

    // Single-statement blocks at bare-statement positions unwrap.
    if node_type == "BlockStatement"
        && let Some(inner) = unwrappable_block(map, parent, key)
    {
        serialize_node(inner, parent, key, state);
        return;
    }

    // Per-class private-slot numbering: a fresh map per class.
    let outer_private_slots = state.private_slots.take();
    if node_type == "ClassExpression" || node_type == "ClassDeclaration" {
        state.private_slots = Some(HashMap::new());
    }

    state.parts.push_str(&node_type);
    state.parts.push('{');
    // serde_json's Map is a BTreeMap: keys iterate alphabetically — a fixed
    // total order, never insertion order (02 §5).
    for (k, v) in map.iter() {
        if SKIP_KEYS.contains(&k.as_str()) || k == "innerComments" || k == "shorthand" {
            continue;
        }
        if v.is_null() {
            continue;
        }
        state.parts.push_str(k);
        state.parts.push(':');
        serialize_value(v, Some(node), k, state);
        state.parts.push(';');
    }
    state.parts.push('}');

    state.private_slots = outer_private_slots;
}

/// The identifier-role rules (structural-hash.ts:554-590) over the ESTree
/// parent/key context. `computed` flips the member/key rules to slot.
fn identifier_role(parent: Option<&Value>, key: &str) -> &'static str {
    let Some(parent) = parent else {
        return "slot";
    };
    let Some(map) = parent.as_object() else {
        return "slot";
    };
    let ptype = map.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let computed = map
        .get("computed")
        .and_then(|c| c.as_bool())
        .unwrap_or(false);
    // oxc's ESTree type names (the JSON this walks is oxc's `to_estree_json`
    // output, NOT babel's): ObjectProperty / BindingProperty /
    // AssignmentTargetProperty* / object methods all emit as `Property`
    // (oxc_ast js.rs renames + oxc merging object methods into
    // ObjectProperty); class methods are `MethodDefinition`, class fields
    // `PropertyDefinition`. babel's type names (ObjectProperty/ObjectMethod/
    // ClassMethod/ClassProperty — structural-hash.ts :554-571) NEVER occur
    // here, and a list keyed on them silently disabled the verbatim rule —
    // a shorthand destructuring key (a binding reference) then fell through
    // to the slot arm while TS hashes it verbatim.
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

fn serialize_identifier(
    node: &serde_json::Map<String, Value>,
    parent: Option<&Value>,
    key: &str,
    state: &mut State<'_>,
) {
    let name = node.get("name").and_then(|v| v.as_str()).unwrap_or("");
    match identifier_role(parent, key) {
        "verbatim" => {
            state.parts.push_str("I=");
            state.parts.push_str(name);
        }
        "label" => {
            let size = state.label_slots.len();
            let slot = state
                .label_slots
                .entry(name.to_string())
                .or_insert_with(|| format!("L{size}"));
            state.parts.push_str(slot);
        }
        _ => {
            let start = node
                .get("start")
                .and_then(|v| v.as_u64())
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
                        // First occurrence of this symbol in the walk: its
                        // ordinal is assigned here, and the placeholder
                        // mapping records the symbol id + original name.
                        state.counter += 1;
                        state
                            .mapping
                            .push((slot.clone(), Some(symbol_id), name.to_string()));
                    }
                    state.parts.push_str(&slot);
                }
                None => {
                    // Free identifier — version-stable content.
                    state.parts.push_str("I=");
                    state.parts.push_str(name);
                }
            }
        }
    }
}

/// The literal token for one literal node, or None when not a literal
/// (structural-hash.ts:712-741).
fn literal_token(
    map: &serde_json::Map<String, Value>,
    node_type: &str,
    keep: bool,
) -> Option<String> {
    match node_type {
        // oxc's ESTree emits the STANDARD name "Literal" for every literal
        // (babel names StringLiteral/NumericLiteral/BigIntLiteral/
        // RegExpLiteral separately); classify by the value's JSON type the
        // way babel's type names classify (statement_hash.rs's mapping —
        // without this arm the generic walk embeds the verbatim value and
        // same-length-different-content strings split classes the TS
        // blurs together: the factory-class divergences the WP1.5 gate
        // caught on the oracle pairs).
        "Literal" => {
            if let Some(pattern) = map
                .get("regex")
                .and_then(|r| r.get("pattern"))
                .and_then(|v| v.as_str())
            {
                let flags = map
                    .get("regex")
                    .and_then(|r| r.get("flags"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                return Some(format!("R={pattern}/{flags}"));
            }
            if let Some(bigint) = map.get("bigint").and_then(|v| v.as_str()) {
                return Some(if keep {
                    format!("B={bigint}")
                } else {
                    "B=0".to_string()
                });
            }
            match map.get("value") {
                Some(Value::String(v)) => Some(string_literal_token(v, keep)),
                Some(Value::Number(n)) => Some(if keep {
                    format!("N={n}")
                } else {
                    numeric_magnitude(n.as_f64().unwrap_or(0.0))
                }),
                // Booleans and null are NOT literal-classed in the TS
                // either (babel's BooleanLiteral/NullLiteral fall through
                // to the generic walk) — None keeps that parity.
                _ => None,
            }
        }
        "StringLiteral" | "DirectiveLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(string_literal_token(value, keep))
        }
        "NumericLiteral" => {
            let value = map.get("value")?.as_f64()?;
            Some(if keep {
                format!("N={value}")
            } else {
                numeric_magnitude(value)
            })
        }
        "BigIntLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(if keep {
                format!("B={value}")
            } else {
                "B=0".to_string()
            })
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
                .and_then(|v| v.as_str())?;
            let tail = map.get("tail").and_then(|v| v.as_bool()).unwrap_or(false);
            Some(format!("{},tail={tail}", template_element_token(raw, keep)))
        }
        _ => None,
    }
}

/// Literal tokens (structural-hash.ts:685-741): exact when preserving, else
/// the volatile class or the length marker.
fn volatile_literal_token(value: &str) -> Option<String> {
    if is_volatile_semver(value) {
        Some("__VOLATILE_SEMVER__".to_string())
    } else if is_volatile_iso8601(value) {
        Some("__VOLATILE_ISO8601__".to_string())
    } else if is_volatile_hex_digest(value) {
        Some(format!("__VOLATILE_HEX_{}__", value.len()))
    } else {
        None
    }
}

/// String literal token: verbatim (JSON-escaped) or the blurred class.
fn string_literal_token(value: &str, keep: bool) -> String {
    if keep {
        return format!("S={}", json_escape(value));
    }
    format!(
        "S={}",
        volatile_literal_token(value)
            .unwrap_or_else(|| format!("__STR_{}__", value.chars().count()))
    )
}

fn template_element_token(raw: &str, keep: bool) -> String {
    if keep {
        return format!("Q={}", json_escape(raw));
    }
    format!(
        "Q={}",
        volatile_literal_token(raw).unwrap_or_else(|| raw.chars().count().to_string())
    )
}

/// Numeric magnitude (structural-hash.ts:719-723).
fn numeric_magnitude(value: f64) -> String {
    if value == 0.0 {
        return "N=0".to_string();
    }
    format!("N={}", (value.abs() + 1.0).log10().floor() as i64)
}

/// Bare-statement positions (structural-hash.ts:748-758).
fn is_bare_statement_position(parent: Option<&Value>, key: &str) -> bool {
    let Some(parent) = parent else { return false };
    let Some(map) = parent.as_object() else {
        return false;
    };
    let ptype = map.get("type").and_then(|t| t.as_str()).unwrap_or("");
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

/// A single-statement block at a bare position unwraps — unless the lone
/// statement is scoping-relevant (let/const/class/function), where the
/// braces change where the binding lives.
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
    let otype = only.get("type").and_then(|t| t.as_str())?;
    if matches!(
        otype,
        "VariableDeclaration" | "FunctionDeclaration" | "ClassDeclaration"
    ) {
        return None;
    }
    Some(only)
}

/// Private-name tokens (structural-hash.ts:841-864): verbatim unless the
/// walk runs under a class's slot numbering.
fn private_name_token(name: &str, state: &mut State<'_>) -> String {
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

/// The stream's string escaping: JSON-safe, matching Node's `JSON.stringify`
/// semantics for the BMP (the corpus is BMP-only; the supplementary-plane
/// fixture is planted before the converter's green means anything — 11 OQ2).
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

/// ^v?\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$
fn is_volatile_semver(value: &str) -> bool {
    let b = value.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'v'));
    let digits = |i: &mut usize| -> bool {
        let start = *i;
        while *i < b.len() && b[*i].is_ascii_digit() {
            *i += 1;
        }
        *i > start
    };
    if !digits(&mut i) {
        return false;
    }
    // The pattern consumes the dot ITSELF (`\.`) before the next digit
    // run — the loop must advance past it. (This used to check the dot
    // without advancing, so the second group read the dot as a non-digit
    // and every semver-looking string blurred as a plain length marker —
    // caught by the WP1.5 gate's class check on the oracle pairs, where
    // version-barrel factories split classes the TS groups.)
    for _ in 0..2 {
        if b.get(i) != Some(&b'.') {
            return false;
        }
        i += 1;
        if !digits(&mut i) {
            return false;
        }
    }
    if b.get(i) == Some(&b'-') {
        i += 1;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'-') {
            i += 1;
        }
    }
    if b.get(i) == Some(&b'+') {
        i += 1;
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'-') {
            i += 1;
        }
    }
    i == b.len()
}

/// ISO-8601 timestamp: ^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(.\d+)?(Z|[+-]\d{2}:?\d{2})?$
fn is_volatile_iso8601(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() < 19 {
        return false;
    }
    let digits_at = |i: usize, n: usize| b[i..i + n].iter().all(|c| c.is_ascii_digit());
    if !(digits_at(0, 4)
        && b[4] == b'-'
        && digits_at(5, 2)
        && b[7] == b'-'
        && digits_at(8, 2)
        && b[10] == b'T'
        && digits_at(11, 2)
        && b[13] == b':'
        && digits_at(14, 2)
        && b[16] == b':'
        && digits_at(17, 2))
    {
        return false;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    match b.get(i) {
        None => true,
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+') | Some(b'-') => {
            i += 1;
            let rest = &b[i..];
            (rest.len() == 2 && rest.iter().all(|c| c.is_ascii_digit()))
                || (rest.len() == 5
                    && rest[..2].iter().all(|c| c.is_ascii_digit())
                    && rest[2] == b':'
                    && rest[3..].iter().all(|c| c.is_ascii_digit()))
        }
        _ => false,
    }
}

/// 16-64 hex digits.
fn is_volatile_hex_digest(value: &str) -> bool {
    (16..=64).contains(&value.len()) && value.bytes().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod semver_probe {
    /// The WP1.5-gate regression: `1.9.0` and `0.208.0` are BOTH semver —
    /// the dot-advance bug used to blur them as plain length markers, so
    /// version-barrel factories split classes the TS groups.
    #[test]
    fn semver_detection() {
        assert!(super::is_volatile_semver("1.9.0"));
        assert!(super::is_volatile_semver("0.208.0"));
        assert!(super::is_volatile_semver("v2.1.215"));
        assert!(super::is_volatile_semver("1.9.0-beta.3"));
        assert!(super::is_volatile_semver("1.9.0+build.7"));
        assert!(!super::is_volatile_semver(">=15.7.0"));
        assert!(!super::is_volatile_semver("1.9"));
        assert!(!super::is_volatile_semver("1.9.0.0"));
    }
}
