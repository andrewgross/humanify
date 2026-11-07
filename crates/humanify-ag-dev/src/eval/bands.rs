//! `eval bands`: per-KPI noise bands from 2+ same-commit repeat labels
//! (TS: `034/noise-bands.ts` + `eval.ts bands`): per KPI, the largest
//! pairwise disagreement between the repeats' totals. A band is a property
//! of a regime — one commit, one naming mode, one SCORER VERSION — so the
//! repeats must share all three (the identity guard's `Bands` purpose:
//! the binary and every leaderboard field), and the bands are written into
//! that mode's slot of that scorer version's section of
//! `noise-bands.json` (scorer 1 at the top; scorer N under
//! `byScorer.<N>`), every other slot kept.

use std::path::Path;

use humanify_ag_model::js::{JsObject, JsValue};

use super::card::{obj, text, texts};
use super::kpis::{KPIS, SPLIT_KPIS};
use crate::identity::{Identity, Purpose, backfill::eval_label, guard};

/// Per KPI, the largest pairwise |disagreement| of the labels' totals;
/// None when fewer than two labels carry it.
pub fn compute_bands(totals: &[JsValue]) -> Vec<(String, Option<f64>)> {
    KPIS.iter()
        .chain(SPLIT_KPIS.iter())
        .map(|k| {
            let values: Vec<f64> = totals.iter().filter_map(|t| k.from_totals(t)).collect();
            if values.len() < 2 {
                return (k.key.clone(), None);
            }
            let mut band = 0.0f64;
            for i in 0..values.len() {
                for j in i + 1..values.len() {
                    band = band.max((values[i] - values[j]).abs());
                }
            }
            (k.key.clone(), Some(band))
        })
        .collect()
}

/// The section written for one mode: provenance + bands (an unmeasured
/// KPI is left out — absent reads as `±?`, never as 0).
fn section(commit: &str, labels: &[String], bands: &[(String, Option<f64>)]) -> JsValue {
    let mut b = JsObject::new();
    for (k, v) in bands {
        if let Some(v) = v {
            b.insert(k.clone(), JsValue::Number(*v));
        }
    }
    obj(vec![
        (
            "provenance",
            obj(vec![
                ("provisional", JsValue::Bool(false)),
                (
                    "sources",
                    texts(&[format!(
                        "measured from {} same-commit repeats",
                        labels.len()
                    )]),
                ),
                ("commit", text(commit)),
                ("labels", texts(labels)),
            ]),
        ),
        ("bands", JsValue::Object(b)),
    ])
}

fn get_obj<'a>(o: &'a mut JsObject, key: &str) -> &'a mut JsObject {
    if !matches!(o.get(key), Some(JsValue::Object(_))) {
        o.insert(key, JsValue::Object(JsObject::new()));
    }
    match o.get_mut(key) {
        Some(JsValue::Object(x)) => x,
        _ => unreachable!("just inserted"),
    }
}

/// Write one (scorer, mode) slot into the bands file's value.
pub fn write_slot(file: &mut JsObject, scorer: u64, mode: &str, slot: JsValue) {
    let JsValue::Object(slot) = slot else { return };
    let target: &mut JsObject = if scorer == 1 {
        file
    } else {
        let by = get_obj(file, "byScorer");
        let key = scorer.to_string();
        if !matches!(by.get(&key), Some(JsValue::Object(_))) {
            // A version's first slot may be a non-off mode: its top level
            // says the off mode was not measured.
            let placeholder = obj(vec![
                (
                    "provenance",
                    obj(vec![
                        ("provisional", JsValue::Bool(true)),
                        ("sources", texts(&["not measured yet"])),
                        ("commit", text("")),
                        ("labels", texts::<&str>(&[])),
                    ]),
                ),
                ("bands", JsValue::Object(JsObject::new())),
            ]);
            by.insert(key.clone(), placeholder);
        }
        get_obj(by, &key)
    };
    if mode == "off" {
        for (k, v) in slot.entries() {
            target.insert(k.clone(), v.clone());
        }
    } else {
        get_obj(target, "byMode").insert(mode, JsValue::Object(slot));
    }
}

/// Measure bands from `labels` (under `results`) and write them into
/// `bands_file`. Returns the line to print.
pub fn bands(results: &Path, labels: &[String], bands_file: &Path) -> Result<String, String> {
    if labels.len() < 2 {
        return Err("usage: eval bands <label> <label> [label...]".into());
    }
    let mut ids: Vec<(String, Identity)> = Vec::new();
    let mut totals = Vec::new();
    let mut commits = Vec::new();
    for l in labels {
        let dir = results.join(l);
        let id = eval_label(&dir)?
            .into_iter()
            .next()
            .ok_or_else(|| format!("{l}: no cards"))?;
        ids.push((l.clone(), id));
        let summary = std::fs::read_to_string(dir.join("summary.json"))
            .map_err(|e| format!("{l}: summary.json: {e} (run `eval summarize {l}`)"))?;
        let s = JsValue::parse(&summary).map_err(|e| format!("{l}: {e}"))?;
        totals.push(
            super::card::at(&s, "totals")
                .cloned()
                .unwrap_or(JsValue::Null),
        );
        commits.push(
            std::fs::read_to_string(dir.join("commit.txt"))
                .map(|c| c.split_whitespace().next().unwrap_or_default().to_string())
                .unwrap_or_else(|_| format!("<missing:{l}>")),
        );
    }
    commits.sort();
    commits.dedup();
    if commits.len() != 1 {
        return Err(format!(
            "labels span {} commits ({}) — two labels from different code measure a CHANGE, not a floor.",
            commits.len(),
            commits.join(", ")
        ));
    }
    for (l, id) in &ids[1..] {
        guard(Purpose::Bands, (&ids[0].0, &ids[0].1), (l, id), false)?;
    }
    let first = &ids[0].1;
    let mode = first
        .naming_mode
        .known()
        .cloned()
        .flatten()
        .unwrap_or_else(|| "off".into());
    let scorer: u64 = first
        .scorer
        .version
        .known()
        .and_then(|v| v.parse().ok())
        .ok_or("the labels record no scorer version")?;
    let measured = compute_bands(&totals);
    let mut file = match std::fs::read_to_string(bands_file) {
        Ok(t) => match JsValue::parse(&t).map_err(|e| format!("{}: {e}", bands_file.display()))? {
            JsValue::Object(o) => o,
            _ => return Err(format!("{}: not an object", bands_file.display())),
        },
        Err(_) => JsObject::new(),
    };
    write_slot(
        &mut file,
        scorer,
        &mode,
        section(&commits[0], labels, &measured),
    );
    std::fs::write(
        bands_file,
        crate::json::js_text(&JsValue::Object(file), true),
    )
    .map_err(|e| format!("{}: {e}", bands_file.display()))?;
    Ok(format!(
        "wrote measured scorer-{scorer} {mode}-mode bands: {}\n",
        bands_file.display()
    ))
}

#[cfg(test)]
#[path = "bands_test.rs"]
mod bands_test;
