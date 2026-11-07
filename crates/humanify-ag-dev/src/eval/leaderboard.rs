//! `eval leaderboard <label>...`: labels side by side, the first the
//! baseline (TS: `034/leaderboard.ts`). Before any number is printed, every
//! named label is put through the ONE comparison guard
//! (`identity::guard`, purpose `Leaderboard`): base mode, naming mode,
//! library index, SCORER VERSION, model, reasoning effort and the pair
//! inputs must agree pair by pair, or the comparison is refused —
//! `--force-mixed` prints every mismatch by field and shows it anyway.
//! Each header carries its KPI's direction; a delta inside the measured
//! band of the baseline's (scorer version, naming mode) prints `~0±band`.

use std::collections::BTreeSet;
use std::path::Path;

use humanify_ag_model::js::{JsValue, number_to_string};

use super::card::at;
use super::kpis::{Kpi, caveat_lines, named};
use crate::committed::eval::NoiseBands;
use crate::identity::{Identity, Purpose, backfill::eval_label, guard, label_cards};

const COLUMNS: &[&str] = &[
    "treeLn",
    "cleanLn",
    "noise",
    "noiseLn",
    "novel",
    "realLn",
    "cleanReal",
    "reloc",
    "relocSt",
    "newName",
    "mints",
    "reorderLn",
    "vendorLn",
    "vendorReal",
];

const SPLIT_TABLES: &[(&str, &[&str])] = &[
    (
        "APP (everything outside vendor/; no vendor contribution)",
        &[
            "app:treeLn",
            "app:cleanLn",
            "app:noise",
            "app:noiseLn",
            "app:novel",
            "app:realLn",
            "app:cleanReal",
            "app:reloc",
            "app:relocSt",
            "app:newName",
            "app:mints",
            "app:reorderLn",
        ],
    ),
    (
        "VENDOR (vendor/; statement columns = bundle code filed into vendor/)",
        &[
            "vendor:treeLn",
            "vendor:vendorLn",
            "vendor:vendorReal",
            "vendor:noise",
            "vendor:noiseLn",
            "vendor:novel",
            "vendor:realLn",
            "vendor:reloc",
            "vendor:relocSt",
            "vendor:newName",
            "vendor:mints",
        ],
    ),
    (
        "CROSS-BOUNDARY (moved src/<->vendor/, charged once as a move)",
        &[
            "cross:treeLn",
            "cross:stmts",
            "cross:relocSt",
            "cross:reloc",
            "mints:unattributed",
        ],
    ),
];

struct Label {
    name: String,
    summary: JsValue,
    ids: Vec<Identity>,
    commit: Option<String>,
}

fn load(results: &Path, name: &str) -> Result<Label, String> {
    let dir = results.join(name);
    let text = std::fs::read_to_string(dir.join("summary.json"))
        .map_err(|e| format!("{name}: summary.json: {e} (run `eval summarize {name}`)"))?;
    let summary = JsValue::parse(&text).map_err(|e| format!("{name}: {e}"))?;
    Ok(Label {
        name: name.to_string(),
        summary,
        ids: eval_label(&dir)?,
        commit: std::fs::read_to_string(dir.join("commit.txt"))
            .ok()
            .and_then(|c| c.split_whitespace().next().map(str::to_string)),
    })
}

fn known(f: &crate::identity::F<Option<String>>) -> String {
    match f {
        crate::identity::F::Known(Some(s)) => s.clone(),
        crate::identity::F::Known(None) => "none".into(),
        crate::identity::F::Unknown(_) => "UNKNOWN".into(),
    }
}

