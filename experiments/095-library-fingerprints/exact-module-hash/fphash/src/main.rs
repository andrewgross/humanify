//! fphash: whole-module fingerprints for exp095, computed with humanify's OWN
//! hash code (humanify-core, path dependency; the repo is not modified).
//!
//! stdin: JSONL jobs {"id": <any>, "path": "<abs file>", "kind": "cjs|lazy-esm|eager-esm|other"}
//!        (or "text" instead of "path")
//! stdout: JSONL {"id", "ok", "err"?, "nStmts", "nCore", hashes...}
//!
//! Every span is parsed STANDALONE, the way humanify's vendor content key does
//! it (`modules::vendor_content::vendor_content_key`): free identifiers that
//! are not known globals are declared around the span (`var a,b,...;`) so a
//! reference to the rest of the bundle (helpers, other modules) becomes a
//! binding slot rather than a verbatim, release-specific name.
//!
//! Hash variants (all 16 hex = first 8 bytes of sha256):
//!   full.stmt   every identifier masked, literals kept (humanify's statement hash, concatenated)
//!   full.canonM binding slots, properties + globals verbatim, strings BLURRED (humanify's structural hash, MatchKey)
//!   full.canonV the same with strings EXACT (Verbatim policy)
//!   core.*      the same three after stripping the bundler's wrapper:
//!               cjs: the factory function's BODY only (params dropped);
//!               lazy-esm: namespace block + bare `var a,b;` dropped, `__esm(()=>{BODY})` marker unwrapped;
//!               eager-esm: namespace blocks + bare var lists dropped.
//!   coreSet.stmt / coreSet.canonM: the core's per-statement hashes SORTED (order-insensitive).
//!   factM       cjs only: canonical MatchKey of the factory FUNCTION (params included) = humanify's
//!               `factory_structural_hash`, i.e. the vendor manifest's structuralHash.

mod norm;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, Write};

use humanify_core::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize};
use humanify_core::hash::statement_hash::statement_hash;
use humanify_core::ingest::{Ingest, parse_estree_json};
use humanify_core::modules::known_globals::is_known_global;
use oxc_allocator::Allocator;
use rayon::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn sha16(s: &str) -> String {
    let d = Sha256::digest(s.as_bytes());
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn ty(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

/// Parse `text` with `declared` declared in front. Returns (program json, free non-global names, prefix stmts).
/// Names declared at the top level of a bundle (root scope; when the program is a single wrapper
/// statement, also the scopes directly under the root). A span's unresolved reference to one of these
/// is a reference to the rest of the bundle, even when the name happens to be a known global (`gc`).
fn bundle_names(path: &str) -> HashSet<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashSet::new();
    };
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, &text);
    let scoping = ingest.semantic().scoping();
    let root = scoping.root_scope_id();
    let wrapped = ingest.program.body.len() <= 2;
    let mut names = HashSet::new();
    for sym in scoping.symbol_ids() {
        let sc = scoping.symbol_scope_id(sym);
        if sc == root || (wrapped && scoping.scope_parent_id(sc) == Some(root)) {
            names.insert(scoping.symbol_name(sym).to_string());
        }
    }
    // the @bun-cjs wrapper's own parameters stay globals, as they are in an unwrapped bundle
    for n in ["exports", "require", "module", "__filename", "__dirname"] {
        names.remove(n);
    }
    names
}

fn parse_with(
    text: &str,
    declared: &[String],
    bundle: Option<&HashSet<String>>,
) -> Result<(Value, Vec<String>, SymbolTables, usize), String> {
    let mut src = String::with_capacity(text.len() + 16 + declared.len() * 8);
    let mut prefix = 0;
    if !declared.is_empty() {
        src.push_str("var ");
        src.push_str(&declared.join(","));
        src.push_str(";\n");
        prefix = 1;
    }
    src.push_str(text);
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, &src);
    if !ingest.errors.is_empty() {
        return Err(format!("parse: {}", ingest.errors[0]));
    }
    let mut free: Vec<String> = ingest
        .semantic()
        .scoping()
        .root_unresolved_references()
        .keys()
        .map(|s| s.to_string())
        .filter(|s| !is_known_global(s) || bundle.is_some_and(|b| b.contains(s.as_str())))
        .collect();
    free.sort_unstable();
    let tables = SymbolTables::build(ingest.semantic());
    let json = parse_estree_json(&ingest.program.to_estree_json(false, true));
    Ok((json, free, tables, prefix))
}

