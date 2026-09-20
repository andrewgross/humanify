//! Fingerprint-index tests (WP2.1). Two layers:
//!
//! 1. PARITY against the frozen TS probe (`test/parity/wp21-probe.mjs`,
//!    frozen at `test/parity/wp21-synthetic-index.json`): the synthetic
//!    bundle's full fingerprints and hash buckets. The structuralHash BYTES
//!    are oxc-vs-babel serializer artifacts (WP1.4's gate established they
//!    differ by design), so every comparison is either a byte-exact
//!    RELATIONAL field (memberKey, features, shapes, two-hop shapes,
//!    shingle counts, external callees) or an equivalence-CLASS check (the
//!    bucket partition, hash presence, callee-hash lengths).
//! 2. UNIT tests for the shared helpers, with the TS behaviors probed
//!    directly (each expectation's probe noted in the test).

use std::collections::{BTreeSet, HashMap};
use std::fs;

use oxc_allocator::Allocator;

use crate::graph::build_unified_graph;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::{
    CalleeShape, CfgType, EdgeNgramMode, FingerprintIndex, FunctionFingerprint, IndexNode,
    SHINGLE_SIMILARITY_FLOOR, StructuralFeatures, build_binding_fingerprint_index,
    build_fingerprint_index, callee_shapes_equal, classify_cfg_type, jaccard_similarity,
};

/// The probe's synthetic bundle, byte-for-byte (the session ids embed line
/// numbers, so the text must be identical).
fn synthetic() -> String {
    let filler: Vec<String> = (0..55).map(|i| format!("var z{i} = {i};")).collect();
    let mut code = String::from("(function(){\n");
    code.push_str(&filler.join("\n"));
    code.push_str(
        r#"
var d1 = "s";
var d2 = "s";
var dep = d1;
var fnRef = leaf;
var uq;
function twin1() { return 9; }
function twin2() { return 9; }
function caller() {
  leaf();
  loopy(3);
}
function leaf() { return console.log("hi", 42); }
function loopy(n) {
  for (let i = 0; i < n; i++) {
    if (i > 2) return i;
  }
  try { leaf(); } catch (e) { other(); }
  return 0;
}
function other() { return "abc".length; }
var api = {
  getCount: function () { return leaf(); },
  loopy2: loopy,
  arrowVal: (x) => x + 1,
};
class Widget {
  render() { return leaf(); }
  compute(a, b) { return a + b; }
}
var throughVar = () => 2;
var wrapper = { viaVar: throughVar };
var assigned;
assigned = () => other();
api.commit = assigned;
if (typeof fetch !== "undefined") { fetch("http://x"); }
var localFetch = 1;
localFetch(leaf);
caller();
})();"#,
    );
    code
}

/// Parse + graph + tables, handed to the assertions inside the scope that
/// owns the arena (the Semantic borrows the allocator, so it cannot leave).
fn with_harness<T>(
    code: &str,
    run: impl FnOnce(&Ingest<'_>, &crate::graph::UnifiedGraph, &SymbolTables) -> T,
) -> T {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let unified = build_unified_graph(
        &ingest.semantic,
        ingest.program,
        "input.js",
        &[],
        None,
        None,
    );
    run(&ingest, &unified, &tables)
}

fn probe_json() -> serde_json::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/parity/wp21-synthetic-index.json"
    );
    serde_json::from_str(&fs::read_to_string(path).expect("probe JSON exists"))
        .expect("probe JSON parses")
}

/// The probe's functions, by session id.
fn probe_functions(probe: &serde_json::Value) -> HashMap<&str, &serde_json::Value> {
    probe["functions"]
        .as_array()
        .expect("functions array")
        .iter()
        .map(|f| (f["sessionId"].as_str().expect("sessionId"), f))
        .collect()
}

