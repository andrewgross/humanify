//! `eval vendor-precision` (experiments/166-restore-or-ask
//! `vendor_precision.py` + `aggregate_precision.py`): what names mode puts
//! under `vendor/`, judged against source-map truth whether or not it was
//! claimed, from ONE run's diagnostics file — in two worlds over the same
//! era body: `exact` (the binary's raw-to-era bridge) and `ordinal` (the
//! retired `i - removed_before` conversion). The card is the JSON
//! vendor_precision.py prints, byte for byte; the aggregate is
//! aggregate_precision.py's.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use serde_json::Value;

use super::corpus::Corpus;
use super::counter::Counter;
use super::families::Families;
use super::py::{dict, from_json, int, kv};
use crate::json::py::PyValue;

/// The Claude Code 2.1.216 labelled sample (never committed).
pub const CC_TRUTH: &str = "/work/cc-library-truth/2.1.216/labels.jsonl";

const NOT_LIBRARY: &[&str] = &["<app>", "<self>", "<unmapped>", "<foreign>"];

/// The truth a check reads: per raw span, its labelled bytes by label.
pub enum TruthAt {
    /// bun-dev: the frozen segment table (raw, as truth.json.gz holds it).
    Segments {
        offs: Vec<i64>,
        srcs: Vec<i64>,
        labels: Vec<String>,
    },
    /// cc216: the labelled sample's units INSIDE the span.
    Sample { units: Vec<(i64, i64, String)> },
}

impl TruthAt {
    fn at(&self, s: i64, e: i64) -> Counter<String> {
        let mut votes = Counter::new();
        match self {
            TruthAt::Segments { offs, srcs, labels } => {
                let mut k = offs.partition_point(|&o| o <= s).saturating_sub(1);
                while k < offs.len() && offs[k] < e {
                    let a = offs[k].max(s);
                    let b = if k + 1 < offs.len() { offs[k + 1] } else { e }.min(e);
                    if b > a {
                        votes.add(labels[srcs[k] as usize].clone(), b - a);
                    }
                    k += 1;
                }
            }
            TruthAt::Sample { units } => {
                let mut k = units.partition_point(|u| u.0 < s);
                while k < units.len() && units[k].0 < e {
                    let (us, ue, label) = &units[k];
                    if *ue <= e {
                        votes.add(label.clone(), ue - us);
                    }
                    k += 1;
                }
            }
        }
        votes
    }
}

#[derive(serde::Deserialize)]
struct RawTruth {
    segments: Vec<(i64, i64)>,
    sources: Vec<Vec<Value>>,
}

/// vendor_precision.py `truth_bundev`: the entry's raw truth (no
/// scorer corrections; the file is read without its sha check, as there).
pub fn truth_bundev(repo: &Path, entry_id: &str) -> Result<TruthAt, String> {
    let corpus = Corpus::open(repo, "bun-dev", None)?;
    let entry = corpus
        .entries()
        .iter()
        .find(|e| e.id == entry_id)
        .ok_or_else(|| format!("no bun-dev entry {entry_id}"))?;
    let t: RawTruth =
        super::corpus::read_gz_json(&corpus.truth_file(entry, ".truth.json.gz"), None, "")?;
    Ok(TruthAt::Segments {
        offs: t.segments.iter().map(|s| s.0).collect(),
        srcs: t.segments.iter().map(|s| s.1).collect(),
        labels: t
            .sources
            .iter()
            .map(|r| r.first().and_then(Value::as_str).unwrap_or("").to_string())
            .collect(),
    })
}

/// vendor_precision.py `truth_cc`: the 620-unit labelled sample.
pub fn truth_cc(labels_jsonl: &Path) -> Result<TruthAt, String> {
    let text = std::fs::read_to_string(labels_jsonl)
        .map_err(|e| format!("{}: {e}", labels_jsonl.display()))?;
    let mut units = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value =
            serde_json::from_str(line).map_err(|e| format!("{}: {e}", labels_jsonl.display()))?;
        let label = row["label"].as_str().unwrap_or("");
        let label = if label == "app" {
            "<app>".to_string()
        } else if label.starts_with("library") {
            match row
                .get("pkg")
                .and_then(Value::as_str)
                .filter(|p| !p.is_empty())
            {
                Some(p) => p.to_string(),
                None => label.split_once(':').map_or(label, |x| x.1).to_string(),
            }
        } else {
            format!("<{label}>")
        };
        let n = |k: &str| {
            row[k]
                .as_i64()
                .ok_or_else(|| format!("labels.jsonl: no {k}"))
        };
        units.push((n("start")?, n("end")?, label));
    }
    units.sort();
    Ok(TruthAt::Sample { units })
}

