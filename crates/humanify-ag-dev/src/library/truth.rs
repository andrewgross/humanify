//! The truth a card is judged against, as scorer 14 builds it from the
//! frozen layers (experiments/096 `truth_audit.py` and score.py's
//! `label_units`): the source table's corrections (scorers 5, 9, 11), the
//! re-decoded segments (scorer 6), the origin layer (scorers 7, 10, 12),
//! and the per-unit audit (carried, copies, misattributed, map defect).
//! Scorer 14 only: every rule is on, as `SCORER_VERSION exp096-score/14`
//! ran them. The switches the Python kept to re-derive older scorers are
//! not ported.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use super::corpus::{Corpus, Role, read_map};
use super::counter::Counter;
use super::decoder::{self, Inner};
use super::index::Unit;
use super::pyre;
use crate::committed::library::{CorpusEntry, LayerKind};

/// Truth labels that are not a library package.
pub const NOT_LIBRARY: &[&str] = &["<app>", "<self>", "<unmapped>", "<foreign>"];

pub fn is_library(label: &str) -> bool {
    !NOT_LIBRARY.contains(&label)
}

/// One source row: [label, version, file].
#[derive(Clone, Debug, PartialEq)]
pub struct Src {
    pub label: String,
    pub version: Option<String>,
    pub file: Option<String>,
}

fn opt_str(v: Option<&Value>) -> Result<Option<String>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("expected a string or null, got {other}")),
    }
}

impl Src {
    fn from_row(row: &[Value]) -> Result<Src, String> {
        let label = match row.first() {
            Some(Value::String(s)) => s.clone(),
            other => return Err(format!("a source row's label is {other:?}")),
        };
        Ok(Src {
            label,
            version: opt_str(row.get(1))?,
            file: opt_str(row.get(2))?,
        })
    }
}

/// One row of the labelled sample (Claude Code).
#[derive(Clone, Debug, Deserialize)]
pub struct SampleRow {
    pub s: i64,
    pub e: i64,
    pub b: i64,
    pub stratum: String,
    pub label: String,
    #[serde(default)]
    pub pkg: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Stratum {
    pub bytes: i64,
}

/// An entry's truth file (`<id>.truth.json.gz`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TruthDoc {
    method: String,
    #[serde(default)]
    segments: Vec<(i64, i64)>,
    #[serde(default)]
    sources: Vec<Vec<Value>>,
    #[serde(default)]
    app_package: Option<String>,
    #[serde(default)]
    rows: Vec<SampleRow>,
    #[serde(default)]
    strata: HashMap<String, Stratum>,
}

/// A truth, after scorer 14's corrections.
pub struct Truth {
    pub method: String,
    pub segments: Vec<(i64, i64)>,
    pub sources: Vec<Src>,
    pub app_package: Option<String>,
    pub rows: Vec<SampleRow>,
    pub strata: HashMap<String, Stratum>,
}

/// An origin layer: per segment its origin, run-length coded.
#[derive(Clone, Debug, Deserialize)]
pub struct Origins {
    pub kinds: Vec<String>,
    pub names: Vec<String>,
    pub runs: Vec<(i64, i64, i64)>,
}

/// The run containing segment `k` (`runs[bisect_right(run_at, k) - 1]`,
/// Python's -1 index included: before the first run, the LAST run).
fn run_at(runs: &[(i64, i64, i64)], k: i64) -> (i64, i64, i64) {
    let i = runs.partition_point(|r| r.0 <= k);
    if i == 0 {
        runs[runs.len() - 1]
    } else {
        runs[i - 1]
    }
}

#[derive(Deserialize)]
struct SourcesLayer {
    sources: std::collections::BTreeMap<String, Vec<Value>>,
}

fn layer_sources(
    corpus: &Corpus,
    entry: &CorpusEntry,
    kind: LayerKind,
    suffix: &str,
) -> Result<Vec<(usize, Vec<Value>)>, String> {
    let doc: SourcesLayer = corpus.layer_file(entry, kind, suffix)?;
    let mut rows = Vec::new();
    for (k, v) in doc.sources {
        let i: usize = k
            .parse()
            .map_err(|_| format!("{}: bad source index {k:?}", entry.id))?;
        rows.push((i, v));
    }
    // Each index appears once, so Python's file order cannot matter.
    rows.sort_by_key(|r| r.0);
    Ok(rows)
}