fn assert_features_match(ts: &serde_json::Value, rust: &StructuralFeatures, what: &str) {
    assert_eq!(
        rust.arity,
        ts["arity"].as_u64().expect("arity") as u32,
        "{what}: arity"
    );
    assert_eq!(
        rust.has_rest_param,
        ts["hasRestParam"].as_bool().expect("hasRestParam"),
        "{what}: hasRestParam"
    );
    assert_eq!(
        rust.return_count,
        ts["returnCount"].as_u64().expect("returnCount") as u32,
        "{what}: returnCount"
    );
    assert_eq!(
        rust.complexity,
        ts["complexity"].as_u64().expect("complexity") as u32,
        "{what}: complexity"
    );
    assert_eq!(
        rust.cfg_shape,
        ts["cfgShape"].as_str().expect("cfgShape"),
        "{what}: cfgShape"
    );
    assert_eq!(
        rust.loop_count,
        ts["loopCount"].as_u64().expect("loopCount") as u32,
        "{what}: loopCount"
    );
    assert_eq!(
        rust.branch_count,
        ts["branchCount"].as_u64().expect("branchCount") as u32,
        "{what}: branchCount"
    );
    assert_eq!(
        rust.try_count,
        ts["tryCount"].as_u64().expect("tryCount") as u32,
        "{what}: tryCount"
    );
    let strings: Vec<String> = ts["stringLiterals"]
        .as_array()
        .expect("stringLiterals")
        .iter()
        .map(|v| v.as_str().expect("string").to_string())
        .collect();
    assert_eq!(rust.string_literals, strings, "{what}: stringLiterals");
    let numbers: Vec<f64> = ts["numericLiterals"]
        .as_array()
        .expect("numericLiterals")
        .iter()
        .map(|v| v.as_f64().expect("number"))
        .collect();
    assert_eq!(rust.numeric_literals, numbers, "{what}: numericLiterals");
    let calls: Vec<String> = ts["externalCalls"]
        .as_array()
        .expect("externalCalls")
        .iter()
        .map(|v| v.as_str().expect("call").to_string())
        .collect();
    assert_eq!(rust.external_calls, calls, "{what}: externalCalls");
    let accesses: Vec<String> = ts["propertyAccesses"]
        .as_array()
        .expect("propertyAccesses")
        .iter()
        .map(|v| v.as_str().expect("access").to_string())
        .collect();
    assert_eq!(rust.property_accesses, accesses, "{what}: propertyAccesses");
}

fn shapes_of(ts: &serde_json::Value, key: &str) -> Vec<String> {
    ts[key]
        .as_array()
        .expect(key)
        .iter()
        .map(|v| v.as_str().expect("shape").to_string())
        .collect()
}

/// The bucket PARTITION: hash keys are serializer artifacts, but which
/// session ids share a bucket is not.
fn ts_partition(buckets: &serde_json::Value) -> Vec<BTreeSet<String>> {
    let mut groups: Vec<BTreeSet<String>> = buckets
        .as_array()
        .expect("buckets")
        .iter()
        .map(|b| {
            b["members"]
                .as_array()
                .expect("members")
                .iter()
                .map(|m| m.as_str().expect("id").to_string())
                .collect()
        })
        .collect();
    groups.sort();
    groups
}

fn rust_partition(index: &FingerprintIndex<'_>) -> Vec<BTreeSet<String>> {
    let mut groups: Vec<BTreeSet<String>> = index
        .by_structural_hash
        .values()
        .map(|members| {
            members
                .iter()
                .map(|&i| index.entries[i].session_id.clone())
                .collect()
        })
        .collect();
    groups.sort();
    groups
}

/// Row-order entry list (the TS Map order) as session ids.
fn rust_row_order<'a>(index: &'a FingerprintIndex<'a>) -> Vec<&'a str> {
    index
        .entries
        .iter()
        .map(|e| e.session_id.as_str())
        .collect()
}

// ---------------------------------------------------------------------------
// Parity: the function index
// ---------------------------------------------------------------------------

