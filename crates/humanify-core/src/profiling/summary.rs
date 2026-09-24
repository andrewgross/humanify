//! The console profile summary (TS: src/profiling/summary.ts), plus the
//! JS number formatting it prints through: `formatDuration`
//! (src/llm/metrics.ts — this is its Rust owner; the LLM metrics port
//! reuses it), `Number.prototype.toString` and `toFixed`. Byte-equal to
//! the TS on the frozen vectors (test/parity/wpb5-profile-vectors.json).

use std::fmt::Write as _;

use humanify_model::profiling::{ProfileReport, StageSummary};

/// JS `String(x)` for a number: shortest round-trip digits, positional
/// for 1e-6 ≤ |x| < 1e21, exponent (`1e-7`, `1.5e+21`) outside it.
pub fn js_number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    let mag = x.abs();
    if (1e-6..1e21).contains(&mag) {
        return format!("{x}");
    }
    let e = format!("{x:e}");
    match e.split_once('e') {
        Some((mantissa, exp)) if !exp.starts_with('-') => format!("{mantissa}e+{exp}"),
        _ => e,
    }
}

/// JS `x.toFixed(digits)` for |x| < 1e21: the EXACT decimal value rounded
/// half AWAY from zero (JS picks the larger n on a tie; the sign is split
/// off first). Rust's `{:.N}` rounds an exact tie to even (0.25 → "0.2"
/// where JS prints "0.3"), so the rounding is done here on the exact
/// expansion (1,100 fractional digits covers every f64 exactly).
pub fn js_to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() || x.abs() >= 1e21 {
        return js_number_to_string(x);
    }
    let negative = x < 0.0;
    let exact = format!("{:.1100}", x.abs());
    let (int_part, frac_part) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut kept: Vec<u8> = int_part
        .bytes()
        .chain(frac_part.bytes().take(digits))
        .collect();
    if frac_part.as_bytes().get(digits).is_some_and(|&d| d >= b'5') {
        round_up_decimal(&mut kept);
    }
    let int_len = kept.len() - digits;
    let mut out = String::with_capacity(kept.len() + 2);
    if negative && kept.iter().any(|&d| d != b'0') {
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&kept[..int_len]).unwrap_or("0"));
    if digits > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&kept[int_len..]).unwrap_or(""));
    }
    out
}

/// Adds one unit in the last place of an ASCII digit string.
fn round_up_decimal(digits: &mut Vec<u8>) {
    for d in digits.iter_mut().rev() {
        if *d == b'9' {
            *d = b'0';
        } else {
            *d += 1;
            return;
        }
    }
    digits.insert(0, b'1');
}

/// TS `formatDuration` (src/llm/metrics.ts), accidents included: seconds
/// are ROUNDED under floored minutes, so 3,599,999 ms prints "59m 60s".
pub fn format_duration(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{}ms", js_number_to_string(ms));
    }
    if ms < 60_000.0 {
        return format!("{}s", js_to_fixed(ms / 1000.0, 1));
    }
    let mins = (ms / 60_000.0).floor();
    // JS Math.round = half toward +∞; the operand here is never negative.
    let secs = ((ms % 60_000.0) / 1000.0).round();
    if mins < 60.0 {
        return format!(
            "{}m {}s",
            js_number_to_string(mins),
            js_number_to_string(secs)
        );
    }
    let hours = (mins / 60.0).floor();
    format!(
        "{}h {}m",
        js_number_to_string(hours),
        js_number_to_string(mins % 60.0)
    )
}

/// `s.padEnd(n)` / `s.padStart(n)` — widths in UTF-16 code units.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

fn pad_end(s: &str, n: usize) -> String {
    format!("{s}{}", " ".repeat(n.saturating_sub(utf16_len(s))))
}

fn pad_start(s: &str, n: usize) -> String {
    format!("{}{s}", " ".repeat(n.saturating_sub(utf16_len(s))))
}

fn stage_lines(stages: &[StageSummary], total: f64, lines: &mut Vec<String>) {
    lines.push("Stage breakdown:".to_string());
    let mut sorted: Vec<&StageSummary> = stages.iter().collect();
    // Stable, descending — ties keep report order (JS Array.sort is stable).
    sorted.sort_by(|a, b| {
        b.duration_ms
            .0
            .partial_cmp(&a.duration_ms.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for stage in sorted {
        let pct = if total > 0.0 {
            js_to_fixed(stage.duration_ms.0 / total * 100.0, 1)
        } else {
            "0.0".to_string()
        };
        let mut line = format!(
            "  {} {}  ({pct}%)",
            pad_end(&stage.name, 25),
            pad_start(&format_duration(stage.duration_ms.0), 10)
        );
        if stage.span_count > 1 {
            let _ = write!(line, "  [{} spans]", stage.span_count);
        }
        lines.push(line);
    }
    lines.push(String::new());
}

/// TS `formatProfileSummary`.
pub fn format_profile_summary(report: &ProfileReport) -> String {
    let total = report.meta.total_duration_ms.0;
    let mut lines = vec![
        "=== Performance Profile ===".to_string(),
        format!("Total duration: {}", format_duration(total)),
        String::new(),
    ];

    if !report.stage_summaries.is_empty() {
        stage_lines(&report.stage_summaries, total, &mut lines);
    }

    if let Some(rt) = &report.rename_timing {
        lines.push(format!("Rename timing ({} functions):", rt.count));
        lines.push(format!(
            "  p50: {}  p95: {}  p99: {}",
            format_duration(rt.p50.0),
            format_duration(rt.p95.0),
            format_duration(rt.p99.0)
        ));
        lines.push(format!(
            "  min: {}  max: {}",
            format_duration(rt.min_ms.0),
            format_duration(rt.max_ms.0)
        ));
        lines.push(String::new());
    }

    let snaps = &report.concurrency_snapshots;
    if !snaps.is_empty() {
        #[allow(clippy::cast_precision_loss)]
        let n = snaps.len() as f64;
        let avg_in_flight = snaps.iter().fold(0.0, |s, c| s + f64::from(c.in_flight)) / n;
        let max_in_flight = snaps.iter().map(|c| c.in_flight).max().unwrap_or(0);
        let avg_ready = snaps.iter().fold(0.0, |s, c| s + f64::from(c.ready)) / n;
        lines.push("Concurrency utilization:".to_string());
        lines.push(format!(
            "  avg in-flight: {}  max in-flight: {max_in_flight}",
            js_to_fixed(avg_in_flight, 1)
        ));
        lines.push(format!(
            "  avg ready: {}  samples: {}",
            js_to_fixed(avg_ready, 1),
            snaps.len()
        ));
        lines.push(String::new());
    }

    lines.join("\n")
}
