//! `eval cjs-placement` (experiments/172-cjs-app-placement `score.py`):
//! placement precision / recall of the CommonJS app-module rule on
//! bun-dev against the source-map truth. The factory population and each
//! factory's truth bytes come from the phase-0 census (one JSONL row per
//! extracted factory); a run label's diagnostics list the factories it
//! kept in the app (`appModules`, raw declarator spans). App bytes =
//! `<self>` + `<app>`; library bytes = every other labelled byte. The text
//! and the `--json` file are score.py's, byte for byte.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use super::counter::Counter;
use super::py::{dict, flt, int, kv, s};
use crate::json::py::PyValue;

const APP: &[&str] = &["<self>", "<app>"];

struct Moved {
    start: i64,
    end: i64,
    reason: PyValue,
}

/// A number as Python prints `{x:.4f}` / writes it to JSON: an int stays
/// an int (score.py's `0` and `1` fallbacks are ints).
enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    fn f4(&self) -> String {
        match self {
            Num::Int(i) => format!("{:.4}", *i as f64),
            Num::Float(x) => format!("{x:.4}"),
        }
    }
    fn py(&self) -> PyValue {
        match self {
            Num::Int(i) => int(*i),
            Num::Float(x) => flt(*x),
        }
    }
}

/// Python's `str(x)` / `repr(x)` for the census values score.py prints.
fn py_str(v: &PyValue) -> String {
    match v {
        PyValue::Str(x) => x.clone(),
        other => py_repr(other),
    }
}

