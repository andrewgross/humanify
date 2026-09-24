//! Performance profiling for the humanify pipeline itself (TS:
//! src/profiling/, WPB.5) — how long each stage takes, as spans, emitted as
//! Chrome trace-event JSON (`chrome://tracing`, ui.perfetto.dev,
//! speedscope) in the SAME shapes the TS writes (02 §3; the types live in
//! `humanify_model::profiling`).
//!
//! A disabled `Profiler` records nothing and every call is a cheap early
//! return (TS `NULL_PROFILER`). An enabled one is `Sync`: spans may end on
//! any thread (the pipeline's parallel stages), and they are recorded in
//! END order, exactly like the TS's `push` in `end()`.

pub mod summary;
pub mod trace_events;

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use humanify_model::profiling::{
    ConcurrencyCounts, ConcurrencySnapshot, JsNumber, JsObject, ProfileMeta, ProfileReport,
    ProfileSpan, RenameTiming, StageSummary, trace_tid,
};

pub use self::summary::format_profile_summary;
pub use self::trace_events::to_trace_events;

/// What the recorder accumulates (shared with the sampling thread).
#[derive(Default)]
struct Recorded {
    spans: Vec<ProfileSpan>,
    snapshots: Vec<ConcurrencySnapshot>,
}

/// The running sampler: dropping `stop` (or sending on it) ends the thread.
struct Sampler {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
}

/// Span-based profiler (TS `Profiler`).
pub struct Profiler {
    enabled: bool,
    start: Instant,
    started_at: String,
    recorded: Arc<Mutex<Recorded>>,
    sampler: Mutex<Option<Sampler>>,
}

/// Locks through poisoning: a panicked span-ender must not take the
/// profile down with it (the data is append-only).
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// An open span (TS `SpanHandle`). Call `end` to record it; a handle
/// dropped without `end` records nothing, as in the TS.
#[must_use = "a span is recorded only when end() is called"]
pub struct SpanHandle<'p> {
    profiler: Option<&'p Profiler>,
    name: String,
    category: String,
    tid: u32,
    start_ms: f64,
    metadata: Option<JsObject>,
}

impl SpanHandle<'_> {
    /// End the span. `end_metadata` is spread over the start metadata
    /// (`{ ...metadata, ...endMetadata }`); with neither, the span carries
    /// no metadata at all (the TS's `undefined`).
    pub fn end(self, end_metadata: Option<JsObject>) {
        let Some(p) = self.profiler else { return };
        let end_ms = elapsed_ms(p.start);
        let metadata = match (self.metadata, end_metadata) {
            (None, None) => None,
            (start, end) => {
                let mut combined = start.unwrap_or_default();
                if let Some(end) = end {
                    combined.spread(&end);
                }
                Some(combined)
            }
        };
        lock(&p.recorded).spans.push(ProfileSpan {
            name: self.name,
            category: self.category,
            start_ms: JsNumber(self.start_ms),
            end_ms: JsNumber(end_ms),
            tid: self.tid,
            metadata,
        });
    }
}

