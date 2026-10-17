//! exp122 lab: unit prints under a candidate normalisation.
//!
//! exp122-lab query <variant> <bundler> <bundle.js> <out.jsonl> [--canon]
//!     discovery's unit model (`humanify discover --unit-prints`, no index):
//!     one line per unit row {s, e, w: [hash, nodes], f: [[s, e, hash, nodes]...]}
//!     (+ `c`: the whole unit's canonical stream with --canon)
//! exp122-lab index <variant> <out.jsonl> [--canon]   (stdin: build-slice dirs)
//!     the builder's `unit_rows` per build (req, dyn): one line per build
//!     {dir, tag, rows: [[owner, version, file, hash, nodes]...]} (+ `canon`:
//!     [[s, e, hash, stream]...] of every unit with --canon)
//!
//! <variant>: letters, applied in this order to every unit's ESTree JSON
//! before its prints (humanify-core's LAB_NORMALISE hook):
//!   `0` none (must reproduce the shipped prints exactly)
//!   K  const / let / var -> var
//!   T  template text: raw spelling -> cooked
//!   C  template literal == string concatenation (both -> one Concat node,
//!      adjacent strings merged)
//!   P  property spelling: a["b"] -> a.b, {"b": x} -> {b: x}
//!   M  minifier literal spellings: !0 -> true, !1 -> false, void 0 ->
//!      undefined, === -> ==, !== -> !=
//!   D  declaration == expression: function f(){} -> var f = function f(){},
//!      class X{} -> var X = class X{}; var f = function(){} names its
//!      expression after the declarator
//!   L  lazy-init hoisting: `a = x, b = y;` == `var a = x, b = y;` (kinds -> var)
//!   I  interop spelling: `(0, e)` -> e, `x.default` -> x
//!   O  a static member of a binding declared outside the unit walks as the binding
//!   A  every arrow walks as a function expression (expression body ->
//!      block with return)
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use humanify_core::hash::forms::LIBRARY_UNITS;
use humanify_core::hash::serialize::{SymbolTables, canonical_serialize};
use rayon::prelude::*;
use serde_json::{Map, Value, json};

static VARIANT: OnceLock<String> = OnceLock::new();
static CANON: OnceLock<bool> = OnceLock::new();
static RECORDED: Mutex<Vec<(u64, u64, String, String)>> = Mutex::new(Vec::new());

fn has(c: char) -> bool {
    VARIANT.get().is_some_and(|v| v.contains(c))
}

fn hook(json: &mut Value, tables: &SymbolTables) {
    if has('O') {
        let mut inner = std::collections::HashSet::new();
        declared_inside(json, tables, &mut inner);
        outer_members(json, tables, &inner);
    }
    normalise(json);
    if *CANON.get().unwrap_or(&false) {
        let out = canonical_serialize(json, tables, &LIBRARY_UNITS);
        let at = |k: &str| json.get(k).and_then(Value::as_u64).unwrap_or(0);
        RECORDED
            .lock()
            .unwrap()
            .push((at("start"), at("end"), out.hash, out.parts));
    }
}

/// Symbols declared inside the unit (their declaring identifier is in it).
fn declared_inside(
    v: &Value,
    tables: &SymbolTables,
    out: &mut std::collections::HashSet<oxc_semantic::SymbolId>,
) {
    match v {
        Value::Array(a) => a.iter().for_each(|x| declared_inside(x, tables, out)),
        Value::Object(m) => {
            if ty_map(m) == "Identifier"
                && let Some(st) = m.get("start").and_then(Value::as_u64)
                && let Some(sym) = tables.decl_by_start.get(&(st as u32))
            {
                out.insert(*sym);
            }
            m.values().for_each(|x| declared_inside(x, tables, out));
        }
        _ => {}
    }
}

fn outer_symbol(
    v: &Value,
    tables: &SymbolTables,
    inner: &std::collections::HashSet<oxc_semantic::SymbolId>,
) -> bool {
    if ty(v) != "Identifier" {
        return false;
    }
    let Some(st) = v.get("start").and_then(Value::as_u64) else {
        return false;
    };
    tables
        .ref_by_start
        .get(&(st as u32))
        .is_some_and(|s| !inner.contains(s))
}

