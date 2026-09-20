//! The Bun module fossils — TS original: `src/split/fossil-map.ts` (324
//! LOC), ported here because WP2.3's module-scoped tier (`pairByModuleContext`,
//! statement-twin.ts :988-1088) reads the extraction while the split's
//! assignment half (WP5.2) reads the whole `FossilExtract`. The grammar is
//! OWNER-documented in `experiments/068-module-fossils/SPEC.md`: every
//! lazily-forceable source file compiles to a CONTIGUOUS wrapper segment
//! terminated by its `__esm` init definition; the init's leading zero-arg
//! init calls are that file's imports.
//!
//! Substrate: the statements' ESTree JSON (the inventory's retained
//! values — the same subtrees the statement hashes were computed from),
//! not a babel AST; the walk reads node types the JSON carries verbatim.

use std::collections::{HashMap, HashSet};

use serde_json::Value;

/// oxc's ESTree PRESERVES parentheses (`ParenthesizedExpression` nodes the
/// babel AST the TS port walked dropped) — every shape check below walks
/// through them.
fn unwrap_paren(v: &Value) -> &Value {
    let mut cur = v;
    while cur.get("type").and_then(Value::as_str) == Some("ParenthesizedExpression") {
        cur = match cur.get("expression") {
            Some(next) => next,
            None => break,
        };
    }
    cur
}

/// One fossil module (TS `FossilModule` :26).
#[derive(Debug, Clone)]
pub struct FossilModule {
    /// Wrapper index of the init def — the segment terminator.
    pub init_index: usize,
    /// Wrapper indexes of the segment, contiguous, init included.
    pub statements: Vec<usize>,
    /// Rename-blind statement hashes of the segment, SORTED (the
    /// cross-version signature).
    pub hashes: Vec<String>,
    /// Module indexes of leading init calls — the import edges.
    pub imports: Vec<usize>,
    /// Names the segment declares (post-rename; same-version use only).
    pub declared: Vec<String>,
}

/// TS `FossilExtract` (:47).
#[derive(Debug, Default)]
pub struct FossilExtract {
    pub modules: Vec<FossilModule>,
    /// Wrapper indexes in no segment (statements after the last init).
    pub eager_zone: Vec<usize>,
}

/// The helper's thunk shape: `(fn, res) => () => ...` — a two-parameter
/// arrow whose body is a zero-arg thunk (:57).
fn thunk_of(init: &Value) -> Option<&Value> {
    if init.get("type").and_then(Value::as_str) != Some("ArrowFunctionExpression") {
        return None;
    }
    let params = init.get("params")?.as_array()?;
    if params.len() != 2 {
        return None;
    }
    let body = unwrap_paren(init.get("body")?);
    let body_type = body.get("type").and_then(Value::as_str)?;
    if body_type != "ArrowFunctionExpression" && body_type != "FunctionExpression" {
        return None;
    }
    let inner_params = body.get("params")?.as_array()?;
    if !inner_params.is_empty() {
        return None;
    }
    Some(body)
}

/// The `(…, ident)` a helper thunk yields, whichever form it takes (:93).
fn thunk_result_sequence(thunk: &Value) -> Option<&Value> {
    let body = unwrap_paren(thunk.get("body")?);
    if body.get("type").and_then(Value::as_str) == Some("SequenceExpression") {
        return Some(body);
    }
    if body.get("type").and_then(Value::as_str) != Some("BlockStatement") {
        return None;
    }
    let ret = body
        .get("body")?
        .as_array()?
        .iter()
        .find(|s| s.get("type").and_then(Value::as_str) == Some("ReturnStatement"))?;
    let arg = ret.get("argument")?;
    (arg.get("type").and_then(Value::as_str) == Some("SequenceExpression")).then_some(arg)
}

/// The `__esm` helper SHAPE (:82): a two-parameter arrow whose body is a
/// zero-arg thunk ending in an identifier — bun's and esbuild's forms
/// agree on this shape (exp075, verified against esbuild 0.27.2 output).
fn is_esm_helper(d: &Value) -> bool {
    if d.get("id")
        .and_then(|i| i.get("type"))
        .and_then(Value::as_str)
        != Some("Identifier")
    {
        return false;
    }
    let Some(init) = d.get("init") else {
        return false;
    };
    let Some(thunk) = thunk_of(init) else {
        return false;
    };
    let Some(seq) = thunk_result_sequence(thunk) else {
        return false;
    };
    let Some(exprs) = seq.get("expressions").and_then(Value::as_array) else {
        return false;
    };
    let Some(last) = exprs.last() else {
        return false;
    };
    last.get("type").and_then(Value::as_str) == Some("Identifier")
}