#[test]
fn function_index_matches_the_ts_probe() {
    let probe = probe_json();
    let ts_rows = probe_functions(&probe);
    let code = synthetic();
    with_harness(&code, |ingest, unified, tables| {
        let index = build_fingerprint_index(unified, &ingest.semantic, tables);

        // ROW ORDER: the TS iterates its functions Map in build order; the
        // Rust entries are in graph.functions order. If oxc's pre-order walk
        // ever diverges from babel's traverse order, this is the test that
        // catches it.
        let ts_order: Vec<&str> = probe["functions"]
            .as_array()
            .expect("functions")
            .iter()
            .map(|f| f["sessionId"].as_str().expect("sessionId"))
            .collect();
        assert_eq!(
            rust_row_order(&index),
            ts_order,
            "row order must match the TS build order"
        );

        for (i, entry) in index.entries.iter().enumerate() {
            let ts = ts_rows[entry.session_id.as_str()];
            let fp = &entry.fingerprint;
            let what = entry.session_id.as_str();

            // memberKey: byte-exact.
            assert_eq!(
                fp.member_key(),
                ts["memberKey"].as_str(),
                "{what}: memberKey (row {i})"
            );
            // features: byte-exact in every field.
            assert_features_match(
                &ts["features"],
                fp.features().expect("function features"),
                what,
            );
            // callee/caller shapes + two-hop shapes: serialized strings,
            // byte-exact.
            assert_eq!(
                fp.callee_shapes()
                    .iter()
                    .map(CalleeShape::serialized)
                    .collect::<Vec<_>>(),
                shapes_of(ts, "calleeShapes"),
                "{what}: calleeShapes"
            );
            assert_eq!(
                fp.caller_shapes()
                    .iter()
                    .map(CalleeShape::serialized)
                    .collect::<Vec<_>>(),
                shapes_of(ts, "callerShapes"),
                "{what}: callerShapes"
            );
            assert_eq!(
                fp.two_hop_shapes(),
                shapes_of(ts, "twoHopShapes"),
                "{what}: twoHopShapes"
            );
            // calleeHashes: the BYTES are serializer artifacts — the COUNT is
            // not (and the sorted equality structure is exercised by the
            // buckets, which the hashes build).
            assert_eq!(
                fp.callee_hashes().len(),
                ts["calleeHashes"].as_array().expect("calleeHashes").len(),
                "{what}: calleeHashes count"
            );
            // shingles: counts must agree (contents embed the hash bytes).
            let shingles = index.compute_shingle_set(i);
            assert_eq!(
                shingles.len(),
                ts["shingleCount"].as_u64().expect("shingleCount") as usize,
                "{what}: shingleCount"
            );
            // graph fields, as sorted sets (the TS prints its Set order; the
            // Rust sorts spans by design — graph.rs's row doc).
            let internal: BTreeSet<String> = ts["internalCallees"]
                .as_array()
                .expect("internalCallees")
                .iter()
                .map(|v| v.as_str().expect("id").to_string())
                .collect();
            let rust_internal: BTreeSet<String> = unified.functions[i]
                .internal_callees
                .iter()
                .map(|span| {
                    unified
                        .functions
                        .iter()
                        .find(|f| f.span == *span)
                        .map(|f| f.session_id.clone())
                        .expect("callee is a row")
                })
                .collect();
            assert_eq!(rust_internal, internal, "{what}: internalCallees as a set");
            let external: BTreeSet<&str> = ts["externalCallees"]
                .as_array()
                .expect("externalCallees")
                .iter()
                .map(|v| v.as_str().expect("name"))
                .collect();
            let rust_external: BTreeSet<&str> = unified.functions[i]
                .external_callees
                .iter()
                .map(String::as_str)
                .collect();
            assert_eq!(rust_external, external, "{what}: externalCallees as a set");
        }

        // BUCKETS: the partition must match.
        assert_eq!(
            rust_partition(&index),
            ts_partition(&probe["functionBuckets"]),
            "function bucket partition"
        );
        // The twins share a bucket (the probe's only multi-member function
        // bucket), and the bucket's members are in row order.
        let twins: Vec<&str> = index
            .by_structural_hash
            .values()
            .map(|m| m.as_slice())
            .find(|m| m.len() > 1)
            .map(|m| {
                m.iter()
                    .map(|&i| index.entries[i].session_id.as_str())
                    .collect()
            })
            .expect("the twins bucket exists");
        assert_eq!(
            twins,
            vec!["input.js:62:0", "input.js:63:0"],
            "twins bucket members"
        );
    });
}

