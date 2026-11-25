use std::collections::BTreeMap;

use humanify_ag_core::layout_record::{Layout, checksum};
use serde_json::{Value, json};

use super::{Files, check};

fn layout(files: Value) -> String {
    json!({
        "format": 1, "inputs": ["a.js", "b.js"], "priorLayout": null, "namedText": "out.js",
        "files": files, "priorAmbiguous": [],
    })
    .to_string()
}

fn good() -> Value {
    json!({"out.js": {"units": [
        {"at": [0, 0], "line": 1, "rows": [[1, 0, "new"], [2, 13], [2, 19, "new"]]},
        {"at": [1, null], "linker": "init", "line": 4, "rows": [[4, "init+9", "new"]],
         "body": [{"at": [1, null], "linker": "call", "target": 0, "line": 5}, {"at": [1, 40], "line": 6}]},
    ]}})
}

fn files(lines: u32) -> Files {
    BTreeMap::from([("out.js".to_string(), lines)])
}

fn failures(text: &str, files: &Files, previous: Option<&str>) -> Vec<String> {
    let layout = Layout::parse(text).unwrap();
    let previous = previous.map(|p| (p, Layout::parse(p).unwrap()));
    let found = check(&layout, files, previous.as_ref().map(|(t, l)| (*t, l)));
    found.failures.iter().map(|f| f.to_string()).collect()
}

#[test]
fn a_layout_that_fits_its_tree_passes() {
    assert_eq!(failures(&layout(good()), &files(7), None), [] as [&str; 0]);
}

#[test]
fn an_entry_without_its_file_and_a_source_file_without_an_entry_fail() {
    let mut tree = files(7);
    tree.remove("out.js");
    tree.insert("src/a.js".into(), 3);
    assert_eq!(
        failures(&layout(good()), &tree, None),
        [
            "1 entry with no file in the tree; first: out.js",
            "1 file under src/ with no entry; first: src/a.js",
        ]
    );
}

#[test]
fn lines_that_do_not_rise_or_run_past_the_file_fail() {
    let mut bad = good();
    bad["out.js"]["units"][1]["body"][1]["line"] = json!(5);
    assert_eq!(
        failures(&layout(bad), &files(4), None),
        [
            "1 unit that does not start after the unit before it; first: out.js line 5",
            "2 units past the end of their file; first: out.js line 5",
        ]
    );
    let mut bad = good();
    bad["out.js"]["units"][0]["rows"][1] = json!([4, 13]);
    assert_eq!(
        failures(&layout(bad), &files(7), None),
        ["2 rows outside their unit's lines or out of order; first: out.js line 4"]
    );
}

#[test]
fn one_identity_given_twice_fails() {
    let mut bad = good();
    bad["out.js"]["units"][1]["body"][1]["at"] = json!([0, 0]);
    bad["out.js"]["units"][1]["rows"] = json!([[4, "init+9", "new"], [4, "init+9", "new"]]);
    assert_eq!(
        failures(&layout(bad), &files(7), None),
        [
            "1 unit with the identity of an earlier unit; first: out.js line 6",
            "1 function with the identity of an earlier function; first: out.js line 4",
        ]
    );
}

#[test]
fn an_input_the_run_did_not_read_fails() {
    let mut bad = good();
    bad["out.js"]["units"][0]["at"] = json!([2, 0]);
    assert_eq!(
        failures(&layout(bad), &files(7), None),
        ["1 unit of an input file the layout does not list; first: out.js line 1"]
    );
}

#[test]
fn a_partner_must_be_a_function_of_the_previous_layout_claimed_once() {
    let previous = layout(good());
    let mut next = good();
    next["out.js"]["units"][0]["rows"] = json!([
        [1, 0, "exact", 0, 19],
        [2, 13],
        [2, 19, "close", 0, 19],
        [3, 30, "exact", 0, 77]
    ]);
    let mut text: Value = serde_json::from_str(&layout(next)).unwrap();
    assert_eq!(
        failures(&text.to_string(), &files(7), Some(&previous)),
        [
            "the layout names previous layout null, the previous tree's is ".to_string()
                + &checksum(&previous),
            "1 function whose partner another function also has; first: out.js line 2".into(),
            "1 function whose partner the previous layout does not list; first: out.js line 3"
                .into(),
        ]
    );
    text["priorLayout"] = json!(checksum(&previous));
    text["priorAmbiguous"] = json!([[1, "init+9"], [1, 5]]);
    let got = failures(&text.to_string(), &files(7), Some(&previous));
    assert_eq!(
        got.last().map(String::as_str),
        Some(
            "1 ambiguous previous function the previous layout does not list; first: input 1 Byte(5)"
        )
    );
    assert_eq!(got.len(), 3);
}

#[test]
fn split_statements_must_be_numbered_once_each() {
    let mut text: Value = serde_json::from_str(&layout(json!({"out.js": {"units": [
        {"at": [0, 0], "line": 1, "n": 0, "hash": "aa"},
        {"at": [0, 9], "line": 2, "n": 2, "hash": "bb"},
    ]}})))
    .unwrap();
    text["hashVersion"] = json!(3);
    assert_eq!(
        failures(&text.to_string(), &files(7), None),
        ["the layout's 2 split statements are not numbered 0 to 2 once each"]
    );
}
