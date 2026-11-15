use super::{JsValue, stringify, stringify_pretty};

fn vectors() -> serde_json::Value {
    let path = format!(
        "{}/../../test/parity/wp41-js-vectors.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// `JSON.stringify(JSON.parse(raw))` as recorded from JavaScript: index
/// keys move to the front, a duplicate key keeps its first slot and its
/// last value.
#[test]
fn stringify_of_a_parsed_value_matches_javascript() {
    for case in vectors()["stringify"].as_array().unwrap() {
        let parsed = JsValue::parse(case["raw"].as_str().unwrap()).unwrap();
        assert_eq!(stringify(&parsed), case["stringified"].as_str().unwrap());
    }
}

#[test]
fn pretty_keeps_empty_containers_inline() {
    let v = JsValue::parse(r#"{"a":[],"b":{},"c":[1,{"d":0.5}]}"#).unwrap();
    assert_eq!(
        stringify_pretty(&v, 2),
        "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    1,\n    {\n      \"d\": 0.5\n    }\n  ]\n}"
    );
}
