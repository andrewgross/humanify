//! `eval rescore`: re-score a label's EXISTING trees with this scorer
//! (TS: `170-app-vendor-split/rescore.ts`) — nothing re-runs, no binary is
//! involved. Writes the new cards, each pair's report page, and copies of
//! the label's run records (statuses, verdicts, manifests, provenance) to
//! the output label directory, then summarizes it. For every pair with an
//! old card it prints the old -> new value of each headline KPI (a v1 card
//! re-scored by v2 is the scorer-version comparison), and it checks every
//! partition of the new card.

use std::path::{Path, PathBuf};

use humanify_ag_model::js::JsValue;

use super::analyze::{PairInputs, partition_failures, score_pair};
use super::card::{is_scorecard, scorer_version};
use super::kpis::KPIS;

/// The run records a label's summary reads, copied beside the new cards.
fn is_run_record(name: &str) -> bool {
    name == "commit.txt"
        || name == "pipeline.json"
        || name == "preflight-status.json"
        || [
            "-run.json",
            "-run-status.json",
            "-boot.json",
            "-self-hop.json",
            ".stats.json",
        ]
        .iter()
        .any(|s| name.ends_with(s))
}

/// One pair to re-score.
struct Job {
    to: String,
    pair: String,
    old: Option<JsValue>,
}

fn read_json(p: &Path) -> Result<JsValue, String> {
    let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
    JsValue::parse(&text).map_err(|e| format!("{}: {e}", p.display()))
}

/// The prior tree of a pair: the manifest's prior, re-rooted under `work`
/// (a frozen copy of the label's trees keeps their names, not their
/// original root), else `<work>/<from>-rebased`.
fn prior_tree(cards: &Path, work: &Path, to: &str, from: &str) -> PathBuf {
    let manifest = cards.join(format!("{to}-run.json"));
    let recorded = read_json(&manifest).ok().and_then(|m| {
        super::card::at(&m, "inputs.prior")?
            .as_str()
            .map(PathBuf::from)
    });
    if let Some(prior) = recorded {
        let tree = prior
            .parent()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_default();
        if let Some(name) = tree.file_name() {
            let moved = work.join(name);
            if moved.is_dir() {
                return moved;
            }
        }
        if tree.is_dir() {
            return tree;
        }
    }
    work.join(format!("{from}-rebased"))
}

/// The stats record of a pair: the label's `<to>.stats.json`, else the
/// diag's coverage block (identical, byte for byte), written beside it.
fn stats_of(cards: &Path, work: &Path, out: &Path, to: &str) -> Result<PathBuf, String> {
    let stats = cards.join(format!("{to}.stats.json"));
    if stats.is_file() {
        return Ok(stats);
    }
    let diag: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(work.join(format!("{to}.diag.json")))
            .map_err(|e| format!("{to}: no stats.json and no diag: {e}"))?,
    )
    .map_err(|e| e.to_string())?;
    let p = out.join(format!("{to}.stats-from-diag.json"));
    std::fs::write(
        &p,
        serde_json::json!({"coverage": diag["coverage"]}).to_string(),
    )
    .map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

/// The jobs: every card in `cards` (copying the run records as it goes).
fn jobs(cards: &Path, out: &Path) -> Result<Vec<Job>, String> {
    let mut names: Vec<String> = std::fs::read_dir(cards)
        .map_err(|e| format!("{}: {e}", cards.display()))?
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    let mut out_jobs = Vec::new();
    for name in names {
        let path = cards.join(&name);
        if !path.is_file() {
            continue;
        }
        if is_run_record(&name) {
            std::fs::copy(&path, out.join(&name)).map_err(|e| format!("{name}: {e}"))?;
        }
        let Some(to) = name.strip_suffix(".json") else {
            continue;
        };
        if to.contains('-')
            || name == "summary.json"
            || name == "pipeline.json"
            || to.ends_with(".stats")
        {
            continue;
        }
        let card = read_json(&path)?;
        if !is_scorecard(&card) {
            continue;
        }
        let pair = super::card::at(&card, "pair")
            .and_then(JsValue::as_str)
            .unwrap_or_default()
            .to_string();
        out_jobs.push(Job {
            to: to.to_string(),
            pair,
            old: Some(card),
        });
    }
    Ok(out_jobs)
}

/// One headline KPI's old -> new line.
fn kpi_delta(old: &JsValue, new: &JsValue) -> Vec<String> {
    KPIS.iter()
        .filter_map(|k| {
            let (a, b) = (k.from_card(old)?, k.from_card(new)?);
            Some(format!(
                "{}={}->{}{}",
                k.key,
                a,
                b,
                if a == b { "" } else { " *" }
            ))
        })
        .collect()
}

/// Re-score `cards`' label from the trees under `work` into `out`. Returns
/// the lines to print and whether every pair scored with its partitions
/// adding up.
pub fn rescore(cards: &Path, work: &Path, out: &Path) -> Result<(Vec<String>, bool), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut lines = Vec::new();
    let mut ok = true;
    for job in jobs(cards, out)? {
        let from = job.pair.split("->").next().unwrap_or_default().to_string();
        let inputs = PairInputs {
            pair: job.pair.clone(),
            fresh_tree: work.join(&job.to),
            prior_tree: prior_tree(cards, work, &job.to, &from),
            stats: stats_of(cards, work, out, &job.to)?,
        };
        let t0 = std::time::Instant::now();
        let card = match score_pair(&inputs) {
            Ok(c) => c,
            Err(e) => {
                lines.push(format!("{}: SCORE FAILED: {e}", job.pair));
                ok = false;
                continue;
            }
        };
        let text = crate::json::js_text(&card, true);
        let card_path = out.join(format!("{}.json", job.to));
        std::fs::write(&card_path, text).map_err(|e| format!("{}: {e}", card_path.display()))?;
        lines.push(format!(
            "{}: scored ({}s)",
            job.pair,
            t0.elapsed().as_secs()
        ));
        if let Some(old) = &job.old {
            lines.push(format!(
                "  v{} -> v{}: {}",
                scorer_version(old),
                scorer_version(&card),
                kpi_delta(old, &card).join(" ")
            ));
        }
        let bad = partition_failures(&card);
        if bad.is_empty() {
            lines.push("  partitions: all add up".into());
        } else {
            ok = false;
            lines.push(format!("  PARTITION FAILED: {}", bad.join("; ")));
        }
        let diag = work.join(format!("{}.diag.json", job.to));
        if diag.is_file() {
            let report = out.join(format!("{}-report.html", job.to));
            if let Err(e) = super::report::write_report(&diag, &report, &inputs) {
                lines.push(format!("  REPORT PAGE FAILED: {e}"));
            }
        } else {
            lines.push(format!(
                "  NO DIAGNOSTICS TRAIL at {} — report page not written",
                diag.display()
            ));
        }
    }
    Ok((lines, ok))
}

#[cfg(test)]
#[path = "rescore_test.rs"]
mod rescore_test;
