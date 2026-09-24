//! Metrics tracking for LLM calls and function processing (llm/metrics.ts):
//! counters behind one mutex, a throttled callback for the progress UI, a
//! rolling 30 s tokens-per-second rate, and the two formatters.
//!
//! The TS `llmCallStart()` returns a closure to call when done; here it
//! returns a [`CallStart`] token handed back to [`MetricsTracker::llm_call_done`].
//! `undefined` metrics fields are `None`.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use humanify_model::js::{math_round, number_to_string, to_fixed};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineStage {
    Parsing,
    BuildingGraph,
    Renaming,
    LibraryParams,
    LibraryPrefix,
    Generating,
    Done,
}

impl PipelineStage {
    pub fn as_str(self) -> &'static str {
        match self {
            PipelineStage::Parsing => "parsing",
            PipelineStage::BuildingGraph => "building-graph",
            PipelineStage::Renaming => "renaming",
            PipelineStage::LibraryParams => "library-params",
            PipelineStage::LibraryPrefix => "library-prefix",
            PipelineStage::Generating => "generating",
            PipelineStage::Done => "done",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LlmMetrics {
    pub total_calls: u64,
    pub in_flight_calls: i64,
    pub completed_calls: u64,
    pub failed_calls: u64,
    pub total_tokens: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub retries: u64,
    pub avg_response_time_ms: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionMetrics {
    pub total: u64,
    pub completed: u64,
    pub in_progress: u64,
    pub pending: u64,
    pub ready: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModuleBindingMetrics {
    pub total: u64,
    pub completed: u64,
    pub in_progress: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessingMetrics {
    pub llm: LlmMetrics,
    pub functions: FunctionMetrics,
    pub module_bindings: ModuleBindingMetrics,
    pub stage: PipelineStage,
    /// Epoch milliseconds (`Date.now()` at construction / reset).
    pub start_time: u64,
    pub elapsed_ms: u64,
    pub estimated_remaining_ms: Option<f64>,
    pub tokens_per_second: f64,
}

pub type MetricsCallback = Box<dyn Fn(&ProcessingMetrics) + Send + Sync>;

/// Rolling window for the tokens-per-second rate.
const TOKEN_RATE_WINDOW: Duration = Duration::from_millis(30_000);

/// The start of one LLM call (the TS closure's captured `startTime`).
#[derive(Clone, Copy, Debug)]
pub struct CallStart(Instant);

#[derive(Debug)]
struct State {
    llm_calls: u64,
    llm_in_flight: i64,
    llm_completed: u64,
    llm_failed: u64,
    llm_total_tokens: u64,
    llm_input_tokens: u64,
    llm_output_tokens: u64,
    llm_retries: u64,
    llm_response_times: Vec<u128>,
    fn_total: u64,
    fn_completed: u64,
    fn_in_progress: u64,
    fn_pending: u64,
    fn_ready: u64,
    mb_total: u64,
    mb_completed: u64,
    mb_in_progress: u64,
    stage: PipelineStage,
    token_history: VecDeque<(Instant, u64)>,
    start: Instant,
    start_epoch_ms: u64,
    last_callback: Option<Instant>,
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl State {
    fn new() -> Self {
        State {
            llm_calls: 0,
            llm_in_flight: 0,
            llm_completed: 0,
            llm_failed: 0,
            llm_total_tokens: 0,
            llm_input_tokens: 0,
            llm_output_tokens: 0,
            llm_retries: 0,
            llm_response_times: Vec::new(),
            fn_total: 0,
            fn_completed: 0,
            fn_in_progress: 0,
            fn_pending: 0,
            fn_ready: 0,
            mb_total: 0,
            mb_completed: 0,
            mb_in_progress: 0,
            stage: PipelineStage::Parsing,
            token_history: VecDeque::new(),
            start: Instant::now(),
            start_epoch_ms: epoch_ms(),
            last_callback: None,
        }
    }

    /// `getTokensPerSecond`: drop entries older than the window, then
    /// tokens / window seconds, 0 under 100 ms of history.
    fn tokens_per_second(&mut self, now: Instant) -> f64 {
        while self
            .token_history
            .front()
            .is_some_and(|(t, _)| now.duration_since(*t) > TOKEN_RATE_WINDOW)
        {
            self.token_history.pop_front();
        }
        let Some((first, _)) = self.token_history.front() else {
            return 0.0;
        };
        let total: u64 = self.token_history.iter().map(|(_, n)| n).sum();
        let window_ms = now.duration_since(*first).as_millis() as f64;
        if window_ms < 100.0 {
            return 0.0;
        }
        math_round(total as f64 / (window_ms / 1000.0))
    }

    fn snapshot(&mut self) -> ProcessingMetrics {
        let now = Instant::now();
        let elapsed_ms = now.duration_since(self.start).as_millis() as u64;
        let avg = if self.llm_response_times.is_empty() {
            0.0
        } else {
            self.llm_response_times.iter().sum::<u128>() as f64
                / self.llm_response_times.len() as f64
        };
        let total_completed = self.fn_completed + self.mb_completed;
        let total_items = self.fn_total + self.mb_total;
        let estimated_remaining_ms = (total_completed > 0).then(|| {
            let ms_per_item = elapsed_ms as f64 / total_completed as f64;
            math_round(ms_per_item * (total_items as f64 - total_completed as f64))
        });
        let positive = |n: u64| (n > 0).then_some(n);
        ProcessingMetrics {
            llm: LlmMetrics {
                total_calls: self.llm_calls,
                in_flight_calls: self.llm_in_flight,
                completed_calls: self.llm_completed,
                failed_calls: self.llm_failed,
                total_tokens: positive(self.llm_total_tokens),
                input_tokens: positive(self.llm_input_tokens),
                output_tokens: positive(self.llm_output_tokens),
                retries: self.llm_retries,
                avg_response_time_ms: math_round(avg),
            },
            functions: FunctionMetrics {
                total: self.fn_total,
                completed: self.fn_completed,
                in_progress: self.fn_in_progress,
                pending: self.fn_pending,
                ready: self.fn_ready,
            },
            module_bindings: ModuleBindingMetrics {
                total: self.mb_total,
                completed: self.mb_completed,
                in_progress: self.mb_in_progress,
            },
            stage: self.stage,
            start_time: self.start_epoch_ms,
            elapsed_ms,
            estimated_remaining_ms,
            tokens_per_second: self.tokens_per_second(now),
        }
    }
}

/// Tracks metrics throughout the processing pipeline (`MetricsTracker`).
pub struct MetricsTracker {
    state: Mutex<State>,
    callback: Option<MetricsCallback>,
    throttle: Duration,
}

impl Default for MetricsTracker {
    fn default() -> Self {
        MetricsTracker::new(None, None)
    }
}

impl MetricsTracker {
    /// `new MetricsTracker({onMetrics, throttleMs})`; throttle default 100 ms.
    pub fn new(callback: Option<MetricsCallback>, throttle_ms: Option<u64>) -> Self {
        MetricsTracker {
            state: Mutex::new(State::new()),
            callback,
            throttle: Duration::from_millis(throttle_ms.unwrap_or(100)),
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.state.lock().expect("metrics lock"))
    }

    /// Mutate, then emit through the throttle.
    fn update(&self, f: impl FnOnce(&mut State)) {
        self.with(f);
        self.emit_throttled();
    }

    // ============ Stage ============

    /// Set the current pipeline stage (force-emits).
    pub fn set_stage(&self, stage: PipelineStage) {
        self.with(|s| s.stage = stage);
        self.emit();
    }

    // ============ LLM Metrics ============

    /// Call when starting an LLM request.
    pub fn llm_call_start(&self) -> CallStart {
        self.update(|s| {
            s.llm_calls += 1;
            s.llm_in_flight += 1;
        });
        CallStart(Instant::now())
    }

    /// The TS closure `llmCallStart` returned: the call completed.
    pub fn llm_call_done(&self, start: CallStart) {
        self.update(|s| {
            s.llm_in_flight -= 1;
            s.llm_completed += 1;
            s.llm_response_times.push(start.0.elapsed().as_millis());
        });
    }

    /// Call when an LLM request fails (after retries).
    pub fn llm_call_failed(&self) {
        self.update(|s| {
            s.llm_in_flight -= 1;
            s.llm_failed += 1;
        });
    }

    /// Record token usage if available (no emit, like the TS).
    pub fn record_tokens(&self, tokens: u64, input: Option<u64>, output: Option<u64>) {
        self.with(|s| {
            s.llm_total_tokens += tokens;
            s.llm_input_tokens += input.unwrap_or(0);
            s.llm_output_tokens += output.unwrap_or(0);
            if tokens > 0 {
                s.token_history.push_back((Instant::now(), tokens));
            }
        });
    }

    /// Record an HTTP-level retry (rate limit, server error).
    pub fn llm_retry(&self) {
        self.update(|s| s.llm_retries += 1);
    }

    /// Rolling tokens-per-second over the 30 s window.
    pub fn tokens_per_second(&self) -> f64 {
        self.with(|s| s.tokens_per_second(Instant::now()))
    }

    // ============ Function Metrics ============

    pub fn set_function_total(&self, total: u64) {
        self.update(|s| {
            s.fn_total = total;
            s.fn_pending = total;
        });
    }

    pub fn function_started(&self) {
        self.update(|s| {
            s.fn_in_progress += 1;
            s.fn_ready = s.fn_ready.saturating_sub(1);
        });
    }

    pub fn function_completed(&self) {
        self.update(|s| {
            s.fn_in_progress = s.fn_in_progress.saturating_sub(1);
            s.fn_completed += 1;
        });
    }

    pub fn functions_ready(&self, count: u64) {
        self.update(|s| {
            s.fn_ready += count;
            s.fn_pending = s.fn_pending.saturating_sub(count);
        });
    }

    // ============ Module Binding Metrics ============

    pub fn set_module_binding_total(&self, total: u64) {
        self.update(|s| s.mb_total = total);
    }

    pub fn module_binding_started(&self) {
        self.update(|s| s.mb_in_progress += 1);
    }

    pub fn module_binding_completed(&self) {
        self.update(|s| {
            s.mb_in_progress = s.mb_in_progress.saturating_sub(1);
            s.mb_completed += 1;
        });
    }

    // ============ Retrieval ============

    /// Current metrics snapshot.
    pub fn metrics(&self) -> ProcessingMetrics {
        self.with(State::snapshot)
    }

    /// Force-emit the current metrics (bypasses the throttle).
    pub fn emit(&self) {
        if let Some(callback) = &self.callback {
            let snapshot = self.metrics();
            callback(&snapshot);
        }
    }

    fn emit_throttled(&self) {
        let due = self.with(|s| {
            let now = Instant::now();
            let due = s
                .last_callback
                .is_none_or(|last| now.duration_since(last) >= self.throttle);
            if due {
                s.last_callback = Some(now);
            }
            due
        });
        if due {
            self.emit();
        }
    }

    /// Reset every counter, the stage and the clock.
    pub fn reset(&self) {
        self.with(|s| {
            let last_callback = s.last_callback;
            *s = State::new();
            s.last_callback = last_callback;
        });
    }
}

/// `formatTokens`: 1.2M / 3.4K / 999.
pub fn format_tokens(n: f64) -> String {
    if n >= 1_000_000.0 {
        format!("{}M", to_fixed(n / 1_000_000.0, 1))
    } else if n >= 1_000.0 {
        format!("{}K", to_fixed(n / 1_000.0, 1))
    } else {
        number_to_string(n)
    }
}

/// `formatDuration`: 500ms / 5.0s / 2m 5s / 2h 5m — with the TS's
/// rounding quirks (59999 ms → "60.0s", 3599999 ms → "59m 60s").
pub fn format_duration(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{}ms", number_to_string(ms));
    }
    if ms < 60_000.0 {
        return format!("{}s", to_fixed(ms / 1000.0, 1));
    }
    let mins = (ms / 60_000.0).floor();
    let secs = math_round((ms % 60_000.0) / 1000.0);
    if mins < 60.0 {
        return format!("{}m {}s", number_to_string(mins), number_to_string(secs));
    }
    let hours = (mins / 60.0).floor();
    let remain_mins = mins % 60.0;
    format!(
        "{}h {}m",
        number_to_string(hours),
        number_to_string(remain_mins)
    )
}
