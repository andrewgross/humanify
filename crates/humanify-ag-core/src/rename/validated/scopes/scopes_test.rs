//! The Babel scope view against the TS probe (WP3.1).
//!
//! `test/parity/wp31-scope-probe.mjs` runs the REAL @babel/traverse over the
//! corpus in `test/parity/wp31-snippets.mjs` and prints every scope, every
//! binding (in `Object.keys` order — registration order is a decision input
//! downstream), each binding's kind, owner scope, reference paths and
//! constant violations (node type, span, `path.scope`), and the program's
//! globals. Frozen at `test/parity/wp31-scope-view.json`. The Rust view must
//! reproduce it exactly, case for case.

use serde_json::{Value, json};

use crate::rename::validated::scopes::{BScopeId, BabelScopes, Site};
use crate::rename::validated::test_support::with_semantic;

fn scope_key(view: &BabelScopes, id: BScopeId) -> String {
    let s = view.scope(id);
    format!("{}@{}:{}", s.ty.as_str(), s.span.start, s.span.end)
}

fn site_json(view: &BabelScopes, site: &Site) -> Value {
    json!({
        "type": site.ty.as_str(),
        "span": [site.span.start, site.span.end],
        "scope": scope_key(view, site.scope),
    })
}

/// The probe's JSON shape, minus `path` (Babel's node-type name for
/// `binding.path`, which the view keeps as an oxc node).
fn view_json(view: &BabelScopes) -> Value {
    let mut scopes: Vec<Value> = (0..view.scopes.len())
        .map(|i| {
            let id = BScopeId(i as u32);
            let bindings: Vec<Value> = view.initial_maps[i]
                .iter()
                .map(|(name, bid)| {
                    let b = view.binding(*bid);
                    json!({
                        "name": name,
                        "kind": b.kind.as_str(),
                        "owner": scope_key(view, b.owner),
                        "id": [b.id_span.start, b.id_span.end],
                        "refs": b.refs.iter().map(|s| site_json(view, s)).collect::<Vec<_>>(),
                        "violations": b.violations.iter().map(|s| site_json(view, s)).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({
                "block": scope_key(view, id),
                "parent": view.scope(id).parent.map(|p| scope_key(view, p)),
                "bindings": bindings,
            })
        })
        .collect();
    scopes.sort_by(|a, b| a["block"].as_str().cmp(&b["block"].as_str()));
    json!({
        "scopes": scopes,
        "globals": view.globals.iter().collect::<Vec<_>>(),
    })
}

fn strip_paths(mut case: Value) -> Value {
    if let Some(scopes) = case["scopes"].as_array_mut() {
        for scope in scopes {
            if let Some(bindings) = scope["bindings"].as_array_mut() {
                for b in bindings {
                    if let Some(o) = b.as_object_mut() {
                        o.remove("path");
                    }
                }
            }
        }
    }
    json!({ "scopes": case["scopes"], "globals": case["globals"] })
}

/// Every probe case: scopes (block type + span, parent), bindings in
/// `Object.keys` order with kind / owner / identifier span, reference paths
/// and constant violations with their `path.scope`, and the globals.
#[test]
fn scope_view_matches_the_babel_probe() {
    let raw = include_str!("../../../../../../test/parity/wp31-scope-view.json");
    let probe: Value = serde_json::from_str(raw).expect("fixture parses");
    let cases = probe["cases"].as_array().expect("cases");
    assert!(cases.len() >= 60, "the corpus shrank: {}", cases.len());
    let mut failures = Vec::new();
    for case in cases {
        let code = case["code"].as_str().expect("code");
        let module = case["sourceType"] == "module";
        let mine = with_semantic(code, module, |semantic| {
            view_json(&BabelScopes::build(semantic))
        });
        let theirs = strip_paths(case.clone());
        if mine != theirs {
            failures.push(format!(
                "--- {code:?}\n  babel: {}\n  rust:  {}",
                theirs, mine
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases diverge:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
