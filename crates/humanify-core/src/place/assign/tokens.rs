//! GRADED shape tokens for one fossil module — TS `moduleTokens`
//! (fossil-assign.ts, exp078): tree-shape triples `n:<grand>><parent>><type>`
//! over BABEL node types in @babel/types `VISITOR_KEYS` child order, plus
//! the rename-proof literal values (`s:<first 40 UTF-16 units>`,
//! `m:<JS number>`), each hashed to 8 hex of sha1; a Set in first-seen
//! order. NO identifier-derived tokens.
//!
//! The substrate is oxc's ESTree JSON, so the walk TRANSLATES to the babel
//! shape node by node (lesson 2/14): `Literal` → its babel literal class;
//! `Property` → ObjectProperty/ObjectMethod (the method's function fields
//! laid flat); `MethodDefinition`/`PropertyDefinition` → ClassMethod /
//! ClassProperty (their Private* forms for a `#key`); `PrivateIdentifier`
//! → PrivateName{id}; a directive statement → Directive{DirectiveLiteral};
//! `import(x)` → CallExpression{callee: Import} (babel 7's default);
//! `ParenthesizedExpression` and `ChainExpression` are transparent, and
//! a link ON an optional chain is babel's Optional* node
//! ([`crate::matching::statement_align::chain_link`]).
//!
//! Iterative (an explicit stack): statements nest thousands deep.

use serde_json::{Map, Value};
use sha1::{Digest, Sha1};

use crate::twins::fossil::FossilModule;

/// @babel/types 7.29.7 `VISITOR_KEYS` for every non-TS/Flow/JSX node type
/// the walk can meet (generated with `t.VISITOR_KEYS`; keys the ESTree JSON
/// never carries — typeParameters, decorators, … — simply find nothing).
static VISITOR_KEYS: &[(&str, &[&str])] = &[
    ("ArrayExpression", &["elements"]),
    ("AssignmentExpression", &["left", "right"]),
    ("BinaryExpression", &["left", "right"]),
    ("Directive", &["value"]),
    ("BlockStatement", &["directives", "body"]),
    ("BreakStatement", &["label"]),
    ("CallExpression", &["callee", "arguments"]),
    ("CatchClause", &["param", "body"]),
    (
        "ConditionalExpression",
        &["test", "consequent", "alternate"],
    ),
    ("ContinueStatement", &["label"]),
    ("DoWhileStatement", &["body", "test"]),
    ("ExpressionStatement", &["expression"]),
    ("ForInStatement", &["left", "right", "body"]),
    ("ForStatement", &["init", "test", "update", "body"]),
    ("FunctionDeclaration", &["id", "params", "body"]),
    ("FunctionExpression", &["id", "params", "body"]),
    ("IfStatement", &["test", "consequent", "alternate"]),
    ("LabeledStatement", &["label", "body"]),
    ("LogicalExpression", &["left", "right"]),
    ("MemberExpression", &["object", "property"]),
    ("NewExpression", &["callee", "arguments"]),
    ("ObjectExpression", &["properties"]),
    ("ObjectMethod", &["key", "params", "body"]),
    ("ObjectProperty", &["key", "value"]),
    ("RestElement", &["argument"]),
    ("ReturnStatement", &["argument"]),
    ("SequenceExpression", &["expressions"]),
    ("SwitchCase", &["test", "consequent"]),
    ("SwitchStatement", &["discriminant", "cases"]),
    ("ThrowStatement", &["argument"]),
    ("TryStatement", &["block", "handler", "finalizer"]),
    ("UnaryExpression", &["argument"]),
    ("UpdateExpression", &["argument"]),
    ("VariableDeclaration", &["declarations"]),
    ("VariableDeclarator", &["id", "init"]),
    ("WhileStatement", &["test", "body"]),
    ("WithStatement", &["object", "body"]),
    ("AssignmentPattern", &["left", "right"]),
    ("ArrayPattern", &["elements"]),
    ("ArrowFunctionExpression", &["params", "body"]),
    ("ClassBody", &["body"]),
    ("ClassExpression", &["id", "superClass", "body"]),
    ("ClassDeclaration", &["id", "superClass", "body"]),
    ("ForOfStatement", &["left", "right", "body"]),
    ("MetaProperty", &["meta", "property"]),
    ("ClassMethod", &["key", "params", "body"]),
    ("ObjectPattern", &["properties"]),
    ("SpreadElement", &["argument"]),
    ("TaggedTemplateExpression", &["tag", "quasi"]),
    ("TemplateLiteral", &["quasis", "expressions"]),
    ("YieldExpression", &["argument"]),
    ("AwaitExpression", &["argument"]),
    ("OptionalMemberExpression", &["object", "property"]),
    ("OptionalCallExpression", &["callee", "arguments"]),
    ("ClassProperty", &["key", "value"]),
    ("ClassAccessorProperty", &["key", "value"]),
    ("ClassPrivateProperty", &["key", "value"]),
    ("ClassPrivateMethod", &["key", "params", "body"]),
    ("PrivateName", &["id"]),
    ("StaticBlock", &["body"]),
];

