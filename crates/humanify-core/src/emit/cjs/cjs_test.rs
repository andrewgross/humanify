//! The runnable emit replayed against the REAL TS on every cjs-emit.test.ts
//! fixture and the Babel-shape extras (test/parity/wp53-cjs-probe.ts →
//! wp53-cjs.json): the whole emitted tree byte for byte, the aliases, the
//! emitted layout — or the TS's exact decline reason.

use std::collections::HashMap;

use oxc_allocator::Allocator;
use serde_json::Value;

use super::{RunnableInput, emit_runnable_cjs, wrapper_view};
use crate::emit::align::AlignSwitches;
use crate::emit::load_order::bundle_load_order_facts;
use crate::ingest::Ingest;
use crate::modules::wrapper::find_wrapper_function;
use crate::rename::validated::scopes::BabelScopes;

const VECTORS: &str = include_str!("../../../../../test/parity/wp53-cjs.json");

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|s| s.as_str().expect("string").to_string())
        .collect()
}

/// Run one vector; Err(description) on the first divergence.
fn replay(v: &Value) -> Result<(), String> {
    let code = v["code"].as_str().expect("code");
    let order = strings(&v["order"]);
    let files = strings(&v["files"]);
    let emit_hashes = strings(&v["emitHashes"]);
    let bundle_hashes = strings(&v["bundleHashes"]);
    let prior_aliases: Option<HashMap<String, String>> = v["priorAliases"].as_object().map(|m| {
        m.iter()
            .map(|(k, a)| (k.clone(), a.as_str().expect("alias").to_string()))
            .collect()
    });
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "fixture.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic()).expect("wrapper");
    let view = wrapper_view(ingest.semantic(), wrapper.span).expect("view");
    let scopes = BabelScopes::build(ingest.semantic());
    let input = crate::place::input::split_input(
        code,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )?;
    let lazy = crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild.lazy_init_helpers(&input.body);
    let facts = bundle_load_order_facts(&view.body.statements, &lazy, false);
    let names = vec![None; order.len()];
    let got = emit_runnable_cjs(&RunnableInput {
        layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
        code,
        semantic: ingest.semantic(),
        scopes: &scopes,
        wrapper: &view,
        files: &files,
        order: &order,
        emit_hashes: &emit_hashes,
        emit_names: &[],
        prior_aliases: prior_aliases.as_ref(),
        bundle_hashes: &bundle_hashes,
        bundle_names: &names,
        facts: &facts,
        switches: AlignSwitches::default(),
        forced_exports: &[],
    });
    match (got, v["declined"].as_str()) {
        (Err(rust), Some(ts)) if rust.reason == ts => {
            // Finding #40: what the decline leaves on the persisted ledger
            // — the aliases once the plan is built, the emitted layout once
            // the tree is being assembled.
            let want = &v["declinedLedger"];
            let ts_aliases: Option<Vec<(String, String)>> = want["aliases"].as_array().map(|a| {
                a.iter()
                    .map(|e| (strings(e)[0].clone(), strings(e)[1].clone()))
                    .collect()
            });
            if rust.aliases != ts_aliases {
                return Err(format!(
                    "declined ledger aliases: rust {:?}, ts {ts_aliases:?}",
                    rust.aliases
                ));
            }
            let ts_indexes: Option<Vec<usize>> = want["emitIndexes"].as_array().map(|a| {
                a.iter()
                    .map(|x| x.as_u64().expect("index") as usize)
                    .collect()
            });
            let rust_indexes = rust.layout.as_ref().map(|l| l.emit_indexes.clone());
            if rust_indexes != ts_indexes {
                return Err(format!(
                    "declined ledger emitIndexes: rust {rust_indexes:?}, ts {ts_indexes:?}"
                ));
            }
            Ok(())
        }
        (Err(rust), Some(ts)) => Err(format!("decline reason: rust {:?}, ts {ts:?}", rust.reason)),
        (Err(rust), None) => Err(format!("rust declined ({}), ts emitted", rust.reason)),
        (Ok(_), Some(ts)) => Err(format!("rust emitted, ts declined ({ts})")),
        (Ok(tree), None) => {
            let ts_tree: Vec<(String, String)> = v["tree"]
                .as_array()
                .expect("tree")
                .iter()
                .map(|e| {
                    (
                        e[0].as_str().expect("path").to_string(),
                        e[1].as_str().expect("content").to_string(),
                    )
                })
                .collect();
            if tree.files.len() != ts_tree.len() {
                return Err(format!("{} files, ts {}", tree.files.len(), ts_tree.len()));
            }
            for ((rp, rc), (tp, tc)) in tree.files.iter().zip(&ts_tree) {
                if rp != tp {
                    return Err(format!("file order: rust {rp}, ts {tp}"));
                }
                if rc != tc {
                    return Err(format!("{rp} differs:\n--- rust\n{rc}\n--- ts\n{tc}"));
                }
            }
            let ts_aliases: Vec<(String, String)> = v["aliases"]
                .as_array()
                .expect("aliases")
                .iter()
                .map(|e| {
                    (
                        e[0].as_str().expect("file").to_string(),
                        e[1].as_str().expect("alias").to_string(),
                    )
                })
                .collect();
            if tree.aliases != ts_aliases {
                return Err(format!(
                    "aliases: rust {:?}, ts {ts_aliases:?}",
                    tree.aliases
                ));
            }
            let ts_indexes: Vec<usize> = v["emitIndexes"]
                .as_array()
                .expect("indexes")
                .iter()
                .map(|x| x.as_u64().expect("index") as usize)
                .collect();
            if tree.emit_indexes != ts_indexes {
                return Err(format!(
                    "emitIndexes: rust {:?}, ts {ts_indexes:?}",
                    &tree.emit_indexes[..8.min(tree.emit_indexes.len())]
                ));
            }
            Ok(())
        }
    }
}

