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
    /// The init's (the lazy-init wrapper's) binding name, as the text
    /// spells it — the module name's record once the naming stage's module
    /// step named it (`place::stems::module_stem_of_wrapper`).
    pub init_name: String,
    /// Wrapper indexes of the segment, contiguous, init included.
    pub statements: Vec<usize>,
    /// Rename-blind statement hashes of the segment, SORTED (the
    /// cross-version signature).
    pub hashes: Vec<String>,
    /// Module indexes of leading init calls — the import edges.
    pub imports: Vec<usize>,
    /// Names the segment declares (post-rename; same-version use only).
    pub declared: Vec<String>,
    /// The module's ORIGINAL source path, when the bundler kept it —
    /// esbuild's unminified form uses it as the init object's key (exp075).
    /// Absent for bun and for any minified build, so nothing may depend on
    /// it; recorded metadata, a gift when present.
    pub source_path: Option<String>,
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

/// The `(…, ident)` a helper thunk yields, whichever form it takes (:93):
/// a SEQUENCE — the raw text's form, expression-bodied (bun) or returned
/// (esbuild) — or esbuild's BEAUTIFIED restructing of the returned one
/// (the stage-6 formatter unrolls `return fn && (…), res;` into
/// `if (fn) { … } return res;`).
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

/// An ESTree Identifier's name (None for anything else).
fn ident_name(v: Option<&Value>) -> Option<&str> {
    let v = v?;
    (v.get("type").and_then(Value::as_str) == Some("Identifier"))
        .then(|| v.get("name")?.as_str())
        .flatten()
}

/// The `__esm` helper SHAPE (:82): a two-parameter arrow whose body is a
/// zero-arg thunk ending in an identifier — the RAW form's sequence
/// (bun's and esbuild's, exp075-verified against real 0.27.2 output), or
/// esbuild's beautified form on the SHIPPED text the split reads: an
/// if-guard on the FIRST parameter and a bare `return <second parameter>`.
/// The beautified recognition is deliberately TIGHT (both parameter names
/// must appear in exactly those roles) so ordinary memoizer-shaped code
/// on a BUN tree cannot mint phantom modules.
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
    if let Some(seq) = thunk_result_sequence(thunk) {
        let Some(exprs) = seq.get("expressions").and_then(Value::as_array) else {
            return false;
        };
        let Some(last) = exprs.last() else {
            return false;
        };
        return last.get("type").and_then(Value::as_str) == Some("Identifier");
    }
    // The beautified esbuild form: every statement is examined only for
    // its role, so the `(0, fn[…](fn = 0))` call inside the if survives
    // any formatting.
    let Some(params) = init.get("params").and_then(Value::as_array) else {
        return false;
    };
    let (Some(fn_name), Some(res_name)) = (ident_name(params.first()), ident_name(params.get(1)))
    else {
        return false;
    };
    let Some(body) = thunk.get("body").map(unwrap_paren) else {
        return false;
    };
    let Some(stmts) = body.get("body").and_then(Value::as_array) else {
        return false;
    };
    let Some((last, head)) = stmts.split_last() else {
        return false;
    };
    if last.get("type").and_then(Value::as_str) != Some("ReturnStatement")
        || ident_name(last.get("argument")) != Some(res_name)
    {
        return false;
    }
    // The final return must name the second parameter, and SOME preceding
    // statement must be an if-guard naming the first.
    head.iter().any(|s| {
        s.get("type").and_then(Value::as_str) == Some("IfStatement")
            && ident_name(s.get("test")) == Some(fn_name)
    })
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
/// (:177). esbuild heads every wrapped module body with `"use strict";`
/// (the Directive Prologue), which the scan reads through — the init
/// calls start after it.
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
    let mut stmts = body.as_slice();
    // The Directive Prologue: leading string-literal expression statements.
    while matches!(
        stmts.first().map(|s| (s.get("type").and_then(Value::as_str), s.get("expression"))),
        Some((Some("ExpressionStatement"), Some(expr)))
            if expr.get("type").and_then(Value::as_str) == Some("Literal")
                && expr.get("value").is_some_and(Value::is_string)
    ) {
        stmts = &stmts[1..];
    }
    for s in stmts {
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
    /// esbuild only: the original source path, from the object key.
    source_path: Option<String>,
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
            let (init_fn, source_path) = init_function_of(arguments.and_then(|a| a.first()));
            if init_fn.is_none() {
                continue;
            }
            raw.push(RawInit {
                index: i,
                name: name.unwrap_or_default().to_string(),
                leading: leading_init_calls(init_fn),
                source_path,
            });
        }
    }
    raw
}

