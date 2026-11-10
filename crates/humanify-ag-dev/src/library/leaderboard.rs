//! `library leaderboard` (experiments/096 `leaderboard.py`): labels side
//! by side, totals first, deltas against the FIRST label — the same text.
//!
//! Which labels may share a table is THE comparison guard's call
//! (`identity::guard(Purpose::LibraryLeaderboard, ..)`): corpus, corpus
//! manifest, every truth layer (families included) and scorer version must
//! be KNOWN and equal. leaderboard.py checked the corpus manifest, the
//! origin layer and the scorer only; the guard is stricter on purpose.

use std::path::Path;

use crate::identity::{self, Purpose, backfill};
use crate::json::py::PyValue;

const SETS: &[(&str, &str)] = &[
    (
        "bun-dev-bun",
        "dev apps (45), Bun build, target=bun, minified (exp150)",
    ),
    (
        "bun-dev-node",
        "dev apps (45), Bun build, target=node, minified (exp150)",
    ),
    (
        "bun-held-bun",
        "held-out apps (15), Bun target=bun (exp150; summaries only, never diagnosed)",
    ),
    (
        "bun-held-node",
        "held-out apps (15), Bun target=node (exp150; summaries only, never diagnosed)",
    ),
    ("cc", "Claude Code 2.1.216 (sample est.)"),
    ("heldout", "held-out apps (67, Bun)"),
    ("wrangler", "wrangler (esbuild, re-minified)"),
    ("esbuild-app", "task-master-ai (esbuild)"),
    (
        "test-bun",
        "TEST split: new apps, Bun (the held-out recipe)",
    ),
    (
        "test-esbuild",
        "TEST split: the same apps, esbuild (the index's esbuild recipe)",
    ),
    (
        "test-webpack",
        "TEST split: the same apps, webpack + terser",
    ),
    ("test-rollup", "TEST split: the same apps, rollup + terser"),
    (
        "test2-bun",
        "TEST v2 split (exp116, untouched judge): new apps, Bun",
    ),
    ("test2-esbuild", "TEST v2 split: the same apps, esbuild"),
    (
        "test2-webpack",
        "TEST v2 split: the same apps, webpack + terser",
    ),
    (
        "test2-rollup",
        "TEST v2 split: the same apps, rollup + terser",
    ),
    (
        "dev-webpack",
        "exp105 dev apps, webpack + terser (the index's webpack recipe)",
    ),
    (
        "dev-rollup",
        "exp105 dev apps, rollup + terser (the index's rollup recipe)",
    ),
];

struct Row {
    label: String,
    summary: PyValue,
    timing: PyValue,
}

fn num(v: Option<&PyValue>) -> f64 {
    match v {
        Some(PyValue::Int(d)) => d.parse().unwrap_or(0.0),
        Some(PyValue::Float(x)) => *x,
        _ => 0.0,
    }
}

fn text(v: Option<&PyValue>) -> &str {
    match v {
        Some(PyValue::Str(s)) => s,
        _ => "",
    }
}

fn pct(x: f64) -> String {
    format!("{:5.1}%", 100.0 * x)
}

fn delta(r: &PyValue, b: &PyValue, key: &str) -> String {
    format!("{:+5.1}", 100.0 * (num(r.get(key)) - num(b.get(key))))
}

fn p_bar(bars: &PyValue) -> String {
    match bars.get("precisionBarPassed") {
        Some(PyValue::Dict(es)) => {
            let passed = es.iter().filter(|(_, v)| *v == PyValue::Bool(true)).count();
            format!("{passed}/{} sets pass", es.len())
        }
        _ => "0/0 sets pass".into(),
    }
}

fn scopes_line(scopes: &PyValue) -> String {
    let st = scopes.get("statement").expect("statement scope");
    let fa = scopes.get("factory").expect("factory scope");
    let lib = num(st.get("libraryBytes")) + num(fa.get("libraryBytes"));
    let share = if lib != 0.0 {
        num(fa.get("libraryBytes")) / lib
    } else {
        0.0
    };
    format!(
        "statement scope {}/{}/F0.5 {}  |  factory scope {}/{}/F0.5 {}  ({} of library bytes)",
        pct(num(st.get("precision"))),
        pct(num(st.get("recall"))),
        pct(num(st.get("f05"))),
        pct(num(fa.get("precision"))),
        pct(num(fa.get("recall"))),
        pct(num(fa.get("f05"))),
        pct(share)
    )
}

/// Scorer 15's columns (exp214), on a label that has them: the credited
/// copies inside the precision, the map-defect bytes out of both sides.
fn scorer15_line(r: &PyValue) -> Option<String> {
    let credit = num(Some(r.get("namedAsCopyCreditBytes")?));
    Some(format!(
        "scorer 15: P {:.6} (without copy credit {:.6})  copy credit {:.1} KB  defect excluded {:.1} KB (claimed {:.1} KB)",
        num(r.get("precision")),
        num(r.get("precisionNoCopyCredit")),
        credit / 1e3,
        num(r.get("defectExcludedBytes")) / 1e3,
        num(r.get("defectExcludedClaimedBytes")) / 1e3,
    ))
}

fn read(path: &Path) -> Result<PyValue, String> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    PyValue::parse(&t).map_err(|e| format!("{}: {e}", path.display()))
}

