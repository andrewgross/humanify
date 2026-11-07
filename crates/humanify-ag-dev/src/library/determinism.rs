//! `library determinism` (experiments/096 `determinism.py`): are two runs
//! of one configuration byte-identical? Compares the two labels' raw
//! discovery reports (they carry no label name), then their cards and
//! summaries with the `label` key aside. On success writes `bands.json`:
//! the score's noise band is ZERO.

use std::path::Path;

use super::corpus::{Corpus, sha256_file};
use super::py::{dict, int, kv, s};
use crate::json::py::PyValue;

const SAME: &[&str] = &[
    "buildCommit",
    "binarySha256",
    "indexId",
    "rule",
    "corpusSha256",
];

fn read(path: &Path) -> Result<PyValue, String> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    PyValue::parse(&t).map_err(|e| format!("{}: {e}", path.display()))
}

fn strip_label(path: &Path) -> Result<String, String> {
    let v = match read(path)? {
        PyValue::Dict(es) => PyValue::Dict(es.into_iter().filter(|(k, _)| k != "label").collect()),
        other => other,
    };
    Ok(crate::json::py_text(&v, true))
}

/// Compare labels `a` and `b` of `corpus` (cards under `results`). Ok:
/// the DETERMINISTIC line and the bands.json text to write; Err(msg, 1)
/// when they differ, Err(msg, 2) when they may not be compared.
pub fn determinism(
    corpus: &Corpus,
    results: &Path,
    a: &str,
    b: &str,
) -> Result<(String, String), (String, u8)> {
    let run = |x: &str| corpus.data.join("results").join(x);
    let pa = read(&run(a).join("pipeline.json")).map_err(|e| (e, 2))?;
    let pb = read(&run(b).join("pipeline.json")).map_err(|e| (e, 2))?;
    let diff: Vec<&str> = SAME
        .iter()
        .copied()
        .filter(|k| pa.get(k) != pb.get(k))
        .collect();
    if !diff.is_empty() {
        return Err((
            format!("REFUSED: the labels differ in {diff:?} — that measures a change, not a floor"),
            2,
        ));
    }
    let mut bad: Vec<(String, String)> = Vec::new();
    let mut n = 0;
    for e in corpus.entries() {
        n += 1;
        let report = |x: &str| run(x).join("reports").join(format!("{}.json", e.id));
        if sha256_file(&report(a)).map_err(|m| (m, 2))?
            != sha256_file(&report(b)).map_err(|m| (m, 2))?
        {
            bad.push(("report".into(), e.id.clone()));
        }
        let card = |x: &str| results.join(x).join("cards").join(format!("{}.json", e.id));
        if strip_label(&card(a)).map_err(|m| (m, 2))?
            != strip_label(&card(b)).map_err(|m| (m, 2))?
        {
            bad.push(("card".into(), e.id.clone()));
        }
    }
    let summary = |x: &str| results.join(x).join("summary.json");
    if strip_label(&summary(a)).map_err(|m| (m, 2))?
        != strip_label(&summary(b)).map_err(|m| (m, 2))?
    {
        bad.push(("summary".into(), "-".into()));
    }
    if !bad.is_empty() {
        let shown: Vec<String> = bad
            .iter()
            .take(10)
            .map(|(k, i)| format!("('{k}', '{i}')"))
            .collect();
        return Err((
            format!(
                "NOT DETERMINISTIC: {} difference(s): [{}]",
                bad.len(),
                shown.join(", ")
            ),
            1,
        ));
    }
    let g = |k: &str| pa.get(k).cloned().unwrap_or(PyValue::Null);
    let bands = dict(vec![
        kv(
            "provenance",
            dict(vec![
                kv("labels", PyValue::List(vec![s(a), s(b)])),
                kv("buildCommit", g("buildCommit")),
                kv("binarySha256", g("binarySha256")),
                kv("indexId", g("indexId")),
                kv("rule", g("rule")),
                kv("corpusSha256", g("corpusSha256")),
                kv(
                    "method",
                    s(
                        "two runs of one configuration: every raw report and every card byte-identical",
                    ),
                ),
            ]),
        ),
        kv("band", int(0)),
        kv(
            "meaning",
            s(
                "library discovery is deterministic (no LLM, no sampling): the noise band of every card column is ZERO, so any movement between labels is a real effect of the change",
            ),
        ),
    ]);
    Ok((
        format!(
            "DETERMINISTIC: {n} reports, {n} cards and the summary byte-identical ({a} vs {b})\n"
        ),
        crate::json::py_text(&bands, true),
    ))
}