impl Profiler {
    /// TS `new Profiler(enabled)`.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            start: Instant::now(),
            started_at: if enabled { iso_now() } else { String::new() },
            recorded: Arc::default(),
            sampler: Mutex::new(None),
        }
    }

    /// TS `NULL_PROFILER`: a disabled profiler.
    pub fn disabled() -> Self {
        Self::new(false)
    }

    /// TS `isEnabled` — check it to skip building hot-path metadata.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// TS `startSpan(name, category, tid = 1, metadata?)`.
    pub fn start_span(
        &self,
        name: &str,
        category: &str,
        tid: u32,
        metadata: Option<JsObject>,
    ) -> SpanHandle<'_> {
        SpanHandle {
            profiler: self.enabled.then_some(self),
            name: name.to_string(),
            category: category.to_string(),
            tid,
            start_ms: if self.enabled {
                elapsed_ms(self.start)
            } else {
                0.0
            },
            metadata,
        }
    }

    /// A pipeline-level span: tid 1, category "pipeline".
    pub fn pipeline_span(&self, name: &str) -> SpanHandle<'_> {
        self.start_span(name, "pipeline", trace_tid::PIPELINE, None)
    }

    /// TS `recordConcurrency`.
    pub fn record_concurrency(&self, counts: ConcurrencyCounts) {
        if self.enabled {
            push_snapshot(&self.recorded, self.start, counts);
        }
    }

    /// TS `startConcurrencySampling(sampler, intervalMs = 250)`: a
    /// background thread records `sampler()` every `interval` until
    /// `stop_concurrency_sampling` or `finalize`.
    pub fn start_concurrency_sampling<F>(&self, mut sampler: F, interval: Duration)
    where
        F: FnMut() -> ConcurrencyCounts + Send + 'static,
    {
        if !self.enabled {
            return;
        }
        self.stop_concurrency_sampling();
        let (stop, stopped) = mpsc::channel::<()>();
        let recorded = Arc::clone(&self.recorded);
        let start = self.start;
        let thread = std::thread::spawn(move || {
            while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(interval) {
                push_snapshot(&recorded, start, sampler());
            }
        });
        *lock(&self.sampler) = Some(Sampler { stop, thread });
    }

    /// TS `stopConcurrencySampling`.
    pub fn stop_concurrency_sampling(&self) {
        if let Some(s) = lock(&self.sampler).take() {
            // The thread may already be gone; either way it stops.
            let _ = s.stop.send(());
            let _ = s.thread.join();
        }
    }

    /// TS `finalize(meta?)`: stop sampling, summarize, and return the
    /// report (the recorded spans stay recorded, as in the TS).
    pub fn finalize(&self, input_file: Option<&str>) -> ProfileReport {
        self.stop_concurrency_sampling();
        if !self.enabled {
            return ProfileReport {
                spans: Vec::new(),
                concurrency_snapshots: Vec::new(),
                stage_summaries: Vec::new(),
                rename_timing: None,
                meta: ProfileMeta {
                    total_duration_ms: JsNumber(0.0),
                    started_at: String::new(),
                    input_file: None,
                },
            };
        }
        let total_duration_ms = elapsed_ms(self.start);
        let recorded = lock(&self.recorded);
        ProfileReport {
            spans: recorded.spans.clone(),
            concurrency_snapshots: recorded.snapshots.clone(),
            stage_summaries: stage_summaries(&recorded.spans),
            rename_timing: rename_timing(&recorded.spans),
            meta: ProfileMeta {
                total_duration_ms: JsNumber(total_duration_ms),
                started_at: self.started_at.clone(),
                input_file: input_file.map(str::to_string),
            },
        }
    }
}

impl Drop for Profiler {
    fn drop(&mut self) {
        self.stop_concurrency_sampling();
    }
}

fn push_snapshot(recorded: &Mutex<Recorded>, start: Instant, c: ConcurrencyCounts) {
    let time_ms = JsNumber(elapsed_ms(start));
    lock(recorded).snapshots.push(ConcurrencySnapshot {
        time_ms,
        in_flight: c.in_flight,
        ready: c.ready,
        blocked: c.blocked,
    });
}

/// Per-name totals over the tid-1 spans, in first-seen order (the TS
/// `Map`'s insertion order).
fn stage_summaries(spans: &[ProfileSpan]) -> Vec<StageSummary> {
    let mut out: Vec<StageSummary> = Vec::new();
    for span in spans.iter().filter(|s| s.tid == trace_tid::PIPELINE) {
        let dur = span.end_ms.0 - span.start_ms.0;
        match out.iter_mut().find(|s| s.name == span.name) {
            Some(s) => {
                s.duration_ms.0 += dur;
                s.span_count += 1;
            }
            None => out.push(StageSummary {
                name: span.name.clone(),
                duration_ms: JsNumber(dur),
                span_count: 1,
            }),
        }
    }
    out
}

/// Percentiles over the rename spans (category "rename", tid 2).
fn rename_timing(spans: &[ProfileSpan]) -> Option<RenameTiming> {
    let mut durations: Vec<f64> = spans
        .iter()
        .filter(|s| s.category == "rename" && s.tid == trace_tid::RENAME_FUNCTION)
        .map(|s| s.end_ms.0 - s.start_ms.0)
        .collect();
    if durations.is_empty() {
        return None;
    }
    durations.sort_by(f64::total_cmp);
    Some(RenameTiming {
        p50: JsNumber(compute_percentile(&durations, 50.0)),
        p95: JsNumber(compute_percentile(&durations, 95.0)),
        p99: JsNumber(compute_percentile(&durations, 99.0)),
        min_ms: JsNumber(durations[0]),
        max_ms: JsNumber(durations[durations.len() - 1]),
        count: u32::try_from(durations.len()).unwrap_or(u32::MAX),
    })
}

/// TS `computePercentile`: nearest-rank on a sorted slice
/// (`sorted[max(0, ceil(p/100 * n) - 1)]`), 0 for an empty slice.
pub fn compute_percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() - 1.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let idx = rank.max(0.0) as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// `new Date().toISOString()`: UTC, millisecond precision, `Z`.
fn iso_now() -> String {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    iso_from_unix_ms(since.as_millis())
}

/// Formats a Unix-epoch millisecond count as `YYYY-MM-DDTHH:MM:SS.mmmZ`
/// (Howard Hinnant's days-to-civil; valid for any post-1970 instant).
pub fn iso_from_unix_ms(ms: u128) -> String {
    let secs = ms / 1000;
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u128::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        ms % 1000
    )
}