fn visitor_keys(babel_type: &str) -> &'static [&'static str] {
    VISITOR_KEYS
        .iter()
        .find(|(t, _)| *t == babel_type)
        .map_or(&[], |(_, keys)| keys)
}

fn node_type(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

/// A child of a babel-shaped node: a real ESTree subtree, or a node the
/// translation synthesizes (a PrivateName's `id`, a Directive's literal,
/// an `import()`'s callee) — carried with its babel type and no children.
enum Child<'a> {
    Node(&'a Value, bool),
    Leaf(&'static str),
}

/// One node in babel shape: its type, literal token (if any), and its
/// children in VISITOR_KEYS order. `None` for a transparent wrapper (the
/// children then belong to the wrapper's parent context).
struct Babel<'a> {
    babel_type: String,
    literal: Option<String>,
    children: Vec<Child<'a>>,
}

/// Push `v` (a node or an array of nodes; nulls/holes skipped) as children.
fn push_value<'a>(out: &mut Vec<Child<'a>>, v: Option<&'a Value>, in_chain: bool) {
    match v {
        Some(Value::Array(items)) => {
            for item in items {
                if item.get("type").is_some() {
                    out.push(Child::Node(item, in_chain));
                }
            }
        }
        Some(node) if node.get("type").is_some() => out.push(Child::Node(node, in_chain)),
        _ => {}
    }
}

/// The children of `map` under the babel type's VISITOR_KEYS.
fn keyed_children<'a>(
    babel_type: &str,
    map: &'a Map<String, Value>,
    in_chain: bool,
) -> Vec<Child<'a>> {
    let mut out = Vec::new();
    for key in visitor_keys(babel_type) {
        push_value(&mut out, map.get(*key), in_chain);
    }
    out
}

/// `s:${value.slice(0, 40)}` — 40 UTF-16 units; a cut through a surrogate
/// pair leaves a lone high surrogate, which `createHash().update(string)`
/// encodes as U+FFFD.
fn string_token(value: &str) -> String {
    let mut out = String::from("s:");
    let mut used = 0;
    for c in value.chars() {
        let units = c.len_utf16();
        if used + units > 40 {
            if used < 40 {
                out.push('\u{FFFD}');
            }
            break;
        }
        out.push(c);
        used += units;
    }
    out
}

/// The babel literal class of an ESTree `Literal`, with its token.
fn literal(map: &Map<String, Value>) -> (&'static str, Option<String>) {
    if map.get("regex").is_some_and(|r| !r.is_null()) {
        return ("RegExpLiteral", None);
    }
    if map.get("bigint").is_some_and(|b| !b.is_null()) {
        return ("BigIntLiteral", None);
    }
    match map.get("value") {
        Some(Value::String(s)) => ("StringLiteral", Some(string_token(s))),
        Some(Value::Number(n)) => {
            // The value from the SOURCE spelling: the JSON `value` went
            // through serde_json's float parser, up to 1 ulp off (lesson 8).
            let value = map
                .get("raw")
                .and_then(Value::as_str)
                .and_then(humanify_model::js::numeric_literal_value)
                .or_else(|| n.as_f64())
                .unwrap_or(f64::NAN);
            (
                "NumericLiteral",
                Some(format!("m:{}", humanify_model::js::number_to_string(value))),
            )
        }
        Some(Value::Bool(_)) => ("BooleanLiteral", None),
        _ => ("NullLiteral", None),
    }
}

/// oxc's `Property` in babel shape: ObjectMethod (a function value, laid
/// flat) or ObjectProperty.
fn property<'a>(map: &'a Map<String, Value>, in_chain: bool) -> Babel<'a> {
    let kind = map.get("kind").and_then(Value::as_str).unwrap_or("init");
    let is_fn =
        map.get("method").and_then(Value::as_bool) == Some(true) || matches!(kind, "get" | "set");
    let mut children = Vec::new();
    push_value(&mut children, map.get("key"), in_chain);
    if is_fn {
        let value = map.get("value");
        push_value(&mut children, value.and_then(|v| v.get("params")), in_chain);
        push_value(&mut children, value.and_then(|v| v.get("body")), in_chain);
        return Babel {
            babel_type: "ObjectMethod".into(),
            literal: None,
            children,
        };
    }
    push_value(&mut children, map.get("value"), in_chain);
    Babel {
        babel_type: "ObjectProperty".into(),
        literal: None,
        children,
    }
}

fn is_private_key(map: &Map<String, Value>) -> bool {
    map.get("key").map(node_type) == Some("PrivateIdentifier")
}