/// The lazy-init (`__esm`) helper NAMES among a wrapper body's top-level
/// statements: every declarator matching the helper SHAPE
/// ([`is_esm_helper`] — Bun's and esbuild's forms, raw and formatted).
pub fn lazy_init_helper_names(body: &[Value]) -> HashSet<String> {
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
    helpers
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
    let helpers = lazy_init_helper_names(body);
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
            init_name: r.name.clone(),
            statements,
            hashes: hashes_of,
            imports: r
                .leading
                .iter()
                .filter_map(|n| name_to_module.get(n).copied())
                .collect(),
            declared,
            source_path: r.source_path.clone(),
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

/// How much of a bundle's top-level APP code the module markers describe
/// — the measure the split method is chosen by
/// ([`crate::place::method`], findings C1/C2 of the 2026-10-05
/// app-specific scan).
///
/// The marker grammar ([`extract_fossil_modules`]) reads every statement
/// up to an init as that init's module and everything after the last init
/// as the entry. That is TRUE only when every source file was loaded
/// lazily (Claude Code's shape: Bun writes a lazy init only for an ES
/// module that is `require()`d or loaded by `import()`). In a bundle with
/// eagerly loaded modules, the eager ones emitted before a lazy one are
/// glued into its file and the rest piles into the entry file.
///
/// A statement is COVERED when it sits inside a segment AND is something
/// a lazy module's top level can hold: a hoisted function or class, a
/// `var` whose initializers are inert (no call runs at load — literals,
/// functions, plain property reads, a call that only DEFINES a thunk), the
/// export registrar call (`__export(ns, { name: () => name })`), or the
/// init definition itself. Bun moves every other top-level statement of a
/// lazy module INTO its init, so anything else inside a segment is eager
/// code glued in (`glued_statements`). The eager tail after the last init
/// is uncovered too: it all lands in one entry file.
///
/// A LOWER bound on the eager code: an eager module holding only
/// functions and constants has a lazy module's shape and counts as
/// covered — which is why the method threshold is strict.
///
/// Mass is source bytes. Module-factory definitions (calls of the
/// bundle's module helper, `var require_x = __commonJS(…)`) are vendor
/// code under every split method and are outside the app mass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkerCoverage {
    /// Recorded modules (init definitions).
    pub modules: usize,
    /// App-code bytes: every top-level statement but module factories.
    pub app_bytes: u64,
    /// Bytes of covered statements.
    pub covered_bytes: u64,
    /// Statements inside a segment that no lazy module's top level holds.
    pub glued_statements: usize,
    /// App statements after the last init (the entry tail).
    pub tail_statements: usize,
}

impl MarkerCoverage {
    /// Covered share of the app code, 0 for no app code.
    pub fn share(&self) -> f64 {
        if self.app_bytes == 0 {
            0.0
        } else {
            self.covered_bytes as f64 / self.app_bytes as f64
        }
    }
}

/// An expression that runs no code of the program when evaluated at load:
/// literals, functions and classes (defined, not called), plain property
/// reads, operators over those, object/array literals of those, and a
/// call whose every argument is a function (or an object of functions) —
/// the bundler's thunk-defining helpers (`__esm(…)`, `__commonJS(…)`,
/// `__lazy(…)`), which store the function and return.
fn is_inert(e: &Value) -> bool {
    let e = unwrap_paren(e);
    match e.get("type").and_then(Value::as_str).unwrap_or("") {
        "Literal"
        | "Identifier"
        | "ArrowFunctionExpression"
        | "FunctionExpression"
        | "ClassExpression" => true,
        "TemplateLiteral" => all_of(e.get("expressions"), is_inert),
        "UnaryExpression" => {
            e.get("operator").and_then(Value::as_str) != Some("delete")
                && e.get("argument").is_some_and(is_inert)
        }
        "BinaryExpression" | "LogicalExpression" => {
            e.get("left").is_some_and(is_inert) && e.get("right").is_some_and(is_inert)
        }
        "ArrayExpression" => all_of(e.get("elements"), |x| {
            x.is_null()
                || (x.get("type").and_then(Value::as_str) != Some("SpreadElement") && is_inert(x))
        }),
        "ObjectExpression" => all_of(e.get("properties"), |p| {
            p.get("type").and_then(Value::as_str) == Some("Property")
                && (p.get("computed") != Some(&Value::Bool(true))
                    || p.get("key").is_some_and(is_inert))
                && p.get("value").is_some_and(is_inert)
        }),
        "MemberExpression" => {
            e.get("computed") != Some(&Value::Bool(true)) && e.get("object").is_some_and(is_inert)
        }
        "CallExpression" => is_thunk_definition(e),
        _ => false,
    }
}

fn all_of(list: Option<&Value>, test: impl Fn(&Value) -> bool) -> bool {
    list.and_then(Value::as_array)
        .is_some_and(|items| items.iter().all(test))
}

