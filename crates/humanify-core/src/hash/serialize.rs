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
//! - literals per form: numbers and bigints EXACT under every form;
//!   strings blurred (volatile semver/ISO/hex-digest classes, length
//!   markers) or exact as the form says (`hash::forms` names every form
//!   and its consumers);
//! - single-statement blocks at bare-statement positions unwrap;
//! - class-private names stay verbatim (`P=#name`; the TS default mode)
//!   or blind to `P=#`, as the form says.
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

use super::forms::{
    CanonicalForm, Locals, Numbers, ParentNames, Privates, PropertyNames, Strings, Walk,
};

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

/// Serialize one subtree to the canonical token stream under `form` (a
/// [`Walk::Canonical`] form from `hash::forms`) and hash it. `root` is the
/// ESTree JSON of the subtree. The form's string, private-name and
/// wrapper-spelling settings drive the walk; its free-name setting is the
/// caller's (`hash::forms::FreeNames`).
pub fn canonical_serialize(
    root: &Value,
    tables: &SymbolTables,
    form: &CanonicalForm,
) -> CanonicalOutput {
    debug_assert!(
        form.walk == Walk::Canonical
            && form.locals == Locals::Slots
            && form.numbers == Numbers::Exact
            && form.property_names == PropertyNames::Verbatim
            && form.parent_names == ParentNames::Oxc
            && form.privates != Privates::Masked,
        "{} is not a setting of the canonical walk",
        form.name
    );
    let mut state = State {
        tables,
        slot_by_symbol: HashMap::new(),
        label_slots: HashMap::new(),
        mapping: Vec::new(),
        counter: 0,
        keep_strings: form.strings == Strings::Exact,
        blind_privates: form.privates == Privates::Blinded,
        wrapper_spelling: form.wrapper_spelling,
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
    keep_strings: bool,
    blind_privates: bool,
    wrapper_spelling: bool,
    parts: String,
}

fn sha256_16(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

// --- the walk ---------------------------------------------------------------

/// Keys skipped from the stream: identity and rename artifacts (the TS
/// side's SERIALIZE_SKIP_KEYS, plus oxc's location aliases). Shared with
/// `matching::statement_align`'s token walk.
pub(crate) const SKIP_KEYS: [&str; 8] = [
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

    // Literals: the form's classes.
    if let Some(token) = literal_token(map, &node_type, state.keep_strings) {
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

    // exp094 (wrapper spelling): a SAFE arrow walks under the
    // FunctionExpression token, so the same code twice-spelled by the
    // packaging tool (arrow vs function expression) hashes equal under
    // every form with the rule on (the match-key family). The ONE rule
    // lives in `hash::wrapper_spelling` (the statement hash's arm reads it
    // too, exp094b).
    if node_type == "ArrowFunctionExpression"
        && state.wrapper_spelling
        && super::wrapper_spelling::arrow_serializes_as_function(map)
    {
        state.parts.push_str("FunctionExpression");
    } else {
        state.parts.push_str(&node_type);
    }
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
}

/// Which spelling of an object member a walk's parents carry — the ONE
/// difference between the two walks that share [`identifier_role`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MemberNodeNames {
    /// oxc's ESTree as serialized: object members (properties, methods,
    /// destructuring keys) are all `Property` — this walk's input.
    Oxc,
    /// babel's names: `matching::statement_align`'s token walk translates
    /// oxc's `Property` into `ObjectProperty` / `ObjectMethod` before it
    /// walks the children (an untranslatable `Property` keeps oxc's name
    /// there, and its key is a slot).
    Babel,
}

/// The identifier-role rules (structural-hash.ts:554-590) over the ESTree
/// parent/key context. `computed` flips the member/key rules to slot.
/// `names` says which object-member spelling the parent carries.
pub(crate) fn identifier_role(
    parent: Option<&Value>,
    key: &str,
    names: MemberNodeNames,
) -> &'static str {
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
    let object_member = match names {
        MemberNodeNames::Oxc => ptype == "Property",
        MemberNodeNames::Babel => matches!(ptype, "ObjectMethod" | "ObjectProperty"),
    };
    let positional = (object_member && key == "key")
        || matches!(
            (ptype, key),
            ("MemberExpression", "property")
                | ("OptionalMemberExpression", "property")
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
    match identifier_role(parent, key, MemberNodeNames::Oxc) {
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
/// (structural-hash.ts:712-741): this walk's spelling of [`literal_class`],
/// a template element as ONE part (the hash concatenates the parts, so
/// the merge is byte-identical to the TS's two pushes).
fn literal_token(
    map: &serde_json::Map<String, Value>,
    node_type: &str,
    keep: bool,
) -> Option<String> {
    Some(match literal_class(map, node_type, keep)? {
        LiteralClass::One(token) => token,
        LiteralClass::Template { element, tail } => format!("{element},tail={tail}"),
    })
}

/// A literal node's class token(s).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LiteralClass {
    /// One token: `S=`, `N=`, `B=`, `R=`.
    One(String),
    /// A template element: the TS pushes `templateElementToken(...)` and
    /// `,tail=${tail}` as TWO parts — the token walk keeps that boundary
    /// (its k-gram windows sit on it), the hash walk concatenates.
    Template { element: String, tail: bool },
}

/// The literal class of one literal node, or None when not a literal
/// (structural-hash.ts:712-741; numbers EXACT under every form since
/// exp093 — only the string classes follow `keep`, the form's `Strings`).
/// Booleans and null are None: babel's BooleanLiteral/NullLiteral are not
/// literal-classed, and
/// each walk spells them its own way. The ONE owner for both walks (the
/// canonical serializer here, `matching::statement_align`'s token walk).
pub(crate) fn literal_class(
    map: &serde_json::Map<String, Value>,
    node_type: &str,
    keep: bool,
) -> Option<LiteralClass> {
    use LiteralClass::One;
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
                return Some(One(format!("R={pattern}/{flags}")));
            }
            if let Some(bigint) = map.get("bigint").and_then(|v| v.as_str()) {
                return Some(One(format!("B={bigint}")));
            }
            match map.get("value") {
                Some(Value::String(v)) => Some(One(string_literal_token(v, keep))),
                Some(Value::Number(n)) => Some(One(format!("N={n}"))),
                // Booleans and null are NOT literal-classed in the TS
                // either (babel's BooleanLiteral/NullLiteral fall through
                // to the generic walk) — None keeps that parity.
                _ => None,
            }
        }
        "StringLiteral" | "DirectiveLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(One(string_literal_token(value, keep)))
        }
        "NumericLiteral" => {
            let value = map.get("value")?.as_f64()?;
            Some(One(format!("N={value}")))
        }
        "BigIntLiteral" => {
            let value = map.get("value")?.as_str()?;
            Some(One(format!("B={value}")))
        }
        "RegExpLiteral" => {
            let pattern = map.get("pattern")?.as_str()?;
            let flags = map.get("flags")?.as_str()?;
            Some(One(format!("R={pattern}/{flags}")))
        }
        "TemplateElement" => {
            let raw = map
                .get("value")
                .and_then(|v| v.get("raw"))
                .and_then(|v| v.as_str())?;
            let tail = map.get("tail").and_then(|v| v.as_bool()).unwrap_or(false);
            Some(LiteralClass::Template {
                element: template_element_token(raw, keep),
                tail,
            })
        }
        _ => None,
    }
}

