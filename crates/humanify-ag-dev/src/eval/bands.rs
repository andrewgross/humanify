//! `eval bands`: per-KPI noise bands from 2+ same-commit repeat labels
//! (TS: `034/noise-bands.ts` + `eval.ts bands`): per KPI, the largest
//! pairwise disagreement between the repeats' totals. A band is a property
//! of a regime — one commit, one naming mode, one SCORER VERSION — so the
//! repeats must share all three (the identity guard's `Bands` purpose:
//! the binary and every leaderboard field, on EVERY pair) and the same
//! pair set (exp201: a band is a spread of totals, so it is a property of
//! the pairs too, recorded as `provenance.pairs`), and the bands are written into
//! that mode's slot of that scorer version's section of
//! `noise-bands.json` (scorer 1 at the top; scorer N under
//! `byScorer.<N>`), every other slot kept. The repeats' matching counts
//! (`eval::matching`, exp208) are checked first: an EXACT count that
//! differs refuses the write; the rest's spreads go in as `matchingBands`.

use std::path::Path;

use humanify_ag_model::js::{JsObject, JsValue};

use super::card::{obj, text, texts};
use super::kpis::{KPIS, SPLIT_KPIS};
use crate::identity::{Identity, Purpose, backfill::eval_label, guard_labels};

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
fn section(
    commit: &str,
    labels: &[String],
    pairs: &[String],
    bands: &[(String, Option<f64>)],
    matching: Option<&[(String, f64)]>,
) -> JsValue {
    let mut b = JsObject::new();
    for (k, v) in bands {
        if let Some(v) = v {
            b.insert(k.clone(), JsValue::Number(*v));
        }
    }
    let mut entries = vec![
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
                ("pairs", texts(pairs)),
            ]),
        ),
        ("bands", JsValue::Object(b)),
    ];
    if let Some(m) = matching {
        let mut o = JsObject::new();
        for (k, v) in m {
            o.insert(k.clone(), JsValue::Number(*v));
        }
        entries.push(("matchingBands", JsValue::Object(o)));
    }
    obj(entries)
}

/// The repeats' matching counts (exp208): their spreads, or the
/// NOT-EQUAL lines that refuse the whole slot (an exact count that
/// differs between same-commit repeats is a harness or determinism
/// problem, so their KPI spread is not a floor either). Ok(None) when a
/// repeat records no matching counts.
fn matching_spreads(
    labels: &[String],
    summaries: &[JsValue],
    pairs: &[String],
) -> Result<Option<Vec<(String, f64)>>, String> {
    let digests: Vec<(String, Vec<JsValue>)> = labels
        .iter()
        .zip(summaries)
        .map(|(l, s)| (l.clone(), super::matching::summary_digests(s)))
        .collect();
    let Some(r) = super::matching::repeats(&digests, pairs) else {
        return Ok(None);
    };
    if !r.not_equal.is_empty() {
        return Err(format!(
            "refusing: an EXACT matching count differs between same-commit repeats — a harness or determinism problem, not the LLM; nothing written.\n{}",
            r.not_equal.join("\n")
        ));
    }
    Ok(Some(r.spreads))
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

/// Each repeat's COLD self-hop diff-line count: the label's one
/// `*-self-hop.json` whose cold leg ran on a fresh cache. None (with why)
/// when any repeat has none, or they self-hopped different versions.
fn cold_self_hops(results: &Path, labels: &[String]) -> Result<(String, Vec<u64>), String> {
    let mut version: Option<String> = None;
    let mut lines = Vec::new();
    for l in labels {
        let dir = results.join(l);
        let files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with("-self-hop.json"))
            })
            .collect();
        let [file] = files.as_slice() else {
            return Err(format!("{l} has {} self-hop records, not one", files.len()));
        };
        let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
        let j = JsValue::parse(&text).map_err(|e| format!("{}: {e}", file.display()))?;
        let at = |p: &str| super::card::at(&j, p);
        if at("selfHop.coldCache").and_then(JsValue::as_str) != Some("fresh") {
            return Err(format!("{l}'s self-hop did not run cold"));
        }
        let v = at("selfHop.version")
            .and_then(JsValue::as_str)
            .ok_or_else(|| format!("{l}: self-hop without a version"))?;
        let n = super::card::number_at(&j, "selfHop.diffLines")
            .ok_or_else(|| format!("{l}: self-hop without diffLines"))?;
        match &version {
            Some(seen) if seen != v => {
                return Err(format!("the repeats self-hopped {seen} and {v}"));
            }
            _ => version = Some(v.to_string()),
        }
        lines.push(n as u64);
    }
    Ok((version.unwrap_or_default(), lines))
}