/// The guard over two labels: every pair the baseline scored against the
/// same pair of the other label, plus a line when the pair sets differ.
fn compare(base: &Label, other: &Label, force: bool) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    let pairs =
        |l: &Label| -> BTreeSet<String> { l.ids.iter().map(|i| i.corpus.entry.clone()).collect() };
    let (bp, op) = (pairs(base), pairs(other));
    if bp != op {
        let msg = format!(
            "{} and {} scored different pairs ({} vs {})",
            base.name,
            other.name,
            bp.iter().cloned().collect::<Vec<_>>().join(","),
            op.iter().cloned().collect::<Vec<_>>().join(",")
        );
        if !force {
            return Err(format!(
                "refusing: {msg}. Pass --force-mixed if mixing is deliberate."
            ));
        }
        lines.push(format!("!! MIXED PAIRS (--force-mixed): {msg}"));
    }
    for b in &base.ids {
        let Some(o) = other.ids.iter().find(|o| o.corpus.entry == b.corpus.entry) else {
            continue;
        };
        let a_name = format!("{}[{}]", base.name, b.corpus.entry);
        let b_name = format!("{}[{}]", other.name, o.corpus.entry);
        lines.extend(guard(
            Purpose::Leaderboard,
            (&a_name, b),
            (&b_name, o),
            force,
        )?);
    }
    Ok(lines)
}

fn cell(v: Option<f64>, base: Option<f64>, is_base: bool, band: Option<f64>) -> String {
    let Some(v) = v else { return "-".into() };
    let s = number_to_string(v);
    let Some(b) = base else { return s };
    if is_base {
        return s;
    }
    let d = v - b;
    if d == 0.0 {
        return format!("{s} (=)");
    }
    let signed = if d > 0.0 {
        format!("+{}", number_to_string(d))
    } else {
        number_to_string(d)
    };
    match band {
        None => format!("{s} ({signed}±?)"),
        Some(band) if d.abs() <= band => format!("{s} (~0±{})", number_to_string(band)),
        Some(_) => format!("{s} ({signed})"),
    }
}

fn table(
    title: &str,
    shown: &[&Kpi],
    labels: &[Label],
    band: &dyn Fn(&str) -> Option<f64>,
    out: &mut Vec<String>,
) {
    out.push(format!("\n=== {title} ==="));
    let w = 18;
    let mut head = vec![format!("{:<20}", "model")];
    head.extend(
        shown
            .iter()
            .map(|k| format!("{:>w$}", format!("{}{}", k.key, k.direction.mark()))),
    );
    out.push(head.join(" "));
    out.push("-".repeat(20 + shown.len() * (w + 1)));
    let totals = |l: &Label| at(&l.summary, "totals").cloned().unwrap_or(JsValue::Null);
    let base = totals(&labels[0]);
    for (i, l) in labels.iter().enumerate() {
        let t = totals(l);
        let mut row = vec![format!("{:<20}", l.name)];
        for k in shown {
            row.push(format!(
                "{:>w$}",
                cell(
                    k.from_totals(&t),
                    k.from_totals(&base),
                    i == 0,
                    band(&k.key)
                )
            ));
        }
        out.push(row.join(" "));
    }
}