/// Load an entry's truth with scorer 5/9/11's source-table corrections
/// (truth_audit `patch_sources`).
pub fn load_truth(corpus: &Corpus, entry: &CorpusEntry) -> Result<Truth, String> {
    load_truth_through(corpus, entry, true)
}

/// The frozen truth's source labels, uncorrected (lib096 `load_truth`).
pub fn load_truth_raw_labels(corpus: &Corpus, entry: &CorpusEntry) -> Result<Vec<String>, String> {
    let doc: TruthDoc = corpus.truth(entry)?;
    doc.sources
        .iter()
        .map(|r| Src::from_row(r).map(|s| s.label))
        .collect()
}

/// `patch_sources` with scorer 11's layer on or off (chained11.py builds
/// its layer from the table without it).
pub fn load_truth_through(
    corpus: &Corpus,
    entry: &CorpusEntry,
    with_chained11: bool,
) -> Result<Truth, String> {
    let doc: TruthDoc = corpus.truth(entry)?;
    let mut truth = Truth {
        method: doc.method,
        segments: doc.segments,
        sources: doc
            .sources
            .iter()
            .map(|r| Src::from_row(r))
            .collect::<Result<_, _>>()?,
        app_package: doc.app_package,
        rows: doc.rows,
        strata: doc.strata,
    };
    if truth.method != "sourcemap" {
        return Ok(truth);
    }
    // virtual: a rollup `?commonjs-*` id is bundler glue, <unmapped>
    let names = read_map(&corpus.source_map_path(entry)?)?.sources;
    for (i, n) in names.iter().enumerate() {
        if let (true, Value::String(n)) = (i < truth.sources.len(), n)
            && pyre::is_commonjs_virtual(n)
        {
            truth.sources[i] = Src {
                label: "<unmapped>".into(),
                version: None,
                file: Some(n.clone()),
            };
        }
    }
    let srcs = &mut truth.sources;
    for (i, row) in layer_sources(corpus, entry, LayerKind::Chained, ".chained.json.gz")? {
        if srcs[i].label == "<app>" {
            srcs[i] = Src::from_row(&row)?;
        }
    }
    let layers: &[(LayerKind, &str)] = if with_chained11 {
        &[
            (LayerKind::Chained9, ".chained9.json.gz"),
            (LayerKind::Chained11, ".chained11.json.gz"),
        ]
    } else {
        &[(LayerKind::Chained9, ".chained9.json.gz")]
    };
    for &(kind, suffix) in layers {
        for (i, row) in layer_sources(corpus, entry, kind, suffix)? {
            if row.get(3).and_then(Value::as_str) == Some(srcs[i].label.as_str()) {
                srcs[i] = Src::from_row(&row[..3])?;
            }
        }
    }
    Ok(truth)
}

/// The entry's origin layer: scorer 12's (`origins12`).
pub fn load_origins(corpus: &Corpus, entry: &CorpusEntry) -> Result<Origins, String> {
    corpus.layer_file(entry, LayerKind::Origins12, ".origins12.json.gz")
}

