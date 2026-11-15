use super::*;
use humanify_ag_model::js::JsValue;

/// A library record is plain serde JSON: two-space indent, keys sorted,
/// non-ASCII as is, a float as serde writes it, one trailing newline.
#[test]
fn text_is_serde_pretty_with_a_newline() {
    let v: serde_json::Value =
        serde_json::from_str(r#"{"b":[1,2.0,{}],"a":"é","e":0.00001,"z":[]}"#).unwrap();
    assert_eq!(
        text(&v),
        "{\n  \"a\": \"é\",\n  \"b\": [\n    1,\n    2.0,\n    {}\n  ],\n  \"e\": 0.00001,\n  \"z\": []\n}\n"
    );
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