// ---------------------------------------------------------------------------
// Parity: the binding index
// ---------------------------------------------------------------------------

#[test]
fn binding_index_matches_the_ts_probe() {
    let probe = probe_json();
    let ts_bindings: Vec<&serde_json::Value> = probe["bindings"]
        .as_array()
        .expect("bindings")
        .iter()
        .collect();
    let code = synthetic();
    with_harness(&code, |ingest, unified, tables| {
        let index = build_binding_fingerprint_index(unified, &ingest.semantic, tables);

        // ROW ORDER + coverage: one entry per hashable binding, in
        // module_bindings order.
        let expected_order: Vec<&str> = ts_bindings
            .iter()
            .filter(|b| b["indexed"].as_bool().expect("indexed"))
            .map(|b| b["sessionId"].as_str().expect("sessionId"))
            .collect();
        assert_eq!(
            rust_row_order(&index),
            expected_order,
            "binding entries: hashable rows only, in build order"
        );

        // Unhashable rows are EXCLUDED from the index (uq has a bare
        // declarator — the probe's only unhashable row).
        assert!(
            !index.entries.iter().any(|e| e.session_id == "module:uq"),
            "unhashable bindings are excluded"
        );
        assert_eq!(
            index.entries.len(),
            ts_bindings.len() - 1,
            "one excluded row"
        );

        let ts_by_id: HashMap<&str, &serde_json::Value> = ts_bindings
            .iter()
            .map(|b| (b["sessionId"].as_str().unwrap(), *b))
            .collect();

        for entry in &index.entries {
            let ts = ts_by_id[entry.session_id.as_str()];
            let fp = &entry.fingerprint;
            let what = entry.session_id.as_str();

            assert_eq!(fp.kind(), "binding", "{what}: kind");
            assert!(
                fp.member_key().is_none(),
                "{what}: bindings carry no memberKey"
            );
            assert!(
                fp.features().is_none(),
                "{what}: bindings carry no features"
            );
            // The structural hash is PRESENT (bytes are serializer artifacts).
            assert!(
                !fp.structural_hash().is_empty(),
                "{what}: structuralHash present"
            );
            assert_eq!(
                fp.callee_shapes()
                    .iter()
                    .map(CalleeShape::serialized)
                    .collect::<Vec<_>>(),
                shapes_of(ts, "calleeShapes"),
                "{what}: calleeShapes"
            );
            assert_eq!(
                fp.caller_shapes()
                    .iter()
                    .map(CalleeShape::serialized)
                    .collect::<Vec<_>>(),
                shapes_of(ts, "callerShapes"),
                "{what}: callerShapes"
            );
            assert_eq!(
                fp.two_hop_shapes(),
                shapes_of(ts, "twoHopShapes"),
                "{what}: twoHopShapes"
            );
            assert_eq!(
                fp.callee_hashes().len(),
                ts["calleeHashes"].as_array().expect("calleeHashes").len(),
                "{what}: calleeHashes count"
            );
        }

        // BUCKETS: the partition must match (this is where the twins d1/d2 and
        // the z1/localFetch, dep/fnRef collisions live).
        assert_eq!(
            rust_partition(&index),
            ts_partition(&probe["bindingBuckets"]),
            "binding bucket partition"
        );
    });
}

// ---------------------------------------------------------------------------
// Parity: the shared helpers
// ---------------------------------------------------------------------------