/// Emit `code` with statement `i` in `order[i]`, write the tree to a temp
/// dir and run its entry under Node: (exit status, stdout, stderr), plus
/// the tree.
fn emit_and_run(code: &str, order: &[&str]) -> (bool, String, String, Vec<(String, String)>) {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "fixture.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic()).expect("wrapper");
    let view = wrapper_view(ingest.semantic(), wrapper.span).expect("view");
    let scopes = BabelScopes::build(ingest.semantic());
    let input = crate::place::input::split_input(
        code,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    )
    .expect("split input");
    let lazy = crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild.lazy_init_helpers(&input.body);
    let facts = bundle_load_order_facts(&view.body.statements, &lazy, false);
    let order: Vec<String> = order.iter().map(|s| s.to_string()).collect();
    let mut files: Vec<String> = Vec::new();
    for f in &order {
        if !files.contains(f) {
            files.push(f.clone());
        }
    }
    let names = vec![None; order.len()];
    let tree = emit_runnable_cjs(&RunnableInput {
        layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
        code,
        semantic: ingest.semantic(),
        scopes: &scopes,
        wrapper: &view,
        files: &files,
        order: &order,
        emit_hashes: &input.hashes,
        emit_names: &[],
        prior_aliases: None,
        bundle_hashes: &input.hashes,
        bundle_names: &names,
        facts: &facts,
        switches: AlignSwitches::default(),
        forced_exports: &[],
    })
    .unwrap_or_else(|d| panic!("declined: {}", d.reason));
    let dir = std::env::temp_dir().join(format!(
        "humanify-load-order-{}-{}",
        std::process::id(),
        code.len()
    ));
    std::fs::remove_dir_all(&dir).ok();
    for (path, content) in &tree.files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().expect("parent")).unwrap();
        std::fs::write(p, content).unwrap();
    }
    let out = std::process::Command::new("node")
        .arg(dir.join("index.js"))
        .output()
        .expect("node is on PATH (npm run check runs under it)");
    std::fs::remove_dir_all(&dir).ok();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        tree.files,
    )
}

/// Node runs a CommonJS file's requires BEFORE its body. a.js needs c.js
/// only inside a function, but its header requires c.js first; c.js
/// requires b.js, and b.js reads a.js's helper AT LOAD — while a.js is
/// still in its header, its helper unassigned. The load-time-cycle check
/// never saw it (no cycle of load-time reads), and the tree crashed at
/// load (the fresh grouping on the esbuild-kept-factory fixture, main
/// with `--disable fossil-split`). The emit now simulates Node's load
/// order and has a.js load its lazily used files on FIRST USE.
#[test]
fn a_file_read_at_load_while_mid_load_loads_its_lazy_deps_on_first_use() {
    let mut code = String::from("(function (exports, require, module) {\n");
    code.push_str("  var helperH = function () { return 41; };\n");
    code.push_str("  function useC() { return valueC; }\n");
    code.push_str("  var valueB = helperH();\n");
    code.push_str("  var valueC = valueB + 1;\n");
    code.push_str("  console.log(\"value \" + useC());\n");
    let mut order = vec!["a.js", "a.js", "b.js", "c.js", "d.js"];
    for i in 0..55 {
        code.push_str(&format!("  var pad{i:02} = {i};\n"));
        order.push("a.js");
    }
    code.push_str("});\n");
    let (ok, stdout, stderr, files) = emit_and_run(&code, &order);
    assert!(ok, "the tree crashed at load:\n{stderr}");
    assert_eq!(stdout, "value 42\n");
    let a = &files.iter().find(|(p, _)| p == "a.js").expect("a.js").1;
    assert!(
        a.contains("= new Proxy({}, { get: (_, k) => require(\"./c.js\")[k]"),
        "a.js loads c.js on first use:\n{a}"
    );
}

/// No load-order hazard: the requires stay in the header, byte for byte
/// as before (the frozen TS vectors above hold every other shape).
#[test]
fn without_a_load_order_hazard_requires_stay_in_the_header() {
    let mut code = String::from("(function (exports, require, module) {\n");
    code.push_str("  var helperH = function () { return 41; };\n");
    code.push_str("  var valueB = helperH();\n");
    code.push_str("  console.log(\"value \" + (valueB + 1));\n");
    let mut order = vec!["a.js", "b.js", "b.js"];
    for i in 0..55 {
        code.push_str(&format!("  var pad{i:02} = {i};\n"));
        order.push("a.js");
    }
    code.push_str("});\n");
    let (ok, stdout, stderr, files) = emit_and_run(&code, &order);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "value 42\n");
    let b = &files.iter().find(|(p, _)| p == "b.js").expect("b.js").1;
    assert!(
        b.find("require(\"./a.js\")") < b.find("var valueB"),
        "header require:\n{b}"
    );
}

#[test]
fn every_ts_vector_replays_byte_for_byte() {
    let vectors: Vec<Value> = serde_json::from_str(VECTORS).expect("vectors");
    assert!(vectors.len() >= 40, "the probe's fixture set shrank");
    let mut failures = Vec::new();
    for v in &vectors {
        if let Err(e) = replay(v) {
            failures.push(format!("[{}] {e}", v["name"].as_str().unwrap_or("?")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