/// Write one naming mode's cold self-hop reference (off = the top level)
/// into `self-hop-reference.json`'s value, every other mode kept.
pub fn write_self_hop(
    file: &mut JsObject,
    mode: &str,
    version: &str,
    labels: &[String],
    lines: &[u64],
    commit: &str,
) {
    let mut cold = JsObject::new();
    let mut commits = JsObject::new();
    let short: String = commit.chars().take(8).collect();
    for (l, n) in labels.iter().zip(lines) {
        cold.insert(l.clone(), JsValue::Number(*n as f64));
        commits.insert(l.clone(), text(&short));
    }
    let comment = format!(
        "The {mode}-mode COLD self-hop range: {} same-commit cold repeats of {short} on scratch bases, the same runs that set noise-bands.json's {mode} slot (written by `cargo dev eval bands`, never by hand). A cold leg replays nothing and the LLM re-rolls, so the count is a range; the WARM leg must be byte-identical with 0 writes.",
        labels.len()
    );
    let entries = [
        ("_comment", text(&comment)),
        ("version", text(version)),
        ("coldDiffLines", JsValue::Object(cold)),
        ("commits", JsValue::Object(commits)),
    ];
    let target: &mut JsObject = if mode == "off" {
        file
    } else {
        let by = get_obj(file, "byMode");
        get_obj(by, mode)
    };
    for (k, v) in entries {
        target.insert(k, v);
    }
}

fn read_object(path: &Path) -> Result<JsObject, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => match JsValue::parse(&t).map_err(|e| format!("{}: {e}", path.display()))? {
            JsValue::Object(o) => Ok(o),
            _ => Err(format!("{}: not an object", path.display())),
        },
        Err(_) => Ok(JsObject::new()),
    }
}

fn write_object(path: &Path, o: JsObject) -> Result<(), String> {
    std::fs::write(path, crate::json::js_text(&JsValue::Object(o), true))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// The repeats' cold self-hops into `self_hop_file`, or why not.
fn self_hop_line(
    results: &Path,
    labels: &[String],
    mode: &str,
    commit: &str,
    self_hop_file: &Path,
) -> Result<String, String> {
    Ok(match cold_self_hops(results, labels) {
        Err(why) => format!("self-hop reference NOT written: {why}\n"),
        Ok((version, lines)) => {
            let mut file = read_object(self_hop_file)?;
            write_self_hop(&mut file, mode, &version, labels, &lines, commit);
            write_object(self_hop_file, file)?;
            format!(
                "wrote the {mode}-mode cold self-hop range for {version} ({}-{} ln): {}\n",
                lines.iter().min().unwrap_or(&0),
                lines.iter().max().unwrap_or(&0),
                self_hop_file.display()
            )
        }
    })
}

/// What the bands verb says about the matching counts it wrote.
fn matching_line(matching: Option<&[(String, f64)]>) -> String {
    match matching {
        None => "matching counts NOT measured: a repeat's summary.json has no matchingDiagnostics for every pair (re-run `eval summarize` on each; a run without stats has none).\n".into(),
        Some(m) => {
            let said: Vec<String> = m
                .iter()
                .map(|(k, v)| {
                    if super::matching::is_exact(k) {
                        format!("{k} equal")
                    } else {
                        format!("{k} ±{}", humanify_ag_model::js::number_to_string(*v))
                    }
                })
                .collect();
            format!(
                "matching counts: every EXACT count equal across the repeats on every pair; spreads written as matchingBands: {}\n",
                said.join(", ")
            )
        }
    }
}

/// Measure bands from `labels` (under `results`) and write them into
/// `bands_file`, and the repeats' cold self-hop range into
/// `self_hop_file`. Returns the lines to print.
pub fn bands(
    results: &Path,
    labels: &[String],
    bands_file: &Path,
    self_hop_file: &Path,
) -> Result<String, String> {
    if labels.len() < 2 {
        return Err("usage: eval bands <label> <label> [label...]".into());
    }
    let mut ids: Vec<(String, Vec<Identity>)> = Vec::new();
    let mut totals = Vec::new();
    let mut summaries = Vec::new();
    let mut commits = Vec::new();
    for l in labels {
        let dir = results.join(l);
        let cards = eval_label(&dir)?;
        if cards.is_empty() {
            return Err(format!("{l}: no cards"));
        }
        ids.push((l.clone(), cards));
        let summary = std::fs::read_to_string(dir.join("summary.json"))
            .map_err(|e| format!("{l}: summary.json: {e} (run `eval summarize {l}`)"))?;
        let s = JsValue::parse(&summary).map_err(|e| format!("{l}: {e}"))?;
        totals.push(
            super::card::at(&s, "totals")
                .cloned()
                .unwrap_or(JsValue::Null),
        );
        summaries.push(s);
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
    let mut pairs: Vec<String> = Vec::new();
    for (l, cards) in &ids[1..] {
        pairs = guard_labels(Purpose::Bands, (&ids[0].0, &ids[0].1), (l, cards), false)?.common;
    }
    let first = &ids[0].1[0];
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
    let matching = matching_spreads(labels, &summaries, &pairs)?;
    let measured = compute_bands(&totals);
    let mut file = read_object(bands_file)?;
    write_slot(
        &mut file,
        scorer,
        &mode,
        section(&commits[0], labels, &pairs, &measured, matching.as_deref()),
    );
    write_object(bands_file, file)?;
    let hop = self_hop_line(results, labels, &mode, &commits[0], self_hop_file)?;
    Ok(format!(
        "wrote measured scorer-{scorer} {mode}-mode bands: {}\n{}{hop}",
        bands_file.display(),
        matching_line(matching.as_deref())
    ))
}

#[cfg(test)]
#[path = "bands_test.rs"]
mod bands_test;
