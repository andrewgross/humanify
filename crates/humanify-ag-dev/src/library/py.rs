//! Small builders for the Python-spelled values the library scorer
//! writes (`crate::json::py::PyValue`), and Python's float arithmetic
//! where a card's bytes depend on it.

use serde_json::Value;

use crate::json::py::PyValue;

pub fn int(i: i64) -> PyValue {
    PyValue::Int(i.to_string())
}

pub fn flt(x: f64) -> PyValue {
    PyValue::Float(x)
}

pub fn s(x: &str) -> PyValue {
    PyValue::Str(x.to_string())
}

pub fn opt_s(x: Option<&str>) -> PyValue {
    x.map_or(PyValue::Null, s)
}

pub fn list_s<'a>(xs: impl IntoIterator<Item = &'a str>) -> PyValue {
    PyValue::List(xs.into_iter().map(s).collect())
}

/// A dict from (key, value) pairs; a later duplicate key replaces the
/// earlier value (Python's `{**a, **b}`).
pub fn dict(entries: Vec<(String, PyValue)>) -> PyValue {
    let mut out: Vec<(String, PyValue)> = Vec::with_capacity(entries.len());
    for (k, v) in entries {
        match out.iter_mut().find(|(x, _)| *x == k) {
            Some(slot) => slot.1 = v,
            None => out.push((k, v)),
        }
    }
    PyValue::Dict(out)
}

/// `(key, value)` with an owned key.
pub fn kv(k: &str, v: PyValue) -> (String, PyValue) {
    (k.to_string(), v)
}

/// Python `round(x, n)`: the correctly rounded decimal (ties to even on
/// the exact binary value), read back as a float.
pub fn round_n(x: f64, n: usize) -> f64 {
    format!("{x:.n$}")
        .parse()
        .expect("a formatted float parses")
}

/// Python `round(x)` for a float: an int, ties to even.
pub fn round_int(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// score.py `r6`.
pub fn r6(x: f64) -> f64 {
    round_n(x, 6)
}

/// score.py `share(n, d)`: `r6(n / d)`, 0.0 when d is 0. Python divides
/// two ints exactly then rounds once; for the scorer's byte counts (far
/// under 2^53) one f64 division does the same.
pub fn share(n: f64, d: f64) -> f64 {
    if d != 0.0 { r6(n / d) } else { 0.0 }
}

/// An exact int as f64 (refused past 2^53, where Python's int division
/// and f64's part ways).
pub fn exact(i: i64) -> f64 {
    assert!(
        i.unsigned_abs() < (1u64 << 53),
        "{i} is past f64's exact integers"
    );
    i as f64
}

/// A JSON value read by serde, as Python wrote it. A float is refused:
/// serde_json's default float parse is not always correctly rounded, so a
/// passed-through float could differ in its last digit.
pub fn from_json(v: &Value) -> Result<PyValue, String> {
    Ok(match v {
        Value::Null => PyValue::Null,
        Value::Bool(b) => PyValue::Bool(*b),
        Value::Number(n) if n.is_i64() || n.is_u64() => PyValue::Int(n.to_string()),
        Value::Number(n) => {
            return Err(format!("a float ({n}) where only ints are passed through"));
        }
        Value::String(x) => s(x),
        Value::Array(xs) => PyValue::List(xs.iter().map(from_json).collect::<Result<_, _>>()?),
        Value::Object(m) => PyValue::Dict(
            m.iter()
                .map(|(k, v)| Ok((k.clone(), from_json(v)?)))
                .collect::<Result<_, String>>()?,
        ),
    })
}

/// `json.dumps(value, sort_keys=True, separators=(",", ":"))` (ensure_ascii
/// on): lib096 `write_gz_json`'s spelling.
pub fn compact(value: &PyValue) -> String {
    let mut out = String::new();
    write_compact(&mut out, value);
    out
}

fn write_compact(out: &mut String, value: &PyValue) {
    match value {
        PyValue::List(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        PyValue::Dict(entries) => {
            let mut sorted: Vec<&(String, PyValue)> = entries.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            out.push('{');
            for (i, (k, v)) in sorted.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                crate::json::py::write_str(out, k);
                out.push(':');
                write_compact(out, v);
            }
            out.push('}');
        }
        scalar => out.push_str(&crate::json::py::dumps(scalar)),
    }
}
