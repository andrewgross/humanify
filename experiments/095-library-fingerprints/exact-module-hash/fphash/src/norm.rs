//! N1: a "minifier-normal form" over oxc ESTree JSON. Rewrites the spellings
//! that differ between minifiers (and between minified and readable output)
//! into one form, so the canonical hash sees the same tree. Hash-only: the
//! output need not be valid or equivalent JavaScript, only deterministic.
//!
//! Rules (each one seen in a real lab diff):
//!  - directives ("use strict") dropped; empty statements dropped
//!  - `a, b;` -> `a; b;`   `return a, b` -> `a; return b`   `if (a, b) S` -> `a; if (b) S`
//!  - `A && B;` -> `if (A) B;`   `A || B;` -> `if (!A) B;`   `A ? B : C;` -> `if (A) B; else C;`
//!  - `if (A) { if (B) S }` -> `if (A && B) S`; single-statement blocks unwrap; nested blocks in lists splice
//!  - `var a = 1, b = 2` -> `var a = 1; var b = 2`; let/const -> var
//!  - `!0` -> true, `!1` -> false, `void 0` -> undefined, `!!x` kept
//!  - `(0, x.f)(...)` -> `x.f(...)`
//!  - `x["y"]` -> `x.y`, `{"y": 1}` -> `{y: 1}`
//!  - `===` -> `==`, `!==` -> `!=`
//!  - parentheses dropped; `` `abc` `` (no substitutions) -> "abc"
//!  - `catch (e) {}` -> `catch {}` (param dropped)
//!  - `() => X` -> `() => { return X }` (then R1 turns `return c ? a : b` into ifs)
//!  - `new Error(x)` -> `Error(x)` for builtins where `new` is optional (Bun drops it)
//!  - `var {a: x, b: y} = o` (o an identifier) -> `var x = o.a; var y = o.b`
//!  - `while (T) S` -> `for (; T; ) S`, `for (; true; )` -> `for (;;)`
//!  - class-private names (`#x`) -> ordinals by first occurrence (minifiers rename them)
//!  - module level (core): function declarations first (bundlers hoist them out of lazy wrappers);
//!    lazy-esm: zero-argument calls of outside names (dependency init calls) dropped;
//!    assignments to hoisted names (`var a; ... a = X`) -> `var a = X`.

use std::collections::HashSet;

use serde_json::{Map, Value, json};

