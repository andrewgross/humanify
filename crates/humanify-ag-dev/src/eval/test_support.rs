//! Test support shared by the eval readers' tests.

use std::path::Path;

use humanify_ag_model::js::JsValue;

use super::card::at;

/// Give `label_dir`'s `summary.json` a per-pair diagnostics array under
/// `key` (`llmDiagnostics`, `matchingDiagnostics`): one `{"pair":…,
/// <fields>}` entry per card, `fields` being the JSON members after the
/// pair.
pub fn set_pair_diagnostics(label_dir: &Path, key: &str, fields: &str) {
    let path = label_dir.join("summary.json");
    let summary = JsValue::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let diags: Vec<JsValue> = match at(&summary, "pairs") {
        Some(JsValue::Array(a)) => a
            .iter()
            .map(|c| {
                let pair = at(c, "pair").and_then(JsValue::as_str).unwrap();
                JsValue::parse(&format!(r#"{{"pair":"{pair}",{fields}}}"#)).unwrap()
            })
            .collect(),
        _ => panic!("summary.json pairs"),
    };
    let JsValue::Object(mut o) = summary else {
        panic!("summary.json is an object")
    };
    o.insert(key, JsValue::Array(diags));
    std::fs::write(&path, crate::json::js_text(&JsValue::Object(o), true)).unwrap();
}

/// The matching fields the bands and leaderboard tests vary.
pub fn set_matching(label_dir: &Path, fn_asked: f64, mb_renamed: f64) {
    set_pair_diagnostics(
        label_dir,
        "matchingDiagnostics",
        &format!(r#""fnTotal":100,"fnAsked":{fn_asked},"mbLLMRenamed":{mb_renamed}"#),
    );
}
