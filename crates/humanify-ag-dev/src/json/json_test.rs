use std::collections::BTreeMap;

use super::py::{PyValue, dumps, float_repr};
use super::*;
use humanify_ag_model::js::JsValue;

/// Recorded from CPython 3 `json.dumps(v)` on this box.
#[test]
fn python_float_repr_matches_cpython() {
    let cases: &[(f64, &str)] = &[
        (0.0, "0.0"),
        (-0.0, "-0.0"),
        (1.0, "1.0"),
        (100.0, "100.0"),
        (1.5, "1.5"),
        (0.1, "0.1"),
        (0.0001, "0.0001"),
        (0.00001, "1e-05"),
        (1e16, "1e+16"),
        (1e15, "1000000000000000.0"),
        (123456789012345678.0, "1.2345678901234568e+17"),
        (1.2345e-7, "1.2345e-07"),
        (-2.5e-10, "-2.5e-10"),
        (2.71, "2.71"),
        (1.0 / 3.0, "0.3333333333333333"),
        (9007199254740992.0, "9007199254740992.0"),
        (1e22, "1e+22"),
        (5e-324, "5e-324"),
        (1.7976931348623157e308, "1.7976931348623157e+308"),
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
    ];
    for (x, want) in cases {
        assert_eq!(float_repr(*x), *want, "repr({x:e})");
    }
}