fn is_function(v: &Value) -> bool {
    matches!(
        unwrap_paren(v).get("type").and_then(Value::as_str),
        Some("ArrowFunctionExpression" | "FunctionExpression")
    )
}

/// `helper(fn…)` / `helper({ "path"() {…} })`: an identifier call whose
/// arguments are all functions or objects of functions.
fn is_thunk_definition(call: &Value) -> bool {
    ident_name(call.get("callee")).is_some()
        && call
            .get("arguments")
            .and_then(Value::as_array)
            .is_some_and(|args| {
                !args.is_empty()
                    && args.iter().all(|a| {
                        is_function(a)
                            || (a.get("type").and_then(Value::as_str) == Some("ObjectExpression")
                                && all_of(a.get("properties"), |p| {
                                    p.get("value").is_some_and(is_function)
                                }))
                    })
            })
}

/// The export registrar call a lazy module keeps at its top level:
/// `register(namespace, { name: () => binding, … })`.
pub(crate) fn is_export_registration(stmt: &Value) -> bool {
    if stmt.get("type").and_then(Value::as_str) != Some("ExpressionStatement") {
        return false;
    }
    let Some(call) = stmt.get("expression").map(unwrap_paren) else {
        return false;
    };
    if call.get("type").and_then(Value::as_str) != Some("CallExpression")
        || ident_name(call.get("callee")).is_none()
    {
        return false;
    }
    let Some([target, getters]) = call
        .get("arguments")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
    else {
        return false;
    };
    ident_name(Some(target)).is_some()
        && getters.get("type").and_then(Value::as_str) == Some("ObjectExpression")
        && all_of(getters.get("properties"), |p| {
            p.get("value").map(unwrap_paren).is_some_and(|v| {
                v.get("type").and_then(Value::as_str) == Some("ArrowFunctionExpression")
                    && v.get("params")
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty)
            })
        })
}

/// Can a lazily loaded module's top level hold this statement?
fn is_lazy_module_shaped(stmt: &Value) -> bool {
    match stmt.get("type").and_then(Value::as_str).unwrap_or("") {
        "FunctionDeclaration" | "ClassDeclaration" => true,
        "VariableDeclaration" => all_of(stmt.get("declarations"), |d| {
            d.get("init").is_none_or(|i| i.is_null() || is_inert(i))
        }),
        _ => is_export_registration(stmt),
    }
}

/// `var x = <factory_helper>(…)` for every declarator: a bundled module's
/// factory (vendor code under every split method).
fn is_module_factory_definition(stmt: &Value, factory_helper: Option<&str>) -> bool {
    let Some(helper) = factory_helper else {
        return false;
    };
    stmt.get("type").and_then(Value::as_str) == Some("VariableDeclaration")
        && all_of(stmt.get("declarations"), |d| {
            d.get("init").map(unwrap_paren).is_some_and(|i| {
                i.get("type").and_then(Value::as_str) == Some("CallExpression")
                    && ident_name(i.get("callee")) == Some(helper)
            })
        })
}

/// The wrapper indexes of the lazy-init definitions — each recorded
/// module's last statement — in bundle order.
pub fn lazy_init_indices(body: &[Value]) -> Vec<usize> {
    let mut indices: Vec<usize> = find_init_defs(body, &lazy_init_helper_names(body))
        .into_iter()
        .map(|r| r.index)
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
}

/// [`MarkerCoverage`] of a wrapper body: `spans` parallel to `body` (the
/// byte mass), `factory_helper` the module helper the run's module wrapper
/// grammar recognised (None: no factories to set aside).
pub fn marker_coverage(
    body: &[Value],
    spans: &[(u32, u32)],
    factory_helper: Option<&str>,
) -> MarkerCoverage {
    let inits: HashSet<usize> = lazy_init_indices(body).into_iter().collect();
    let last_init = inits.iter().copied().max();
    let mut c = MarkerCoverage {
        modules: inits.len(),
        ..MarkerCoverage::default()
    };
    for (i, stmt) in body.iter().enumerate() {
        if is_module_factory_definition(stmt, factory_helper) && !inits.contains(&i) {
            continue;
        }
        let bytes = spans
            .get(i)
            .map_or(0, |&(s, e)| u64::from(e.saturating_sub(s)));
        c.app_bytes += bytes;
        if last_init.is_none_or(|last| i > last) {
            c.tail_statements += 1;
        } else if inits.contains(&i) || is_lazy_module_shaped(stmt) {
            c.covered_bytes += bytes;
        } else {
            c.glued_statements += 1;
        }
    }
    c
}

/// A module's cross-version signature (statement-twin.ts :1017): the
/// segment's hashes joined in the SORTED order the extraction produced.
pub fn module_signature(m: &FossilModule) -> String {
    m.hashes.join("|")
}

#[cfg(test)]
mod fossil_test;
