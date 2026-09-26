//! Progress rendering (TS: `src/ui/progress.ts`).
//!
//! `TtyRenderer`: the in-place dashboard redrawn every 250 ms on stderr;
//! `LineRenderer`: one line per stage change or 5 s, for piped output (what
//! the harness sees — its `.stdout` merges stderr). `message()` is how the
//! driver prints prose AND its `ERROR:` blocks, so the line renderer's
//! `message` writing `text\n` to stderr is contract surface (14 §3).
//!
//! Every byte a scripted session writes is gated against the TS renderers
//! (test/parity/wpb4-vectors.json `progress`, Date.now and the column count
//! stubbed). The clock, the column count and the sink are injected for
//! that; production uses the wall clock, 80 columns (Rust std cannot ask
//! the terminal; TS reads `process.stderr.columns` — dashboard layout only,
//! not contract) and stderr.
//!
//! One deliberate departure (finding #21, fixed): the TS dashboard printed
//! its percent column as `(  34.4)`, the `%` only in the zero case; it now
//! prints `( 34.4%)`, and the recorded vectors carry the corrected bytes.

use std::sync::{Arc, Mutex};

use humanify_llm::metrics::{PipelineStage, ProcessingMetrics, format_duration, format_tokens};
use humanify_model::js::{math_round, number_to_string, to_fixed, utf16_len};

/// Where renderer bytes go, one call per TS `process.stderr.write`.
pub type Sink = Box<dyn FnMut(&str) + Send>;
/// `Date.now()`.
pub type Clock = Box<dyn Fn() -> u64 + Send>;

fn stage_label(stage: PipelineStage) -> &'static str {
    match stage {
        PipelineStage::Parsing => "Parsing",
        PipelineStage::BuildingGraph => "Building dependency graph",
        PipelineStage::Renaming => "Renaming functions & modules",
        PipelineStage::LibraryParams => "Renaming library parameters",
        PipelineStage::LibraryPrefix => "Applying library prefixes",
        PipelineStage::Generating => "Generating output",
        PipelineStage::Done => "Done",
    }
}

/// `n.toLocaleString("en-US")`: grouped integer digits, at most three
/// fraction digits (rounded half-up on the exact value, trailing zeros
/// dropped).
pub fn format_number(n: f64) -> String {
    let fixed = to_fixed(n.abs(), 3);
    let (int, frac) = fixed.split_once('.').unwrap_or((&fixed, ""));
    let frac = frac.trim_end_matches('0');
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let sign = if n < 0.0 && (int != "0" || !frac.is_empty()) {
        "-"
    } else {
        ""
    };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac}")
    }
}

fn build_progress_bar(completed: u64, total: u64, width: usize) -> String {
    if total == 0 {
        return format!("[{}]", "\u{b7}".repeat(width));
    }
    let ratio = (completed as f64 / total as f64).min(1.0);
    let filled = math_round(ratio * width as f64) as usize;
    let empty = width.saturating_sub(filled);
    format!(
        "[{}{}{}]",
        "=".repeat(filled.saturating_sub(1)),
        if filled > 0 { ">" } else { "" },
        "\u{b7}".repeat(empty)
    )
}

/// The dashboard's percent column, `%` included, right-aligned to 6.
fn pct(completed: u64, total: u64) -> String {
    let p = if total == 0 {
        "0.0".to_string()
    } else {
        to_fixed(completed as f64 / total as f64 * 100.0, 1)
    };
    format!("{:>6}", format!("{p}%"))
}

fn compute_eta(fresh_elapsed: f64, total_completed: u64, total_items: u64) -> String {
    let pct_done = if total_items > 0 {
        total_completed as f64 / total_items as f64
    } else {
        0.0
    };
    if pct_done > 0.0 {
        let eta = math_round(fresh_elapsed * (1.0 - pct_done) / pct_done);
        if eta != 0.0 {
            return format_duration(eta);
        }
    }
    "...".to_string()
}

fn truthy(v: Option<u64>) -> Option<u64> {
    v.filter(|x| *x != 0)
}

fn llm_metric_lines(m: &ProcessingMetrics) -> Vec<String> {
    let mut parts = vec![
        format!("{} reqs", format_number(m.llm.completed_calls as f64)),
        format!("{} in-flight", m.llm.in_flight_calls),
        format!("{} failed", m.llm.failed_calls),
        format!("avg {}ms", number_to_string(m.llm.avg_response_time_ms)),
    ];
    if m.llm.retries > 0 {
        parts.push(format!("{} retries", m.llm.retries));
    }
    let mut lines = vec![format!(" LLM        {}", parts.join(" \u{b7} "))];
    if let Some(total) = truthy(m.llm.total_tokens) {
        let mut tok = Vec::new();
        match (truthy(m.llm.input_tokens), truthy(m.llm.output_tokens)) {
            (Some(i), Some(o)) => tok.push(format!(
                "{} in / {} out",
                format_tokens(i as f64),
                format_tokens(o as f64)
            )),
            _ => tok.push(format!("{} total", format_tokens(total as f64))),
        }
        tok.push(format!("{} tok/s", format_number(m.tokens_per_second)));
        lines.push(format!(" Tokens      {}", tok.join(" \u{b7} ")));
    }
    lines
}