/// Recorded from CPython 3 `json.dumps(..., sort_keys=True, indent=1,
/// ensure_ascii=True)`.
#[test]
fn python_dumps_matches_cpython() {
    let want = "{\n \"a\": \"\\u00e9\\ud83d\\ude00\\u007f\\n\\\"\",\n \"b\": [\n  1,\n  2.0,\n  {}\n ],\n \"c\": [],\n \"d\": {\n  \"y\": true,\n  \"z\": null\n }\n}";
    let v = PyValue::parse(want).unwrap();
    assert_eq!(dumps(&v), want);
    // Keys arrive unsorted; ints and floats stay apart; the last duplicate wins.
    let v = PyValue::parse(r#"{"b": 1.0, "a": 1, "b": 2.50}"#).unwrap();
    assert_eq!(dumps(&v), "{\n \"a\": 1,\n \"b\": 2.5\n}");
}

#[test]
fn js_spelling_is_json_stringify_with_two_spaces() {
    let v = JsValue::parse(r#"{"b":[1,2.0,{}],"a":"é","1":null}"#).unwrap();
    // Index keys first (JS object order), 2.0 prints as 2, non-ASCII literal.
    assert_eq!(
        js_text(&v, true),
        "{\n  \"1\": null,\n  \"b\": [\n    1,\n    2,\n    {}\n  ],\n  \"a\": \"é\"\n}\n"
    );
}

#[test]
fn a_spelling_is_detected_with_its_trailing_newline() {
    let js = "{\n  \"a\": 1\n}\n";
    assert_eq!(
        spelling_of(js),
        Some(Written {
            spelling: Spelling::Js,
            trailing_newline: true
        })
    );
    let py = "{\n \"a\": 1.0\n}";
    assert_eq!(
        spelling_of(py),
        Some(Written {
            spelling: Spelling::Python,
            trailing_newline: false
        })
    );
    assert_eq!(
        spelling_of("{\"a\":1}").map(|w| w.spelling),
        Some(Spelling::JsCompact)
    );
    // None: odd spacing, or unsorted keys at indent 1.
    assert_eq!(spelling_of("{ \"a\":1}"), None);
    assert_eq!(spelling_of("{\n \"b\": 1,\n \"a\": 2\n}"), None);
}

/// prettier keeps an object the original expanded, collapses one that
/// fits on a line, and breaks an array that does not fit (the shapes of
/// the committed `*-boot.json`, `self-hop.json` and `*.stats.json`).
#[test]
fn prettier_reflow_matches_the_committed_shapes() {
    let boot = "{\n  \"boot\": { \"version\": \"2.1.86 (Claude Code)\", \"prompt\": \"skipped\", \"ok\": true }\n}\n";
    let v = JsValue::parse(boot).unwrap();
    assert_eq!(
        prettier::reflow(&v, &prettier::expanded_objects(boot)),
        boot
    );
    let hop =
        "{ \"selfHop\": { \"version\": \"2.1.216\", \"identical\": true, \"diffLines\": 0 } }\n";
    let v = JsValue::parse(hop).unwrap();
    assert_eq!(prettier::reflow(&v, &prettier::expanded_objects(hop)), hop);
    // The same value, originally expanded, stays expanded at the top.
    assert_eq!(
        prettier::reflow(&v, &[true, false]),
        "{\n  \"selfHop\": { \"version\": \"2.1.216\", \"identical\": true, \"diffLines\": 0 }\n}\n"
    );
    let long = JsValue::parse(
        r#"{"names":["initializeModule_","initialize_","initializeApp_","another_long_name_"]}"#,
    )
    .unwrap();
    assert_eq!(
        prettier::reflow(&long, &[true]),
        "{\n  \"names\": [\n    \"initializeModule_\",\n    \"initialize_\",\n    \"initializeApp_\",\n    \"another_long_name_\"\n  ]\n}\n"
    );
}

/// Recorded from the repo's prettier (`npx prettier --parser json`) on a
/// `JSON.stringify(x, null, 2)` file: a number array FILLS its lines, an
/// array of objects breaks, an expanded object breaks its array.
#[test]
fn prettier_reflow_matches_recorded_prettier_output() {
    let want = "{\n  \"nums\": [\n    0, 137, 274, 411, 548, 685, 822, 959, 1096, 1233, 1370, 1507, 1644, 1781,\n    1918, 2055, 2192, 2329, 2466, 2603, 2740, 2877, 3014, 3151, 3288, 3425,\n    3562, 3699, 3836, 3973, 4110, 4247, 4384, 4521, 4658, 4795, 4932, 5069,\n    5206, 5343\n  ],\n  \"objs\": [\n    {\n      \"a\": 1,\n      \"b\": 2\n    },\n    {\n      \"a\": 3,\n      \"b\": 4\n    }\n  ],\n  \"one\": [\n    {\n      \"a\": 1\n    }\n  ],\n  \"deep\": {\n    \"x\": {\n      \"y\": {\n        \"z\": [1, 2]\n      }\n    }\n  }\n}\n";
    let v = JsValue::parse(want).unwrap();
    let original = js_text(&v, true);
    assert_eq!(
        prettier::reflow(&v, &prettier::expanded_objects(&original)),
        want
    );
    assert_eq!(
        spelling_of(want).map(|w| w.spelling),
        Some(Spelling::Prettier)
    );
}

/// Every committed card, summary and JSON record under the two results
/// trees round-trips byte for byte through one of the two spellings — the
/// harness can read every legacy label and write a file the TS / Python
/// instruments would have written.
#[test]
fn every_committed_results_json_round_trips_byte_for_byte() {
    let root = crate::paths::repo_root();
    let listed = crate::proc::git(
        &root,
        &[
            "ls-files",
            "--",
            crate::paths::EVAL_RESULTS,
            crate::paths::LIBRARY_RESULTS,
        ],
    )
    .expect("git ls-files");
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for rel in listed.lines().filter(|l| l.ends_with(".json")) {
        let bytes = std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        let text = String::from_utf8(bytes).unwrap_or_else(|_| panic!("{rel}: not UTF-8"));
        match spelling_of(&text) {
            Some(w) => {
                let tree = if rel.starts_with(crate::paths::EVAL_RESULTS) {
                    "eval"
                } else {
                    "library"
                };
                *counts
                    .entry(format!(
                        "{tree} {:?} newline={}",
                        w.spelling, w.trailing_newline
                    ))
                    .or_default() += 1;
            }
            None => failures.push(rel.to_string()),
        }
    }
    let total: usize = counts.values().sum();
    eprintln!("round-tripped {total} files: {counts:#?}");
    assert!(
        failures.is_empty(),
        "{} of {} committed .json files match neither spelling:\n  {}",
        failures.len(),
        total + failures.len(),
        failures
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(total > 28_000, "the walk found only {total} files");
}