#[test]
fn shingle_floor_and_self_similarity_match_the_ts_probe() {
    let probe = probe_json();
    assert_eq!(
        SHINGLE_SIMILARITY_FLOOR,
        probe["shingleSimilarityFloor"].as_f64().unwrap()
    );
    // The probe's selfSimilarity: jaccardSimilarity(set, set) === 1.
    let code = synthetic();
    with_harness(&code, |ingest, unified, tables| {
        let index = build_fingerprint_index(unified, &ingest.semantic, tables);
        let shingles = index.compute_shingle_set(0);
        assert_eq!(
            jaccard_similarity(&shingles, &shingles),
            probe["selfSimilarity"].as_f64().unwrap()
        );
    });
}

// ---------------------------------------------------------------------------
// Unit: callee shapes (probe: serializeCalleeShape/classifyCfgType)
// ---------------------------------------------------------------------------

#[test]
fn callee_shape_serialization_and_classification() {
    let features = StructuralFeatures {
        arity: 3,
        complexity: 12,
        loop_count: 2,
        branch_count: 1,
        external_calls: vec!["console.log".to_string()],
        ..StructuralFeatures::default()
    };
    let shape = CalleeShape::of_features(&features);
    assert_eq!(
        shape.cfg_type,
        CfgType::Complex,
        "loop>0 && branch>0 -> complex"
    );
    assert!(shape.has_external_calls);
    assert_eq!(shape.serialized(), "(3,12,complex,true)");

    // linear / branching / looping
    assert_eq!(
        CalleeShape::of_features(&StructuralFeatures::default()).serialized(),
        "(0,0,linear,false)"
    );
    let branching = StructuralFeatures {
        branch_count: 2,
        ..StructuralFeatures::default()
    };
    assert_eq!(
        CalleeShape::of_features(&branching).cfg_type,
        CfgType::Branching
    );
    let looping = StructuralFeatures {
        loop_count: 1,
        ..StructuralFeatures::default()
    };
    assert_eq!(
        CalleeShape::of_features(&looping).cfg_type,
        CfgType::Looping
    );
    assert_eq!(classify_cfg_type(&branching), CfgType::Branching);

    // calleeShapesEqual: order-insensitive equality (TS sorts both sides).
    let a = [
        CalleeShape {
            arity: 1,
            complexity: 1,
            cfg_type: CfgType::Linear,
            has_external_calls: false,
        },
        CalleeShape {
            arity: 0,
            complexity: 4,
            cfg_type: CfgType::Complex,
            has_external_calls: true,
        },
    ];
    let b = [
        CalleeShape {
            arity: 0,
            complexity: 4,
            cfg_type: CfgType::Complex,
            has_external_calls: true,
        },
        CalleeShape {
            arity: 1,
            complexity: 1,
            cfg_type: CfgType::Linear,
            has_external_calls: false,
        },
    ];
    assert!(callee_shapes_equal(&a, &b));
    let c = [a[0]];
    assert!(!callee_shapes_equal(&a, &c), "length mismatch");
    let d = [
        CalleeShape {
            arity: 1,
            complexity: 1,
            cfg_type: CfgType::Linear,
            has_external_calls: false,
        },
        CalleeShape {
            arity: 0,
            complexity: 4,
            cfg_type: CfgType::Complex,
            has_external_calls: false,
        },
    ];
    assert!(!callee_shapes_equal(&a, &d), "external-calls flag differs");
}

// ---------------------------------------------------------------------------
// Unit: features (TS probe: test/parity/wp21-features-scratch.mts runs,
// captured 2026-09-20 — each assertion's TS value in the comment)
// ---------------------------------------------------------------------------

fn features_of(code: &str) -> StructuralFeatures {
    with_harness(code, |ingest, unified, tables| {
        let index = build_fingerprint_index(unified, &ingest.semantic, tables);
        index.features[0].clone()
    })
}

