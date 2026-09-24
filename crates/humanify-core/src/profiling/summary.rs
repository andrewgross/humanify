//! The console profile summary (TS: src/profiling/summary.ts). Its JS
//! number text — `formatDuration`, `Number.prototype.toString`, `toFixed`
//! — comes from the one owner, `humanify_model::js`. Byte-equal to the TS
//! on the frozen vectors (test/parity/wpb5-profile-vectors.json).

use std::fmt::Write as _;

use humanify_model::js::{format_duration, to_fixed as js_to_fixed, utf16_len};
use humanify_model::profiling::{ProfileReport, StageSummary};

/// `s.padEnd(n)` / `s.padStart(n)` — widths in UTF-16 code units.
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