/// The function argument of a wrapper call `H(fn)` / `H({"path"(){}})`.
fn call_fn_arg(call: &Value) -> Option<&Value> {
    if ty(call) != "CallExpression" {
        return None;
    }
    let args = call.get("arguments")?.as_array()?;
    if args.len() != 1 {
        return None;
    }
    let a = &args[0];
    match ty(a) {
        "FunctionExpression" | "ArrowFunctionExpression" => Some(a),
        "ObjectExpression" => {
            let props = a.get("properties")?.as_array()?;
            if props.len() != 1 {
                return None;
            }
            let v = props[0].get("value")?;
            matches!(ty(v), "FunctionExpression" | "ArrowFunctionExpression").then_some(v)
        }
        _ => None,
    }
}

/// `var X = H(fn)` with exactly one declarator: the fn.
fn wrapper_decl_fn(stmt: &Value) -> Option<&Value> {
    if ty(stmt) != "VariableDeclaration" {
        return None;
    }
    let decls = stmt.get("declarations")?.as_array()?;
    if decls.len() != 1 {
        return None;
    }
    call_fn_arg(decls[0].get("init")?)
}

fn fn_body_stmts(f: &Value) -> Option<&Vec<Value>> {
    let body = f.get("body")?;
    if ty(body) == "BlockStatement" {
        body.get("body")?.as_array()
    } else {
        None
    }
}

fn is_bare_var_list(stmt: &Value) -> bool {
    ty(stmt) == "VariableDeclaration"
        && stmt
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|ds| ds.iter().all(|d| d.get("init").is_none_or(Value::is_null)))
}

fn is_empty_obj_var(stmt: &Value) -> bool {
    ty(stmt) == "VariableDeclaration"
        && stmt
            .get("declarations")
            .and_then(Value::as_array)
            .is_some_and(|ds| {
                ds.len() == 1
                    && ds[0].get("init").is_some_and(|i| {
                        ty(i) == "ObjectExpression"
                            && i.get("properties")
                                .and_then(Value::as_array)
                                .is_some_and(Vec::is_empty)
                    })
            })
}

/// `H(N, {a: () => b, ...})` — the namespace export registration.
fn is_ns_register(stmt: &Value) -> bool {
    if ty(stmt) != "ExpressionStatement" {
        return false;
    }
    let Some(call) = stmt.get("expression") else {
        return false;
    };
    if ty(call) != "CallExpression" {
        return false;
    }
    let Some(args) = call.get("arguments").and_then(Value::as_array) else {
        return false;
    };
    if args.len() != 2 || ty(&args[0]) != "Identifier" || ty(&args[1]) != "ObjectExpression" {
        return false;
    }
    args[1]
        .get("properties")
        .and_then(Value::as_array)
        .is_some_and(|ps| {
            ps.iter().all(|p| {
                p.get("value")
                    .is_some_and(|v| ty(v) == "ArrowFunctionExpression")
            })
        })
}

/// The lazy-ESM init marker: `var I = H(() => {BODY})` (zero-param fn).
fn esm_marker_body(stmt: &Value) -> Option<&Vec<Value>> {
    let f = wrapper_decl_fn(stmt)?;
    let params = f.get("params")?.as_array()?;
    if !params.is_empty() {
        return None;
    }
    fn_body_stmts(f)
}

fn core_stmts<'a>(stmts: &'a [Value], kind: &str) -> (Vec<&'a Value>, Option<&'a Value>) {
    let mut out: Vec<&Value> = Vec::new();
    let mut factory: Option<&Value> = None;
    match kind {
        "cjs" => {
            // the LAST wrapper declaration in the span is the factory (a banner comment may precede)
            for s in stmts {
                if let Some(f) = wrapper_decl_fn(s)
                    && let Some(b) = fn_body_stmts(f)
                {
                    factory = Some(f);
                    out = b.iter().collect();
                }
            }
            if factory.is_none() {
                out = stmts.iter().collect();
            }
        }
        "lazy-esm" | "eager-esm" => {
            for s in stmts {
                if is_empty_obj_var(s) || is_ns_register(s) || is_bare_var_list(s) {
                    continue;
                }
                if kind == "lazy-esm"
                    && let Some(b) = esm_marker_body(s)
                {
                    out.extend(b.iter());
                    continue;
                }
                out.push(s);
            }
        }
        _ => out = stmts.iter().collect(),
    }
    (out, factory)
}

fn hashes_of(
    stmts: &[&Value],
    tables: &SymbolTables,
) -> (String, String, String, String, String, usize) {
    let arr = Value::Array(stmts.iter().map(|v| (*v).clone()).collect());
    let cm = canonical_serialize(&arr, tables, LiteralPolicy::MatchKey);
    let canon_len = cm.parts.len();
    let canon_m = cm.hash;
    let canon_v = canonical_serialize(&arr, tables, LiteralPolicy::Verbatim).hash;
    let per: Vec<String> = stmts.iter().map(|s| statement_hash(s)).collect();
    let stmt = sha16(&per.join(","));
    let mut sorted = per.clone();
    sorted.sort();
    let set_stmt = sha16(&sorted.join(","));
    let mut per_m: Vec<String> = stmts
        .iter()
        .map(|s| canonical_serialize(s, tables, LiteralPolicy::MatchKey).hash)
        .collect();
    per_m.sort();
    let set_m = sha16(&per_m.join(","));
    (stmt, canon_m, canon_v, set_stmt, set_m, canon_len)
}

