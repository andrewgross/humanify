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

/// Two texts on which GNU `diff`'s default speed heuristic charges more
/// lines than the minimal edit script (exp222), with that minimum.
///
/// `a` is `n` distinct lines in three blocks P (n/8), M (n/2), R (the
/// rest); `b` is P R M with every tenth line of `b` edited. A line of `b`
/// matches only its own unedited original, and M and R cannot both stay in
/// order, so the longest common subsequence keeps P's unedited lines and
/// M's (the larger block's): 9/10 of n/8 + n/2. Every other line of each
/// side is changed: the minimal count is 2 * (n - that).
pub fn overcharged_pair(n: usize) -> (String, String, usize) {
    assert!(n.is_multiple_of(80), "n/8 and n/2 must be whole tens");
    let a: Vec<String> = (0..n).map(|i| format!("line {i}")).collect();
    let (p, m) = (n / 8, n / 2);
    let reordered = a[..p].iter().chain(&a[p + m..]).chain(&a[p..p + m]);
    let b: Vec<String> = reordered
        .enumerate()
        .map(|(j, x)| {
            if j % 10 == 3 {
                format!("{x} x")
            } else {
                x.clone()
            }
        })
        .collect();
    let kept = (p + m) / 10 * 9;
    let text = |v: &[String]| v.join("\n") + "\n";
    (text(&a), text(&b), 2 * (n - kept))
}
