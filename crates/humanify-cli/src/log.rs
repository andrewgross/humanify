//! The two output loggers (TS: `src/verbose.ts` + `src/debug.ts`) — the
//! process-wide `-v`/`-vv` surfaces.
//!
//! Verbose is level-gated prose (0..2, clamped). Debug is enabled at level
//! 2 and carries the structured LLM-roundtrip/rename/validation formatters
//! the `-vv` log and every census downstream of it consume. Output goes to
//! stdout by default and can be redirected (the `--log-file` stream); the
//! log PROSE is never a gate — what matters is that level gating and
//! redirection behave identically (07 §11: stdout prose is not compared,
//! except `ERROR:` lines, which the CLI-level reporting owns).

use std::io::Write;
use std::sync::{Mutex, OnceLock, RwLock};

type Writer = Box<dyn Fn(&str) + Send + Sync>;

fn stdout_writer() -> Writer {
    Box::new(|text: &str| {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{text}");
    })
}

/// The verbose logger (verbose.ts:10-58): level 0..2, output redirect.
pub struct VerboseLogger {
    level: RwLock<u8>,
    output: Mutex<Option<Writer>>,
}

static VERBOSE: OnceLock<VerboseLogger> = OnceLock::new();

pub fn verbose() -> &'static VerboseLogger {
    VERBOSE.get_or_init(|| VerboseLogger {
        level: RwLock::new(0),
        output: Mutex::new(None),
    })
}

impl VerboseLogger {
    pub fn level(&self) -> u8 {
        *self.level.read().unwrap()
    }

    /// Set the level, clamped to 0..2 (verbose.ts:59-64).
    pub fn set_level(&self, v: u8) {
        *self.level.write().unwrap() = v.clamp(0, 2);
    }

    /// Redirect verbose output to a custom writer (e.g. the log file).
    pub fn set_output(&self, writer: Writer) {
        *self.output.lock().unwrap() = Some(writer);
    }

    /// Reset output to the default (stdout).
    pub fn reset_output(&self) {
        *self.output.lock().unwrap() = None;
    }

    fn emit(&self, msg: &str) {
        let guard = self.output.lock().unwrap();
        match &*guard {
            Some(w) => w(msg),
            None => stdout_writer()(msg),
        }
    }

    /// Level >= 1 prose with a timestamp.
    pub fn log(&self, msg: &str) {
        if self.level() >= 1 {
            self.emit(&format!("[{}] {}", timestamp(), msg));
        }
    }
}

/// The debug logger's enabled gate: level 2 (debug.ts:298-300).
pub fn debug_enabled() -> bool {
    verbose().level() >= 2
}

static DEBUG_OUTPUT: Mutex<Option<Writer>> = Mutex::new(None);

/// Redirect all debug output to a custom writer (debug.ts:302-308).
pub fn debug_set_output(writer: Writer) {
    *DEBUG_OUTPUT.lock().unwrap() = Some(writer);
}

/// Reset debug output to the default (stdout).
pub fn debug_reset_output() {
    *DEBUG_OUTPUT.lock().unwrap() = None;
}

fn debug_write(text: &str) {
    let guard = DEBUG_OUTPUT.lock().unwrap();
    match &*guard {
        Some(w) => w(text),
        None => stdout_writer()(text),
    }
}

fn indent(text: &str, spaces: usize) -> String {
    let prefix = " ".repeat(spaces);
    text.split('\n')
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    format!("{}...", &text[..max_len - 3])
}