/// A class member in babel shape.
fn class_member<'a>(estree: &str, map: &'a Map<String, Value>, in_chain: bool) -> Babel<'a> {
    let private = is_private_key(map);
    let mut children = Vec::new();
    push_value(&mut children, map.get("key"), in_chain);
    let babel_type = if estree == "MethodDefinition" {
        let value = map.get("value");
        push_value(&mut children, value.and_then(|v| v.get("params")), in_chain);
        push_value(&mut children, value.and_then(|v| v.get("body")), in_chain);
        if private {
            "ClassPrivateMethod"
        } else {
            "ClassMethod"
        }
    } else {
        push_value(&mut children, map.get("value"), in_chain);
        match (estree, private) {
            ("AccessorProperty", _) => "ClassAccessorProperty",
            (_, true) => "ClassPrivateProperty",
            _ => "ClassProperty",
        }
    };
    Babel {
        babel_type: babel_type.into(),
        literal: None,
        children,
    }
}

/// Translate one ESTree node into its babel shape; `None` = transparent
/// (its single child is visited in its place).
fn babel_of(node: &Value, in_chain: bool) -> Result<Babel<'_>, (&Value, bool)> {
    let Some(map) = node.as_object() else {
        return Ok(Babel {
            babel_type: String::new(),
            literal: None,
            children: Vec::new(),
        });
    };
    let estree = node_type(node);
    let plain = |t: &str| Babel {
        babel_type: t.to_string(),
        literal: None,
        children: keyed_children(t, map, in_chain),
    };
    Ok(match estree {
        "ParenthesizedExpression" => return Err((&map["expression"], in_chain)),
        "ChainExpression" => return Err((&map["expression"], true)),
        "Literal" => {
            let (t, token) = literal(map);
            Babel {
                babel_type: t.into(),
                literal: token,
                children: Vec::new(),
            }
        }
        "Property" => property(map, in_chain),
        "MethodDefinition" | "PropertyDefinition" | "AccessorProperty" => {
            class_member(estree, map, in_chain)
        }
        "PrivateIdentifier" => Babel {
            babel_type: "PrivateName".into(),
            literal: None,
            children: vec![Child::Leaf("Identifier")],
        },
        "ExpressionStatement" if map.contains_key("directive") => Babel {
            babel_type: "Directive".into(),
            literal: None,
            children: vec![Child::Leaf("DirectiveLiteral")],
        },
        "ImportExpression" => {
            let mut children = vec![Child::Leaf("Import")];
            push_value(&mut children, map.get("source"), in_chain);
            push_value(&mut children, map.get("options"), in_chain);
            Babel {
                babel_type: "CallExpression".into(),
                literal: None,
                children,
            }
        }
        "MemberExpression" if in_chain && crate::matching::statement_align::chain_link(node) => {
            plain("OptionalMemberExpression")
        }
        "CallExpression" if in_chain && crate::matching::statement_align::chain_link(node) => {
            plain("OptionalCallExpression")
        }
        other => plain(other),
    })
}

/// 8 hex of sha1 over the UTF-8 bytes (`shortHash`).
fn short_hash(s: &str) -> String {
    let digest = Sha1::digest(s.as_bytes());
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

/// A JS `Set<string>` in first-seen order.
#[derive(Default)]
struct TokenSet {
    order: Vec<String>,
    seen: std::collections::HashSet<String>,
}

impl TokenSet {
    fn add(&mut self, token: String) {
        if self.seen.insert(token.clone()) {
            self.order.push(token);
        }
    }
}

/// One pending visit: a node (or a synthesized leaf) with its parent and
/// grandparent babel types.
enum Visit<'a> {
    Node(&'a Value, bool, String, String),
    Leaf(&'static str, String, String),
}

fn schedule<'a>(stack: &mut Vec<Visit<'a>>, children: Vec<Child<'a>>, parent: &str, grand: &str) {
    for child in children.into_iter().rev() {
        stack.push(match child {
            Child::Node(v, chain) => Visit::Node(v, chain, parent.to_string(), grand.to_string()),
            Child::Leaf(t) => Visit::Leaf(t, parent.to_string(), grand.to_string()),
        });
    }
}

/// `moduleTokens(module, body)`.
pub fn module_tokens(module: &FossilModule, body: &[Value]) -> Vec<String> {
    let mut tokens = TokenSet::default();
    for &i in &module.statements {
        let mut stack = vec![Visit::Node(&body[i], false, "root".into(), "root".into())];
        while let Some(visit) = stack.pop() {
            match visit {
                Visit::Leaf(t, parent, grand) => {
                    tokens.add(short_hash(&format!("n:{grand}>{parent}>{t}")));
                }
                Visit::Node(node, in_chain, parent, grand) => match babel_of(node, in_chain) {
                    Err((inner, chain)) => stack.push(Visit::Node(inner, chain, parent, grand)),
                    Ok(b) => {
                        tokens.add(short_hash(&format!("n:{grand}>{parent}>{}", b.babel_type)));
                        if let Some(lit) = &b.literal {
                            tokens.add(short_hash(lit));
                        }
                        schedule(&mut stack, b.children, &b.babel_type, &parent);
                    }
                },
            }
        }
    }
    tokens.order
}

#[cfg(test)]
mod tokens_test;
