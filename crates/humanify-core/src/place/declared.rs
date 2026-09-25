//! What a top-level statement DECLARES, the way stable-split.ts asks it:
//! `Object.keys(t.getBindingIdentifiers(stmt, false))` (`declaredNames` —
//! includes FUNCTION PARAMETERS, which is load-bearing: the all-same tier
//! exists because a parameter can outvote its function's own name) and
//! `Object.keys(t.getOuterBindingIdentifiers(stmt, false))` (the anchor
//! preempt's "outer" names).
//!
//! Ported from @babel/types' `getBindingIdentifiers`: a BREADTH-first queue
//! over the `getBindingIdentifiers.keys` table, first occurrence wins the
//! key order (a null-prototype object; no identifier is array-index-like).
//! The walk reads oxc's ESTree JSON, whose binding positions spell
//! babel's with two translations: a pattern property is ESTree `Property`
//! (babel `ObjectProperty`, key `value`), and a method's params live under
//! its `value` function (babel `ObjectMethod` carries them itself).
//!
//! Not `twins::fossil::declared_names` — that one (fossil-map.ts) takes only
//! declarator/function/class ids and no parameters: a different question
//! with a different answer, both declared here.

use std::collections::HashSet;
use std::collections::VecDeque;

use serde_json::Value;

/// `getBindingIdentifiers.keys` (@babel/types 7.29.7) for the ESTree types
/// a wrapper statement can carry (the Flow/TS/import/export keys never
/// occur in a wrapper body).
fn binding_keys(estree_type: &str) -> &'static [&'static str] {
    match estree_type {
        "CatchClause" => &["param"],
        "LabeledStatement" => &["label"],
        "UnaryExpression" | "UpdateExpression" | "RestElement" => &["argument"],
        "AssignmentExpression" | "AssignmentPattern" | "ForInStatement" | "ForOfStatement" => {
            &["left"]
        }
        "FunctionDeclaration" | "FunctionExpression" => &["id", "params"],
        "ArrowFunctionExpression" => &["params"],
        "ClassDeclaration" | "ClassExpression" => &["id"],
        // babel ObjectProperty: ["value"] (ESTree spells it `Property`).
        "Property" => &["value"],
        "ArrayPattern" => &["elements"],
        "ObjectPattern" => &["properties"],
        "VariableDeclaration" => &["declarations"],
        "VariableDeclarator" => &["id"],
        _ => &[],
    }
}

fn node_type(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

/// Is `v` an ESTree `Property` that babel would model as an ObjectMethod
/// (whose `params` key babel's table visits)?
fn is_method_property(v: &Value) -> bool {
    node_type(v) == "Property"
        && (v.get("method").and_then(Value::as_bool) == Some(true)
            || matches!(v.get("kind").and_then(Value::as_str), Some("get" | "set")))
}

fn push_children<'a>(queue: &mut VecDeque<&'a Value>, node: &'a Value, keys: &[&str]) {
    for key in keys {
        match node.get(*key) {
            Some(Value::Array(items)) => queue.extend(items.iter()),
            Some(v) if !v.is_null() => queue.push_back(v),
            _ => {}
        }
    }
}

/// The breadth-first walk; `outer_only` is `getOuterBindingIdentifiers`.
fn binding_identifiers(stmt: &Value, outer_only: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<&Value> = VecDeque::from([stmt]);
    while let Some(node) = queue.pop_front() {
        // oxc keeps parens babel drops (`var a = (b = 1)` is not a binding
        // position anyway, but a paren can wrap a pattern target).
        let t = node_type(node);
        if t == "ParenthesizedExpression" {
            push_children(&mut queue, node, &["expression"]);
            continue;
        }
        if t == "Identifier" {
            if let Some(name) = node.get("name").and_then(Value::as_str)
                && seen.insert(name.to_string())
            {
                out.push(name.to_string());
            }
            continue;
        }
        if outer_only {
            if t == "FunctionDeclaration" {
                push_children(&mut queue, node, &["id"]);
                continue;
            }
            if t == "FunctionExpression" {
                continue;
            }
        }
        if is_method_property(node) {
            // babel ObjectMethod: ["params"], laid flat on the method node.
            if let Some(value) = node.get("value") {
                push_children(&mut queue, value, &["params"]);
            }
            continue;
        }
        push_children(&mut queue, node, binding_keys(t));
    }
    out
}

/// `declaredNames` (stable-split.ts): `Object.keys(getBindingIdentifiers(stmt))`.
pub fn declared_names(stmt: &Value) -> Vec<String> {
    binding_identifiers(stmt, false)
}

/// `Object.keys(getOuterBindingIdentifiers(stmt))`.
pub fn outer_declared_names(stmt: &Value) -> Vec<String> {
    binding_identifiers(stmt, true)
}

#[cfg(test)]
mod declared_test;
