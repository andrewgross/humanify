//! Small builders for the JSON values the library scorer writes, and the
//! rounding a card's numbers are computed with.

use serde_json::{Map, Number, Value};

pub fn int(i: i64) -> Value {
    Value::from(i)
}

/// A float that stays a float (`5.0`, not `5`). A non-finite float has no
/// JSON spelling: refused.
pub fn flt(x: f64) -> Value {
    Value::Number(Number::from_f64(x).unwrap_or_else(|| panic!("{x} is not a finite float")))
}

pub fn s(x: &str) -> Value {
    Value::String(x.to_string())
}

pub fn opt_s(x: Option<&str>) -> Value {
    x.map_or(Value::Null, s)
}

pub fn list_s<'a>(xs: impl IntoIterator<Item = &'a str>) -> Value {
    Value::Array(xs.into_iter().map(s).collect())
}

/// An object from (key, value) pairs; a later duplicate key replaces the
/// earlier value.
pub fn dict(entries: Vec<(String, Value)>) -> Value {
    Value::Object(entries.into_iter().collect::<Map<String, Value>>())
}

/// `(key, value)` with an owned key.
pub fn kv(k: &str, v: Value) -> (String, Value) {
    (k.to_string(), v)
}

/// An int field of an object.
pub fn int_at(v: &Value, k: &str) -> Option<i64> {
    v.get(k).and_then(Value::as_i64)
}

/// `round(x, n)` to the correctly rounded decimal (ties to even on the
/// exact binary value), read back as a float.
pub fn round_n(x: f64, n: usize) -> f64 {
    format!("{x:.n$}")
        .parse()
        .expect("a formatted float parses")
}

/// A float rounded to an int, ties to even.
pub fn round_int(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// Six decimals: every share a card records.
pub fn r6(x: f64) -> f64 {
    round_n(x, 6)
}

/// `r6(n / d)`, 0.0 when d is 0. Exact division then one rounding; for the
/// scorer's byte counts (far under 2^53) one f64 division does the same.
pub fn share(n: f64, d: f64) -> f64 {
    if d != 0.0 { r6(n / d) } else { 0.0 }
}

/// An exact int as f64 (refused past 2^53, where exact division and f64's
/// part ways).
pub fn exact(i: i64) -> f64 {
    assert!(
        i.unsigned_abs() < (1u64 << 53),
        "{i} is past f64's exact integers"
    );
    i as f64
}