/// `[YYYY-MM-DD HH:MM:SS]` in UTC — the TS formatter's shape
/// (`toISOString().replace("T", " ").replace(/\..+/, "")`).
fn timestamp() -> String {
    // Wall clock: log prose only — never a decision input (07 §11).
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let days = secs / 86_400;
    let (y, m, d) = civil_from_days(days as i64);
    let rem = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Days-since-epoch to civil date (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A debug category line (`[ts] [DEBUG:category] message`, body indented 2).
pub fn debug_log(category: &str, message: &str) {
    if !debug_enabled() {
        return;
    }
    debug_write(&format!("\n[{}] [DEBUG:{category}]", timestamp()));
    for line in indent(message, 2).lines() {
        debug_write(line);
    }
}

/// A rename decision line (`[RENAME] old -> new [fn:id]`).
pub fn debug_rename(function_id: &str, old_name: &str, new_name: &str, retry: Option<u32>) {
    if !debug_enabled() {
        return;
    }
    let retry = retry.map(|n| format!(" (retry #{n})")).unwrap_or_default();
    debug_write(&format!(
        "[RENAME] {old_name} -> {new_name}{retry} [fn:{function_id}]"
    ));
}

/// A validation-result block (the four buckets).
pub fn debug_validation(
    valid: &std::collections::BTreeMap<String, String>,
    duplicates: &[String],
    invalid: &[String],
    missing: &[String],
) {
    if !debug_enabled() {
        return;
    }
    let valid_pairs: Vec<String> = valid
        .iter()
        .map(|(k, v)| format!("\"{k}\":\"{v}\""))
        .collect();
    debug_write("\n[VALIDATION RESULT]");
    debug_write(&format!("  Valid: {{{}}}", valid_pairs.join(",")));
    if !duplicates.is_empty() {
        debug_write(&format!("  Duplicates: {}", duplicates.join(", ")));
    }
    if !invalid.is_empty() {
        debug_write(&format!("  Invalid: {}", invalid.join(", ")));
    }
    if !missing.is_empty() {
        debug_write(&format!("  Missing: {}", missing.join(", ")));
    }
}

/// A rename-fallback line with its searchable prefix.
pub fn debug_rename_fallback(parts: &[(&str, String)], context: Option<&str>) {
    if !debug_enabled() {
        return;
    }
    let rendered: Vec<String> = parts.iter().map(|(k, v)| format!("{k}={v}")).collect();
    debug_write(&rendered.join(" "));
    if let Some(ctx) = context {
        let truncated = truncate(ctx, 500);
        for line in indent(&truncated, 2).lines() {
            debug_write(line);
        }
    }
}

/// A QUEUE-STATE line. The formatter is defined here and, as in TS, the
/// emitter is dormant until a scheduler emits it (08's open question).
pub fn debug_queue_state(
    event: &str,
    ready: usize,
    processing: usize,
    pending: usize,
    done: usize,
    total: usize,
    in_flight_llm: usize,
    detail: Option<&str>,
) {
    if !debug_enabled() {
        return;
    }
    let mut parts = vec![
        format!("[{}] [QUEUE-STATE] {event}", timestamp()),
        format!("ready={ready} processing={processing} pending={pending} done={done}/{total}"),
        format!("inflight-llm={in_flight_llm}"),
    ];
    if let Some(d) = detail {
        parts.push(d.to_string());
    }
    debug_write(&parts.join(" | "));
}

/// A full LLM roundtrip block (the `-vv` log's main event), compacted to
/// the fields a census consumes. Prompts are written UNTRUNCATED when given
/// (the user prompt), the response truncated at 2000 chars.
pub fn debug_llm_roundtrip(
    method: &str,
    model: Option<&str>,
    identifiers: &[String],
    system_prompt: Option<&str>,
    user_prompt: Option<&str>,
    raw_response: Option<&str>,
    duration_ms: Option<u128>,
    status_ok: bool,
) {
    if !debug_enabled() {
        return;
    }
    let ts = timestamp();
    let status = if status_ok { "SUCCESS" } else { "ERROR" };
    let duration = duration_ms.map(|d| format!(" ({d}ms)")).unwrap_or_default();
    debug_write(&format!("\n{}", "=".repeat(80)));
    debug_write(&format!("[{ts}] [LLM] {method} - {status}{duration}"));
    if let Some(m) = model {
        debug_write(&format!("Model: {m}"));
    }
    if !identifiers.is_empty() {
        debug_write(&format!("Identifiers: {}", identifiers.join(", ")));
    }
    if let Some(sp) = system_prompt {
        debug_write("\n--- SYSTEM PROMPT ---");
        debug_write(&truncate(sp, 2000));
    }
    if let Some(up) = user_prompt {
        debug_write("\n--- USER PROMPT ---");
        debug_write(up);
    }
    if let Some(raw) = raw_response {
        debug_write("\n--- RAW RESPONSE ---");
        debug_write(&truncate(raw, 2000));
    }
    debug_write(&"=".repeat(80));
}