fn process(job: &Value, bundles: &HashMap<String, HashSet<String>>) -> Value {
    let bundle = job
        .get("bundle")
        .and_then(Value::as_str)
        .and_then(|b| bundles.get(b));
    let id = job.get("id").cloned().unwrap_or(Value::Null);
    let kind = job.get("kind").and_then(Value::as_str).unwrap_or("other");
    let text = if let Some(t) = job.get("text").and_then(Value::as_str) {
        t.to_string()
    } else {
        match std::fs::read_to_string(job.get("path").and_then(Value::as_str).unwrap_or("")) {
            Ok(t) => t,
            Err(e) => return json!({"id": id, "ok": false, "err": format!("read: {e}")}),
        }
    };
    let first = match parse_with(&text, &[], bundle) {
        Ok(r) => r,
        Err(e) => return json!({"id": id, "ok": false, "err": e}),
    };
    let free_names = first.1.clone();
    let (json, _free, tables, prefix) = if first.1.is_empty() {
        first
    } else {
        match parse_with(&text, &first.1, bundle) {
            Ok(r) => r,
            Err(e) => return json!({"id": id, "ok": false, "err": e}),
        }
    };
    let Some(body) = json.get("body").and_then(Value::as_array) else {
        return json!({"id": id, "ok": false, "err": "no body"});
    };
    let stmts = &body[prefix..];
    let all: Vec<&Value> = stmts.iter().collect();
    let full = hashes_of(&all, &tables);
    let (core, factory) = core_stmts(stmts, kind);
    let corehs = hashes_of(&core, &tables);
    let fact_m = factory.map(|f| canonical_serialize(f, &tables, LiteralPolicy::MatchKey).hash);
    // N1: the minifier-normal form (src/norm.rs) of the core
    let free: std::collections::HashSet<String> = free_names.iter().cloned().collect();
    let normed = norm::norm_core(&core, stmts, kind, &free);
    let nrefs: Vec<&Value> = normed.iter().collect();
    let normhs = hashes_of(&nrefs, &tables);
    if job.get("parts").and_then(Value::as_bool).unwrap_or(false) {
        let arr = Value::Array(normed.clone());
        eprintln!(
            "{}",
            canonical_serialize(&arr, &tables, LiteralPolicy::MatchKey).parts
        );
    }
    if job.get("dump").and_then(Value::as_bool).unwrap_or(false) {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&Value::Array(normed.clone())).unwrap()
        );
    }
    json!({
        "id": id, "ok": true, "nStmts": stmts.len(), "nCore": core.len(),
        "factory": factory.is_some(),
        "fullStmt": full.0, "fullM": full.1, "fullV": full.2,
        "coreStmt": corehs.0, "coreM": corehs.1, "coreV": corehs.2,
        "coreSetStmt": corehs.3, "coreSetM": corehs.4,
        "factM": fact_m,
        "nNorm": normed.len(), "normLen": normhs.5, "coreLen": corehs.5,
        "normStmt": normhs.0, "normM": normhs.1, "normV": normhs.2, "normSetStmt": normhs.3, "normSetM": normhs.4,
    })
}

fn main() {
    let stdin = std::io::stdin();
    let jobs: Vec<Value> = stdin
        .lock()
        .lines()
        .map_while(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(&l).expect("job json"))
        .collect();
    let mut bpaths: Vec<String> = jobs
        .iter()
        .filter_map(|j| j.get("bundle").and_then(Value::as_str).map(String::from))
        .collect();
    bpaths.sort();
    bpaths.dedup();
    let bundles: HashMap<String, HashSet<String>> = bpaths
        .par_iter()
        .map(|p| (p.clone(), bundle_names(p)))
        .collect();
    eprintln!("bundles parsed: {}", bundles.len());
    let out: Vec<String> = jobs
        .par_iter()
        .map(|j| {
            let r = std::panic::catch_unwind(|| process(j, &bundles));
            match r {
                Ok(v) => v.to_string(),
                Err(_) => json!({"id": j.get("id"), "ok": false, "err": "panic"}).to_string(),
            }
        })
        .collect();
    let stdout = std::io::stdout();
    let mut w = std::io::BufWriter::new(stdout.lock());
    for l in out {
        writeln!(w, "{l}").unwrap();
    }
}