/// The renderer interface (TS `ProgressRenderer`).
pub trait ProgressRenderer: Send {
    fn update(&mut self, metrics: &ProcessingMetrics);
    fn message(&mut self, text: &str);
    fn finish(&mut self);
}

/// `createProgressRenderer({tty})` for production: stderr, the wall clock,
/// 80 columns; the TTY dashboard redraws on a 250 ms thread.
pub fn create_progress_renderer(tty: bool) -> Box<dyn ProgressRenderer> {
    let sink: Sink = Box::new(|s: &str| {
        use std::io::Write;
        let _ = std::io::stderr().write_all(s.as_bytes());
    });
    let clock: Clock = Box::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default()
    });
    if tty {
        Box::new(TtyRenderer::start(sink, clock, 80))
    } else {
        Box::new(LineRenderer::new(sink, clock))
    }
}

/// Periodic one-line updates for non-TTY / piped output.
pub struct LineRenderer {
    sink: Sink,
    clock: Clock,
    last_emit_time: u64,
    last_stage: Option<PipelineStage>,
}

const EMIT_INTERVAL_MS: u64 = 5000;

impl LineRenderer {
    pub fn new(sink: Sink, clock: Clock) -> Self {
        LineRenderer {
            sink,
            clock,
            last_emit_time: 0,
            last_stage: None,
        }
    }

    fn format_line(&self, m: &ProcessingMetrics) -> String {
        let total_completed = m.functions.completed + m.module_bindings.completed;
        let total_items = m.functions.total + m.module_bindings.total;
        let p = if total_items > 0 {
            math_round(total_completed as f64 / total_items as f64 * 100.0)
        } else {
            0.0
        };
        let fresh_elapsed = (self.clock)() as f64 - m.start_time as f64;
        let eta = compute_eta(fresh_elapsed, total_completed, total_items);
        let mut line = format!(
            "[{}%] {}/{} functions",
            number_to_string(p),
            format_number(m.functions.completed as f64),
            format_number(m.functions.total as f64)
        );
        if m.module_bindings.total > 0 {
            line.push_str(&format!(
                " | {}/{} modules",
                format_number(m.module_bindings.completed as f64),
                format_number(m.module_bindings.total as f64)
            ));
        }
        line.push_str(&format!(" | LLM: {} in-flight", m.llm.in_flight_calls));
        if m.llm.retries > 0 {
            line.push_str(&format!(", {} retries", m.llm.retries));
        }
        if let (Some(i), Some(o)) = (truthy(m.llm.input_tokens), truthy(m.llm.output_tokens)) {
            line.push_str(&format!(
                " | {} in / {} out",
                format_tokens(i as f64),
                format_tokens(o as f64)
            ));
        }
        line.push_str(&format!(" | ETA: {eta}"));
        line
    }
}

impl ProgressRenderer for LineRenderer {
    fn update(&mut self, m: &ProcessingMetrics) {
        let now = (self.clock)();
        let stage_changed = self.last_stage.is_some_and(|s| s != m.stage);
        self.last_stage = Some(m.stage);
        if stage_changed || now.saturating_sub(self.last_emit_time) >= EMIT_INTERVAL_MS {
            self.last_emit_time = now;
            let line = self.format_line(m);
            (self.sink)(&format!("{line}\n"));
        }
    }

    fn message(&mut self, text: &str) {
        (self.sink)(&format!("{text}\n"));
    }

    fn finish(&mut self) {}
}

struct TtyState {
    sink: Sink,
    clock: Clock,
    columns: usize,
    last_metrics: Option<ProcessingMetrics>,
    last_line_count: usize,
    finished: bool,
    pending: Vec<String>,
    last_stage: Option<PipelineStage>,
}

impl TtyState {
    fn clear_lines(&mut self) {
        if self.last_line_count > 0 {
            let n = self.last_line_count;
            (self.sink)(&format!("\x1b[{n}A"));
            for _ in 0..n {
                (self.sink)("\x1b[2K\n");
            }
            (self.sink)(&format!("\x1b[{n}A"));
            self.last_line_count = 0;
        }
    }

    fn flush_pending(&mut self) {
        for msg in std::mem::take(&mut self.pending) {
            (self.sink)(&format!("{msg}\n"));
        }
    }