/// Scorer 6 (truth_audit `redecode`): the segments re-decoded with UTF-16
/// columns and unmapped segments kept; the origin runs re-run-length coded
/// over the new segment list. Refused unless the re-decoded MAPPED
/// segments are scorer 4's, in order.
pub fn redecode(
    corpus: &Corpus,
    entry: &CorpusEntry,
    truth: &mut Truth,
    origins: Option<Origins>,
) -> Result<Option<Origins>, String> {
    let bpath = corpus.corpus_path(entry, Role::Bundle)?;
    let text = std::fs::read_to_string(&bpath).map_err(|e| format!("{}: {e}", bpath.display()))?;
    let smap = read_map(&corpus.corpus_path(entry, Role::Map)?)?;
    let inner = match entry.files.inner_map {
        Some(_) => Some(Inner {
            lines: decoder::decode(
                &read_map(&corpus.corpus_path(entry, Role::InnerMap)?)?.mappings,
                true,
            )?,
            offset: 1,
        }),
        None => None,
    };
    let segs = decoder::segments(&text, &smap.mappings, inner.as_ref(), true, true)?;
    let mapped: Vec<i64> = segs.iter().filter_map(|s| s.1).collect();
    if mapped.len() != truth.segments.len()
        || mapped.iter().zip(&truth.segments).any(|(a, b)| *a != b.1)
    {
        return Err(format!(
            "REFUSED: {}: the re-decoded mapped segments are not scorer 4's, in order",
            entry.id
        ));
    }
    let none = truth.sources.len() as i64;
    truth.segments = segs.iter().map(|s| (s.0, s.1.unwrap_or(none))).collect();
    truth.sources.push(Src {
        label: "<unmapped>".into(),
        version: None,
        file: None,
    });
    let Some(origins) = origins else {
        return Ok(None);
    };
    let mut runs = Vec::new();
    let mut prev: Option<(i64, i64)> = None;
    let mut k = 0i64;
    for (j, s) in segs.iter().enumerate() {
        let o = if s.1.is_none() {
            (-1, -1)
        } else {
            let r = if origins.runs.is_empty() {
                (0, -1, -1)
            } else {
                run_at(&origins.runs, k)
            };
            k += 1;
            (r.1, r.2)
        };
        if prev != Some(o) {
            runs.push((j as i64, o.0, o.1));
            prev = Some(o);
        }
    }
    Ok(Some(Origins { runs, ..origins }))
}

/// A layer row keyed by its span.
type SpanRows = HashMap<(i64, i64), Vec<Value>>;

#[derive(Deserialize)]
struct RowsLayer {
    rows: Vec<Vec<Value>>,
}

#[derive(Deserialize)]
struct CopiesLayer {
    ranges: Vec<Vec<Value>>,
}

fn span_rows(rows: Vec<Vec<Value>>) -> SpanRows {
    let mut out = HashMap::new();
    for r in rows {
        let key = (
            r[0].as_i64().unwrap_or(i64::MIN),
            r[1].as_i64().unwrap_or(i64::MIN),
        );
        out.insert(key, r);
    }
    out
}

/// Python's ordering of two JSON values (lists compared item by item).
fn py_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering::*;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_i64(), y.as_i64()) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => x.as_f64().partial_cmp(&y.as_f64()).unwrap_or(Equal),
        },
        (Value::String(x), Value::String(y)) => x.cmp(y),
        (Value::Array(x), Value::Array(y)) => {
            for (p, q) in x.iter().zip(y) {
                let c = py_cmp(p, q);
                if c != Equal {
                    return c;
                }
            }
            x.len().cmp(&y.len())
        }
        (Value::Null, Value::Null) => Equal,
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => Equal,
    }
}

/// A copy range's (origin, declaring dependency).
type CopyKey = (Option<String>, Option<String>);

/// One copy range: [start, end, shipper, origin, declaring dependency, ...].
struct CopyRange {
    a: i64,
    b: i64,
    ship: Option<String>,
    origin: Option<String>,
    dep: Option<String>,
}

/// truth_audit `Audit`: the per-unit corrections read from the frozen
/// bundle, map and layers.
pub struct Audit {
    offs: Vec<i64>,
    seg_src: Vec<i64>,
    text: Vec<u8>,
    tok: HashMap<i64, std::collections::HashSet<String>>,
    all: std::collections::HashSet<String>,
    copies: Vec<CopyRange>,
    copy_at: Vec<i64>,
    mis13: SpanRows,
    md14: SpanRows,
}

const MIN_TOKENS: usize = 3;
const CARRIED_SHARE: f64 = 0.5;

/// truth_audit `carried_verdict` (scorer 6: `unverified` on).
pub fn carried_verdict(
    unit: &std::collections::HashSet<String>,
    source: &std::collections::HashSet<String>,
    unverified: bool,
) -> bool {
    if unit.len() < MIN_TOKENS {
        return unverified;
    }
    (unit.intersection(source).count() as f64) < CARRIED_SHARE * unit.len() as f64
}