/// `function f() { ... }` — the only function row.
#[test]
fn features_directives_are_not_string_literals() {
    // TS probe "directive:": sl [] nl [1] rc 1 cfg "ret"
    let f = features_of("function f() { 'use strict'; return 1; }");
    assert!(
        f.string_literals.is_empty(),
        "a directive is not a string literal"
    );
    assert_eq!(f.numeric_literals, vec![1.0]);
    assert_eq!(f.return_count, 1);
    assert_eq!(f.cfg_shape, "ret");
    // TS probe "midstring:": a mid-body string statement IS counted
    let f = features_of("function f() { x(); 'str'; return 1; }");
    assert_eq!(f.string_literals, vec!["str"]);
}

#[test]
fn features_optional_chain_hops_are_excluded() {
    // TS probe "optional:": sl ["b"] (the computed `a['b']` key) pa [".b"]
    let f = features_of("function f() { a?.b; a?.b(); a.b; a['b']; a[b]; (a?.b)(); x?.f().g; }");
    assert_eq!(f.property_accesses, vec![".b"]);
    assert_eq!(f.string_literals, vec!["b"]);
    assert!(f.external_calls.is_empty());
}

#[test]
fn features_optional_chain_spine_rule() {
    // TS probe "spine:": pa [".c",".h"] — the arg `g().h` and the
    // paren-broken `(a?.b).c` COUNT; the chain hops `y?.z.w` and
    // `a?.b?.c` do not.
    let f = features_of("function f() { x?.f(g().h); y?.z.w; (a?.b).c; a?.b?.c; }");
    assert_eq!(f.property_accesses, vec![".c", ".h"]);
}

#[test]
fn features_if_switch_complexity() {
    // TS probe "control:": cc 5 bc 2 nl [1] cfg "if-switch-case-break-default"
    let f = features_of(
        "function f() { if (a ?? b) { c && d; } switch (x) { case 1: y(); break; default: z(); } }",
    );
    assert_eq!(f.complexity, 5);
    assert_eq!(f.branch_count, 2);
    assert_eq!(f.numeric_literals, vec![1.0]);
    assert_eq!(f.cfg_shape, "if-switch-case-break-default");
}

#[test]
fn features_loops_and_rest() {
    // TS probe "rest:": cc 3 lc 2 hasRest cfg "loop-do"
    let f = features_of("function f(a, ...rest) { for (const x of y) {} do { x(); } while (c); }");
    assert_eq!(f.complexity, 3);
    assert_eq!(f.loop_count, 2);
    assert!(f.has_rest_param);
    assert_eq!(f.arity, 2);
    assert_eq!(f.cfg_shape, "loop-do");
}

#[test]
fn features_try_finally_throw() {
    // TS probe "try:": tc 1 sl ["x"] cfg "try-catch-finally-throw"
    // (`new Error` is a NewExpression — never an external call).
    let f = features_of(
        "function f() { try { a(); } catch (e) { b(); } finally { c(); } throw new Error('x'); }",
    );
    assert_eq!(f.try_count, 1);
    assert_eq!(f.string_literals, vec!["x"]);
    assert!(f.external_calls.is_empty());
    assert_eq!(f.cfg_shape, "try-catch-finally-throw");
}

#[test]
fn features_if_else_returns() {
    // TS probe "ifelse:": cc 3 bc 1 rc 2 cfg "loop-if-ret-else-ret"
    let f = features_of("function f() { while (x) { if (y) return; else return; } }");
    assert_eq!(f.complexity, 3);
    assert_eq!(f.branch_count, 1);
    assert_eq!(f.return_count, 2);
    assert_eq!(f.cfg_shape, "loop-if-ret-else-ret");
}