    fn redraw(&mut self) {
        if self.finished {
            return;
        }
        let Some(m) = self.last_metrics.clone() else {
            return;
        };
        self.clear_lines();
        if !self.pending.is_empty() {
            self.flush_pending();
        }
        let cols = self.columns;
        let bar_width = 10usize.max(cols.saturating_sub(50));
        let fresh_elapsed = (self.clock)() as f64 - m.start_time as f64;
        let total_completed = m.functions.completed + m.module_bindings.completed;
        let total_items = m.functions.total + m.module_bindings.total;
        let eta = compute_eta(fresh_elapsed, total_completed, total_items);
        let mut lines: Vec<String> = Vec::new();
        let header = " humanify";
        let timing = format!("elapsed {}  ETA {eta}", format_duration(fresh_elapsed));
        let pad = (cols as isize - utf16_len(header) as isize - utf16_len(&timing) as isize).max(1);
        lines.push(format!("{header}{}{timing}", " ".repeat(pad as usize)));
        let stage_line = format!(" \u{2500}\u{2500} {} ", stage_label(m.stage));
        let rule = (cols as isize - utf16_len(&stage_line) as isize - 1).max(0) as usize;
        lines.push(format!("{stage_line}{}", "\u{2500}".repeat(rule)));
        for (label, completed, total) in [
            (" Functions  ", m.functions.completed, m.functions.total),
            (
                " Modules    ",
                m.module_bindings.completed,
                m.module_bindings.total,
            ),
        ] {
            if total > 0 {
                lines.push(format!(
                    "{label}{} {:>8} / {:<8} ({})",
                    build_progress_bar(completed, total, bar_width),
                    format_number(completed as f64),
                    format_number(total as f64),
                    pct(completed, total)
                ));
            }
        }
        lines.extend(llm_metric_lines(&m));
        (self.sink)(&format!("{}\n", lines.join("\n")));
        self.last_line_count = lines.len();
    }

    fn finish(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.clear_lines();
        self.flush_pending();
        if let Some(m) = self.last_metrics.clone() {
            let elapsed = format_duration((self.clock)() as f64 - m.start_time as f64);
            (self.sink)(&format!(" \u{2713} Done in {elapsed}\n"));
            if let Some(total) = truthy(m.llm.total_tokens) {
                let token_detail = match (truthy(m.llm.input_tokens), truthy(m.llm.output_tokens)) {
                    (Some(i), Some(o)) => format!(
                        "{} in / {} out",
                        format_tokens(i as f64),
                        format_tokens(o as f64)
                    ),
                    _ => format!("{} tokens", format_tokens(total as f64)),
                };
                let retry_detail = if m.llm.retries > 0 {
                    format!(" | {} retries", m.llm.retries)
                } else {
                    String::new()
                };
                (self.sink)(&format!(
                    "   {token_detail} | {} LLM calls | {} failed{retry_detail}\n",
                    format_number(m.llm.completed_calls as f64),
                    m.llm.failed_calls
                ));
            }
        }
    }
}

/// The in-place dashboard. State sits behind a mutex shared with the
/// 250 ms redraw thread (`start`); `new` has no thread (the scripted gate).
pub struct TtyRenderer {
    state: Arc<Mutex<TtyState>>,
}

impl TtyRenderer {
    pub fn new(sink: Sink, clock: Clock, columns: usize) -> Self {
        TtyRenderer {
            state: Arc::new(Mutex::new(TtyState {
                sink,
                clock,
                columns,
                last_metrics: None,
                last_line_count: 0,
                finished: false,
                pending: Vec::new(),
                last_stage: None,
            })),
        }
    }

    /// With the TS `setInterval(redraw, 250)`: a thread that redraws until
    /// `finish()`.
    pub fn start(sink: Sink, clock: Clock, columns: usize) -> Self {
        let r = TtyRenderer::new(sink, clock, columns);
        let weak = Arc::downgrade(&r.state);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
                let Some(state) = weak.upgrade() else { return };
                let mut s = state.lock().unwrap();
                if s.finished {
                    return;
                }
                s.redraw();
            }
        });
        r
    }
}

impl ProgressRenderer for TtyRenderer {
    fn update(&mut self, m: &ProcessingMetrics) {
        let mut s = self.state.lock().unwrap();
        if let Some(last) = s.last_stage
            && last != m.stage
            && m.stage != PipelineStage::Done
        {
            let line = format!(
                " \u{2713} {} ({})",
                stage_label(last),
                format_duration(m.elapsed_ms as f64)
            );
            s.pending.push(line);
        }
        s.last_stage = Some(m.stage);
        s.last_metrics = Some(m.clone());
    }

    fn message(&mut self, text: &str) {
        let mut s = self.state.lock().unwrap();
        s.pending.push(text.to_string());
        s.redraw();
    }

    fn finish(&mut self) {
        self.state.lock().unwrap().finish();
    }
}

impl Drop for TtyRenderer {
    /// The TS `process.once("exit", finish)`.
    fn drop(&mut self) {
        if let Ok(mut s) = self.state.lock() {
            s.finish();
        }
    }
}
