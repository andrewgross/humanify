//! function-carry.test.ts's fail-loud cases and the join onto the graph.
//! (The frozen library-carry.json probe — the TS beautify's recorded walk,
//! regions and classification — was retired 2026-09-28; the region and
//! classification rules themselves are covered natively by libdetect_test.)

use oxc_span::Span;
use serde_json::Value;

use super::{
    LibraryClassification, LibraryFunctionKey, carry_function_libraries,
    classify_library_functions, library_function_rows, resolve_function_libraries,
};
use crate::graph::build_unified_graph;
use crate::ingest::{Ingest, program_estree_json};
use crate::libdetect::CommentRegion;

fn json_of(text: &str) -> Value {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, text);
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    program_estree_json(ingest.program)
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
        crate::rename::eligibility::NeverRename::UNIVERSAL,
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

/// No banner regions, no carry (the TS adds `libraryCarryPlugin` only when
/// the file has regions).
#[test]
fn the_native_format_carries_nothing_without_regions() {
    use crate::format::{FormatOptions, format_file};
    let got = format_file("var a=function(){};", &FormatOptions::default(), &[]).unwrap();
    assert_eq!(got.library_carry, None);
    assert_eq!(got.text, "var a = function () {};");
}
