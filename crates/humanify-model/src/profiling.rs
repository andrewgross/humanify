//! Profiling types (TS: src/profiling/types.ts + the trace-event shape in
//! src/profiling/trace-events.ts, WPB.5). Same JSON shapes as the TS
//! (02 §3), so existing viewers and report tooling read Rust profiles
//! unchanged:
//!
//! - field names and ORDER follow the TS object literals;
//! - absent optionals are omitted (JSON.stringify drops `undefined`);
//! - numbers serialize like a JS number: an integral finite value prints
//!   without a fraction (`0`, not serde's `0.0`) — `JsNumber`;
//! - metadata / `args` objects keep JS property order — `JsObject`.

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// Thread ids for trace-event grouping (TS `TRACE_TID`).
pub mod trace_tid {
    /// Pipeline-level spans (parse, graph-build, generate, ...).
    pub const PIPELINE: u32 = 1;
    /// Per-function rename spans.
    pub const RENAME_FUNCTION: u32 = 2;
    /// Module binding rename spans.
    pub const RENAME_MODULE_BINDING: u32 = 3;
}

/// An f64 that serializes the way `JSON.stringify` prints a JS number.
#[derive(Clone, Copy, PartialEq, PartialOrd, Debug, Default)]
pub struct JsNumber(pub f64);

impl Serialize for JsNumber {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        // 2^53: every integral f64 below it is exactly an i64 JS prints
        // without a fraction or exponent.
        if x.is_finite() && x.fract() == 0.0 && x.abs() < 9_007_199_254_740_992.0 {
            #[allow(clippy::cast_possible_truncation)]
            return s.serialize_i64(x as i64);
        }
        s.serialize_f64(x)
    }
}

impl<'de> Deserialize<'de> for JsNumber {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        f64::deserialize(d).map(JsNumber)
    }
}

/// A JS object's own properties in JS enumeration order: array-index
/// keys first (ascending numerically), then string keys in insertion
/// order. Assigning an existing key keeps its position (object spread).
#[derive(Clone, PartialEq, Debug, Default)]
pub struct JsObject(Vec<(String, Value)>);

/// A canonical array index (`"0"`, `"17"`, not `"017"`), < 2^32 - 1.
fn array_index(key: &str) -> Option<u32> {
    let n: u32 = key.parse().ok()?;
    (n != u32::MAX && n.to_string() == key).then_some(n)
}

impl JsObject {
    pub fn new() -> Self {
        Self::default()
    }

    /// `obj[key] = value`.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        let key = key.into();
        let value = value.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    /// Builder form of `set`.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.set(key, value);
        self
    }

    /// `{ ...self, ...other }`.
    pub fn spread(&mut self, other: &JsObject) {
        for (k, v) in &other.0 {
            self.set(k.clone(), v.clone());
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The properties in JS enumeration order.
    pub fn entries(&self) -> Vec<(&str, &Value)> {
        let mut indexed: Vec<(u32, &str, &Value)> = self
            .0
            .iter()
            .filter_map(|(k, v)| array_index(k).map(|n| (n, k.as_str(), v)))
            .collect();
        indexed.sort_by_key(|&(n, _, _)| n);
        let named = self
            .0
            .iter()
            .filter(|(k, _)| array_index(k).is_none())
            .map(|(k, v)| (k.as_str(), v));
        indexed
            .into_iter()
            .map(|(_, k, v)| (k, v))
            .chain(named)
            .collect()
    }
}

impl Serialize for JsObject {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let entries = self.entries();
        let mut map = s.serialize_map(Some(entries.len()))?;
        for (k, v) in entries {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for JsObject {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = JsObject;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<JsObject, A::Error> {
                let mut o = JsObject::new();
                while let Some((k, v)) = a.next_entry::<String, Value>()? {
                    o.set(k, v);
                }
                Ok(o)
            }
        }
        d.deserialize_map(V)
    }
}

/// A completed timing span (TS `ProfileSpan`).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSpan {
    pub name: String,
    pub category: String,
    pub start_ms: JsNumber,
    pub end_ms: JsNumber,
    pub tid: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub metadata: Option<JsObject>,
}

/// The three live counts a concurrency sample carries (TS
/// `Omit<ConcurrencySnapshot, "timeMs">`).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrencyCounts {
    pub in_flight: u32,
    pub ready: u32,
    pub blocked: u32,
}

/// TS `ConcurrencySnapshot` (`timeMs` first — the TS spreads the counts
/// after it).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrencySnapshot {
    pub time_ms: JsNumber,
    pub in_flight: u32,
    pub ready: u32,
    pub blocked: u32,
}

/// TS `StageSummary`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StageSummary {
    pub name: String,
    pub duration_ms: JsNumber,
    pub span_count: u32,
}

/// TS `RenameTiming`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RenameTiming {
    pub p50: JsNumber,
    pub p95: JsNumber,
    pub p99: JsNumber,
    pub min_ms: JsNumber,
    pub max_ms: JsNumber,
    pub count: u32,
}

/// TS `ProfileReport["meta"]`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMeta {
    pub total_duration_ms: JsNumber,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub input_file: Option<String>,
}

/// TS `ProfileReport`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileReport {
    pub spans: Vec<ProfileSpan>,
    pub concurrency_snapshots: Vec<ConcurrencySnapshot>,
    pub stage_summaries: Vec<StageSummary>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub rename_timing: Option<RenameTiming>,
    pub meta: ProfileMeta,
}

/// One Chrome trace event (TS `TraceEvent`): `ph` is `"M"` (metadata),
/// `"X"` (complete span, with `dur`) or `"C"` (counter). Times are µs.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TraceEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    pub ts: JsNumber,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dur: Option<JsNumber>,
    pub pid: u32,
    pub tid: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub args: Option<JsObject>,
}

/// The file `--profile` writes: `{ "traceEvents": [...] }`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TraceFile {
    pub trace_events: Vec<TraceEvent>,
}