/// Literal tokens (structural-hash.ts:685-741): the volatile class or the
/// length marker — the STRING rules, untouched by exp093 (numbers are
/// exact under every form).
pub(crate) fn volatile_literal_token(value: &str) -> Option<String> {
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
pub(crate) fn string_literal_token(value: &str, keep: bool) -> String {
    if keep {
        return format!("S={}", json_escape(value));
    }
    format!(
        "S={}",
        volatile_literal_token(value)
            .unwrap_or_else(|| format!("__STR_{}__", value.chars().count()))
    )
}

pub(crate) fn template_element_token(raw: &str, keep: bool) -> String {
    if keep {
        return format!("Q={}", json_escape(raw));
    }
    format!(
        "Q={}",
        volatile_literal_token(raw).unwrap_or_else(|| raw.chars().count().to_string())
    )
}

// `numeric_magnitude` (structural-hash.ts:719-723) — DELETED by exp093
// (2026-09-29): the magnitude buckets bridged 0 of 64,061 real matched
// pairs while their edges manufactured the only known miss, so numbers
// are exact under every form now.

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
/// braces change where the binding lives. Shared with
/// `matching::statement_align`'s token walk.
pub(crate) fn unwrappable_block<'a>(
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

// The wrapper-spelling rule (exp094) lives in `super::wrapper_spelling` —
// shared with the statement hash since exp094b, one rule for both Rust
// arms plus the TS instruments' `experiments/lib/js/wrapper-spelling.ts`.

/// Private-name tokens (structural-hash.ts:841-864): VERBATIM — the TS
/// default. The TS's `privateNamesAsSlots` mode (per-class order-keyed
/// `P=$n`) has one caller, `computeRenameInvariantSignature`
/// (output-validation.ts), which is not a hash this serializer produces;
/// the per-class slot map this function used to consult was clobbered by
/// every non-class child before a private token could read it (removed
/// 2026-09-25 — it never changed a byte). Under a `Privates::Blinded` form
/// (the twin gate's masked stream, the library units, the rename
/// signature) every private token reads `P=#` — at token emission, never
/// as a substring pass over the stream, where a string literal could carry
/// the same text.
fn private_name_token(name: &str, state: &State<'_>) -> String {
    if state.blind_privates {
        return "P=#".to_string();
    }
    format!("P=#{name}")
}

/// The stream's string escaping: JSON-safe, matching Node's `JSON.stringify`
/// semantics for the BMP (the corpus is BMP-only; the supplementary-plane
/// fixture is planted before the converter's green means anything — 11 OQ2).
/// Shared with `matching::statement_align`'s token walk.
pub(crate) fn json_escape(s: &str) -> String {
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