/// O: a static member of a binding declared OUTSIDE the unit (an import's
/// namespace object, `import_x.foo`, webpack's `n.A`) walks as the binding.
fn outer_members(
    v: &mut Value,
    tables: &SymbolTables,
    inner: &std::collections::HashSet<oxc_semantic::SymbolId>,
) {
    match v {
        Value::Array(a) => a.iter_mut().for_each(|x| outer_members(x, tables, inner)),
        Value::Object(m) => {
            for x in m.values_mut() {
                outer_members(x, tables, inner);
            }
        }
        _ => return,
    }
    if ty(v) == "MemberExpression"
        && v["computed"] != json!(true)
        && outer_symbol(&v["object"], tables, inner)
    {
        *v = v["object"].take();
    }
}

fn normalise(v: &mut Value) {
    if VARIANT.get().is_none_or(|v| v == "0") {
        return;
    }
    walk(v, false);
}

fn ty(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

fn lit_str(s: &str) -> Value {
    json!({"type": "Literal", "value": s, "raw": format!("{s:?}")})
}

fn is_ident_name(s: &str) -> bool {
    let mut c = s.chars();
    match c.next() {
        Some(f) if f.is_ascii_alphabetic() || f == '_' || f == '$' => {}
        _ => return false,
    }
    c.all(|x| x.is_ascii_alphanumeric() || x == '_' || x == '$')
}

/// Post-order rewrite. `tagged`: this node is a tagged template's quasi.
fn walk(v: &mut Value, tagged: bool) {
    match v {
        Value::Array(items) => {
            for i in items {
                walk(i, false);
            }
            return;
        }
        Value::Object(map) => {
            let is_tagged = ty_map(map) == "TaggedTemplateExpression";
            let keys: Vec<String> = map.keys().cloned().collect();
            for k in keys {
                let child_tagged = is_tagged && k == "quasi";
                if let Some(c) = map.get_mut(&k) {
                    walk(c, child_tagged);
                }
            }
        }
        _ => return,
    }
    rewrite(v, tagged);
}

fn ty_map(m: &Map<String, Value>) -> &str {
    m.get("type").and_then(Value::as_str).unwrap_or("")
}

fn rewrite(v: &mut Value, tagged: bool) {
    let t = ty(v).to_string();
    if has('I') {
        // (0, e) -> e: the indirect-call spelling of an imported function
        if t == "ParenthesizedExpression" || t == "SequenceExpression" {
            let inner = if t == "ParenthesizedExpression" {
                v["expression"].clone()
            } else {
                v.clone()
            };
            if ty(&inner) == "SequenceExpression"
                && let Some(es) = inner["expressions"].as_array()
                && es.len() == 2
                && ty(&es[0]) == "Literal"
                && es[0].get("value").and_then(Value::as_f64) == Some(0.0)
            {
                *v = es[1].clone();
                return;
            }
        }
        if t == "MemberExpression"
            && v["computed"] != json!(true)
            && v["property"].get("name").and_then(Value::as_str) == Some("default")
            && ty(&v["object"]) == "Identifier"
        {
            *v = v["object"].take();
            return;
        }
    }
    if has('L') && t == "ExpressionStatement" {
        // a = x, b = y;  ==  var a = x, b = y;  (a lazy module's hoisted declarations)
        let e = &v["expression"];
        let list: Vec<Value> = if ty(e) == "SequenceExpression" {
            e["expressions"].as_array().cloned().unwrap_or_default()
        } else {
            vec![e.clone()]
        };
        if !list.is_empty()
            && list.iter().all(|a| {
                ty(a) == "AssignmentExpression"
                    && a["operator"] == json!("=")
                    && ty(&a["left"]) == "Identifier"
            })
        {
            let decls: Vec<Value> = list
                .into_iter()
                .map(|a| json!({"type": "VariableDeclarator", "id": a["left"], "init": a["right"]}))
                .collect();
            *v = json!({"type": "VariableDeclaration", "kind": "var", "declarations": decls});
            return;
        }
    }
    if (has('K') || has('L')) && t == "VariableDeclaration" {
        v["kind"] = json!("var");
    }
    if has('T') && t == "TemplateElement" {
        if let Some(c) = v.get("value").and_then(|x| x.get("cooked")).cloned()
            && c.is_string()
        {
            v["value"]["raw"] = c;
        }
    }
    if has('C') && !tagged && (t == "TemplateLiteral" || t == "BinaryExpression") {
        if let Some(parts) = concat_parts(v) {
            *v = json!({"type": "Concat", "parts": parts});
            return;
        }
    }
    if has('P') {
        if t == "MemberExpression" && v["computed"] == json!(true) {
            if let Some(s) = v["property"].get("value").and_then(Value::as_str)
                && ty(&v["property"]) == "Literal"
                && is_ident_name(s)
            {
                let s = s.to_string();
                v["property"] = json!({"type": "Identifier", "name": s});
                v["computed"] = json!(false);
            }
        }
        if t == "Property" && v["computed"] != json!(true) && ty(&v["key"]) == "Literal" {
            if let Some(s) = v["key"].get("value").and_then(Value::as_str)
                && is_ident_name(s)
            {
                let s = s.to_string();
                v["key"] = json!({"type": "Identifier", "name": s});
            }
        }
    }
    if has('M') {
        if t == "UnaryExpression" && v["operator"] == json!("!") && ty(&v["argument"]) == "Literal"
        {
            if let Some(n) = v["argument"].get("value").and_then(Value::as_f64)
                && (n == 0.0 || n == 1.0)
            {
                let b = n == 0.0;
                *v = json!({"type": "Literal", "value": b, "raw": b.to_string()});
                return;
            }
        }
        if t == "UnaryExpression"
            && v["operator"] == json!("void")
            && ty(&v["argument"]) == "Literal"
        {
            if v["argument"].get("value").and_then(Value::as_f64) == Some(0.0) {
                *v = json!({"type": "Identifier", "name": "undefined"});
                return;
            }
        }
        if t == "BinaryExpression" {
            let op = v["operator"].as_str().unwrap_or("").to_string();
            if op == "===" {
                v["operator"] = json!("==");
            } else if op == "!==" {
                v["operator"] = json!("!=");
            }
        }
    }
    if has('D') {
        if t == "FunctionDeclaration" || t == "ClassDeclaration" {
            if !v["id"].is_null() {
                let id = v["id"].clone();
                let mut e = v.clone();
                e["type"] = json!(if t == "FunctionDeclaration" {
                    "FunctionExpression"
                } else {
                    "ClassExpression"
                });
                *v = json!({"type": "VariableDeclaration", "kind": "var", "declarations": [
                    {"type": "VariableDeclarator", "id": id, "init": e}]});
                return;
            }
        }
        if t == "VariableDeclarator" && ty(&v["id"]) == "Identifier" {
            let it = ty(&v["init"]).to_string();
            if (it == "FunctionExpression" || it == "ClassExpression") && v["init"]["id"].is_null()
            {
                v["init"]["id"] = v["id"].clone();
            }
        }
    }
    if has('A') && t == "ArrowFunctionExpression" {
        v["type"] = json!("FunctionExpression");
        if v["expression"] == json!(true) {
            let body = v["body"].take();
            v["body"] = json!({"type": "BlockStatement", "body": [
                {"type": "ReturnStatement", "argument": body}]});
            v["expression"] = json!(false);
        }
    }
}

/// A template literal or a `+` chain that is a string concatenation, as its
/// flat parts (adjacent strings merged, empty strings dropped); None when the
/// node is not one.
fn concat_parts(v: &Value) -> Option<Vec<Value>> {
    let mut raw: Vec<Value> = Vec::new();
    let mut stringy = false;
    flatten(v, &mut raw, &mut stringy);
    if !stringy {
        return None;
    }
    let mut out: Vec<Value> = Vec::new();
    for p in raw {
        if let Some(s) = p.get("__s").and_then(Value::as_str) {
            if s.is_empty() {
                continue;
            }
            if let Some(last) = out.last_mut()
                && let Some(prev) = last.get("__s").and_then(Value::as_str)
            {
                *last = json!({"__s": format!("{prev}{s}")});
                continue;
            }
            out.push(json!({"__s": s}));
        } else {
            out.push(p);
        }
    }
    Some(
        out.into_iter()
            .map(|p| match p.get("__s").and_then(Value::as_str) {
                Some(s) => lit_str(s),
                None => p,
            })
            .collect(),
    )
}

fn flatten(v: &Value, out: &mut Vec<Value>, stringy: &mut bool) {
    match ty(v) {
        "BinaryExpression" if v["operator"] == json!("+") => {
            flatten(&v["left"], out, stringy);
            flatten(&v["right"], out, stringy);
        }
        "Concat" => {
            *stringy = true;
            for p in v["parts"].as_array().into_iter().flatten() {
                if ty(p) == "Literal" && p["value"].is_string() {
                    out.push(json!({"__s": p["value"]}));
                } else {
                    out.push(p.clone());
                }
            }
        }
        "TemplateLiteral" => {
            *stringy = true;
            let quasis = v["quasis"].as_array().cloned().unwrap_or_default();
            let exprs = v["expressions"].as_array().cloned().unwrap_or_default();
            for (i, q) in quasis.iter().enumerate() {
                let s = q["value"]["cooked"].as_str().unwrap_or("");
                out.push(json!({"__s": s}));
                if let Some(e) = exprs.get(i) {
                    out.push(e.clone());
                }
            }
        }
        "Literal" if v["value"].is_string() => {
            *stringy = true;
            out.push(json!({"__s": v["value"]}));
        }
        _ => out.push(v.clone()),
    }
}

fn print_row(p: &Value) -> Value {
    json!([p["hash"], p["nodes"], p["content"]])
}

fn query(bundler: &str, bundle: &str, out: &str) -> Result<(), String> {
    let code = std::fs::read_to_string(bundle).map_err(|e| e.to_string())?;
    let library = humanify_cli::discover_verb::library_settings(None)?;
    let args = humanify_cli::discover_verb::DiscoverArgs {
        input: bundle,
        library_index: None,
        library: &library,
        bundler: Some(bundler),
        unit_prints: true,
        work_dir: None,
    };
    let doc = humanify_cli::discover_verb::discover(&code, &args)?;
    let rows = doc["libraryDiscovery"]["unitRows"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let canon: std::collections::HashMap<(u64, u64), (String, String)> = RECORDED
        .lock()
        .unwrap()
        .drain(..)
        .map(|(s, e, h, p)| ((s, e), (h, p)))
        .collect();
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).map_err(|e| e.to_string())?);
    for r in rows {
        let Some(w) = r.get("whole") else { continue };
        let (s, e) = (
            r["start"].as_u64().unwrap_or(0),
            r["end"].as_u64().unwrap_or(0),
        );
        let fns: Vec<Value> = r["functions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| json!([p["start"], p["end"], p["hash"], p["nodes"], p["content"]]))
            .collect();
        let mut line = json!({"s": s, "e": e, "w": print_row(w), "f": fns});
        if let Some((_, parts)) = canon.get(&(s, e)) {
            line["c"] = json!(parts);
        }
        writeln!(f, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn index_build(dir: &str, tag: &str) -> Option<Value> {
    let out = Path::new(dir).join(tag);
    let code = std::fs::read_to_string(out.join("out.js")).ok()?;
    let map_text = std::fs::read_to_string(out.join("out.js.map")).ok()?;
    let stamp = std::fs::read_to_string(out.join(".recipe")).ok()?;
    let recipe_line = stamp.lines().nth(1)?;
    let recipe = humanify_core::libindex::recipe::Recipe::parse(recipe_line).ok()?;
    let map = humanify_index::sourcemap::SourceMap::parse(&map_text).ok()?;
    let (rows, _) = humanify_index::build::unit_rows(recipe, &code, &map, &out);
    let rows: Vec<Value> = rows
        .into_iter()
        .map(|u| json!([u.owner, u.version, u.file, u.hash, u.nodes, u.content]))
        .collect();
    Some(json!({"dir": dir, "tag": tag, "rows": rows}))
}

fn index(out: &str) -> Result<(), String> {
    let dirs: Vec<String> = std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.is_empty())
        .collect();
    let canon = *CANON.get().unwrap_or(&false);
    let f = Mutex::new(std::io::BufWriter::new(
        std::fs::File::create(out).map_err(|e| e.to_string())?,
    ));
    let one = |d: &String| {
        for tag in ["req", "dyn"] {
            if canon {
                RECORDED.lock().unwrap().clear();
            }
            if let Some(mut line) = index_build(d, tag) {
                if canon {
                    let rec: Vec<Value> = RECORDED
                        .lock()
                        .unwrap()
                        .drain(..)
                        .map(|(s, e, h, p)| json!([s, e, h, p]))
                        .collect();
                    line["canon"] = json!(rec);
                }
                writeln!(f.lock().unwrap(), "{line}").ok();
            }
        }
    };
    if canon {
        dirs.iter().for_each(one);
    } else {
        dirs.par_iter().for_each(one);
    }
    Ok(())
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let canon = a.iter().any(|x| x == "--canon");
    let a: Vec<String> = a.into_iter().filter(|x| x != "--canon").collect();
    CANON.set(canon).ok();
    humanify_core::libindex::units::LAB_NORMALISE.set(hook).ok();
    let r = match a.get(1).map(String::as_str) {
        Some("query") if a.len() == 6 => {
            VARIANT.set(a[2].clone()).ok();
            query(&a[3], &a[4], &a[5])
        }
        Some("index") if a.len() == 4 => {
            VARIANT.set(a[2].clone()).ok();
            index(&a[3])
        }
        _ => Err("usage: exp122-lab query <variant> <bundler> <bundle> <out> | index <variant> <out> [--canon]".into()),
    };
    if let Err(e) = r {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
