//! The console profile summary (TS: src/profiling/summary.ts). Its JS
//! number text — `formatDuration`, `Number.prototype.toString`, `toFixed`
//! — comes from the one owner, `humanify_model::js`. (Was byte-equal to
//! the TS on the frozen wpb5-profile vectors, retired 2026-09-28 with the
//! other TS-capture replays; the unit tests pin the shapes.)

use std::fmt::Write as _;

use humanify_model::js::{format_duration, to_fixed as js_to_fixed, utf16_len};
use humanify_model::profiling::{ProfileReport, StageSummary};
use serde_json::Value;

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

/// The run-time library index block (exp154, library-names-plan step 3):
/// every span named `library.index.*` (any tid) renders as two small lines
/// after the stage breakdown — what the machinery DID (found / built /
/// failed, read from the metadata of the LAST span with an `outcome`:
/// the wait ends last), then each stage's total in first-seen order.
fn library_index_lines(report: &ProfileReport, lines: &mut Vec<String>) {
    const PREFIX: &str = "library.index.";
    let spans: Vec<&humanify_model::profiling::ProfileSpan> = report
        .spans
        .iter()
        .filter(|s| s.name.starts_with(PREFIX) && s.name.len() > PREFIX.len())
        .collect();
    if spans.is_empty() {
        return;
    }
    lines.push("Library index:".to_string());
    if let Some(meta) = spans
        .iter()
        .rev()
        .find_map(|s| s.metadata.as_ref().filter(|m| m.get("outcome").is_some()))
    {
        let id = meta.get("indexId").and_then(Value::as_str).unwrap_or("-");
        let count = |k: &str| {
            meta.get(k)
                .and_then(Value::as_u64)
                .map(|n| n.to_string())
                .unwrap_or_else(|| "?".to_string())
        };
        match meta.get("outcome").and_then(Value::as_str) {
            Some("found") => lines.push(format!("  found {id} in the index store")),
            Some("built") => lines.push(format!(
                "  built {}: {} version(s) ({} built, {} reused, {} refused)",
                id,
                count("versions"),
                count("built"),
                count("reused"),
                count("refused")
            )),
            Some("failed") => lines.push(format!(
                "  failed{}",
                meta.get("error")
                    .and_then(Value::as_str)
                    .map(|e| format!(": {e}"))
                    .unwrap_or_default()
            )),
            _ => {}
        }
    }
    let mut stages: Vec<(String, f64, usize)> = Vec::new();
    for span in &spans {
        let dur = span.end_ms.0 - span.start_ms.0;
        match stages.iter_mut().find(|(n, _, _)| n == &span.name) {
            Some((_, total, count)) => {
                *total += dur;
                *count += 1;
            }
            None => stages.push((span.name.clone(), dur, 1)),
        }
    }
    let joined = stages
        .iter()
        .map(|(name, total, count)| {
            format!(
                "{} {}{}",
                &name[PREFIX.len()..],
                format_duration(*total),
                if *count > 1 {
                    format!(" ({count})")
                } else {
                    Default::default()
                }
            )
        })
        .collect::<Vec<_>>()
        .join("  ");
    lines.push(format!("  {joined}"));
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

    library_index_lines(report, &mut lines);

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
