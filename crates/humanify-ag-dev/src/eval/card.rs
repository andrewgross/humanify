//! The card's JSON in the JS spelling (`JSON.stringify(x, null, 2)`, via
//! `humanify_ag_model::js`): small builders for writing a card and a path
//! reader for reading one back.

use humanify_ag_model::js::{JsObject, JsValue, to_fixed};

/// The scorer version every card this scorer writes carries
/// (`scorerVersion`), equal to the KPI statement form's version
/// (`hash::forms::KPI_STATEMENT`). A card without the field is version 1
/// (the TS scorer).
pub const SCORER_VERSION: u64 = humanify_ag_core::hash::forms::KPI_STATEMENT.version;

/// An object from `(key, value)` pairs, in order.
pub fn obj(entries: Vec<(&str, JsValue)>) -> JsValue {
    let mut o = JsObject::new();
    for (k, v) in entries {
        o.insert(k, v);
    }
    JsValue::Object(o)
}

/// A number.
pub fn num<T: Into<f64>>(x: T) -> JsValue {
    JsValue::Number(x.into())
}

/// A count.
pub fn count(x: usize) -> JsValue {
    JsValue::Number(x as f64)
}

/// A signed count.
pub fn int(x: i64) -> JsValue {
    JsValue::Number(x as f64)
}

/// `+x.toFixed(d)`: rounded the JS way, then back to a number.
pub fn fixed(x: f64, digits: usize) -> JsValue {
    JsValue::Number(to_fixed(x, digits).parse().unwrap_or(0.0))
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

/// A card's scorer version: its `scorerVersion`, else 1 (the TS scorer's
/// cards record none).
pub fn scorer_version(card: &JsValue) -> u64 {
    number_at(card, "scorerVersion").map_or(1, |n| n as u64)
}

/// Whether a parsed JSON file is a scorecard (shape, never file name: a
/// run manifest also has a string `pair`).
pub fn is_scorecard(v: &JsValue) -> bool {
    at(v, "pair").and_then(JsValue::as_str).is_some()
        && at(v, "churn.statements")
            .and_then(JsValue::as_object)
            .is_some()
        && at(v, "determinism").and_then(JsValue::as_object).is_some()
}