impl Audit {
    pub fn new(corpus: &Corpus, entry: &CorpusEntry, truth: &Truth) -> Result<Audit, String> {
        let bpath = corpus.corpus_path(entry, Role::Bundle)?;
        let text = std::fs::read(&bpath).map_err(|e| format!("{}: {e}", bpath.display()))?;
        let contents = read_map(&corpus.source_map_path(entry)?)?
            .sources_content
            .unwrap_or_default();
        let mut tok = HashMap::new();
        let mut all = std::collections::HashSet::new();
        for (si, c) in contents.iter().enumerate() {
            let Some(c) = c.as_deref().filter(|c| !c.is_empty()) else {
                continue;
            };
            let set: std::collections::HashSet<String> =
                pyre::tokens(c).into_iter().map(String::from).collect();
            all.extend(set.iter().cloned());
            tok.insert(si as i64, set);
        }
        let copies_doc: CopiesLayer =
            corpus.layer_file(entry, LayerKind::Copies2, ".copies2.json.gz")?;
        let mut ranges = copies_doc.ranges;
        ranges.sort_by(|x, y| py_cmp(&Value::Array(x.clone()), &Value::Array(y.clone())));
        let copies: Vec<CopyRange> = ranges
            .iter()
            .map(|r| {
                Ok(CopyRange {
                    a: r[0].as_i64().ok_or("copy range start")?,
                    b: r[1].as_i64().ok_or("copy range end")?,
                    ship: opt_str(r.get(2))?,
                    origin: opt_str(r.get(3))?,
                    dep: opt_str(r.get(4))?,
                })
            })
            .collect::<Result<_, String>>()?;
        let mis13: RowsLayer =
            corpus.layer_file(entry, LayerKind::Misattr13, ".misattr13.json.gz")?;
        let md14: RowsLayer =
            corpus.layer_file(entry, LayerKind::Mapdefect14, ".mapdefect14.json.gz")?;
        Ok(Audit {
            offs: truth.segments.iter().map(|s| s.0).collect(),
            seg_src: truth.segments.iter().map(|s| s.1).collect(),
            text,
            tok,
            all,
            copy_at: copies.iter().map(|c| c.a).collect(),
            copies,
            mis13: span_rows(mis13.rows),
            md14: span_rows(md14.rows),
        })
    }

    /// The unit [s, e) has no mapping of its own and its tokens are mostly
    /// absent from the source the previous mapping names.
    pub fn carried(&self, s: i64, e: i64) -> bool {
        let k0 = self.offs.partition_point(|&o| o < s);
        if k0 == 0 || self.offs.partition_point(|&o| o < e) > k0 {
            return false;
        }
        let si = self.seg_src[k0 - 1];
        let Some(src) = self.tok.get(&si) else {
            return false;
        };
        let (a, b) = py_byte_bounds(self.text.len(), s, e);
        let unit_text = String::from_utf8_lossy(&self.text[a..b]);
        let t: std::collections::HashSet<String> = pyre::tokens(&unit_text)
            .into_iter()
            .filter(|w| self.all.contains(*w))
            .map(String::from)
            .collect();
        carried_verdict(&t, src, true)
    }

    /// Scorer 13: {factory unit i: misattr13 row} for the run's Factory
    /// units the layer relabels (matched by span).
    pub fn misattributed(&self, units: &[Unit]) -> HashMap<usize, &Vec<Value>> {
        units
            .iter()
            .filter(|u| u.k == "Factory")
            .filter_map(|u| self.mis13.get(&(u.s, u.e)).map(|r| (u.i, r)))
            .collect()
    }

    /// Scorer 14: the mapdefect14 row for [s, e) when its old package is
    /// still the unit's label.
    pub fn mapdefect(&self, s: i64, e: i64, label: &str) -> Option<&Vec<Value>> {
        self.md14
            .get(&(s, e))
            .filter(|r| r.get(2).and_then(Value::as_str) == Some(label))
    }