#[test]
fn features_parens_and_member_callees() {
    // TS probe "parens:": ec ["*.b"] pa [".b"]
    let f = features_of("function f() { ((g))(); ((a)).b(); }");
    assert_eq!(f.external_calls, vec!["*.b"]);
    assert_eq!(f.property_accesses, vec![".b"]);

    // TS probe "globals:": ec ["*.map","*.method","*.write","fetch"]
    // (fetch free, Bun NOT a known global on the features set)
    let f = features_of(
        "function f() { fetch('a'); Bun.write('b'); localFetch('c'); obj.method(); arr.map(fn); }",
    );
    assert_eq!(
        f.external_calls,
        vec!["*.map", "*.method", "*.write", "fetch"]
    );
    assert_eq!(f.string_literals, vec!["a", "b", "c"]);

    // The bound-aware walk (the graph path): a BOUND object named like a
    // known global must not leak its name (TS doc :233-236; the TS
    // node-only default keeps it — the Rust index path is always
    // bound-aware).
    let f = features_of("var fetch = 1; function f() { fetch('a'); obj.fetch(); }");
    assert_eq!(
        f.external_calls,
        vec!["*.fetch"],
        "the bound `fetch` is excluded"
    );
}

#[test]
fn features_bodies_and_labels() {
    // TS probe "arrowexpr:": nl [2,3] cfg "expr"
    let f = features_of("const f = () => 2 + 3;");
    assert_eq!(f.numeric_literals, vec![2.0, 3.0]);
    assert_eq!(f.cfg_shape, "expr");
    assert_eq!(f.arity, 0);

    // TS probe "arrowblock:": cfg "empty" (a block body, not "expr")
    let f = features_of("const f = () => {};");
    assert_eq!(f.cfg_shape, "empty");

    // TS probe "label:": lc 1 cc 2 cfg "empty" (the labeled for is not a
    // direct statement of the body)
    let f = features_of("function f() { label: for (;;) { break label; } }");
    assert_eq!(f.loop_count, 1);
    assert_eq!(f.complexity, 2);
    assert_eq!(f.cfg_shape, "empty");

    // TS probe "nestedloop:": nl [0,3] lc 2 cfg "loop-loop-cont"
    let f = features_of(
        "function f() { for (var i = 0; i < 3; i++) { for (const k in o) { continue; } } }",
    );
    assert_eq!(f.numeric_literals, vec![0.0, 3.0]);
    assert_eq!(f.loop_count, 2);
    assert_eq!(f.cfg_shape, "loop-loop-cont");

    // TS probe "dedupe:": Set semantics — sl ["dup"], ec ["console.log"]
    let f = features_of("function f() { console.log('dup', 'dup'); }");
    assert_eq!(f.string_literals, vec!["dup"]);
    assert_eq!(f.external_calls, vec!["console.log"]);
}

// ---------------------------------------------------------------------------
// Unit: member keys (TS probe: wp21 memberKey probe, 2026-09-20)
// ---------------------------------------------------------------------------

fn member_key_of(code: &str) -> Option<String> {
    with_harness(code, |ingest, unified, tables| {
        let index = build_fingerprint_index(unified, &ingest.semantic, tables);
        index.entries[0]
            .fingerprint
            .member_key()
            .map(str::to_string)
    })
}

#[test]
fn member_key_matches_the_ts_probes() {
    // zustand shape: one reference, one key
    assert_eq!(
        member_key_of(
            "var state = 1;\nvar store = { getState: () => state };\nstore.getState();\n"
        ),
        Some("getState".to_string())
    );
    // a variable used under TWO keys is a contradiction -> None
    assert_eq!(
        member_key_of(
            "var h = () => 1;\nvar a = { alpha: h };\nvar b = { beta: h };\na.alpha();\n"
        ),
        None
    );
    // the assignment form still recovers the key
    assert_eq!(
        member_key_of(
            "var confirmHandler;\nconfirmHandler = () => 1;\nvar ui = { onConfirm: confirmHandler };\nui.onConfirm();\n"
        ),
        Some("onConfirm".to_string())
    );
    // a computed key reads a binding's current name -> None
    assert_eq!(
        member_key_of("var k = 'dyn';\nvar o = { [k]: () => 1 };\no.dyn();\n"),
        None
    );
    // TWO hops of indirection are unread
    assert_eq!(
        member_key_of("var mid = () => 1;\nvar held = mid;\nvar o = { via: held };\no.via();\n"),
        None
    );
    // obj.foo = function(){}
    assert_eq!(
        member_key_of("var obj = {};\nobj.foo = function () { return 1; };\nobj.foo();\n"),
        Some("foo".to_string())
    );
    // a paren-wrapped value (oxc keeps the parens; babel drops them)
    assert_eq!(
        member_key_of("var o = { f: (function () { return 1; }) };\no.f();\n"),
        Some("f".to_string())
    );
}