fn py_repr(v: &PyValue) -> String {
    match v {
        PyValue::Dict(es) => {
            let items: Vec<String> = es
                .iter()
                .map(|(k, v)| format!("{}: {}", py_repr(&PyValue::Str(k.clone())), py_repr(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        PyValue::List(xs) => format!(
            "[{}]",
            xs.iter().map(py_repr).collect::<Vec<_>>().join(", ")
        ),
        PyValue::Str(x) => {
            let q = if x.contains('\'') && !x.contains('"') {
                '"'
            } else {
                '\''
            };
            let body = x.replace('\\', "\\\\");
            let body = if q == '\'' {
                body.replace('\'', "\\'")
            } else {
                body
            };
            format!("{q}{body}{q}")
        }
        PyValue::Null => "None".into(),
        PyValue::Bool(b) => if *b { "True" } else { "False" }.into(),
        PyValue::Int(d) => d.clone(),
        PyValue::Float(x) => crate::json::py::float_repr(*x),
    }
}

fn int_of(v: Option<&PyValue>) -> i64 {
    match v {
        Some(PyValue::Int(d)) => d.parse().unwrap_or(0),
        _ => 0,
    }
}

/// Score `runs` against `census`; (printed text, JSON text).
pub fn score(census: &Path, runs: &Path) -> Result<(String, String), String> {
    let text = std::fs::read_to_string(census).map_err(|e| format!("{}: {e}", census.display()))?;
    let mut rows: BTreeMap<String, Vec<PyValue>> = BTreeMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let r = PyValue::parse(line).map_err(|e| format!("{}: {e}", census.display()))?;
        let b = match r.get("bundle") {
            Some(PyValue::Str(b)) => b.clone(),
            _ => return Err("census row without a bundle".into()),
        };
        rows.entry(b).or_default().push(r);
    }
    let mut tot: Counter<String> = Counter::new();
    let mut per_bundle: Vec<PyValue> = Vec::new();
    let mut per_lines = String::new();
    let mut wrong: Vec<(PyValue, String)> = Vec::new();
    let mut wrong_major = 0;
    let mut missing: Vec<String> = Vec::new();
    for (bundle, rs) in &rows {
        let diag = runs.join(bundle).join("diag.json");
        if !diag.exists() {
            missing.push(bundle.clone());
            continue;
        }
        let d: Value = serde_json::from_slice(
            &std::fs::read(&diag).map_err(|e| format!("{}: {e}", diag.display()))?,
        )
        .map_err(|e| format!("{}: {e}", diag.display()))?;
        let moved: Vec<Moved> = d
            .get("appModules")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|m| Moved {
                start: m["start"].as_i64().unwrap_or(0),
                end: m["end"].as_i64().unwrap_or(0),
                reason: super::py::from_json(&m["reason"]).unwrap_or(PyValue::Null),
            })
            .collect();
        let mut b: Counter<String> = Counter::new();
        for r in rs {
            let empty = PyValue::Dict(vec![]);
            let truth = match r.get("truth") {
                Some(t @ PyValue::Dict(es)) if !es.is_empty() => t,
                _ => &empty,
            };
            let PyValue::Dict(obj) = truth else {
                unreachable!()
            };
            let app: i64 = obj
                .iter()
                .filter(|(k, _)| APP.contains(&k.as_str()))
                .map(|(_, v)| int_of(Some(v)))
                .sum();
            let lib: i64 = obj
                .iter()
                .filter(|(k, _)| !APP.contains(&k.as_str()))
                .map(|(_, v)| int_of(Some(v)))
                .sum();
            let (rs_, re_) = (int_of(r.get("start")), int_of(r.get("end")));
            let hit: Vec<&Moved> = moved
                .iter()
                .filter(|m| rs_ <= m.start && m.end <= re_)
                .collect();
            b.add("factories".into(), 1);
            b.add("appBytes".into(), app);
            b.add("libBytes".into(), lib);
            if let Some(first) = hit.first() {
                b.add("moved".into(), 1);
                b.add("movedAppBytes".into(), app);
                b.add("movedLibBytes".into(), lib);
                if lib != 0 {
                    let major = lib * 2 > app;
                    wrong_major += i64::from(major);
                    let w = dict(vec![
                        kv("libraryMajority", PyValue::Bool(major)),
                        kv("bundle", s(bundle)),
                        kv(
                            "factory",
                            r.get("factory").cloned().unwrap_or(PyValue::Null),
                        ),
                        kv("reason", first.reason.clone()),
                        kv("bytes", r.get("bytes").cloned().unwrap_or(PyValue::Null)),
                        kv("libBytes", int(lib)),
                        kv("appBytes", int(app)),
                        kv("truth", truth.clone()),
                        kv(
                            "vendorFileBefore",
                            r.get("vendorFile").cloned().unwrap_or(PyValue::Null),
                        ),
                    ]);
                    let line = format!(
                        "  {bundle} {} ({}): {lib} lib / {app} app bytes, truth {}, was {}\n",
                        py_str(r.get("factory").unwrap_or(&PyValue::Null)),
                        py_str(&first.reason),
                        py_repr(truth),
                        py_str(r.get("vendorFile").unwrap_or(&PyValue::Null))
                    );
                    wrong.push((w, line));
                }
            }
        }
        let unmatched = moved.len() as i64 - b.get("moved");
        b.set("unmatchedAppModules".into(), unmatched);
        tot.update(&b);
        if b.get("appBytes") != 0 || b.get("moved") != 0 {
            let mut e = vec![kv("bundle", s(bundle))];
            for k in [
                "factories",
                "appBytes",
                "libBytes",
                "moved",
                "movedAppBytes",
                "movedLibBytes",
                "unmatchedAppModules",
            ] {
                e.push(kv(k, int(b.get(k))));
            }
            per_bundle.push(dict(e));
            per_lines.push_str(&format!(
                "  {bundle}: app {}/{} moved, lib moved {}, factories moved {}/{}\n",
                b.get("movedAppBytes"),
                b.get("appBytes"),
                b.get("movedLibBytes"),
                b.get("moved"),
                b.get("factories")
            ));
        }
    }
    let recall = if tot.get("appBytes") != 0 {
        Num::Float(tot.get("movedAppBytes") as f64 / tot.get("appBytes") as f64)
    } else {
        Num::Int(0)
    };
    let moved_all = tot.get("movedAppBytes") + tot.get("movedLibBytes");
    let precision = if moved_all != 0 {
        Num::Float(tot.get("movedAppBytes") as f64 / moved_all as f64)
    } else {
        Num::Int(1)
    };
    let mut out = String::new();
    out.push_str(&format!(
        "TOTAL bundles scored {} (no run: {})\n",
        rows.len() - missing.len(),
        missing.len()
    ));
    out.push_str(&format!(
        "  factories {}, moved to the app {}\n",
        tot.get("factories"),
        tot.get("moved")
    ));
    out.push_str(&format!(
        "  app bytes in factories {}, moved {} (recall {})\n",
        tot.get("appBytes"),
        tot.get("movedAppBytes"),
        recall.f4()
    ));
    out.push_str(&format!(
        "  library bytes moved into the app {} (precision {}); library-majority factories moved: {wrong_major}\n",
        tot.get("movedLibBytes"),
        precision.f4()
    ));
    out.push_str(&format!(
        "  app-module spans with no census factory: {}\n",
        tot.get("unmatchedAppModules")
    ));
    out.push_str("library bytes moved, itemized:\n");
    for (_, line) in &wrong {
        out.push_str(line);
    }
    out.push_str("per bundle (app bytes in factories, or anything moved):\n");
    out.push_str(&per_lines);
    if !missing.is_empty() {
        out.push_str(&format!("no run: {}\n", missing.join(" ")));
    }
    let json = dict(vec![
        kv(
            "totals",
            dict(tot.items().map(|(k, v)| (k.clone(), int(v))).collect()),
        ),
        kv("recall", recall.py()),
        kv("precision", precision.py()),
        kv(
            "wrong",
            PyValue::List(wrong.into_iter().map(|w| w.0).collect()),
        ),
        kv("perBundle", PyValue::List(per_bundle)),
        kv(
            "missingRuns",
            PyValue::List(missing.into_iter().map(PyValue::Str).collect()),
        ),
    ]);
    Ok((out, crate::json::py_text(&json, true)))
}

#[cfg(test)]
#[path = "cjs_placement_test.rs"]
mod cjs_placement_test;