fn ty(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

fn is_num(v: &Value, n: f64) -> bool {
    ty(v) == "Literal" && v.get("value").and_then(Value::as_f64) == Some(n)
}

fn is_ident_name(s: &str) -> bool {
    let mut cs = s.chars();
    match cs.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    cs.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn ident(name: &str) -> Value {
    json!({"type": "Identifier", "name": name})
}

fn expr_stmt(e: Value) -> Value {
    json!({"type": "ExpressionStatement", "expression": e})
}

fn not(e: Value) -> Value {
    if ty(&e) == "UnaryExpression" && e.get("operator").and_then(Value::as_str) == Some("!") {
        return e.get("argument").cloned().unwrap_or(Value::Null);
    }
    json!({"type": "UnaryExpression", "operator": "!", "prefix": true, "argument": e})
}

const NEW_OPTIONAL: [&str; 12] = [
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "AggregateError",
    "RegExp",
    "Array",
    "Object",
    "Function",
];

const LIST_KEYS: [(&str, &str); 4] = [
    ("Program", "body"),
    ("BlockStatement", "body"),
    ("StaticBlock", "body"),
    ("SwitchCase", "consequent"),
];

/// Normalize any node (post-order).
pub fn norm(v: Value) -> Value {
    match v {
        Value::Array(items) => Value::Array(items.into_iter().map(norm).collect()),
        Value::Object(map) => norm_node(map),
        other => other,
    }
}

fn norm_node(mut map: Map<String, Value>) -> Value {
    let t = map
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // children first
    let keys: Vec<String> = map.keys().cloned().collect();
    for k in keys {
        if matches!(k.as_str(), "type" | "start" | "end" | "range" | "loc") {
            continue;
        }
        let child = map.remove(&k).unwrap();
        let child = if LIST_KEYS.iter().any(|(tt, kk)| *tt == t && *kk == k) {
            match child {
                Value::Array(items) => {
                    Value::Array(norm_list(items.into_iter().map(norm).collect()))
                }
                other => norm(other),
            }
        } else {
            norm(child)
        };
        map.insert(k, child);
    }
    match t.as_str() {
        "ParenthesizedExpression" => {
            return map.remove("expression").unwrap_or(Value::Null);
        }
        "TemplateLiteral" => {
            if map
                .get("expressions")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
                && let Some(q) = map.get("quasis").and_then(Value::as_array)
                && q.len() == 1
                && let Some(c) = q[0]
                    .get("value")
                    .and_then(|v| v.get("cooked"))
                    .and_then(Value::as_str)
            {
                return json!({"type": "Literal", "value": c, "raw": ""});
            }
        }
        "UnaryExpression" => {
            let op = map.get("operator").and_then(Value::as_str).unwrap_or("");
            let arg = map.get("argument").cloned().unwrap_or(Value::Null);
            if op == "!" && is_num(&arg, 0.0) {
                return json!({"type": "Literal", "value": true, "raw": "true"});
            }
            if op == "!" && is_num(&arg, 1.0) {
                return json!({"type": "Literal", "value": false, "raw": "false"});
            }
            if op == "void" && is_num(&arg, 0.0) {
                return ident("undefined");
            }
        }
        "CallExpression" => {
            if let Some(c) = map.get("callee")
                && ty(c) == "SequenceExpression"
                && let Some(ex) = c.get("expressions").and_then(Value::as_array)
                && ex.len() == 2
                && is_num(&ex[0], 0.0)
            {
                let inner = ex[1].clone();
                map.insert("callee".into(), inner);
            }
        }
        "MemberExpression" => {
            if map.get("computed").and_then(Value::as_bool) == Some(true)
                && let Some(p) = map.get("property")
                && ty(p) == "Literal"
                && let Some(s) = p.get("value").and_then(Value::as_str)
                && is_ident_name(s)
            {
                let id = ident(s);
                map.insert("property".into(), id);
                map.insert("computed".into(), Value::Bool(false));
            }
        }
        "Property" => {
            if let Some(k) = map.get("key")
                && ty(k) == "Literal"
                && let Some(s) = k.get("value").and_then(Value::as_str)
                && is_ident_name(s)
            {
                let id = ident(s);
                map.insert("key".into(), id);
                map.insert("computed".into(), Value::Bool(false));
            }
        }
        "BinaryExpression" => {
            let op = map
                .get("operator")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if op == "===" {
                map.insert("operator".into(), "==".into());
            } else if op == "!==" {
                map.insert("operator".into(), "!=".into());
            }
            // R9: `typeof x > "u"` (minifier) == `typeof x === "undefined"`
            if (op == ">" || op == "<")
                && map.get("left").is_some_and(|l| {
                    ty(l) == "UnaryExpression"
                        && l.get("operator").and_then(Value::as_str) == Some("typeof")
                })
                && map
                    .get("right")
                    .is_some_and(|r| r.get("value").and_then(Value::as_str) == Some("u"))
            {
                map.insert(
                    "operator".into(),
                    if op == ">" { "==" } else { "!=" }.into(),
                );
                map.insert(
                    "right".into(),
                    json!({"type": "Literal", "value": "undefined", "raw": ""}),
                );
            }
        }
        "CatchClause" => {
            map.insert("param".into(), Value::Null);
        }
        "ArrowFunctionExpression" => {
            // `() => X` -> `() => { return X }`, then the return rules apply (R1 etc.)
            if map.get("expression").and_then(Value::as_bool) == Some(true)
                && let Some(body) = map.remove("body")
            {
                let ret = json!({"type": "ReturnStatement", "argument": body});
                let block = json!({"type": "BlockStatement", "body": norm_list(vec![ret])});
                map.insert("body".into(), block);
                map.insert("expression".into(), Value::Bool(false));
            }
        }
        "NewExpression" => {
            // Bun drops `new` for builtins that behave the same called (`new Error(x)` -> `Error(x)`)
            if let Some(c) = map.get("callee")
                && ty(c) == "Identifier"
                && c.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| NEW_OPTIONAL.contains(&n))
            {
                map.insert("type".into(), "CallExpression".into());
                map.insert("optional".into(), Value::Bool(false));
            }
        }
        "VariableDeclaration" => {
            map.insert("kind".into(), "var".into());
        }
        "WhileStatement" => {
            let test = map.remove("test").unwrap_or(Value::Null);
            let body = map.remove("body").unwrap_or(Value::Null);
            let test = if ty(&test) == "Literal" && test.get("value") == Some(&Value::Bool(true)) {
                Value::Null
            } else {
                test
            };
            return json!({"type": "ForStatement", "init": null, "test": test, "update": null, "body": body});
        }
        "ForStatement" => {
            if let Some(test) = map.get("test")
                && ty(test) == "Literal"
                && test.get("value") == Some(&Value::Bool(true))
            {
                map.insert("test".into(), Value::Null);
            }
            // R3: `for (; c; u) if (x) break;` -> `for (; c && !x; u);`
            if let Some(b) = map.get("body")
                && ty(b) == "IfStatement"
                && b.get("alternate").is_none_or(Value::is_null)
                && b.get("consequent").is_some_and(|c| {
                    ty(c) == "BreakStatement" && c.get("label").is_none_or(Value::is_null)
                })
            {
                let x = not(b.get("test").cloned().unwrap_or(Value::Null));
                let test = match map.remove("test") {
                    Some(t) if !t.is_null() => {
                        json!({"type": "LogicalExpression", "operator": "&&", "left": t, "right": x})
                    }
                    _ => x,
                };
                map.insert("test".into(), test);
                map.insert("body".into(), json!({"type": "EmptyStatement"}));
            }
        }
        "IfStatement" => return finish_if(map),
        "ConditionalExpression" => {
            if map.get("test").is_some_and(|t| {
                ty(t) == "UnaryExpression" && t.get("operator").and_then(Value::as_str) == Some("!")
            }) {
                let t = map.remove("test").unwrap();
                let c = map.remove("consequent").unwrap_or(Value::Null);
                let a = map.remove("alternate").unwrap_or(Value::Null);
                map.insert("test".into(), not(t));
                map.insert("consequent".into(), a);
                map.insert("alternate".into(), c);
            }
        }
        _ => {}
    }
    Value::Object(map)
}

/// A statement in a single-statement position, from a normalized list.
fn from_list(mut items: Vec<Value>) -> Value {
    if items.len() == 1 {
        return items.pop().unwrap();
    }
    json!({"type": "BlockStatement", "body": items})
}

fn as_list(stmt: Value) -> Vec<Value> {
    if ty(&stmt) == "BlockStatement" {
        if let Some(Value::Array(items)) = stmt.get("body") {
            return norm_list(items.clone());
        }
    }
    norm_list(vec![stmt])
}

fn finish_if(mut map: Map<String, Value>) -> Value {
    let cons = map.remove("consequent").unwrap_or(Value::Null);
    let cons = from_list(as_list(cons));
    let alt = map.remove("alternate").unwrap_or(Value::Null);
    let alt = if alt.is_null() {
        Value::Null
    } else {
        from_list(as_list(alt))
    };
    if alt.is_null()
        && ty(&cons) == "IfStatement"
        && cons.get("alternate").is_none_or(Value::is_null)
    {
        let test = map.remove("test").unwrap_or(Value::Null);
        let inner_test = cons.get("test").cloned().unwrap_or(Value::Null);
        let inner_cons = cons.get("consequent").cloned().unwrap_or(Value::Null);
        let merged = json!({"type": "LogicalExpression", "operator": "&&", "left": test, "right": inner_test});
        let mut m = Map::new();
        m.insert("type".into(), "IfStatement".into());
        m.insert("test".into(), merged);
        m.insert("consequent".into(), inner_cons);
        m.insert("alternate".into(), Value::Null);
        return finish_if(m);
    }
    if !alt.is_null()
        && map.get("test").is_some_and(|t| {
            ty(t) == "UnaryExpression" && t.get("operator").and_then(Value::as_str) == Some("!")
        })
    {
        let t = map.remove("test").unwrap();
        map.insert("test".into(), not(t));
        map.insert("consequent".into(), alt);
        map.insert("alternate".into(), cons);
        return Value::Object(map);
    }
    map.insert("consequent".into(), cons);
    map.insert("alternate".into(), alt);
    Value::Object(map)
}

fn ends_abruptly(s: &Value) -> bool {
    match ty(s) {
        "ReturnStatement" | "ThrowStatement" | "BreakStatement" | "ContinueStatement" => true,
        "BlockStatement" => s
            .get("body")
            .and_then(Value::as_array)
            .and_then(|b| b.last())
            .is_some_and(ends_abruptly),
        _ => false,
    }
}

/// Normalize a statement list (its items are already normalized nodes).
pub fn norm_list(items: Vec<Value>) -> Vec<Value> {
    let mut out = Vec::with_capacity(items.len());
    for s in items {
        push_stmt(s, &mut out);
    }
    out
}

fn push_stmt(s: Value, out: &mut Vec<Value>) {
    match ty(&s) {
        "EmptyStatement" => {}
        "BlockStatement" => {
            if let Some(Value::Array(items)) = s.get("body") {
                for i in items.clone() {
                    push_stmt(i, out);
                }
            }
        }
        "ExpressionStatement" => {
            if s.get("directive").is_some_and(|d| !d.is_null()) {
                return;
            }
            let e = s.get("expression").cloned().unwrap_or(Value::Null);
            // a bare string statement is a directive ("use strict"), even where a prefix demoted it
            if ty(&e) == "Literal" && e.get("value").is_some_and(Value::is_string) {
                return;
            }
            match ty(&e) {
                "SequenceExpression" => {
                    if let Some(Value::Array(ex)) = e.get("expressions") {
                        for x in ex.clone() {
                            push_stmt(expr_stmt(x), out);
                        }
                    }
                }
                "LogicalExpression" => {
                    let op = e.get("operator").and_then(Value::as_str).unwrap_or("");
                    let left = e.get("left").cloned().unwrap_or(Value::Null);
                    let right = e.get("right").cloned().unwrap_or(Value::Null);
                    let test = match op {
                        "&&" => left,
                        "||" => not(left),
                        _ => {
                            out.push(s);
                            return;
                        }
                    };
                    let mut m = Map::new();
                    m.insert("type".into(), "IfStatement".into());
                    m.insert("test".into(), test);
                    m.insert("consequent".into(), expr_stmt(right));
                    m.insert("alternate".into(), Value::Null);
                    push_stmt(finish_if(m), out);
                }
                "ConditionalExpression" => {
                    let mut m = Map::new();
                    m.insert("type".into(), "IfStatement".into());
                    m.insert("test".into(), e.get("test").cloned().unwrap_or(Value::Null));
                    m.insert(
                        "consequent".into(),
                        expr_stmt(e.get("consequent").cloned().unwrap_or(Value::Null)),
                    );
                    m.insert(
                        "alternate".into(),
                        expr_stmt(e.get("alternate").cloned().unwrap_or(Value::Null)),
                    );
                    push_stmt(finish_if(m), out);
                }
                _ => out.push(s),
            }
        }
        "VariableDeclaration" => {
            let decls = s
                .get("declarations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if decls.len() == 1
                && let Some(split) = split_destructure(&decls[0])
            {
                for d in split {
                    let mut m = s.as_object().unwrap().clone();
                    m.insert("declarations".into(), Value::Array(vec![d]));
                    out.push(Value::Object(m));
                }
                return;
            }
            if decls.len() <= 1 {
                out.push(s);
                return;
            }
            for d in decls {
                let mut m = s.as_object().unwrap().clone();
                m.insert("declarations".into(), Value::Array(vec![d]));
                out.push(Value::Object(m));
            }
        }
        "ForStatement" => {
            // R2: `for (init; ...)` -> `init; for (; ...)`
            if let Some(init) = s.get("init")
                && !init.is_null()
            {
                let init = init.clone();
                let mut m = s.as_object().unwrap().clone();
                m.insert("init".into(), Value::Null);
                if ty(&init) == "VariableDeclaration" {
                    push_stmt(init, out);
                } else {
                    push_stmt(expr_stmt(init), out);
                }
                out.push(Value::Object(m));
                return;
            }
            out.push(s);
        }
        "ReturnStatement"
            if s.get("argument")
                .is_some_and(|a| ty(a) == "ConditionalExpression") =>
        {
            // R1: `return c ? a : b` -> `if (c) return a; return b`
            let e = s.get("argument").unwrap();
            let mut m = Map::new();
            m.insert("type".into(), "IfStatement".into());
            m.insert("test".into(), e.get("test").cloned().unwrap_or(Value::Null));
            m.insert("consequent".into(), json!({"type": "ReturnStatement", "argument": e.get("consequent").cloned().unwrap_or(Value::Null)}));
            m.insert("alternate".into(), Value::Null);
            let alt = json!({"type": "ReturnStatement", "argument": e.get("alternate").cloned().unwrap_or(Value::Null)});
            push_stmt(finish_if(m), out);
            push_stmt(alt, out);
        }
        "ReturnStatement" | "ThrowStatement" => {
            if let Some(arg) = s.get("argument")
                && ty(arg) == "SequenceExpression"
                && let Some(Value::Array(ex)) = arg.get("expressions")
            {
                let mut ex = ex.clone();
                let last = ex.pop().unwrap_or(Value::Null);
                for x in ex {
                    push_stmt(expr_stmt(x), out);
                }
                let mut m = s.as_object().unwrap().clone();
                m.insert("argument".into(), last);
                out.push(Value::Object(m));
                return;
            }
            out.push(s);
        }
        "IfStatement" => {
            if let Some(test) = s.get("test")
                && ty(test) == "SequenceExpression"
                && let Some(Value::Array(ex)) = test.get("expressions")
            {
                let mut ex = ex.clone();
                let last = ex.pop().unwrap_or(Value::Null);
                for x in ex {
                    push_stmt(expr_stmt(x), out);
                }
                let mut m = s.as_object().unwrap().clone();
                m.insert("test".into(), last);
                push_stmt(finish_if(m), out);
                return;
            }
            // R4: `if (c) { ...; return } else S` -> `if (c) { ...; return } S`
            if let Some(alt) = s.get("alternate")
                && !alt.is_null()
                && s.get("consequent").is_some_and(ends_abruptly)
            {
                let alt = alt.clone();
                let mut m = s.as_object().unwrap().clone();
                m.insert("alternate".into(), Value::Null);
                out.push(finish_if(m));
                for x in as_list(alt) {
                    push_stmt(x, out);
                }
                return;
            }
            out.push(s);
        }
        _ => out.push(s),
    }
}

/// `{a: x, b: y} = o` with o an identifier and plain props -> declarators `x = o.a`, `y = o.b`.
fn split_destructure(d: &Value) -> Option<Vec<Value>> {
    let id = d.get("id")?;
    let init = d.get("init")?;
    if ty(id) != "ObjectPattern" || ty(init) != "Identifier" {
        return None;
    }
    let props = id.get("properties")?.as_array()?;
    let mut out = Vec::new();
    for p in props {
        if ty(p) != "Property" || p.get("computed").and_then(Value::as_bool) != Some(false) {
            return None;
        }
        let key = p.get("key")?;
        let val = p.get("value")?;
        if ty(key) != "Identifier" || ty(val) != "Identifier" {
            return None;
        }
        let member = json!({"type": "MemberExpression", "object": init.clone(), "property": key.clone(), "computed": false, "optional": false});
        out.push(json!({"type": "VariableDeclarator", "id": val.clone(), "init": member}));
    }
    Some(out)
}

fn hoisted_names(all: &[Value]) -> HashSet<String> {
    let mut h = HashSet::new();
    for s in all {
        if ty(s) != "VariableDeclaration" {
            continue;
        }
        let Some(ds) = s.get("declarations").and_then(Value::as_array) else {
            continue;
        };
        for d in ds
            .iter()
            .filter(|d| d.get("init").is_none_or(Value::is_null))
        {
            if let Some(n) = d
                .get("id")
                .and_then(|i| i.get("name"))
                .and_then(Value::as_str)
            {
                h.insert(n.to_string());
            }
        }
    }
    h
}

/// The ESM wrapper, taken apart: every `var` split to single declarators; bare declarators (hoisted
/// names) dropped, the namespace block dropped, the `__esm(() => {BODY})` marker unwrapped. Works
/// when esbuild merges the marker into the hoisted list (`var a, b, init = __esm(...)`).
fn esm_core(all: &[Value], lazy: bool) -> Vec<Value> {
    let mut out = Vec::new();
    for s in all {
        if ty(s) == "ExpressionStatement"
            && let Some(call) = s.get("expression")
            && ty(call) == "CallExpression"
            && let Some(args) = call.get("arguments").and_then(Value::as_array)
            && args.len() == 2
            && ty(&args[0]) == "Identifier"
            && ty(&args[1]) == "ObjectExpression"
            && args[1]
                .get("properties")
                .and_then(Value::as_array)
                .is_some_and(|ps| {
                    ps.iter().all(|p| {
                        p.get("value")
                            .is_some_and(|v| ty(v) == "ArrowFunctionExpression")
                    })
                })
        {
            continue; // namespace registration
        }
        if ty(s) != "VariableDeclaration" {
            out.push(s.clone());
            continue;
        }
        for d in s
            .get("declarations")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let init = d.get("init").cloned().unwrap_or(Value::Null);
            if init.is_null() {
                continue; // hoisted name
            }
            if ty(&init) == "ObjectExpression"
                && init
                    .get("properties")
                    .and_then(Value::as_array)
                    .is_some_and(Vec::is_empty)
            {
                continue; // namespace object
            }
            if lazy && let Some(body) = marker_body(&init) {
                out.extend(body.iter().cloned());
                continue;
            }
            let mut m = s.as_object().unwrap().clone();
            m.insert("declarations".into(), Value::Array(vec![d]));
            out.push(Value::Object(m));
        }
    }
    out
}

fn marker_body(init: &Value) -> Option<&Vec<Value>> {
    if ty(init) != "CallExpression" {
        return None;
    }
    let args = init.get("arguments")?.as_array()?;
    if args.len() != 1 {
        return None;
    }
    let mut f = &args[0];
    if ty(f) == "ObjectExpression" {
        let ps = f.get("properties")?.as_array()?;
        if ps.len() != 1 {
            return None;
        }
        f = ps[0].get("value")?;
    }
    if !matches!(ty(f), "FunctionExpression" | "ArrowFunctionExpression") {
        return None;
    }
    if !f.get("params")?.as_array()?.is_empty() {
        return None;
    }
    let b = f.get("body")?;
    if ty(b) != "BlockStatement" {
        return None;
    }
    b.get("body")?.as_array()
}

/// The module-level normal form of a span's core statements.
pub fn norm_core(core: &[&Value], all: &[Value], kind: &str, free: &HashSet<String>) -> Vec<Value> {
    let hoisted = hoisted_names(all);
    let base: Vec<Value> = match kind {
        "lazy-esm" => esm_core(all, true),
        "eager-esm" => esm_core(all, false),
        _ => core.iter().map(|v| (*v).clone()).collect(),
    };
    let items: Vec<Value> = base.into_iter().map(norm).collect();
    let items = norm_list(items);
    let mut funcs = Vec::new();
    let mut rest = Vec::new();
    for s in items {
        // dependency init calls in a lazy module: `X()` with X from outside the span
        if kind == "lazy-esm"
            && ty(&s) == "ExpressionStatement"
            && let Some(e) = s.get("expression")
            && ty(e) == "CallExpression"
            && e.get("arguments")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            && let Some(c) = e.get("callee")
            && ty(c) == "Identifier"
            && c.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| free.contains(n))
        {
            continue;
        }
        // `a = X` for a hoisted `var a;` -> `var a = X`
        if ty(&s) == "ExpressionStatement"
            && let Some(e) = s.get("expression")
            && ty(e) == "AssignmentExpression"
            && e.get("operator").and_then(Value::as_str) == Some("=")
            && let Some(l) = e.get("left")
            && ty(l) == "Identifier"
            && l.get("name")
                .and_then(Value::as_str)
                .is_some_and(|n| hoisted.contains(n))
        {
            let decl = json!({"type": "VariableDeclaration", "kind": "var",
                "declarations": [{"type": "VariableDeclarator", "id": l.clone(), "init": e.get("right").cloned().unwrap_or(Value::Null)}]});
            rest.push(decl);
            continue;
        }
        if ty(&s) == "FunctionDeclaration" {
            funcs.push(s);
        } else {
            rest.push(s);
        }
    }
    funcs.extend(rest);
    // class-private names: minifiers rename them (`#retryOpts` -> `#A`), so they become ordinals too
    let mut privates: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for f in funcs.iter_mut() {
        rename_privates(f, &mut privates);
    }
    funcs
}

fn rename_privates(v: &mut Value, seen: &mut std::collections::HashMap<String, String>) {
    match v {
        Value::Array(items) => items.iter_mut().for_each(|i| rename_privates(i, seen)),
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("PrivateIdentifier") {
                let name = map
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let n = seen.len();
                let new = seen.entry(name).or_insert_with(|| format!("p{n}")).clone();
                map.insert("name".into(), Value::String(new));
                return;
            }
            for (_, c) in map.iter_mut() {
                rename_privates(c, seen);
            }
        }
        _ => {}
    }
}
