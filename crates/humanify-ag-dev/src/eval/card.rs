//! Small builders and a path reader for the harness's JSON records.

use crate::json::js::{JsObject, JsValue};

/// The version label cards carry (`scorerVersion`): 4 is the diff
/// labeller.
pub const SCORER_VERSION: u64 = 4;

/// An object from `(key, value)` pairs, in order.
pub fn obj(entries: Vec<(&str, JsValue)>) -> JsValue {
    let mut o = JsObject::new();
    for (k, v) in entries {
        o.insert(k, v);
    }
    JsValue::Object(o)
}

pub fn text(s: &str) -> JsValue {
    JsValue::str(s)
}

pub fn texts<S: AsRef<str>>(items: &[S]) -> JsValue {
    JsValue::str_array(items)
}

/// The value at a dotted path (`churn.layout.real`), if present.
pub fn at<'v>(v: &'v JsValue, path: &str) -> Option<&'v JsValue> {
    path.split('.')
        .try_fold(v, |cur, key| cur.as_object()?.get(key))
}

/// The number at a dotted path, if present and numeric.
pub fn number_at(v: &JsValue, path: &str) -> Option<f64> {
    match at(v, path)? {
        JsValue::Number(n) => Some(*n),
        _ => None,
    }
}

/// Whether a parsed JSON file is a label card (shape, not file name: a
/// run manifest also has a string `pair`).
pub fn is_scorecard(v: &JsValue) -> bool {
    at(v, "pair").and_then(JsValue::as_str).is_some()
        && at(v, "labels").and_then(JsValue::as_object).is_some()
}