/// `vendor/<pkg>/…` → Some(Some(pkg)); a file at the vendor root →
/// Some(None); not under vendor/ → None.
fn vendor_package(path: &str) -> Option<Option<String>> {
    let rest = path.strip_prefix("vendor/")?;
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 2 {
        return Some(None);
    }
    if parts[0].starts_with('@') && parts.len() >= 3 {
        return Some(Some(format!("{}/{}", parts[0], parts[1])));
    }
    Some(Some(parts[0].to_string()))
}

fn loose(p: &str) -> String {
    p.replace('\\', "/").trim_matches('/').to_lowercase()
}

/// One population's counts: ints, and file sets.
#[derive(Default)]
struct Out {
    n: BTreeMap<String, i64>,
    sets: BTreeMap<String, BTreeSet<String>>,
}

impl Out {
    fn add(&mut self, k: &str, v: i64) {
        *self.n.entry(k.to_string()).or_insert(0) += v;
    }
    fn file(&mut self, k: &str, path: &str) {
        self.sets
            .entry(k.to_string())
            .or_default()
            .insert(path.to_string());
    }
    fn jsonable(&self) -> PyValue {
        let mut e: Vec<(String, PyValue)> =
            self.n.iter().map(|(k, v)| (k.clone(), int(*v))).collect();
        for (k, v) in &self.sets {
            e.push((
                k.clone(),
                PyValue::List(v.iter().map(|p| PyValue::Str(p.clone())).collect()),
            ));
        }
        dict(e)
    }
}

type Span = (i64, i64);

fn span_of(v: &Value) -> Option<Span> {
    let a = v.as_array()?;
    Some((a.first()?.as_i64()?, a.get(1)?.as_i64()?))
}

struct Check<'a> {
    er: &'a Value,
    trails: Vec<(i64, Option<String>)>,
    index_of: HashMap<Span, usize>,
    raw: Vec<Option<Span>>,
    factories: Vec<(Option<Span>, String)>,
    truth: Option<&'a TruthAt>,
    fam: Option<&'a Families>,
}

