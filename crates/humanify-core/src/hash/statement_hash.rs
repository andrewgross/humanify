//! The rename-invariant statement hash (TS: split/statement-hash.ts) — the
//! content-identity key for the split's hash-keyed file inheritance.
//!
//! Rules carried over exactly: the hash covers node types, tree shape,
//! literals, operators and declaration kinds; EVERY identifier name is
//! masked (humanify renames bindings AND export-object member names, so
//! property identifiers are masked too — two statements hash equal iff
//! they are the same code modulo renaming). Consequence (measured, and the
//! inheritance tier compensates): short generic statements collide across
//! unrelated code.
//!
//! Iterative (explicit stack) over the ESTree JSON so multi-thousand-line
//! statements cannot overflow the stack; array holes get an explicit
//! marker so they cannot alias the hole-free spelling. Field order is the
//! child order of the ESTree JSON (oxc's own, alphabetical in the map) —
//! the partition comparison makes the order's exact choice immaterial.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// The ledger's `hashVersion` — bump when the serialization changes shape.
/// A prior ledger hashed under a different version is REFUSED by every
/// reader (`StableSplitLedger::hashes_current`), never misread.
///
/// 1 = the TS statement-hash bytes (`src/split/statement-hash.ts`), which
/// every TS-era ledger carries. 2 = THIS function's bytes (WP5.6e,
/// 2026-09-25: the structuralSignature exemption ended, the Rust hashes
/// are the only hashes). The two define the same partition on every
/// measured input (the M3 injection's bijection proof), but never the same
/// bytes, so a v1 ledger read as v2 would silently join nothing.
pub const STATEMENT_HASH_VERSION: u64 = 2;