/// What a top-level statement DECLARES — the names the placement trail is
/// searched by, and the per-segment `declared` export list (:105).
pub fn declared_names(stmt: &Value) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let stmt_type = stmt.get("type").and_then(Value::as_str).unwrap_or("");
    match stmt_type {
        "FunctionDeclaration" | "ClassDeclaration" => {
            if let Some(name) = stmt
                .get("id")
                .and_then(|id| id.get("name"))
                .and_then(Value::as_str)
            {
                out.push(name.to_string());
            }
        }
        "VariableDeclaration" => {
            for d in stmt
                .get("declarations")
                .and_then(Value::as_array)
                .unwrap_or(&vec![])
            {
                if d.get("id")
                    .and_then(|i| i.get("type"))
                    .and_then(Value::as_str)
                    == Some("Identifier")
                    && let Some(name) = d
                        .get("id")
                        .and_then(|i| i.get("name"))
                        .and_then(Value::as_str)
                {
                    out.push(name.to_string());
                }
            }
        }
        _ => {}
    }
    out
}

/// The init function inside an `__esm(...)` argument, in either observed
/// form (:131): bun passes the function directly; esbuild passes an OBJECT
/// with a single keyed method whose KEY IS THE ORIGINAL SOURCE PATH.
/// Returns the init's zero-arg-call-terminated shape marker plus the
/// source path when present.
fn init_function_of(arg: Option<&Value>) -> (Option<&Value>, Option<String>) {
    let Some(arg) = arg else {
        return (None, None);
    };
    let arg = unwrap_paren(arg);
    let arg_type = arg.get("type").and_then(Value::as_str).unwrap_or("");
    if arg_type == "ArrowFunctionExpression" || arg_type == "FunctionExpression" {
        return (Some(arg), None);
    }
    if arg_type != "ObjectExpression" {
        return (None, None);
    }
    let Some(properties) = arg.get("properties").and_then(Value::as_array) else {
        return (None, None);
    };
    if properties.len() != 1 {
        return (None, None);
    }
    let prop = &properties[0];
    let prop_type = prop.get("type").and_then(Value::as_str).unwrap_or("");
    // The ESTree JSON's spelling is `Property` (the oxc-native
    // `ObjectProperty`/`ObjectMethod` names are the babel spellings the TS
    // original walked — both accepted here for the port's clarity).
    let key = if matches!(prop_type, "Property" | "ObjectProperty" | "ObjectMethod") {
        prop.get("key")
    } else {
        None
    };
    let source_path = match key.map(|k| (k.get("type").and_then(Value::as_str), k.get("value"))) {
        Some((Some("StringLiteral" | "Literal"), Some(Value::String(v)))) => Some(v.clone()),
        Some((Some("Identifier"), _)) => key
            .and_then(|k| k.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    };
    let value_type = prop
        .get("value")
        .and_then(|v| v.get("type"))
        .and_then(Value::as_str);
    let prop_value = prop.get("value");
    match (prop_type, value_type) {
        ("ObjectMethod", _) => (prop_value, source_path),
        ("Property" | "ObjectProperty", Some("ArrowFunctionExpression" | "FunctionExpression")) => {
            (prop_value.map(unwrap_paren), source_path)
        }
        _ => (None, None),
    }
}

/// Leading zero-arg identifier calls of an init body — the import edges
/// (:177).
fn leading_init_calls(init_fn: Option<&Value>) -> Vec<String> {
    let mut leading: Vec<String> = Vec::new();
    let Some(fn_body) = init_fn.and_then(|f| f.get("body")) else {
        return leading;
    };
    if fn_body.get("type").and_then(Value::as_str) != Some("BlockStatement") {
        return leading;
    }
    let Some(body) = fn_body.get("body").and_then(Value::as_array) else {
        return leading;
    };
    for s in body {
        let is_call = s.get("type").and_then(Value::as_str) == Some("ExpressionStatement")
            && s.get("expression")
                .and_then(|e| e.get("type"))
                .and_then(Value::as_str)
                == Some("CallExpression")
            && s.get("expression")
                .and_then(|e| e.get("arguments"))
                .and_then(Value::as_array)
                .is_some_and(|a| a.is_empty())
            && s.get("expression")
                .and_then(|e| e.get("callee"))
                .and_then(|c| c.get("type"))
                .and_then(Value::as_str)
                == Some("Identifier");
        if is_call {
            leading.push(
                s.get("expression")
                    .and_then(|e| e.get("callee"))
                    .and_then(|c| c.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            );
            continue;
        }
        break;
    }
    leading
}

struct RawInit {
    index: usize,
    name: String,
    leading: Vec<String>,
}

fn find_init_defs(body: &[Value], esm_helpers: &HashSet<String>) -> Vec<RawInit> {
    let mut raw: Vec<RawInit> = Vec::new();
    for (i, stmt) in body.iter().enumerate() {
        if stmt.get("type").and_then(Value::as_str) != Some("VariableDeclaration") {
            continue;
        }
        let Some(declarations) = stmt.get("declarations").and_then(Value::as_array) else {
            continue;
        };
        for d in declarations {
            let name = d
                .get("id")
                .and_then(|id| id.get("name"))
                .and_then(Value::as_str);
            let callee = d
                .get("init")
                .and_then(|init| init.get("callee"))
                .and_then(|c| c.get("name"))
                .and_then(Value::as_str);
            let arguments = d
                .get("init")
                .and_then(|init| init.get("arguments"))
                .and_then(Value::as_array);
            let is_call = d
                .get("init")
                .and_then(|init| init.get("type"))
                .and_then(Value::as_str)
                == Some("CallExpression");
            if name.is_none()
                || !is_call
                || callee.is_none_or(|c| !esm_helpers.contains(c))
                || arguments.is_none_or(|a| a.is_empty())
            {
                continue;
            }
            let (init_fn, _source_path) = init_function_of(arguments.and_then(|a| a.first()));
            if init_fn.is_none() {
                continue;
            }
            raw.push(RawInit {
                index: i,
                name: name.unwrap_or_default().to_string(),
                leading: leading_init_calls(init_fn),
            });
        }
    }
    raw
}

/// Extract the fossil modules of a wrapper body (:298). `hashes` is the
/// rename-blind statement hash per statement, SAME ORDER AS `body` — the
/// caller computes them once for everything (the inventory's own).
pub fn extract_fossil_modules(body: &[Value], hashes: &[String]) -> Result<FossilExtract, String> {
    if hashes.len() != body.len() {
        return Err(format!(
            "fossil map: {} hashes for {} statements",
            hashes.len(),
            body.len()
        ));
    }
    // The helper NAMES: every declarator matching the helper SHAPE.
    let mut helpers: HashSet<String> = HashSet::new();
    for stmt in body {
        if stmt.get("type").and_then(Value::as_str) != Some("VariableDeclaration") {
            continue;
        }
        for d in stmt
            .get("declarations")
            .and_then(Value::as_array)
            .unwrap_or(&vec![])
        {
            if is_esm_helper(d)
                && let Some(name) = d
                    .get("id")
                    .and_then(|i| i.get("name"))
                    .and_then(Value::as_str)
            {
                helpers.insert(name.to_string());
            }
        }
    }

    let mut raw = find_init_defs(body, &helpers);
    raw.sort_by_key(|r| r.index);
    let name_to_module: HashMap<String, usize> = raw
        .iter()
        .enumerate()
        .map(|(k, r)| (r.name.clone(), k))
        .collect();

    let mut modules: Vec<FossilModule> = Vec::new();
    let mut prev: isize = -1;
    for r in &raw {
        let statements: Vec<usize> = ((prev + 1) as usize..=r.index).collect();
        let declared: Vec<String> = statements
            .iter()
            .flat_map(|&i| declared_names(&body[i]))
            .collect();
        let mut hashes_of: Vec<String> = statements.iter().map(|&i| hashes[i].clone()).collect();
        hashes_of.sort();
        modules.push(FossilModule {
            init_index: r.index,
            statements,
            hashes: hashes_of,
            imports: r
                .leading
                .iter()
                .filter_map(|n| name_to_module.get(n).copied())
                .collect(),
            declared,
        });
        prev = r.index as isize;
    }
    let eager_zone: Vec<usize> = if prev >= 0 {
        ((prev as usize + 1)..body.len()).collect()
    } else {
        (0..body.len()).collect()
    };
    Ok(FossilExtract {
        modules,
        eager_zone,
    })
}

/// A module's cross-version signature (statement-twin.ts :1017): the
/// segment's hashes joined in the SORTED order the extraction produced.
pub fn module_signature(m: &FossilModule) -> String {
    m.hashes.join("|")
}

#[cfg(test)]
mod fossil_test;
