//! The library carry against the REAL TS (test/parity/library-carry.json,
//! frozen by test/parity/library-carry-probe.ts from the TS beautify with
//! the carry armed), plus function-carry.test.ts's fail-loud cases and the
//! join onto the graph.

use oxc_span::Span;
use serde_json::Value;

use super::{
    FunctionLibraryCarry, LibraryClassification, LibraryFunctionKey, carry_function_libraries,
    classify_library_functions, functions_in_tree_order, library_function_rows,
    resolve_function_libraries,
};
use crate::graph::build_unified_graph;
use crate::ingest::{Ingest, program_estree_json};
use crate::libdetect::{CommentRegion, find_comment_regions};

fn vectors() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/parity/library-carry.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn json_of(text: &str) -> Value {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    program_estree_json(ingest.program)
}

fn carry_of(v: &Value) -> FunctionLibraryCarry {
    FunctionLibraryCarry {
        libraries: v["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l.as_str().map(str::to_string))
            .collect(),
        types: v["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect(),
    }
}

fn span_of(v: &Value) -> Span {
    Span::new(
        v["start"].as_u64().unwrap() as u32,
        v["end"].as_u64().unwrap() as u32,
    )
}

/// The ONE walk, over the TS-beautified text, visits exactly the TS's
/// functions in the TS's order (type + babel node span, UTF-8 bytes).
#[test]
fn the_walk_matches_the_ts_walk_on_every_vector() {
    for (name, case) in vectors().as_object().unwrap() {
        let walk = functions_in_tree_order(&json_of(case["beautified"].as_str().unwrap()));
        let expected: Vec<(String, Span)> = case["walk"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| (w["type"].as_str().unwrap().to_string(), span_of(w)))
            .collect();
        let got: Vec<(String, Span)> = walk.into_iter().map(|f| (f.babel_type, f.span)).collect();
        assert_eq!(got, expected, "{name}");
    }
}

/// Resolving the TS's carry on the re-parsed text names the TS's library
/// functions.
#[test]
fn resolve_matches_the_ts_on_every_vector() {
    for (name, case) in vectors().as_object().unwrap() {
        let carry = carry_of(&case["carry"]);
        let got =
            resolve_function_libraries(&json_of(case["beautified"].as_str().unwrap()), &carry)
                .unwrap();
        let expected: Vec<(Span, String)> = case["resolved"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| (span_of(r), r["library"].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(got, expected, "{name}");
    }
}

/// The Rust regions + carry over the RAW parse (an untransformed tree)
/// reproduce the TS's raw-tree classification, and equal the TS's
/// output-tree carry on every vector but the one a transform reorders —
/// which is why phase 5b must carry on the beautify OUTPUT tree.
#[test]
fn raw_tree_carry_matches_the_ts_and_the_reorder_is_the_only_difference() {
    for (name, case) in vectors().as_object().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let regions = find_comment_regions(raw);
        let expected_regions: Vec<CommentRegion> = case["regions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| CommentRegion {
                library_name: r["library"].as_str().unwrap().to_string(),
                start: r["start"].as_u64().unwrap() as usize,
                end: r["end"].as_u64().map(|e| e as usize),
            })
            .collect();
        assert_eq!(regions, expected_regions, "{name}");
        let carry = carry_function_libraries(&json_of(raw), &regions).unwrap();
        let expected: Vec<Option<String>> = case["rawWalk"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["library"].as_str().map(str::to_string))
            .collect();
        assert_eq!(carry.libraries, expected, "{name}");
        let ts_output_carry = carry_of(&case["carry"]);
        assert_eq!(
            carry == ts_output_carry,
            name != "reorder",
            "{name}: raw-tree carry vs the TS output-tree carry"
        );
    }
}

/// function-carry.test.ts: a misaligned re-parse fails loud.
#[test]
fn a_misaligned_reparse_fails_loud() {
    let regions = [CommentRegion {
        library_name: "tinylib".into(),
        start: 0,
        end: None,
    }];
    let carry =
        carry_function_libraries(&json_of("var a = function(){}; var b = () => 1;"), &regions)
            .unwrap();
    let count = resolve_function_libraries(&json_of("var a = function(){};"), &carry).unwrap_err();
    assert!(
        count.contains("2 functions carried across beautify, 1 found"),
        "{count}"
    );
    let ty = resolve_function_libraries(&json_of("var a = () => 1; var b = function(){};"), &carry)
        .unwrap_err();
    assert!(
        ty.contains(
            "function #0 is a FunctionExpression before re-parse and a ArrowFunctionExpression after"
        ),
        "{ty}"
    );
}

/// The consumed classification joins onto the graph by fresh span (graph
/// row order), cross-checks the sessionId, and fails loud on a key that
/// names no function; skipLibraries off or a wrapper classifies nothing;
/// a file with regions but no classification fails when consulted.
#[test]
fn consumed_classification_joins_by_fresh_span() {
    let fresh = "var app = function (a) {\n  return a;\n};\nvar lib = function (z) {\n  return z;\n};\nvar o = {\n  m(q) {\n    return q;\n  }\n};";
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, fresh, "input.js");
    let graph = build_unified_graph(
        ingest.semantic(),
        ingest.program,
        "input.js",
        &[],
        None,
        None,
    );
    let json = program_estree_json(ingest.program);
    let lib_start = fresh.find("function (z)").unwrap() as u32;
    let lib_end = fresh[lib_start as usize..].find('}').unwrap() as u32 + lib_start + 1;
    let m_start = fresh.find("m(q)").unwrap() as u32;
    let m_end = fresh.rfind("}\n}").unwrap() as u32 + 1;
    let row_of = |s: Span| graph.functions.iter().position(|f| f.span == s).unwrap();
    let lib_row = row_of(Span::new(lib_start, lib_end));
    let m_row = row_of(Span::new(m_start, m_end));
    let key = |span: Span, row: usize| LibraryFunctionKey {
        span,
        session_id: graph.functions[row].session_id.clone(),
        library: "tinylib".into(),
    };
    // Keys arrive sorted by span; the result is in graph row order.
    let consumed = LibraryClassification::Consumed(vec![
        key(Span::new(m_start, m_end), m_row),
        key(Span::new(lib_start, lib_end), lib_row),
    ]);
    let got = classify_library_functions(&json, &graph, false, true, Some(&consumed)).unwrap();
    let mut expected = vec![
        (lib_row, "tinylib".to_string()),
        (m_row, "tinylib".to_string()),
    ];
    expected.sort();
    assert_eq!(got, expected);
    assert_eq!(
        library_function_rows(&got, &graph),
        vec![
            key(Span::new(lib_start, lib_end), lib_row),
            key(Span::new(m_start, m_end), m_row)
        ]
    );
    // skipLibraries off / a wrapper: nothing, even a Missing source.
    for (wrapper, skip) in [(true, true), (false, false)] {
        let none = classify_library_functions(
            &json,
            &graph,
            wrapper,
            skip,
            Some(&LibraryClassification::Missing),
        )
        .unwrap();
        assert!(none.is_empty());
    }
    let missing = classify_library_functions(
        &json,
        &graph,
        false,
        true,
        Some(&LibraryClassification::Missing),
    )
    .unwrap_err();
    assert!(
        missing.contains("without a function classification"),
        "{missing}"
    );
    // A key off by one names no function; a wrong sessionId disagrees.
    let off =
        LibraryClassification::Consumed(vec![key(Span::new(lib_start + 1, lib_end), lib_row)]);
    assert!(
        off.classify(&json, &graph)
            .unwrap_err()
            .contains("no function at")
    );
    let mut wrong = key(Span::new(lib_start, lib_end), lib_row);
    wrong.session_id = "input.js:99:0".into();
    let wrong = LibraryClassification::Consumed(vec![wrong]);
    assert!(
        wrong
            .classify(&json, &graph)
            .unwrap_err()
            .contains("in the graph")
    );
}

/// regions.json is written in the TS's key order and bytes (write.test.ts:
/// an open-ended region is `end: null`).
#[test]
fn regions_json_writes_the_ts_bytes() {
    let regions = [
        CommentRegion {
            library_name: "beta".into(),
            start: 30,
            end: None,
        },
        CommentRegion {
            library_name: "alpha".into(),
            start: 9,
            end: Some(30),
        },
    ];
    let functions = [LibraryFunctionKey {
        span: Span::new(22, 40),
        session_id: "input.js:2:10".into(),
        library: "alpha".into(),
    }];
    let text =
        humanify_model::js::stringify(&super::regions_json(&regions, &functions, Vec::new()));
    assert_eq!(
        text,
        r#"{"schemaVersion":1,"commentRegions":[{"span":{"start":9,"end":30},"library":"alpha"},{"span":{"start":30,"end":null},"library":"beta"}],"libraryFunctions":[{"key":{"text":"fresh","start":22,"end":40},"sessionId":"input.js:2:10","library":"alpha"}],"bannerClassifications":[]}"#
    );
}

/// WP5.6c / G3: the native stage 6 carries the TS's OUTPUT-tree carry on
/// every vector and prints the TS's beautified bytes — except `reorder`.
/// That vector's reorder came from the comparison flip swapping a template
/// literal WITH EXPRESSIONS to the right, which ran the other side's code
/// first (a meaning change the formatter no longer makes, fix/format-bugs
/// 2026-09-26): the native format keeps source order there, so its
/// output-tree carry is the raw-tree walk's.
#[test]
fn the_native_format_carries_the_ts_output_tree_carry_on_every_vector() {
    use crate::format::{FormatOptions, format_file};
    for (name, case) in vectors().as_object().unwrap() {
        let raw = case["raw"].as_str().unwrap();
        let regions = find_comment_regions(raw);
        assert!(!regions.is_empty(), "{name}");
        let got = format_file(raw, &FormatOptions::default(), &regions).unwrap();
        if name == "reorder" {
            assert_eq!(
                got.text,
                "`${function (early) {\n  return early;\n}}` === f(function (late) {\n  return late;\n});"
            );
            let raw_carry = carry_function_libraries(&json_of(raw), &regions).unwrap();
            assert_eq!(
                got.library_carry,
                Some(raw_carry),
                "{name}: source order kept"
            );
            continue;
        }
        assert_eq!(got.text, case["beautified"].as_str().unwrap(), "{name}");
        assert_eq!(
            got.library_carry,
            Some(carry_of(&case["carry"])),
            "{name}: carry on the native format's output tree"
        );
    }
}

/// No banner regions, no carry (the TS adds `libraryCarryPlugin` only when
/// the file has regions).
#[test]
fn the_native_format_carries_nothing_without_regions() {
    use crate::format::{FormatOptions, format_file};
    let got = format_file("var a=function(){};", &FormatOptions::default(), &[]).unwrap();
    assert_eq!(got.library_carry, None);
    assert_eq!(got.text, "var a = function () {};");
}
