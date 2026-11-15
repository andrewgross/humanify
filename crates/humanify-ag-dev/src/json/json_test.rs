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