    /// (origin, declaring dependency) when most of [s, e) lies in one
    /// origin's copy ranges of this shipper.
    pub fn copy_origin(
        &self,
        s: i64,
        e: i64,
        shipper: &str,
    ) -> Option<(Option<String>, Option<String>)> {
        let mut k = self.copy_at.partition_point(|&a| a <= s).saturating_sub(1);
        let mut by: Counter<(Option<String>, Option<String>)> = Counter::new();
        while k < self.copies.len() && self.copies[k].a < e {
            let c = &self.copies[k];
            let ov = c.b.min(e) - c.a.max(s);
            if ov > 0 && c.ship.as_deref() == Some(shipper) {
                by.add((c.origin.clone(), c.dep.clone()), ov);
            }
            k += 1;
        }
        let mut items: Vec<(&CopyKey, i64)> = by.items().collect();
        items.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(y.0)));
        let (best, n) = items.first()?;
        if 2 * n > e - s {
            Some((*best).clone())
        } else {
            None
        }
    }
}

/// Python's `bytes[s:e]` bounds.
fn py_byte_bounds(len: usize, s: i64, e: i64) -> (usize, usize) {
    let fix = |i: i64| -> usize {
        let i = if i < 0 { i + len as i64 } else { i };
        i.clamp(0, len as i64) as usize
    };
    let (a, b) = (fix(s), fix(e));
    (a, b.max(a))
}

/// One scored unit's truth (score.py `label_units` row).
#[derive(Clone, Debug, Default)]
pub struct Label {
    pub label: String,
    pub pkg: Option<String>,
    pub version: Option<String>,
    pub file: Option<String>,
    /// Python's `"origin" in row`: a copy range may name no origin
    /// (`None`) and still mark the unit as a copy.
    pub has_origin: bool,
    pub origin: Option<String>,
    pub via: Option<String>,
}

impl Label {
    fn plain(label: String) -> Label {
        Label {
            label,
            ..Label::default()
        }
    }
}

const VIA: &str = "chunk/via:";

/// The mapped bytes of [s, e): votes per label, bytes per source, and
/// per label the bytes per origin (label_units' segment walk).
struct Votes {
    votes: Counter<String>,
    files: Counter<i64>,
    by_origin: HashMap<String, Counter<(i64, i64)>>,
}

fn vote(
    truth: &Truth,
    offs: &[i64],
    runs: &[(i64, i64, i64)],
    s: i64,
    e: i64,
) -> Result<Votes, String> {
    let segs = &truth.segments;
    let mut v = Votes {
        votes: Counter::new(),
        files: Counter::new(),
        by_origin: HashMap::new(),
    };
    let mut k = offs.partition_point(|&o| o <= s).saturating_sub(1);
    while k < segs.len() && segs[k].0 < e {
        let a = segs[k].0.max(s);
        let b = if k + 1 < segs.len() { segs[k + 1].0 } else { e }.min(e);
        if b > a {
            let src = usize::try_from(segs[k].1)
                .ok()
                .and_then(|i| truth.sources.get(i))
                .ok_or_else(|| format!("segment {k} names no source"))?;
            v.votes.add(src.label.clone(), b - a);
            v.files.add(segs[k].1, b - a);
            if !runs.is_empty() {
                let r = run_at(runs, k as i64);
                v.by_origin
                    .entry(src.label.clone())
                    .or_default()
                    .add((r.1, r.2), b - a);
            }
        }
        k += 1;
    }
    Ok(v)
}

/// Scorer 13: the misattr13 row's relabel, when it applies to this unit.
fn misattributed_label(
    row: &[Value],
    lab: &str,
    src_file: &Option<String>,
    audit: &Audit,
    s: i64,
    e: i64,
) -> Result<Option<Label>, String> {
    if row.get(2).and_then(Value::as_str) != Some(lab) || opt_str(row.get(7))? != *src_file {
        return Ok(None);
    }
    let pkg = opt_str(row.get(3))?.ok_or("misattr13 row without a package")?;
    let mut l = Label {
        label: pkg.clone(),
        pkg: Some(pkg.clone()),
        version: opt_str(row.get(4))?,
        file: opt_str(row.get(5))?,
        ..Label::default()
    };
    if let Some((origin, _)) = audit.copy_origin(s, e, &pkg) {
        l.has_origin = true;
        l.origin = origin;
    }
    Ok(Some(l))
}