// ---------------------------------------------------------------------------
// Unit: shingles
// ---------------------------------------------------------------------------

#[test]
fn shingles_and_jaccard() {
    // jaccardSimilarity: |A∩B|/|A∪B|; both empty = 1 (TS :384-394).
    let empty: BTreeSet<String> = BTreeSet::new();
    assert_eq!(jaccard_similarity(&empty, &empty), 1.0);
    let a: BTreeSet<String> = ["x", "y", "z"].iter().map(|s| s.to_string()).collect();
    let b: BTreeSet<String> = ["y", "z", "w"].iter().map(|s| s.to_string()).collect();
    assert_eq!(jaccard_similarity(&a, &b), 2.0 / 4.0);
    assert_eq!(jaccard_similarity(&a, &empty), 0.0);

    // Edge n-grams: exact mode embeds the callee hash, blurred the shape;
    // one per internal callee (TS :319-333).
    let code = "function callee() { return 1; }\nfunction callerFn() { callee(); }\ncallerFn();\n";
    with_harness(code, |ingest, unified, tables| {
        let index = build_fingerprint_index(unified, &ingest.semantic, tables);
        let callee_idx = 0; // row order: callee first
        let caller_idx = 1;
        assert_eq!(unified.functions[callee_idx].name, "callee");
        assert_eq!(unified.functions[caller_idx].name, "callerFn");
        let exact = index.compute_edge_ngrams(caller_idx, EdgeNgramMode::Exact);
        assert_eq!(exact.len(), 1);
        assert!(exact[0].starts_with(&format!(
            "{}→",
            unified.functions[caller_idx].structural_hash
        )));
        assert!(exact[0].ends_with(&unified.functions[callee_idx].structural_hash));
        let blurred = index.compute_edge_ngrams(caller_idx, EdgeNgramMode::Blurred);
        assert_eq!(
            blurred[0],
            format!(
                "{}→(0,1,linear,false)",
                unified.functions[caller_idx].structural_hash
            )
        );

        // The shingle set = blurred n-grams + feature tokens (str:/ext:/prop:).
        let shingles = index.compute_shingle_set(caller_idx);
        assert!(shingles.contains(&blurred[0]));
    });
}

// ---------------------------------------------------------------------------
// Unit: the index surface
// ---------------------------------------------------------------------------

#[test]
fn index_entry_lookup_and_kinds() {
    let code = synthetic();
    with_harness(&code, |ingest, unified, tables| {
        let index = build_fingerprint_index(unified, &ingest.semantic, tables);

        // Every function entry is a Function node with a function-side
        // fingerprint carrying features.
        assert_eq!(index.entries.len(), unified.functions.len());
        for (i, entry) in index.entries.iter().enumerate() {
            assert_eq!(entry.node, IndexNode::Function(i));
            assert!(matches!(
                entry.fingerprint,
                FunctionFingerprint::Function(_)
            ));
        }

        // entry_of_span answers by span identity.
        for (i, f) in unified.functions.iter().enumerate() {
            assert_eq!(index.entry_of_span(f.span), Some(i));
        }

        // The binding index holds ONLY binding entries.
        let binding_index = build_binding_fingerprint_index(unified, &ingest.semantic, tables);
        assert!(
            binding_index
                .entries
                .iter()
                .all(|e| matches!(e.node, IndexNode::Binding(_)))
        );
        // bucket() answers the uniqueHash tier's pool.
        let hash = index.entries[0].fingerprint.structural_hash().to_string();
        assert!(index.bucket(&hash).is_some());
        assert!(
            index.bucket("no-such-hash").is_none()
                || index.bucket("no-such-hash") == Some(&[] as &[usize])
        );
    });
}