/// The leaderboard text for `names` (under `results`), the bands file's
/// value and `force`.
pub fn leaderboard(
    results: &Path,
    names: &[String],
    bands: Option<&NoiseBands>,
    force: bool,
) -> Result<String, String> {
    if names.is_empty() {
        return Err("usage: eval leaderboard <label> [label...] [--force-mixed]".into());
    }
    let labels: Vec<Label> = names
        .iter()
        .map(|n| load(results, n))
        .collect::<Result<_, _>>()?;
    let mut notes = Vec::new();
    for l in &labels {
        if let Err(e) = label_cards(&l.ids) {
            let msg = format!("{} is itself mixed: {}", l.name, e.join("; "));
            if !force {
                return Err(format!(
                    "refusing: {msg}. Pass --force-mixed if mixing is deliberate."
                ));
            }
            notes.push(format!("!! MIXED LABEL (--force-mixed): {msg}"));
        }
    }
    for other in &labels[1..] {
        notes.extend(compare(&labels[0], other, force)?);
    }
    let first = labels[0]
        .ids
        .first()
        .ok_or_else(|| format!("{}: no cards", labels[0].name))?;
    let mode = known(&first.naming_mode);
    let mode = if mode == "names" { "names" } else { "off" };
    let scorer: u64 = first
        .scorer
        .version
        .known()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let slot = bands.and_then(|b| b.provenance_and_bands(scorer, mode));
    let band = |key: &str| slot.and_then(|(_, b)| b.get(key).copied());

    let mut out = Vec::new();
    let cols = named(COLUMNS);
    table(
        "eval leaderboard (totals across pairs)",
        &cols,
        &labels,
        &band,
        &mut out,
    );
    let mut split_shown: Vec<&Kpi> = Vec::new();
    let any_split = labels.iter().any(|l| {
        at(&l.summary, "totals")
            .and_then(JsValue::as_object)
            .is_some_and(|t| t.entries().iter().any(|(k, _)| k.starts_with("split.")))
    });
    if any_split {
        for (title, keys) in SPLIT_TABLES {
            let shown = named(keys);
            table(title, &shown, &labels, &band, &mut out);
            split_shown.extend(shown);
        }
    }
    out.push(format!(
        "\nbaseline = {} (first listed). ↓ drive to zero · = REAL CODE CHANGE, must not move in either direction · ~ a move means nothing on its own. A column is '-' on models scored before that KPI existed.",
        labels[0].name
    ));
    let all: Vec<&Kpi> = cols.iter().copied().chain(split_shown).collect();
    out.extend(caveat_lines(&all));
    out.push(format!(
        "  identity: {}",
        labels
            .iter()
            .map(|l| {
                let i = l.ids.first();
                format!(
                    "{}=base:{}/naming:{}/scorer:v{}",
                    l.name,
                    i.map_or("?".into(), |i| known(&i.base_mode)),
                    i.map_or("?".into(), |i| known(&i.naming_mode)),
                    i.and_then(|i| i.scorer.version.known().cloned())
                        .unwrap_or_else(|| "?".into())
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    ));
    match slot {
        None => out.push(format!(
            "  bands: NONE for scorer v{scorer} {mode} mode — every nonzero delta prints ±?; no sub-floor claim is possible."
        )),
        Some((p, _)) => {
            out.push(format!("  bands: scorer v{scorer}, {mode} mode (noise-bands.json), measured at {}", p.commit));
            if p.provisional {
                out.push("  bands: PROVISIONAL — produce a measured slot with `eval bands <r1> <r2> [r3]` over same-commit cold repeats.".into());
            }
            let foreign: Vec<String> = labels
                .iter()
                .filter_map(|l| {
                    let c = l.commit.as_ref()?;
                    (!c.starts_with(&p.commit) && !p.commit.starts_with(c.as_str())).then(|| format!("{}@{c}", l.name))
                })
                .collect();
            if !p.provisional && !foreign.is_empty() {
                out.push(format!("  bands: MEASURED AT {}, which is NOT the commit of {}.", p.commit, foreign.join(", ")));
                out.push("    A band is a property of a regime, not of a repo: if the layout or the pipeline changed between those commits, these ± figures are not this run's floor and a delta 'outside the band' may be unmeasured.".into());
                out.push("    Re-measure with `cargo dev eval bands <r1> <r2>` on same-commit cold repeats of the tree you are judging.".into());
            }
        }
    }
    // Endpoints differ by host and are not part of the identity guard
    // (only the endpoint's id is); the provenance still names a spread.
    for (name, path) in [
        ("model", "provenance.models"),
        ("endpoint", "provenance.endpoints"),
        ("reasoning effort", "provenance.reasoningEfforts"),
    ] {
        let mut values: BTreeSet<String> = BTreeSet::new();
        for l in &labels {
            if let Some(JsValue::Array(a)) = at(&l.summary, path) {
                values.extend(a.iter().filter_map(|x| x.as_str().map(str::to_string)));
            }
        }
        if values.len() > 1 && name != "endpoint" {
            out.push(format!(
                "  provenance: these labels span {} {name}s ({}). The naming stage IS an LLM call, so every naming-derived column differs for a reason that is not the change under test.",
                values.len(),
                values.into_iter().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    out.extend(notes);
    Ok(out.join("\n") + "\n")
}

#[cfg(test)]
#[path = "leaderboard_test.rs"]
mod leaderboard_test;