impl Check<'_> {
    fn same(&self, a: &str, b: &str) -> bool {
        loose(a) == loose(b) || self.fam.is_some_and(|f| f.same(Some(a), Some(b)))
    }

    fn judge(&self, population: &str, path: Option<&str>, span: Option<Span>, out: &mut Out) {
        let Some(path) = path else { return };
        let (Some(pkg), Some(span)) = (vendor_package(path), span) else {
            return;
        };
        let p = |k: &str| format!("{population}:{k}");
        out.add(&p("bytes"), span.1 - span.0);
        out.file(&p("files"), path);
        let Some(truth) = self.truth else { return };
        let votes = truth.at(span.0, span.1);
        let mut labelled = 0;
        for (label, b) in votes.items() {
            if label == "<app>" {
                out.add(&p("app"), b);
                out.file(&p("appFiles"), path);
            } else if label == "<self>" {
                out.add(&p("self"), b);
            } else if NOT_LIBRARY.contains(&label.as_str()) || label.starts_with('<') {
                out.add(&p("other"), b);
            } else {
                match &pkg {
                    None => out.add(&p("libraryAtRoot"), b),
                    Some(pkg) if self.same(pkg, label) => out.add(&p("libraryRight"), b),
                    Some(_) => {
                        out.add(&p("libraryWrongPackage"), b);
                        out.file(&p("wrongFiles"), path);
                    }
                }
            }
            labelled += b;
        }
        out.add(&p("labelled"), labelled);
    }

    fn moved_statements(&self, files: &Value) -> Vec<(usize, String)> {
        // {era index: path}, later files overriding earlier (a dict).
        let mut out: Vec<(usize, String)> = Vec::new();
        for f in files.as_array().into_iter().flatten() {
            let (Some(path), Some(spans)) = (
                f.get(0).and_then(Value::as_str),
                f.get(1).and_then(Value::as_array),
            ) else {
                continue;
            };
            for span in spans {
                if let Some(i) = span_of(span).and_then(|s| self.index_of.get(&s)) {
                    match out.iter_mut().find(|(j, _)| j == i) {
                        Some(slot) => slot.1 = path.to_string(),
                        None => out.push((*i, path.to_string())),
                    }
                }
            }
        }
        out
    }

    fn world(&self, regions: &Value, moved_files: &Value) -> PyValue {
        let mut out = Out::default();
        for s in ["files", "appFiles", "wrongFiles"] {
            out.sets.insert(format!("era:{s}"), BTreeSet::new());
        }
        let moved = self.moved_statements(moved_files);
        let empty = Value::Array(vec![]);
        let mut both: std::collections::HashSet<usize> = self
            .moved_statements(self.er.get("movedFiles").unwrap_or(&empty))
            .into_iter()
            .map(|x| x.0)
            .collect();
        both.extend(
            self.moved_statements(self.er.get("ordinalMovedFiles").unwrap_or(&empty))
                .into_iter()
                .map(|x| x.0),
        );
        let mut seen: std::collections::HashSet<(Option<String>, Option<Span>)> =
            Default::default();
        let mut placed: Vec<(usize, Option<String>)> =
            moved.iter().map(|(i, p)| (*i, Some(p.clone()))).collect();
        placed.extend(
            self.trails
                .iter()
                .filter(|(i, _)| !(*i >= 0 && both.contains(&(*i as usize))))
                .map(|(i, f)| (*i as usize, f.clone())),
        );
        for (i, path) in placed {
            let raw = self.raw.get(i).copied().flatten();
            if !seen.insert((path.clone(), raw)) {
                continue;
            }
            self.judge("era", path.as_deref(), raw, &mut out);
        }
        let mut claim_of: HashMap<Span, Option<String>> = HashMap::new();
        for region in regions.as_array().into_iter().flatten() {
            out.add("regions", 1);
            let st = region
                .get("statements")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let raws = region
                .get("rawSpans")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let claims = region
                .get("claims")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for ((st, raw), claim) in st.iter().zip(&raws).zip(&claims) {
                let Some(raw) = span_of(raw) else { continue };
                claim_of.insert(raw, claim.as_str().map(String::from));
                out.add("regionStatements", 1);
                let st = st.as_i64().unwrap_or(-1);
                if st >= 0 && (st as usize) < self.raw.len() && self.raw[st as usize] == Some(raw) {
                    out.add("regionStatementsContentCorrect", 1);
                }
            }
        }
        for (i, path) in &moved {
            let raw = self.raw.get(*i).copied().flatten();
            let claim = raw.and_then(|r| claim_of.get(&r).cloned().flatten());
            out.add("movedStatements", 1);
            let bytes = raw.map_or(0, |r| r.1 - r.0);
            out.add("movedBytes", bytes);
            let pkg = vendor_package(path).flatten();
            let ok = matches!((&claim, &pkg), (Some(c), Some(p)) if self.same(p, c));
            if !ok {
                out.add("movedStatementsNotClaimedByMover", 1);
                out.add("movedBytesNotClaimedByMover", bytes);
            }
        }
        out.n.insert(
            "movedModules".into(),
            moved_files.as_array().map_or(0, |a| a.len() as i64),
        );
        out.jsonable()
    }

    fn factory(&self) -> PyValue {
        let mut out = Out::default();
        for s in ["files", "appFiles", "wrongFiles"] {
            out.sets.insert(format!("factory:{s}"), BTreeSet::new());
        }
        for (span, path) in &self.factories {
            self.judge("factory", Some(path), *span, &mut out);
        }
        out.jsonable()
    }
}