/// The value-bearing part of a node (statement-hash.ts's nodeContent): what
/// distinguishes two structurally-identical trees; identifier names
/// deliberately absent.
fn node_content(node: &serde_json::Map<String, Value>) -> String {
    let get = |k: &str| node.get(k).cloned().unwrap_or(Value::Null);
    let t = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    // oxc's ESTree emits literals as type "Literal" (ESTree standard);
    // babel names StringLiteral/NumericLiteral/BooleanLiteral/NullLiteral/
    // RegExpLiteral separately. The TS hash distinguishes literals by TYPE
    // NAME, so the Rust content must carry the same CLASS distinctions:
    // every literal class gets an explicit marker, and within a class the
    // value distinguishes. (String "1" and number 1 must not collide.)
    match t {
        "StringLiteral" | "DirectiveLiteral" => {
            format!("s:{}", get("value").as_str().unwrap_or(""))
        }
        "BigIntLiteral" => format!("n:{}", get("value").as_str().unwrap_or("")),
        "Literal" if get("value").is_string() => {
            format!("s:{}", get("value").as_str().unwrap_or(""))
        }
        "Literal" if get("value").is_number() => {
            format!("n:{}", get("value"))
        }
        "Literal" if get("value").is_boolean() => {
            format!("b:{}", get("value"))
        }
        "Literal" => {
            // Classify on the NODE's own fields, not `value` alone: oxc's
            // ESTree BigInt is {value: null, raw: "0n", bigint: "0"} and a
            // plain null is {value: null, raw: "null"} — value-null alone
            // cannot tell them apart (the collision this branch had).
            if let Some(pattern) = get("regex").get("pattern").and_then(|v| v.as_str()) {
                return format!(
                    "r:{}/{}",
                    pattern,
                    get("regex")
                        .get("flags")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                );
            }
            if let Some(bigint) = get("bigint").as_str() {
                return format!("B:{bigint}");
            }
            match get("value") {
                Value::String(v) => format!("s:{v}"),
                Value::Number(n) => format!("n:{n}"),
                Value::Bool(b) => format!("b:{b}"),
                _ => {
                    // value-null: a template's raw (value:{raw}) or plain null.
                    if let Some(raw) = get("value").get("raw").and_then(|v| v.as_str()) {
                        return format!("s:{raw}");
                    }
                    if let Some(raw) = get("raw").as_str() {
                        return format!("z:{raw}");
                    }
                    "null".to_string()
                }
            }
        }
        "NumericLiteral" | "BooleanLiteral" => match get("value") {
            Value::Number(n) => format!("n:{n}"),
            Value::Bool(b) => format!("b:{b}"),
            _ => String::new(),
        },
        "NullLiteral" => "null".to_string(),
        "RegExpLiteral" => format!(
            "r:{}/{}",
            get("pattern").as_str().unwrap_or(""),
            get("flags").as_str().unwrap_or("")
        ),
        "TemplateElement" => get("value")
            .get("raw")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        "VariableDeclaration" => get("kind").as_str().unwrap_or("").to_string(),
        "BinaryExpression" | "LogicalExpression" | "AssignmentExpression" | "UnaryExpression" => {
            get("operator").as_str().unwrap_or("").to_string()
        }
        "UpdateExpression" => format!(
            "{}{}",
            get("operator").as_str().unwrap_or(""),
            if get("prefix").as_bool().unwrap_or(false) {
                "pre"
            } else {
                "post"
            }
        ),
        "MemberExpression"
        | "OptionalMemberExpression"
        | "ObjectProperty"
        | "ObjectMethod"
        | "ClassMethod"
        | "ClassProperty" => {
            // Babel encodes optional access in the TYPE NAME
            // (OptionalMemberExpression); oxc's ESTree uses a plain type +
            // an `optional` scalar — the content must carry the same
            // class distinction.
            let computed = if get("computed").as_bool().unwrap_or(false) {
                "computed"
            } else {
                ""
            };
            let optional = if get("optional").as_bool().unwrap_or(false) {
                "optional"
            } else {
                ""
            };
            format!("{computed}{optional}")
        }
        "CallExpression" | "OptionalCallExpression" | "NewExpression" => {
            // Same class distinction as the member access above: babel's
            // OptionalCallExpression is a type; oxc carries the flag.
            if get("optional").as_bool().unwrap_or(false) {
                "optional".to_string()
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// Child fields of a node, in the JSON map's (alphabetical) order; arrays
/// flatten with hole markers; `close`-sentinels bracket each node.
enum Item<'a> {
    Node(&'a Value),
    Close,
    Hole,
}

fn is_node(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        .is_some()
}

fn push_children<'a>(stack: &mut Vec<Item<'a>>, child: &'a Value) {
    match child {
        Value::Array(items) => {
            for item in items {
                if item.is_null() {
                    stack.push(Item::Hole);
                } else {
                    push_children(stack, item);
                }
            }
        }
        v if is_node(v) => stack.push(Item::Node(v)),
        _ => {}
    }
}

/// Hash one statement (ESTree JSON of the statement).
pub fn statement_hash(stmt: &Value) -> String {
    let mut hasher = Sha256::new();
    let mut stack: Vec<Item<'_>> = vec![Item::Node(stmt)];
    while let Some(item) = stack.pop() {
        match item {
            Item::Close => {
                hasher.update(")");
            }
            Item::Hole => {
                hasher.update("_");
            }
            Item::Node(node) => {
                let Some(map) = node.as_object() else {
                    continue;
                };
                let t = map.get("type").and_then(|v| v.as_str()).unwrap_or("");
                // The node line carries EXACTLY the TS nodeContent's value
                // (node types, literals, operators, declaration kinds,
                // computed-ness) — no more: the equivalence relation must
                // match the TS side's for the partition gate. Identifiers
                // contribute their type alone (names masked); scalars not
                // in nodeContent are dropped, exactly as the TS walk drops
                // everything outside VISITOR_KEYS + nodeContent.
                hasher.update(format!("({t}\x00{}\x00", node_content(map)));
                stack.push(Item::Close);
                // Descend only into child NODES / arrays of them — the
                // VISITOR_KEYS walk's semantics; scalars never descend.
                for v in map.values() {
                    push_children(&mut stack, v);
                }
            }
        }
    }
    let digest = hasher.finalize();
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}
