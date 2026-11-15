//! Profiling types: the profile report and the Chrome trace-event file
//! (`--perf-trace`). Absent optionals are omitted; metadata and `args`
//! keep the order their keys were set ([`Fields`]).

use serde::Serialize;
use serde_json::Value;

use crate::ordered::OrderedMap;

/// Thread ids for trace-event grouping (TS `TRACE_TID`).
pub mod trace_tid {
    /// Pipeline-level spans (parse, graph-build, generate, ...).
    pub const PIPELINE: u32 = 1;
    /// Per-function rename spans.
    pub const RENAME_FUNCTION: u32 = 2;
    /// Module binding rename spans.
    pub const RENAME_MODULE_BINDING: u32 = 3;
    /// The run-time library index machinery (exp154, library-names-plan
    /// step 3): the store search, each build stage, and the run's wait.
    pub const LIBRARY_INDEX: u32 = 5;
}

/// A span's or event's metadata, keys in the order they were set.
pub type Fields = OrderedMap<Value>;

/// A completed timing span (TS `ProfileSpan`).
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSpan {
    pub name: String,
    pub category: String,
    pub start_ms: f64,
    pub end_ms: f64,
    pub tid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Fields>,
}

/// The three live counts a concurrency sample carries (TS
/// `Omit<ConcurrencySnapshot, "timeMs">`).
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrencyCounts {
    pub in_flight: u32,
    pub ready: u32,
    pub blocked: u32,
}

/// TS `ConcurrencySnapshot` (`timeMs` first — the TS spreads the counts
/// after it).
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConcurrencySnapshot {
    pub time_ms: f64,
    pub in_flight: u32,
    pub ready: u32,
    pub blocked: u32,
}

/// TS `StageSummary`.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StageSummary {
    pub name: String,
    pub duration_ms: f64,
    pub span_count: u32,
}

/// TS `RenameTiming`.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RenameTiming {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub min_ms: f64,
    pub max_ms: f64,
    pub count: u32,
}

/// TS `ProfileReport["meta"]`.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMeta {
    pub total_duration_ms: f64,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_file: Option<String>,
}

/// TS `ProfileReport`.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProfileReport {
    pub spans: Vec<ProfileSpan>,
    pub concurrency_snapshots: Vec<ConcurrencySnapshot>,
    pub stage_summaries: Vec<StageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename_timing: Option<RenameTiming>,
    pub meta: ProfileMeta,
}

/// One Chrome trace event (TS `TraceEvent`): `ph` is `"M"` (metadata),
/// `"X"` (complete span, with `dur`) or `"C"` (counter). Times are µs.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct TraceEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    pub ts: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dur: Option<f64>,
    pub pid: u32,
    pub tid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Fields>,
}

/// The file `--perf-trace` writes: `{ "traceEvents": [...] }`.
#[derive(Serialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TraceFile {
    pub trace_events: Vec<TraceEvent>,
}