/// vendor_precision.py `run`: the card for one diagnostics file.
pub fn check(
    diag: &Value,
    truth: Option<&TruthAt>,
    fam: Option<&Families>,
) -> Result<PyValue, String> {
    let empty = Value::Object(Default::default());
    let er = diag
        .get("eraRestoration")
        .filter(|v| !v.is_null())
        .unwrap_or(&empty);
    if er.get("bodyRawSpans").is_none() {
        return Ok(dict(vec![kv("noBridge", int(1))]));
    }
    let trails = diag["placementTrails"]["trails"]
        .as_array()
        .ok_or("placementTrails.trails")?
        .iter()
        .map(|t| {
            (
                t["index"].as_i64().unwrap_or(-1),
                t.get("file").and_then(Value::as_str).map(String::from),
            )
        });
    // {index: trail}: a later trail of the same index replaces it in place.
    let mut tr: Vec<(i64, Option<String>)> = Vec::new();
    for (i, f) in trails {
        match tr.iter_mut().find(|(j, _)| *j == i) {
            Some(slot) => slot.1 = f,
            None => tr.push((i, f)),
        }
    }
    let body: Vec<Option<Span>> = er
        .get("bodySpans")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(span_of)
        .collect();
    let mut index_of = HashMap::new();
    for (i, s) in body.iter().enumerate() {
        if let Some(s) = s {
            index_of.insert(*s, i);
        }
    }
    let raw = er["bodyRawSpans"]
        .as_array()
        .into_iter()
        .flatten()
        .map(span_of)
        .collect();
    let factories = er
        .get("factoryFiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|f| (span_of(&f[0]), f[1].as_str().unwrap_or("").to_string()))
        .collect();
    let c = Check {
        er,
        trails: tr,
        index_of,
        raw,
        factories,
        truth,
        fam,
    };
    let empty_list = Value::Array(vec![]);
    let g = |k: &str| er.get(k).unwrap_or(&empty_list);
    Ok(dict(vec![
        kv("exact", c.world(g("regions"), g("movedFiles"))),
        kv(
            "ordinal",
            c.world(g("ordinalRegions"), g("ordinalMovedFiles")),
        ),
        kv("factory", c.factory()),
        kv(
            "conversion",
            er.get("conversion").map_or(Ok(PyValue::Null), from_json)?,
        ),
    ]))
}

/// The card's text as vendor_precision.py prints it.
pub fn card_text(card: &PyValue) -> String {
    crate::json::py_text(card, true)
}

/// aggregate_precision.py: sum one label's cards per world.
pub fn aggregate(dir: &Path) -> Result<String, String> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with(".precision.json"))
        .collect();
    paths.sort();
    let (mut bundles, mut no_bridge, mut no_card) = (0i64, Vec::new(), Vec::new());
    let mut lists: BTreeMap<String, Vec<(String, PyValue)>> = BTreeMap::new();
    let worlds = ["exact", "ordinal", "factory"];
    let mut sums: Vec<BTreeMap<String, i64>> = vec![BTreeMap::new(); 3];
    for path in paths {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let bid = name
            .strip_suffix(".precision.json")
            .unwrap_or(&name)
            .to_string();
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if text.is_empty() {
            no_card.push(bid);
            continue;
        }
        let card: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if card
            .get("noBridge")
            .is_some_and(|v| v != &Value::from(0) && !v.is_null() && v != &Value::Bool(false))
        {
            no_bridge.push(bid);
            continue;
        }
        bundles += 1;
        for (w, counts) in worlds.iter().zip(sums.iter_mut()) {
            for (key, value) in card[*w].as_object().ok_or("a world block")? {
                match value {
                    Value::Array(items) => {
                        *counts.entry(key.clone()).or_insert(0) += items.len() as i64;
                        if !items.is_empty()
                            && (key.ends_with("appFiles") || key.ends_with("wrongFiles"))
                        {
                            lists
                                .entry(format!("{w}:{key}"))
                                .or_default()
                                .push((bid.clone(), from_json(value)?));
                        }
                    }
                    other => {
                        *counts.entry(key.clone()).or_insert(0) +=
                            other.as_i64().ok_or("an int count")?;
                    }
                }
            }
        }
    }
    let ids = |v: Vec<String>| PyValue::List(v.into_iter().map(PyValue::Str).collect());
    let mut e = vec![
        kv("bundles", int(bundles)),
        kv("noBridge", ids(no_bridge)),
        kv("noCard", ids(no_card)),
        kv(
            "lists",
            dict(lists.into_iter().map(|(k, v)| (k, dict(v))).collect()),
        ),
    ];
    for (w, counts) in worlds.iter().zip(sums) {
        e.push(kv(
            w,
            dict(counts.into_iter().map(|(k, v)| (k, int(v))).collect()),
        ));
    }
    Ok(crate::json::py_text(&dict(e), true))
}

#[cfg(test)]
#[path = "vendor_precision_test.rs"]
mod vendor_precision_test;