/// The leaderboard text for `labels` (under `results`), or the refusal.
pub fn leaderboard(
    results: &Path,
    bands_exist: bool,
    labels: &[String],
    force_mixed: bool,
) -> Result<String, String> {
    if labels.is_empty() {
        return Err("usage: library leaderboard <label> <label> [label...] [--force-mixed]".into());
    }
    let mut out = String::new();
    let mut rows = Vec::new();
    let mut ids = Vec::new();
    for lab in labels {
        let d = results.join(lab);
        let summary = read(&d.join("summary.json"))?;
        let tp = d.join("timing.json");
        let timing = if tp.exists() {
            read(&tp)?
        } else {
            PyValue::Dict(vec![])
        };
        let mut id = backfill::library_label(&d)?
            .into_iter()
            .next()
            .ok_or_else(|| format!("{lab}: no identity"))?;
        id.corpus.entry = "*".into();
        ids.push(id);
        rows.push(Row {
            label: lab.clone(),
            summary,
            timing,
        });
    }
    for (row, id) in rows.iter().zip(&ids).skip(1) {
        for line in identity::guard(
            Purpose::LibraryLeaderboard,
            (&rows[0].label, &ids[0]),
            (&row.label, id),
            force_mixed,
        )? {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out.push_str(if bands_exist {
        "band: 0 (measured: two runs byte-identical, bands.json) — every delta is real\n"
    } else {
        "band: UNMEASURED (no bands.json) — run `library determinism` first\n"
    });
    out.push('\n');
    let w = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    out.push_str(&format!(
        "{:w$}  commit        index         index MB  rule        app bar  P bar           wall s  peak MB\n",
        "label"
    ));
    for r in &rows {
        let c = r.summary.get("config").expect("config");
        let bars = r.summary.get("bars").expect("bars");
        let head = |k: &str| text(c.get(k)).chars().take(12).collect::<String>();
        out.push_str(&format!(
            "{:w$}  {}  {}  {:8.0}  {:10}  {:7}  {:14}  {:6.0}  {:7.0}\n",
            r.label,
            head("buildCommit"),
            head("indexId"),
            num(c.get("indexBytes")) / 1e6,
            text(c.get("rule")),
            if bars.get("appBarPassed") == Some(&PyValue::Bool(true)) {
                "PASS"
            } else {
                "FAIL"
            },
            p_bar(bars),
            num(r.timing.get("totalWallSeconds")),
            num(r.timing.get("maxPeakRssMb")),
        ));
    }
    out.push_str(
        "\nprecision / recall / F0.5 (the headline: beta 0.5, precision over recall) / F1 over bytes, the ORIGIN-AWARE truth (a claim naming the shipping package OR, for a\
         \nvendored / pre-bundled copy, its origin is right). precision = named / (named + wrong family + app\
         \nclaimed); bar: precision >= 99.8% and app claimed = 0. [shipper] = the old shipper-only truth; as origin =\
         \nnamed bytes right only through the origin; deltas against the first label\n",
    );
    let base = rows[0].summary.get("sets").expect("sets");
    for (set, title) in SETS {
        if rows
            .iter()
            .all(|r| r.summary.get("sets").and_then(|s| s.get(set)).is_none())
        {
            continue;
        }
        out.push_str(&format!("\n{title}\n"));
        out.push_str(&format!(
            "  {:w$}  precision (delta)  recall  (delta)    F0.5  (delta)      F1  (delta)  [shipper P/R]  as origin  \
             app claimed  wrong fam KB  not indexed  version  too small  hash differs  guard  other pkg  no discovery\n",
            "label"
        ));
        for row in &rows {
            let Some(r) = row.summary.get("sets").and_then(|s| s.get(set)) else {
                out.push_str(&format!("  {:w$}  (absent)\n", row.label));
                continue;
            };
            let b = base.get(set).unwrap_or(r);
            let m = r.get("missShares").expect("missShares");
            let ms = |k: &str| num(m.get(k));
            let sum_prefix = |p: &str| match m {
                PyValue::Dict(es) => es
                    .iter()
                    .filter(|(k, _)| k.starts_with(p))
                    .map(|(_, v)| num(Some(v)))
                    .fold(0.0, |a, x| a + x),
                _ => 0.0,
            };
            let lib = num(r.get("libraryBytes"));
            let origin = if lib != 0.0 {
                num(r.get("namedAsOriginBytes")) / lib
            } else {
                0.0
            };
            let shipper = format!(
                "{}/{}",
                pct(num(r.get("precisionShipperOnly"))),
                pct(num(r.get("recallShipperOnly")))
            );
            out.push_str(&format!(
                "  {:w$}  {:>9} ({})  {} ({})  {} ({})  {} ({})  {:>13}  {:>9}  {:11}  {:12.1}  {:>11}  {:>7}  {:>9}  {:>12}  {:>5}  {:>9}  {:>12}\n",
                row.label,
                pct(num(r.get("precision"))),
                delta(r, b, "precision"),
                pct(num(r.get("recall"))),
                delta(r, b, "recall"),
                pct(num(r.get("f05"))),
                delta(r, b, "f05"),
                pct(num(r.get("f1"))),
                delta(r, b, "f1"),
                shipper,
                pct(origin),
                num(r.get("appClaimedBytes")) as i64,
                num(r.get("wrongFamilyBytes")) / 1e3,
                pct(ms("a")),
                pct(ms("b")),
                pct(ms("c")),
                pct(sum_prefix("d-")),
                pct(sum_prefix("e-")),
                pct(ms("f")),
                pct(ms("no-discovery")),
            ));
            if let Some(sc) = r.get("scopes") {
                out.push_str(&format!("  {:w$}  {}\n", "", scopes_line(sc)));
            }
            if let Some(line) = scorer15_line(r) {
                out.push_str(&format!("  {:w$}  {line}\n", ""));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "leaderboard_test.rs"]
mod leaderboard_test;