/// The origin layer's majority origin of the label's own bytes.
fn layer_origin(row: &mut Label, o: &Origins, by: &Counter<(i64, i64)>) {
    let mut items: Vec<(&(i64, i64), i64)> = by.items().collect();
    items.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(y.0)));
    let (oi, ki) = *items[0].0;
    if oi >= 0 {
        let kind = &o.kinds[ki as usize];
        row.origin = Some(o.names[oi as usize].clone());
        row.has_origin = true;
        if let Some(via) = kind.strip_prefix(VIA) {
            row.via = Some(via.to_string());
        }
    }
}

/// Scorer 14's map-defect relabel, then a copy range's origin.
fn audit_library(row: &mut Label, lab: &str, audit: &Audit, s: i64, e: i64) -> Result<(), String> {
    let mut lab = lab.to_string();
    if let Some(md) = audit.mapdefect(s, e, &lab) {
        let pkg = opt_str(md.get(3))?.ok_or("mapdefect14 row without a package")?;
        *row = Label {
            label: pkg.clone(),
            pkg: Some(pkg.clone()),
            version: opt_str(md.get(4))?,
            file: opt_str(md.get(5))?,
            ..Label::default()
        };
        lab = pkg;
    }
    if let Some((origin, _)) = audit.copy_origin(s, e, &lab) {
        row.has_origin = true;
        row.origin = origin;
    }
    Ok(())
}

/// score.py `label_units`: majority mapped bytes per scored unit, with
/// the origin layer and scorer 4-14's audit.
pub fn label_units(
    truth: &Truth,
    units: &[Unit],
    scored: &[&Unit],
    origins: Option<&Origins>,
    audit: &Audit,
) -> Result<HashMap<usize, Label>, String> {
    let srcs = &truth.sources;
    let offs: Vec<i64> = truth.segments.iter().map(|s| s.0).collect();
    let runs: &[(i64, i64, i64)] = origins.map_or(&[], |o| &o.runs);
    let mis = audit.misattributed(units);
    let mut out = HashMap::new();
    for u in scored {
        let (s, e) = (u.s, u.e);
        let v = vote(truth, &offs, runs, s, e)?;
        let lab = v
            .votes
            .most_common_first()
            .cloned()
            .unwrap_or_else(|| "<unmapped>".into());
        if lab != "<unmapped>" && audit.carried(s, e) {
            out.insert(u.i, Label::plain("<unmapped>".into()));
            continue;
        }
        let module = match u.k.as_str() {
            "FactoryBody" => u.m,
            "Factory" => Some(u.i as i64),
            _ => None,
        };
        let src_file = v
            .files
            .items()
            .max_by(|x, y| x.1.cmp(&y.1).then_with(|| y.0.cmp(x.0)))
            .and_then(|(si, _)| srcs[*si as usize].file.clone());
        if let Some(row) = module
            .and_then(|m| usize::try_from(m).ok())
            .and_then(|m| mis.get(&m))
            && let Some(l) = misattributed_label(row, &lab, &src_file, audit, s, e)?
        {
            out.insert(u.i, l);
            continue;
        }
        let mut row = Label::plain(lab.clone());
        if is_library(&lab) {
            let src = v
                .files
                .most_common()
                .into_iter()
                .map(|(si, _)| *si)
                .find(|si| srcs[*si as usize].label == lab)
                .expect("a library label has a source");
            row.pkg = Some(lab.clone());
            row.version = srcs[src as usize].version.clone();
            row.file = srcs[src as usize].file.clone();
        }
        if let (Some(o), Some(by)) = (origins, v.by_origin.get(&lab)) {
            layer_origin(&mut row, o, by);
        }
        if is_library(&lab) && !row.has_origin {
            audit_library(&mut row, &lab, audit, s, e)?;
        }
        out.insert(u.i, row);
    }
    Ok(out)
}
