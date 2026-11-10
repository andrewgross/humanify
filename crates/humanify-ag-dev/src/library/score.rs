//! The exp096 library scorer, scorer 14 (experiments/096 `score.py`): one
//! CARD per (label, corpus entry) and one summary per label, from a
//! label's raw discovery reports, byte for byte what score.py writes.
//!
//! A card judges every statement and factory unit of the run against the
//! frozen truth (`truth`): a claim is RIGHT when it names the unit's
//! shipping package or its origin (or a member of either's family);
//! precision / recall / F1 / F0.5 are over bytes; app code claimed as any
//! package is the hard bar (0). Claude Code is scored on its labelled
//! sample with the stratified ratio estimate. See score.py's docstring for
//! every column; the bytes here are its bytes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::audit15::{self, EntryAudit};
use super::copy_credit::{LiteralIndex, Registry};
use super::corpus::{Corpus, RULES7, RULES10, RULES12, Role, read_map, rules_are};
use super::counter::Counter;
use super::defect15::EntryTexts;
use super::families::{self, Families};
use super::index::{self, Index, Print, TruthFacts, Unit};
use super::py::{dict, exact, flt, from_json, int, kv, round_int, s, share};
use super::truth::{self, Audit, Label, Truth, is_library};
use super::wrong_families::{self, EntryWrongs, WrongClaim};
use crate::committed::library::{CorpusEntry, LayerKind};
use crate::json::py::PyValue;

pub const SCORER_VERSION: &str = "exp096-score/15";
pub const PRECISION_BAR: f64 = 0.998;
pub const MISS_CLASSES: &[&str] = &[
    "no-discovery",
    "a",
    "b",
    "c",
    "d-file",
    "d-variant",
    "d-absent",
    "d-nofile",
    "e-veto",
    "e-agree",
    "e-support",
    "e-spec",
    "e-ambig",
    "e-other",
    "f",
    "unit-model",
    "factory",
];
/// Card and report field names that are also pipeline setting names: kept
/// as constants so env_guard's "a setting read out of a JSON map" rule does
/// not mistake a scorer field for a setting.
const LIBRARY: &str = "library";
const BUNDLER: &str = "bundler";
const SCOPES: [&str; 2] = ["statement", "factory"];

type Ctr = Counter<String>;

// ------------------------------------------------------------- the report

#[derive(Deserialize)]
struct WholeRow {
    hash: String,
    nodes: i64,
    content: i64,
}

#[derive(Deserialize)]
struct FnRow {
    start: i64,
    end: i64,
    hash: String,
    nodes: i64,
    content: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnitRow {
    start: i64,
    end: i64,
    bytes: i64,
    kind: String,
    runtime: bool,
    module_bytes: Option<i64>,
    bindings: i64,
    #[serde(default)]
    module: Option<i64>,
    #[serde(default)]
    whole: Option<WholeRow>,
    #[serde(default)]
    functions: Option<Vec<FnRow>>,
}

#[derive(Deserialize)]
struct ClaimRow {
    start: i64,
    end: i64,
    package: String,
    #[serde(default)]
    rule: Option<String>,
    #[serde(default)]
    evidence: Option<Value>,
}

#[derive(Deserialize)]
struct SpanRow {
    start: i64,
    end: i64,
    package: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    unit_rows: Vec<UnitRow>,
    #[serde(default)]
    claims: Vec<ClaimRow>,
    #[serde(default)]
    vetoed: Vec<SpanRow>,
    #[serde(default)]
    index_id: Option<String>,
    #[serde(default)]
    seed_statement_bytes: Option<i64>,
    #[serde(default)]
    grown_statement_bytes: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportDoc {
    toolchain: Value,
    detected: Value,
    #[serde(default)]
    no_discovery: Option<Value>,
    library_discovery: Option<Report>,
}

/// score.py `run_units`.
fn run_units(report: &Report) -> Result<Vec<Unit>, String> {
    units_of(&report.unit_rows)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnitsOnly {
    unit_rows: Vec<UnitRow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnitsDoc {
    library_discovery: Option<UnitsOnly>,
}

/// A label's run of `entry`: `run_units` of its report's
/// `libraryDiscovery` (`Ok(None)` when the report has none), or
/// `Err` when the report is missing (the layer builders' readers: a
/// builder that tolerates a missing report checks first).
pub fn label_units(
    corpus: &Corpus,
    label: &str,
    entry: &CorpusEntry,
) -> Result<Option<Vec<Unit>>, String> {
    let p = report_path(corpus, label, entry);
    let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    let doc: UnitsDoc =
        serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", p.display()))?;
    doc.library_discovery
        .map(|r| units_of(&r.unit_rows))
        .transpose()
}

/// `<DATA>/results/<label>/reports/<id>.json`.
pub fn report_path(corpus: &Corpus, label: &str, entry: &CorpusEntry) -> PathBuf {
    corpus
        .data
        .join("results")
        .join(label)
        .join("reports")
        .join(format!("{}.json", entry.id))
}

fn units_of(rows: &[UnitRow]) -> Result<Vec<Unit>, String> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let w = match &r.whole {
                None => None,
                Some(w) => {
                    let whole = Print {
                        s: r.start,
                        e: r.end,
                        h: index::hash_key(&w.hash)?,
                        nodes: w.nodes,
                        content: w.content,
                    };
                    let fns = r
                        .functions
                        .iter()
                        .flatten()
                        .map(|f| {
                            Ok(Print {
                                s: f.start,
                                e: f.end,
                                h: index::hash_key(&f.hash)?,
                                nodes: f.nodes,
                                content: f.content,
                            })
                        })
                        .collect::<Result<_, String>>()?;
                    Some((whole, fns))
                }
            };
            Ok(Unit {
                i,
                s: r.start,
                e: r.end,
                b: r.bytes,
                k: r.kind.clone(),
                rt: r.runtime,
                mb: r.module_bytes,
                bindings: r.bindings,
                m: r.module,
                w,
            })
        })
        .collect()
}

fn statement_units(units: &[Unit]) -> Vec<&Unit> {
    units
        .iter()
        .filter(|u| (u.k == "Statement" || u.k == "LazyBody") && !u.rt)
        .collect()
}

fn factory_units(units: &[Unit]) -> Vec<&Unit> {
    let cut: HashSet<Option<i64>> = units
        .iter()
        .filter(|u| u.k == "FactoryBody")
        .map(|u| u.m)
        .collect();
    units
        .iter()
        .filter(|u| {
            !u.rt
                && (u.k == "FactoryBody" || (u.k == "Factory" && !cut.contains(&Some(u.i as i64))))
        })
        .collect()
}

pub fn scored_units(units: &[Unit]) -> Vec<&Unit> {
    let keep: HashSet<usize> = statement_units(units)
        .iter()
        .chain(factory_units(units).iter())
        .map(|u| u.i)
        .collect();
    units.iter().filter(|u| keep.contains(&u.i)).collect()
}

// ---------------------------------------------------------------- verdicts

/// score.py `prf`: precision, recall, F1 and F0.5 over bytes.
fn prf(
    named: f64,
    wrong_family: f64,
    app: f64,
    library: f64,
    suffix: &str,
) -> Vec<(String, PyValue)> {
    let p = share(named, named + wrong_family + app);
    let r = share(named, library);
    vec![
        (format!("precision{suffix}"), flt(p)),
        (format!("recall{suffix}"), flt(r)),
        (format!("f1{suffix}"), flt(share(2.0 * p * r, p + r))),
        (
            format!("f05{suffix}"),
            flt(share(1.25 * p * r, 0.25 * p + r)),
        ),
    ]
}

fn prf_i(named: i64, wrong: i64, app: i64, lib: i64, suffix: &str) -> Vec<(String, PyValue)> {
    prf(exact(named), exact(wrong), exact(app), exact(lib), suffix)
}

/// Scorer 15's three columns (after `namedAsOriginBytes`): the credited
/// copies (inside `namedBytes`), the defect bytes taken out of both sides,
/// the claims on them, and the precision with the credit charged as wrong.
fn s15_cols(
    named: i64,
    credit: i64,
    wrong: i64,
    app: i64,
    defect: (i64, i64),
) -> Vec<(String, PyValue)> {
    vec![
        kv("namedAsCopyCreditBytes", int(credit)),
        kv("defectExcludedBytes", int(defect.0)),
        kv("defectExcludedClaimedBytes", int(defect.1)),
        kv(
            "precisionNoCopyCredit",
            flt(share(exact(named - credit), exact(named + wrong + app))),
        ),
    ]
}

/// Bytes named right: shipper, family, origin, and (scorer 15) a credited copy.
fn named_of(claimed: &Ctr) -> i64 {
    claimed.get("right") + claimed.get("family") + claimed.get("origin") + claimed.get(COPY)
}

/// Scorer 15's verdict for a credited copy (`copy_credit`).
const COPY: &str = "copyCredit";

struct Judge<'a> {
    fam: &'a Families,
    app_pkg: Option<&'a str>,
}

impl Judge<'_> {
    /// score.py `judge`.
    fn judge(&self, claimed: &str, label: &str) -> String {
        match label {
            "<self>" => if Some(claimed) == self.app_pkg {
                "self"
            } else {
                "selfOther"
            }
            .into(),
            "<app>" => "app".into(),
            "<unmapped>" | "<foreign>" | "<defect>" => label.trim_matches(['<', '>']).into(),
            _ if claimed == label => "right".into(),
            _ if self.fam.same(Some(claimed), Some(label)) => "family".into(),
            _ => "wrongFamily".into(),
        }
    }

    /// score.py `judge_origin`: (new verdict, shipper-only verdict).
    fn judge_origin(&self, claimed: &str, row: &Label) -> (String, String) {
        let old = self.judge(claimed, &row.label);
        if (old == "wrongFamily" || old == "selfOther")
            && [row.origin.as_deref(), row.via.as_deref()]
                .into_iter()
                .any(|o| o.is_some() && self.fam.same(Some(claimed), o))
        {
            let new = if old == "wrongFamily" {
                "origin"
            } else {
                "selfOrigin"
            };
            return (new.into(), old);
        }
        (old.clone(), old)
    }
}

// --------------------------------------------------- the exploratory names

#[derive(Default)]
struct Names {
    exact_units: i64,
    exact_bytes: i64,
    bindings: i64,
    known: i64,
    same: i64,
}

impl Names {
    fn add(
        &mut self,
        ix: &Index,
        fam: &Families,
        u: &Unit,
        package: &str,
        truth: Option<(&str, Option<&str>)>,
    ) {
        let Some((whole, _)) = &u.w else { return };
        let rows: Vec<&index::Row> = ix
            .rows_of(whole.h)
            .filter(|r| fam.same(Some(ix.name(r.owner)), Some(package)))
            .collect();
        if rows.is_empty() {
            return;
        }
        self.exact_units += 1;
        self.exact_bytes += u.b;
        self.bindings += u.bindings;
        if let Some((pkg, Some(version))) = truth {
            self.known += u.bindings;
            if rows
                .iter()
                .any(|r| ix.name(r.owner) == pkg && ix.expands_to(pkg, &r.versions, version))
            {
                self.same += u.bindings;
            }
        }
    }

    fn block(&self) -> Vec<(String, PyValue)> {
        vec![
            kv("exactUnits", int(self.exact_units)),
            kv("exactBytes", int(self.exact_bytes)),
            kv("bindings", int(self.bindings)),
            kv("bindingsVersionKnown", int(self.known)),
            kv("bindingsSameVersion", int(self.same)),
            kv(
                "sameVersionShare",
                flt(share(exact(self.same), exact(self.known))),
            ),
        ]
    }
}

// ---------------------------------------------------------------- one card

fn sorted_block(c: &Ctr) -> PyValue {
    dict(c.items().map(|(k, v)| (k.clone(), int(v))).collect())
}

fn miss_block(m: &Ctr) -> PyValue {
    dict(
        MISS_CLASSES
            .iter()
            .filter(|k| m.get(**k) != 0)
            .map(|k| kv(k, int(m.get(*k))))
            .collect(),
    )
}

/// One scope's four counters: totals, claimed, shipper-only, misses.
#[derive(Default, Clone)]
struct ScopeCounts {
    t: Ctr,
    claimed: Ctr,
    old: Ctr,
    misses: Ctr,
}

fn scope_block(sc: &ScopeCounts) -> PyValue {
    let (t, claimed, old) = (&sc.t, &sc.claimed, &sc.old);
    let lib = t.get(LIBRARY);
    let named = named_of(claimed);
    let old_named = old.get("right") + old.get("family");
    let mut e = vec![kv("libraryBytes", int(lib)), kv("namedBytes", int(named))];
    e.extend(prf_i(
        named,
        claimed.get("wrongFamily"),
        claimed.get("app"),
        lib,
        "",
    ));
    e.extend([
        kv("appClaimedBytes", int(claimed.get("app"))),
        kv("wrongFamilyBytes", int(claimed.get("wrongFamily"))),
        kv("namedAsOriginBytes", int(claimed.get("origin"))),
    ]);
    e.extend(s15_cols(
        named,
        claimed.get(COPY),
        claimed.get("wrongFamily"),
        claimed.get("app"),
        (t.get(audit15::DEFECT), claimed.get("defect")),
    ));
    e.extend([
        kv("copyBytes", int(t.get("copy"))),
        kv("namedBytesShipperOnly", int(old_named)),
    ]);
    e.extend(prf_i(
        old_named,
        old.get("wrongFamily"),
        old.get("app"),
        lib,
        "ShipperOnly",
    ));
    e.extend([
        kv("wrongFamilyBytesShipperOnly", int(old.get("wrongFamily"))),
        kv("statementBytes", sorted_block(t)),
        kv("claimedBytes", sorted_block(claimed)),
        kv("missBytes", miss_block(&sc.misses)),
    ]);
    dict(e)
}

/// score.py `totals_scope`: a scope with no per-unit verdicts.
fn totals_scope(lib: i64, named: i64, wrong: i64, app: i64, misses: PyValue) -> PyValue {
    let mut e = vec![kv("libraryBytes", int(lib)), kv("namedBytes", int(named))];
    e.extend(prf_i(named, wrong, app, lib, ""));
    e.extend([
        kv("appClaimedBytes", int(app)),
        kv("wrongFamilyBytes", int(wrong)),
        kv("namedAsOriginBytes", int(0)),
    ]);
    e.extend(s15_cols(named, 0, wrong, app, (0, 0)));
    e.extend([
        kv("copyBytes", int(0)),
        kv("namedBytesShipperOnly", int(named)),
    ]);
    e.extend(prf_i(named, wrong, app, lib, "ShipperOnly"));
    e.extend([
        kv("wrongFamilyBytesShipperOnly", int(wrong)),
        kv("statementBytes", dict(vec![])),
        kv("claimedBytes", dict(vec![])),
        kv("missBytes", misses),
    ]);
    dict(e)
}

struct Scorer<'a> {
    ix: &'a Index,
    fam: &'a Families,
    rule: &'a str,
    ctx: &'a audit15::Ctx<'a>,
}

fn rule_name(rule: &str) -> &'static str {
    if rule == "agreement" {
        "agree"
    } else {
        "struct"
    }
}

/// `{key[(start, end)]: claim}`: the last claim of a span wins, in the
/// position of its first (a Python dict).
fn claims_by_unit<'a>(
    claims: &'a [ClaimRow],
    key: &HashMap<(i64, i64), usize>,
) -> Vec<(usize, &'a ClaimRow)> {
    let mut out: Vec<(usize, &ClaimRow)> = Vec::new();
    let mut at: HashMap<usize, usize> = HashMap::new();
    for c in claims {
        if let Some(&i) = key.get(&(c.start, c.end)) {
            match at.get(&i) {
                Some(&slot) => out[slot].1 = c,
                None => {
                    at.insert(i, out.len());
                    out.push((i, c));
                }
            }
        }
    }
    out
}

/// score.py's `vetoed`: unit index -> the vetoed package (last wins).
fn vetoed_by_unit(vetoed: &[SpanRow], key: &HashMap<(i64, i64), usize>) -> HashMap<usize, String> {
    let mut spans: std::collections::BTreeMap<(i64, i64), &str> = Default::default();
    for v in vetoed {
        spans.insert((v.start, v.end), &v.package);
    }
    let mut out = HashMap::new();
    for (k, p) in spans {
        if let Some(&i) = key.get(&k) {
            out.insert(i, p.to_string());
        }
    }
    out
}

/// A sample unit's verdict for claim `p` (score_sample's inline judge).
fn sample_verdict(fam: &Families, p: &str, t: &truth::SampleRow) -> String {
    match t.pkg.as_deref() {
        Some(pkg) if p == pkg => "right".into(),
        Some(pkg) if fam.same(Some(p), Some(pkg)) => "family".into(),
        Some(_) => "wrongFamily".into(),
        None if t.label == "<app>" => "app".into(),
        None => t.label.trim_matches(['<', '>']).into(),
    }
}

/// A map card's body from its scope counters (score_map's return).
fn map_body(by_scope: &[ScopeCounts; 2], seeds: &Ctr, names: &Names) -> Vec<(String, PyValue)> {
    let sum =
        |f: fn(&ScopeCounts) -> &Ctr| Counter::new().plus(f(&by_scope[0])).plus(f(&by_scope[1]));
    let (total, claimed, old, misses) = (
        sum(|x| &x.t),
        sum(|x| &x.claimed),
        sum(|x| &x.old),
        sum(|x| &x.misses),
    );
    let lib = total.get(LIBRARY);
    let as_shipper = claimed.get("right") + claimed.get("family");
    let named = named_of(&claimed);
    let old_named = old.get("right") + old.get("family");
    let seed_named = named_of(seeds);
    let mut totals = vec![kv("libraryBytes", int(lib)), kv("namedBytes", int(named))];
    totals.extend(prf_i(
        named,
        claimed.get("wrongFamily"),
        claimed.get("app"),
        lib,
        "",
    ));
    totals.extend([
        kv("appClaimedBytes", int(claimed.get("app"))),
        kv("wrongFamilyBytes", int(claimed.get("wrongFamily"))),
        kv("namedAsShipperBytes", int(as_shipper)),
        kv("namedAsOriginBytes", int(claimed.get("origin"))),
    ]);
    totals.extend(s15_cols(
        named,
        claimed.get(COPY),
        claimed.get("wrongFamily"),
        claimed.get("app"),
        (total.get(audit15::DEFECT), claimed.get("defect")),
    ));
    totals.extend([
        kv("copyBytes", int(total.get("copy"))),
        kv("namedBytesShipperOnly", int(old_named)),
    ]);
    totals.extend(prf_i(
        old_named,
        old.get("wrongFamily"),
        old.get("app"),
        lib,
        "ShipperOnly",
    ));
    totals.push(kv(
        "wrongFamilyBytesShipperOnly",
        int(old.get("wrongFamily")),
    ));
    let mut seeds_block = vec![kv("namedBytes", int(seed_named))];
    seeds_block.extend(prf_i(
        seed_named,
        seeds.get("wrongFamily"),
        seeds.get("app"),
        lib,
        "",
    ));
    seeds_block.push(kv("appClaimedBytes", int(seeds.get("app"))));
    vec![
        kv("totals", dict(totals)),
        kv(
            "scopes",
            dict(vec![
                kv(SCOPES[0], scope_block(&by_scope[0])),
                kv(SCOPES[1], scope_block(&by_scope[1])),
            ]),
        ),
        kv("statementBytes", sorted_block(&total)),
        kv("claimedBytes", sorted_block(&claimed)),
        kv("indexSeeds", dict(seeds_block)),
        kv("missBytes", miss_block(&misses)),
        kv("exploratory", dict(vec![kv("names", dict(names.block()))])),
    ]
}

impl Scorer<'_> {
    fn score_map(
        &self,
        report: &Report,
        truth: &Truth,
        origins: Option<&truth::Origins>,
        (audit, a15): (&Audit, &mut EntryAudit),
    ) -> Result<(Vec<(String, PyValue)>, EntryWrongs), String> {
        let units = run_units(report)?;
        let st = statement_units(&units);
        let mut scope_of: HashMap<usize, usize> = st.iter().map(|u| (u.i, 0)).collect();
        scope_of.extend(factory_units(&units).iter().map(|u| (u.i, 1)));
        let every = scored_units(&units);
        let mut labels = truth::label_units(truth, &units, &every, origins, audit)?;
        let judge = Judge {
            fam: self.fam,
            app_pkg: truth.app_package.as_deref(),
        };
        let key: HashMap<(i64, i64), usize> = every.iter().map(|u| ((u.s, u.e), u.i)).collect();
        let claims = claims_by_unit(&report.claims, &key);
        let vetoed = vetoed_by_unit(&report.vetoed, &key);
        let claimed: HashMap<usize, &str> = claims
            .iter()
            .map(|(i, c)| (*i, c.package.as_str()))
            .collect();
        a15.defects(self.ctx, truth, (&every, &mut labels), &claimed);
        let mut by_scope = [ScopeCounts::default(), ScopeCounts::default()];
        for u in &every {
            let l = &labels[&u.i];
            let tot = &mut by_scope[scope_of[&u.i]].t;
            let lib = is_library(&l.label);
            tot.add(
                if lib {
                    "library".into()
                } else {
                    l.label.clone()
                },
                u.b,
            );
            if lib && l.has_origin {
                tot.add("copy".into(), u.b);
            }
        }
        let by_i: HashMap<usize, &Unit> = units.iter().map(|u| (u.i, u)).collect();
        let mut wrongs = EntryWrongs::default();
        let (seeds, names, credited) = self.count_claims(
            &claims,
            &labels,
            &by_i,
            &judge,
            &scope_of,
            (&mut by_scope, &mut wrongs, a15, truth),
        );
        let struct_claims = if self.rule == "agreement" {
            index::claim_all(self.ix, &st)
        } else {
            HashMap::new()
        };
        let claim_of: HashMap<usize, &ClaimRow> = claims.iter().map(|(i, c)| (*i, *c)).collect();
        for u in every.iter().filter(|u| !credited.contains(&u.i)) {
            if let Some(cls) = self.miss_class(
                u,
                &labels[&u.i],
                claim_of.get(&u.i).copied(),
                &judge,
                &vetoed,
                &struct_claims,
            ) {
                by_scope[scope_of[&u.i]].misses.add(cls.into(), u.b);
            }
        }
        Ok((map_body(&by_scope, &seeds, &names), wrongs))
    }

    /// The claims' verdicts into the scope counters; (index seeds, names,
    /// the units whose claim scorer 15 credited as a copy).
    fn count_claims(
        &self,
        claims: &[(usize, &ClaimRow)],
        labels: &HashMap<usize, Label>,
        by_i: &HashMap<usize, &Unit>,
        judge: &Judge,
        scope_of: &HashMap<usize, usize>,
        (by_scope, wrongs, a15, truth): (
            &mut [ScopeCounts; 2],
            &mut EntryWrongs,
            &mut EntryAudit,
            &Truth,
        ),
    ) -> (Ctr, Names, HashSet<usize>) {
        let mut seeds: Ctr = Counter::new();
        let mut names = Names::default();
        let mut credited = HashSet::new();
        for (i, c) in claims {
            let (mut v, vo) = judge.judge_origin(&c.package, &labels[i]);
            let b = by_i[i].b;
            let sc = &mut by_scope[scope_of[i]];
            if v == "wrongFamily"
                && a15.copy_credit(self.ctx, self.ix, truth, (by_i[i], &c.package, &labels[i]))
            {
                v = COPY.into();
                credited.insert(*i);
            }
            if v == "wrongFamily" {
                wrongs.add(WrongClaim {
                    claimed: &c.package,
                    truth: &labels[i].label,
                    file: labels[i].file.as_deref(),
                    rule: c.rule.as_deref(),
                    bytes: b,
                });
            }
            sc.claimed.add(v.clone(), b);
            sc.old.add(vo, b);
            let kind = c
                .evidence
                .as_ref()
                .and_then(|e| e.get("kind"))
                .and_then(Value::as_str);
            if c.rule.as_deref() == Some("seed") && matches!(kind, Some("index" | "queueIndex")) {
                seeds.add(v.clone(), b);
            }
            if v == "right" || v == "family" {
                let l = &labels[i];
                let truth = Some((l.pkg.as_deref().unwrap_or(""), l.version.as_deref()));
                names.add(self.ix, self.fam, by_i[i], &c.package, truth);
            }
        }
        (seeds, names, credited)
    }

    /// A labelled sample unit's miss class; None when it is not library
    /// or a claim names its family.
    fn sample_miss(
        &self,
        t: &truth::SampleRow,
        p: Option<&str>,
        u: Option<&Unit>,
        vet: &HashMap<i64, String>,
        struct_claims: &HashMap<usize, String>,
    ) -> Option<&'static str> {
        let pkg = t.pkg.as_deref()?;
        if p.is_some_and(|p| self.fam.same(Some(p), Some(pkg))) {
            return None;
        }
        let Some(u) = u.filter(|u| u.w.is_some()) else {
            return Some("unit-model");
        };
        let vetoed: HashMap<usize, String> = vet
            .get(&u.s)
            .map(|p| (u.i, p.clone()))
            .into_iter()
            .collect();
        let facts = TruthFacts {
            pkg,
            version: t.version.as_deref(),
            file: None,
        };
        Some(index::classify(
            self.ix,
            &self.fam.libix,
            u,
            &facts,
            p,
            &vetoed,
            struct_claims,
            rule_name(self.rule),
        ))
    }

    /// A library unit not named right: its miss class (decompose.classify;
    /// `factory` for an unprinted external factory); None when named.
    fn miss_class(
        &self,
        u: &Unit,
        t: &Label,
        c: Option<&ClaimRow>,
        judge: &Judge,
        vetoed: &HashMap<usize, String>,
        struct_claims: &HashMap<usize, String>,
    ) -> Option<&'static str> {
        let pkg = t.pkg.as_deref()?;
        if c.is_some_and(|c| {
            matches!(
                judge.judge_origin(&c.package, t).0.as_str(),
                "right" | "family" | "origin"
            )
        }) {
            return None;
        }
        if u.k == "Factory" {
            return Some("factory");
        }
        if u.w.is_none() {
            return Some("unit-model");
        }
        let facts = TruthFacts {
            pkg,
            version: t.version.as_deref(),
            file: t.file.as_deref(),
        };
        Some(index::classify(
            self.ix,
            &self.fam.libix,
            u,
            &facts,
            c.map(|c| c.package.as_str()),
            vetoed,
            struct_claims,
            rule_name(self.rule),
        ))
    }

    fn score_sample(
        &self,
        report: &Report,
        truth: &Truth,
    ) -> Result<Vec<(String, PyValue)>, String> {
        let (ix, fam) = (self.ix, self.fam);
        let units = run_units(report)?;
        let st = statement_units(&units);
        let by_s: HashMap<i64, &Unit> = st.iter().map(|u| (u.s, *u)).collect();
        let mut sb: Counter<String> = Counter::new();
        for t in &truth.rows {
            sb.add(t.stratum.clone(), t.b);
        }
        let est = |t: &truth::SampleRow| -> f64 {
            let bytes = truth.strata[&t.stratum].bytes;
            exact(bytes * t.b) / exact(sb.get(&t.stratum))
        };
        let mut cl: Vec<(i64, i64, &str)> = report
            .claims
            .iter()
            .map(|c| (c.start, c.end, c.package.as_str()))
            .collect();
        cl.sort();
        let starts: Vec<i64> = cl.iter().map(|c| c.0).collect();
        let claim_at = |x: i64| -> Option<&str> {
            let k = starts.partition_point(|&a| a <= x);
            if k == 0 {
                return None;
            }
            let c = cl[k - 1];
            (c.0 <= x && x < c.1).then_some(c.2)
        };
        let mut vet: HashMap<i64, String> = HashMap::new();
        for v in &report.vetoed {
            vet.insert(v.start, v.package.clone());
        }
        let struct_claims = if self.rule == "agreement" {
            index::claim_all(ix, &st)
        } else {
            HashMap::new()
        };
        let mut total: Counter<String, f64> = Counter::new();
        let mut claimed: Counter<String, f64> = Counter::new();
        let mut misses: Counter<String, f64> = Counter::new();
        let mut units_claimed: Ctr = Counter::new();
        let mut names = Names::default();
        for t in &truth.rows {
            let p = claim_at(t.s);
            let e = est(t);
            total.add(
                if t.pkg.is_some() {
                    "library".into()
                } else {
                    t.label.clone()
                },
                e,
            );
            if let Some(p) = p {
                let v = sample_verdict(fam, p, t);
                claimed.add(v.clone(), e);
                units_claimed.add(v.clone(), 1);
                if (v == "right" || v == "family") && by_s.contains_key(&t.s) {
                    let truth = Some((t.pkg.as_deref().unwrap_or(""), t.version.as_deref()));
                    names.add(ix, fam, by_s[&t.s], p, truth);
                }
            }
            if let Some(cls) = self.sample_miss(t, p, by_s.get(&t.s).copied(), &vet, &struct_claims)
            {
                misses.add(cls.into(), e);
            }
        }
        let lib = total.get(LIBRARY);
        let named = claimed.get("right") + claimed.get("family");
        let mut whole_names = Names::default();
        let key: HashMap<(i64, i64), &Unit> = st.iter().map(|u| ((u.s, u.e), *u)).collect();
        for c in &report.claims {
            if let Some(u) = key.get(&(c.start, c.end)) {
                whole_names.add(ix, fam, u, &c.package, None);
            }
        }
        let rnd = |c: &Counter<String, f64>, k: &str| round_int(c.get(k));
        let (wrong, app) = (claimed.get("wrongFamily"), claimed.get("app"));
        let mut totals = vec![
            kv("libraryBytes", int(round_int(lib))),
            kv("namedBytes", int(round_int(named))),
        ];
        totals.extend(prf(named, wrong, app, lib, ""));
        totals.extend([
            kv("appClaimedBytes", int(rnd(&claimed, "app"))),
            kv("appClaimedSampleUnits", int(units_claimed.get("app"))),
            kv("wrongFamilyBytes", int(rnd(&claimed, "wrongFamily"))),
            kv("namedAsShipperBytes", int(round_int(named))),
            kv("namedAsOriginBytes", int(0)),
        ]);
        // Scorer 15's corrections need per-unit map truth: not judged on
        // a hand-labelled sample (zero, so the sums stay defined).
        totals.extend(s15_cols(
            round_int(named),
            0,
            rnd(&claimed, "wrongFamily"),
            rnd(&claimed, "app"),
            (0, 0),
        ));
        totals.extend([
            kv("copyBytes", int(0)),
            kv("namedBytesShipperOnly", int(round_int(named))),
        ]);
        totals.extend(prf(named, wrong, app, lib, "ShipperOnly"));
        totals.push(kv(
            "wrongFamilyBytesShipperOnly",
            int(rnd(&claimed, "wrongFamily")),
        ));
        let miss_rounded = dict(
            MISS_CLASSES
                .iter()
                .filter(|k| misses.get(**k) != 0.0)
                .map(|k| kv(k, int(rnd(&misses, k))))
                .collect(),
        );
        let rounded = |c: &Counter<String, f64>| {
            dict(
                c.items()
                    .map(|(k, v)| (k.clone(), int(round_int(v))))
                    .collect(),
            )
        };
        let seed = report
            .seed_statement_bytes
            .ok_or("a sample report without seedStatementBytes")?;
        let grown = report
            .grown_statement_bytes
            .ok_or("a sample report without grownStatementBytes")?;
        let nb = names.block();
        let mut wb = whole_names.block();
        let pick = |b: &[(String, PyValue)], k: &str| {
            b.iter().find(|(x, _)| x == k).expect("names key").1.clone()
        };
        wb.extend([
            kv("bindingsVersionKnown", pick(&nb, "bindingsVersionKnown")),
            kv("bindingsSameVersion", pick(&nb, "bindingsSameVersion")),
            kv("sameVersionShare", pick(&nb, "sameVersionShare")),
            kv("versionScope", s("labelled sample units only")),
        ]);
        Ok(vec![
            kv("totals", dict(totals)),
            kv(
                "scopes",
                dict(vec![
                    kv(
                        "statement",
                        totals_scope(
                            round_int(lib),
                            round_int(named),
                            rnd(&claimed, "wrongFamily"),
                            rnd(&claimed, "app"),
                            miss_rounded.clone(),
                        ),
                    ),
                    kv("factory", totals_scope(0, 0, 0, 0, dict(vec![]))),
                ]),
            ),
            kv(
                "factoryScope",
                s("not judged: the labelled sample holds statement units only"),
            ),
            kv("originTruth", s("none: sample truth (hand-labelled units)")),
            kv(
                "estimate",
                s(
                    "stratified ratio estimate from the 620-unit labelled sample (bytes are src-wide estimates)",
                ),
            ),
            kv("statementBytes", rounded(&total)),
            kv("claimedBytes", rounded(&claimed)),
            kv("claimedSampleUnits", sorted_block(&units_claimed)),
            kv("bundleClaimedBytes", int(seed + grown)),
            kv("missBytes", miss_rounded),
            kv("exploratory", dict(vec![kv("names", dict(wb))])),
        ])
    }
}

/// score.py `score_floor`: no discovery ran; the denominator is the
/// MAPPED library bytes.
fn score_floor(entry: &CorpusEntry, truth: &Truth) -> Result<Vec<(String, PyValue)>, String> {
    if truth.method == "sample" {
        return Err(format!(
            "{}: no discovery ran on a sample-truth bundle",
            entry.id
        ));
    }
    let segs = &truth.segments;
    let mut lib = 0i64;
    for (k, &(off, src)) in segs.iter().enumerate() {
        let end = if k + 1 < segs.len() {
            segs[k + 1].0
        } else {
            off
        };
        if is_library(&truth.sources[src as usize].label) {
            lib += end - off;
        }
    }
    let mut totals = vec![kv("libraryBytes", int(lib)), kv("namedBytes", int(0))];
    totals.extend(prf_i(0, 0, 0, lib, ""));
    totals.extend([
        kv("appClaimedBytes", int(0)),
        kv("wrongFamilyBytes", int(0)),
        kv("namedAsShipperBytes", int(0)),
        kv("namedAsOriginBytes", int(0)),
    ]);
    totals.extend(s15_cols(0, 0, 0, 0, (0, 0)));
    totals.extend([kv("copyBytes", int(0)), kv("namedBytesShipperOnly", int(0))]);
    totals.extend(prf_i(0, 0, 0, lib, "ShipperOnly"));
    totals.push(kv("wrongFamilyBytesShipperOnly", int(0)));
    let nd = || dict(vec![kv("no-discovery", int(lib))]);
    let mut seeds = vec![kv("namedBytes", int(0))];
    seeds.extend(prf_i(0, 0, 0, lib, ""));
    seeds.push(kv("appClaimedBytes", int(0)));
    Ok(vec![
        kv("totals", dict(totals)),
        kv(
            "scopes",
            dict(vec![
                kv("statement", totals_scope(lib, 0, 0, 0, nd())),
                kv("factory", totals_scope(0, 0, 0, 0, dict(vec![]))),
            ]),
        ),
        kv(
            "libraryBytesBasis",
            s(
                "mapped library bytes (no unit model: discovery never ran; all in the statement scope, as scorers 1-9 counted them)",
            ),
        ),
        kv("statementBytes", dict(vec![])),
        kv("claimedBytes", dict(vec![])),
        kv("indexSeeds", dict(seeds)),
        kv("missBytes", nd()),
        kv(
            "exploratory",
            dict(vec![kv("names", dict(Names::default().block()))]),
        ),
    ])
}

// ---------------------------------------------------------------- the label

/// A card's body entries (after its header).
type CardBody = Vec<(String, PyValue)>;

/// One scope block's ints and its miss bytes (in file order).
type ScopeSum = (HashMap<String, i64>, Vec<(String, i64)>);

/// What `aggregate` reads from one card.
struct CardSum {
    entry: Option<String>,
    totals: HashMap<String, i64>,
    miss: Vec<(String, i64)>,
    names: [i64; 5],
    claimed: HashMap<String, i64>,
    discovery_ran: bool,
    bundler_detected: bool,
    scopes: [ScopeSum; 2],
    app_claimed_sample_units: Option<i64>,
}

fn ints_of(v: &PyValue) -> HashMap<String, i64> {
    match v {
        PyValue::Dict(es) => es
            .iter()
            .filter_map(|(k, v)| match v {
                PyValue::Int(d) => Some((k.clone(), d.parse().expect("an i64"))),
                _ => None,
            })
            .collect(),
        _ => HashMap::new(),
    }
}

fn ordered_ints(v: &PyValue) -> Vec<(String, i64)> {
    match v {
        PyValue::Dict(es) => es
            .iter()
            .filter_map(|(k, v)| match v {
                PyValue::Int(d) => Some((k.clone(), d.parse().expect("an i64"))),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

const SCOPE_SUMS: &[&str] = &[
    "libraryBytes",
    "namedBytes",
    "appClaimedBytes",
    "wrongFamilyBytes",
    "namedAsOriginBytes",
    "namedAsCopyCreditBytes",
    "defectExcludedBytes",
    "defectExcludedClaimedBytes",
    "copyBytes",
    "namedBytesShipperOnly",
    "wrongFamilyBytesShipperOnly",
];
const TOTAL_SUMS: &[&str] = &[
    "libraryBytes",
    "namedBytes",
    "appClaimedBytes",
    "wrongFamilyBytes",
    "namedAsShipperBytes",
    "namedAsOriginBytes",
    "namedAsCopyCreditBytes",
    "defectExcludedBytes",
    "defectExcludedClaimedBytes",
    "copyBytes",
    "wrongFamilyBytesShipperOnly",
    "namedBytesShipperOnly",
];
const NAME_KEYS: [&str; 5] = [
    "bindings",
    "bindingsSameVersion",
    "bindingsVersionKnown",
    "exactBytes",
    "exactUnits",
];

fn card_sum(card: &PyValue) -> CardSum {
    let get = |k: &str| card.get(k).expect("a card key");
    let names = ints_of(get("exploratory").get("names").expect("names"));
    let scope = |n: &str| {
        let b = get("scopes").get(n).expect("scope");
        (
            ints_of(b),
            ordered_ints(b.get("missBytes").expect("missBytes")),
        )
    };
    CardSum {
        entry: match card.get("entry") {
            Some(PyValue::Str(e)) => Some(e.clone()),
            _ => None,
        },
        totals: ints_of(get("totals")),
        miss: ordered_ints(get("missBytes")),
        names: NAME_KEYS.map(|k| names[k]),
        claimed: ints_of(get("claimedBytes")),
        discovery_ran: get("discoveryRan") == &PyValue::Bool(true),
        bundler_detected: get("detected").get(BUNDLER) == get("toolchain").get(BUNDLER),
        scopes: [scope("statement"), scope("factory")],
        app_claimed_sample_units: ints_of(get("totals")).get("appClaimedSampleUnits").copied(),
    }
}

fn miss_shares(mb: &Ctr, lib: i64) -> PyValue {
    dict(
        MISS_CLASSES
            .iter()
            .filter(|k| mb.get(**k) != 0)
            .map(|k| kv(k, flt(share(exact(mb.get(*k)), exact(lib)))))
            .collect(),
    )
}

fn aggregate_scope(blocks: &[&ScopeSum]) -> PyValue {
    let mut t: HashMap<&str, i64> = HashMap::new();
    let mut mb: Ctr = Counter::new();
    for (b, m) in blocks {
        for k in SCOPE_SUMS {
            *t.entry(k).or_insert(0) += b[*k];
        }
        for (k, v) in m {
            mb.add(k.clone(), *v);
        }
    }
    let mut e: Vec<(String, PyValue)> = SCOPE_SUMS.iter().map(|k| kv(k, int(t[k]))).collect();
    e.extend(prf_i(
        t["namedBytes"],
        t["wrongFamilyBytes"],
        t["appClaimedBytes"],
        t["libraryBytes"],
        "",
    ));
    e.push(no_credit_precision(&t));
    e.extend(prf_i(
        t["namedBytesShipperOnly"],
        t["wrongFamilyBytesShipperOnly"],
        t["appClaimedBytes"],
        t["libraryBytes"],
        "ShipperOnly",
    ));
    e.push(kv("missShares", miss_shares(&mb, t["libraryBytes"])));
    dict(e)
}

/// The summed precision with every credited copy charged as wrong.
fn no_credit_precision(t: &HashMap<&str, i64>) -> (String, PyValue) {
    let named = t["namedBytes"];
    kv(
        "precisionNoCopyCredit",
        flt(share(
            exact(named - t["namedAsCopyCreditBytes"]),
            exact(named + t["wrongFamilyBytes"] + t["appClaimedBytes"]),
        )),
    )
}

/// score.py `aggregate`: per set, summed bytes and shares from the sums.
fn aggregate(
    sets: &std::collections::BTreeMap<String, Vec<(CardSum, PyValue)>>,
) -> Vec<(String, PyValue, f64)> {
    let mut out = Vec::new();
    for (set, cs) in sets {
        let mut t: HashMap<&str, i64> = HashMap::new();
        let mut mb: Ctr = Counter::new();
        let mut nm = [0i64; 5];
        let (mut self_other, mut self_origin) = (0, 0);
        for (c, _) in cs {
            for k in TOTAL_SUMS {
                *t.entry(k).or_insert(0) += c.totals[*k];
            }
            for (k, v) in &c.miss {
                mb.add(k.clone(), *v);
            }
            for (n, v) in nm.iter_mut().zip(c.names) {
                *n += v;
            }
            self_other += c.claimed.get("selfOther").copied().unwrap_or(0);
            self_origin += c.claimed.get("selfOrigin").copied().unwrap_or(0);
        }
        let lib = t["libraryBytes"];
        let p_named = (t["namedBytes"], t["wrongFamilyBytes"], t["appClaimedBytes"]);
        let precision = share(exact(p_named.0), exact(p_named.0 + p_named.1 + p_named.2));
        let mut e = vec![
            kv("entries", int(cs.len() as i64)),
            kv(
                "discoveryRan",
                int(cs.iter().filter(|c| c.0.discovery_ran).count() as i64),
            ),
            kv(
                "bundlerDetected",
                int(cs.iter().filter(|c| c.0.bundler_detected).count() as i64),
            ),
            kv("libraryBytes", int(lib)),
            kv("namedBytes", int(t["namedBytes"])),
        ];
        e.extend(prf_i(p_named.0, p_named.1, p_named.2, lib, ""));
        e.extend([
            kv("appClaimedBytes", int(t["appClaimedBytes"])),
            kv("wrongFamilyBytes", int(t["wrongFamilyBytes"])),
            kv("namedAsShipperBytes", int(t["namedAsShipperBytes"])),
            kv("namedAsOriginBytes", int(t["namedAsOriginBytes"])),
            kv("namedAsCopyCreditBytes", int(t["namedAsCopyCreditBytes"])),
            kv("defectExcludedBytes", int(t["defectExcludedBytes"])),
            kv(
                "defectExcludedClaimedBytes",
                int(t["defectExcludedClaimedBytes"]),
            ),
            no_credit_precision(&t),
            kv("copyBytes", int(t["copyBytes"])),
        ]);
        e.extend(prf_i(
            t["namedBytesShipperOnly"],
            t["wrongFamilyBytesShipperOnly"],
            t["appClaimedBytes"],
            lib,
            "ShipperOnly",
        ));
        let mut names: Vec<(String, PyValue)> = NAME_KEYS
            .iter()
            .zip(nm)
            .map(|(k, v)| kv(k, int(v)))
            .collect();
        names.push(kv(
            "sameVersionShare",
            flt(share(exact(nm[1]), exact(nm[2]))),
        ));
        e.extend([
            kv("selfOtherBytes", int(self_other)),
            kv("selfOriginBytes", int(self_origin)),
            kv("missShares", miss_shares(&mb, lib)),
            kv("exploratoryNames", dict(names)),
        ]);
        let scopes = SCOPES
            .iter()
            .enumerate()
            .map(|(j, sc)| {
                kv(
                    sc,
                    aggregate_scope(&cs.iter().map(|c| &c.0.scopes[j]).collect::<Vec<_>>()),
                )
            })
            .collect();
        e.push(kv("scopes", dict(scopes)));
        if set == "cc" {
            e.push(kv(
                "appClaimedSampleUnits",
                int(cs[0].0.app_claimed_sample_units.unwrap_or(0)),
            ));
            e.push(kv(
                "factoryScope",
                cs[0]
                    .1
                    .get("factoryScope")
                    .cloned()
                    .unwrap_or(PyValue::Null),
            ));
        }
        out.push((set.clone(), dict(e), precision));
    }
    out
}

/// A label's pipeline.json (the runner's record).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pipeline {
    #[serde(default)]
    corpus: Option<String>,
    corpus_sha256: String,
    index_id: String,
    index_dir: String,
    rule: String,
    #[serde(default)]
    sets: Option<Vec<String>>,
    #[serde(default)]
    settings: Option<Value>,
    index_bytes: Value,
    build_commit: Value,
    binary_sha256: Value,
}

/// Where a rescore writes: the committed results dir (cards of a private
/// corpus beside its reports, as score.py does), or one directory.
pub enum Out {
    Committed,
    Dir(PathBuf),
}

/// Read a label's pipeline.json and check it belongs to `corpus`.
fn load_pipeline(corpus: &Corpus, run_dir: &Path, label: &str) -> Result<Pipeline, String> {
    let path = run_dir.join("pipeline.json");
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pipeline: Pipeline =
        serde_json::from_slice(&bytes).map_err(|e| format!("{label} pipeline.json: {e}"))?;
    let ran_on = pipeline.corpus.clone().unwrap_or_else(|| "dev".into());
    if ran_on != corpus.name() {
        return Err(format!(
            "REFUSED: the label ran on the {ran_on} corpus — pass --corpus"
        ));
    }
    if pipeline.corpus_sha256 != corpus.loaded.manifest_id {
        return Err("REFUSED: the label ran on another corpus manifest".into());
    }
    check_layers(corpus, &pipeline.corpus_sha256)?;
    Ok(pipeline)
}

/// One label being scored: its corpus, run record and loaded indexes.
struct Run<'a> {
    corpus: &'a Corpus,
    pipeline: &'a Pipeline,
    fam: Families,
    indexes: HashMap<String, Index>,
    /// Scorer 15: the index's literal table (outer None: not read yet).
    literals: Option<Option<LiteralIndex>>,
    registry: Registry,
}

/// One entry's scored body: the card body, its wrong-family breakdown and
/// scorer 15's corrections (both None on sample truth).
type Scored = (CardBody, Option<EntryWrongs>, Option<EntryAudit>);

/// One entry's bundle and corpus-map sources, for scorer 15's checks.
fn entry_texts(corpus: &Corpus, entry: &CorpusEntry) -> Result<EntryTexts, String> {
    let bpath = corpus.corpus_path(entry, Role::Bundle)?;
    let bundle = std::fs::read(&bpath).map_err(|e| format!("{}: {e}", bpath.display()))?;
    let map = read_map(&corpus.source_map_path(entry)?)?;
    let paths = map
        .sources
        .iter()
        .map(|v| v.as_str().unwrap_or("").to_string())
        .collect();
    Ok(EntryTexts::new(
        bundle,
        &super::pypath::dirname(&entry.files.bundle.origin),
        paths,
        map.sources_content.unwrap_or_default(),
    ))
}

impl Run<'_> {
    /// The index slice `slice` of the label's index, loaded once; its
    /// identity is its manifest's sha (run.py `index_facts`), so a dir
    /// that moved since the run (a re-pointed `current`) is refused.
    fn index(&mut self, slice: &str) -> Result<(), String> {
        if self.indexes.contains_key(slice) {
            return Ok(());
        }
        let dir = Path::new(&self.pipeline.index_dir);
        let id = super::corpus::sha256_file(&dir.join("manifest.json"))?;
        let want = &self.pipeline.index_id;
        if id != *want {
            return Err(format!(
                "REFUSED: {} holds index {}, the label ran on {}",
                dir.display(),
                &id[..16],
                &want[..16.min(want.len())]
            ));
        }
        self.indexes
            .insert(slice.to_string(), Index::load(dir, slice)?);
        if self.literals.is_none() {
            self.literals = Some(LiteralIndex::open(dir)?);
        }
        Ok(())
    }

    /// One entry's card body, and its wrong-family breakdown (None on
    /// sample truth: no per-claim bytes there).
    fn body(
        &mut self,
        entry: &CorpusEntry,
        doc: &ReportDoc,
        truth: &Truth,
        origins: Option<&truth::Origins>,
    ) -> Result<Scored, String> {
        let Some(report) = &doc.library_discovery else {
            let none = EntryAudit::new(EntryTexts::new(Vec::new(), "", Vec::new(), Vec::new()));
            return Ok((
                score_floor(entry, truth)?,
                Some(EntryWrongs::default()),
                Some(none),
            ));
        };
        if report.index_id.as_deref() != Some(self.pipeline.index_id.as_str()) {
            return Err(format!(
                "REFUSED: {} ran on index {:?}, label says {}",
                entry.id, report.index_id, self.pipeline.index_id
            ));
        }
        let slice = doc
            .toolchain
            .get("slice")
            .and_then(Value::as_str)
            .ok_or("toolchain.slice")?
            .to_string();
        self.index(&slice)?;
        let ctx = audit15::Ctx {
            lits: self.literals.as_ref().and_then(Option::as_ref),
            registry: &self.registry,
        };
        let scorer = Scorer {
            ix: &self.indexes[&slice],
            fam: &self.fam,
            rule: &self.pipeline.rule,
            ctx: &ctx,
        };
        if truth.method == "sample" {
            Ok((scorer.score_sample(report, truth)?, None, None))
        } else {
            let audit = Audit::new(self.corpus, entry, truth)?;
            let mut a15 = EntryAudit::new(entry_texts(self.corpus, entry)?);
            let (body, wrongs) = scorer.score_map(report, truth, origins, (&audit, &mut a15))?;
            a15.release();
            Ok((body, Some(wrongs), Some(a15)))
        }
    }

    /// One entry's card (score.py's `card` dict) and its breakdown.
    fn card(
        &mut self,
        label: &str,
        entry: &CorpusEntry,
        run_dir: &Path,
    ) -> Result<(PyValue, Option<EntryWrongs>, Option<EntryAudit>), String> {
        let rp = run_dir.join("reports").join(format!("{}.json", entry.id));
        let bytes = std::fs::read(&rp).map_err(|e| format!("{}: {e}", rp.display()))?;
        let doc: ReportDoc =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", rp.display()))?;
        let mut truth = truth::load_truth(self.corpus, entry)?;
        let mut origins = None;
        if truth.method == "sourcemap" {
            let o = truth::load_origins(self.corpus, entry)?;
            origins = truth::redecode(self.corpus, entry, &mut truth, Some(o))?;
        }
        let (body, wrongs, a15) = self.body(entry, &doc, &truth, origins.as_ref())?;
        let p = self.pipeline;
        let mut card = vec![
            kv("schema", s(SCORER_VERSION)),
            kv("label", s(label)),
            kv("entry", s(&entry.id)),
            kv("set", s(&entry.set)),
            kv("truthMethod", s(&truth.method)),
            kv("bundleSha256", s(&entry.files.bundle.sha256)),
            kv("indexId", s(&p.index_id)),
            kv("rule", s(&p.rule)),
            kv("toolchain", from_json(&doc.toolchain)?),
            kv("detected", from_json(&doc.detected)?),
            kv(
                "discoveryRan",
                PyValue::Bool(doc.library_discovery.is_some()),
            ),
            kv(
                "noDiscovery",
                doc.no_discovery
                    .as_ref()
                    .map_or(Ok(PyValue::Null), from_json)?,
            ),
            kv("reportSha256", s(&crate::committed::sha256_hex(&bytes))),
        ];
        card.extend(body);
        Ok((dict(card), wrongs, a15))
    }
}

/// summary.json's `config` block.
fn summary_config(corpus: &Corpus, pipeline: &Pipeline) -> Result<PyValue, String> {
    let mut config = vec![
        kv("indexId", s(&pipeline.index_id)),
        kv("indexBytes", from_json(&pipeline.index_bytes)?),
        kv("rule", s(&pipeline.rule)),
        kv("buildCommit", from_json(&pipeline.build_commit)?),
        kv("binarySha256", from_json(&pipeline.binary_sha256)?),
        kv("corpusSha256", s(&pipeline.corpus_sha256)),
        kv("familiesSha256", s(&families::derived_sha())),
    ];
    for kind in [
        LayerKind::Origins,
        LayerKind::Copies2,
        LayerKind::Chained,
        LayerKind::Origins7,
        LayerKind::Origins10,
        LayerKind::Origins12,
        LayerKind::Chained9,
        LayerKind::Chained11,
        LayerKind::Misattr13,
        LayerKind::Mapdefect14,
    ] {
        config.push((kind.summary_key(), s(corpus.layer_id(kind)?)));
    }
    if let Some(st) = pipeline.settings.as_ref().filter(|v| truthy(v)) {
        config.push(kv("settings", from_json(st)?));
    }
    if corpus.name() != "dev" {
        config.push(kv("corpus", s(corpus.name())));
    }
    if let Some(sets) = pipeline.sets.as_ref().filter(|s| !s.is_empty()) {
        config.push(kv(
            "sets",
            super::py::list_s(sets.iter().map(String::as_str)),
        ));
    }
    Ok(dict(config))
}

/// summary.json's `bars` block.
fn bars(agg: &[(String, PyValue, f64)]) -> PyValue {
    let app_total: i64 = agg
        .iter()
        .map(|(_, r, _)| match r.get("appClaimedBytes") {
            Some(PyValue::Int(d)) => d.parse::<i64>().expect("an i64"),
            _ => 0,
        })
        .sum();
    dict(vec![
        kv("appClaimedBytes", int(app_total)),
        kv("appBarPassed", PyValue::Bool(app_total == 0)),
        kv("precisionBar", flt(PRECISION_BAR)),
        kv(
            "precisionBarPassed",
            dict(
                agg.iter()
                    .map(|(set, _, p)| (set.clone(), PyValue::Bool(*p >= PRECISION_BAR)))
                    .collect(),
            ),
        ),
    ])
}

/// Score one label (score.py `main`). Returns the lines score.py prints.
pub fn rescore(
    repo: &Path,
    label: &str,
    (corpus_name, data_root): (&str, Option<&str>),
    registry: &Path,
    out: &Out,
) -> Result<String, String> {
    let corpus = Corpus::open(repo, corpus_name, data_root)?;
    let run_dir = corpus.data.join("results").join(label);
    let pipeline = load_pipeline(&corpus, &run_dir, label)?;
    let private = corpus.is_private();
    let (out_dir, cards_dir) = match out {
        Out::Committed => {
            let o = repo.join(crate::paths::LIBRARY_RESULTS).join(label);
            let c = if private {
                run_dir.join("cards")
            } else {
                o.join("cards")
            };
            (o, c)
        }
        Out::Dir(d) => (d.clone(), d.join("cards")),
    };
    std::fs::create_dir_all(&cards_dir).map_err(|e| format!("{}: {e}", cards_dir.display()))?;
    let mut run = Run {
        corpus: &corpus,
        pipeline: &pipeline,
        fam: Families::shipped(),
        indexes: HashMap::new(),
        literals: None,
        registry: Registry::new(registry),
    };
    let mut sets: std::collections::BTreeMap<String, Vec<(CardSum, PyValue)>> = Default::default();
    let mut audits: std::collections::BTreeMap<String, Vec<(String, EntryAudit)>> =
        Default::default();
    let mut timing: Vec<(String, PyValue)> = Vec::new();
    let mut wrongs: std::collections::BTreeMap<String, Vec<(String, EntryWrongs)>> =
        Default::default();
    let mut sample_sets: Vec<String> = Vec::new();
    let mut log = String::new();
    let only = pipeline.sets.as_ref().filter(|s| !s.is_empty());
    for entry in corpus
        .entries()
        .iter()
        .filter(|e| only.is_none_or(|o| o.contains(&e.set)))
    {
        let (card, entry_wrongs, a15) = run.card(label, entry, &run_dir)?;
        if let Some(a) = a15 {
            audits
                .entry(entry.set.clone())
                .or_default()
                .push((entry.id.clone(), a));
        }
        match entry_wrongs {
            Some(w) => wrongs
                .entry(entry.set.clone())
                .or_default()
                .push((entry.id.clone(), w)),
            None if !sample_sets.contains(&entry.set) => sample_sets.push(entry.set.clone()),
            None => {}
        }
        write(
            &cards_dir.join(format!("{}.json", entry.id)),
            &crate::json::py_text(&card, true),
        )?;
        let tp = run_dir
            .join("reports")
            .join(format!("{}.run.json", entry.id));
        if tp.exists() {
            let t = std::fs::read_to_string(&tp).map_err(|e| format!("{}: {e}", tp.display()))?;
            timing.push((
                entry.id.clone(),
                PyValue::parse(&t).map_err(|e| format!("{}: {e}", tp.display()))?,
            ));
        }
        if !private {
            log.push_str(&entry_line(&entry.id, &card));
        }
        sets.entry(entry.set.clone())
            .or_default()
            .push((card_sum(&card), card));
    }
    let agg = aggregate(&sets);
    write_wrong_families(&cards_dir, label, &agg, &wrongs, &sample_sets, &run.fam)?;
    write_truth15(&cards_dir, label, &sets, &audits)?;
    let bars = bars(&agg);
    let mut config = summary_config(&corpus, &pipeline)?;
    if let PyValue::Dict(es) = &mut config {
        es.push(kv("copyCredit", copy_credit_config(&run, registry)));
    }
    let summary = dict(vec![
        kv("schema", s(SCORER_VERSION)),
        kv("label", s(label)),
        kv("config", config),
        kv("bars", bars.clone()),
        kv(
            "sets",
            dict(agg.into_iter().map(|(k, v, _)| (k, v)).collect()),
        ),
    ]);
    write(
        &out_dir.join("summary.json"),
        &crate::json::py_text(&summary, true),
    )?;
    write(
        &out_dir.join("timing.json"),
        &crate::json::py_text(&timing_doc(&timing, private), true),
    )?;
    log.push_str(&crate::json::py_text(&bars, true));
    Ok(log)
}

/// summary.json's `config.copyCredit`: what scorer 15's credits read.
fn copy_credit_config(run: &Run, registry: &Path) -> PyValue {
    let lits = match &run.literals {
        Some(Some(_)) => "the label's index literal table (literals.bin)",
        Some(None) => "none: the label's index has no literal table, so no claim is credited",
        None => "not read: no entry ran discovery",
    };
    dict(vec![
        kv("literals", s(lits)),
        kv("registry", s(&registry.display().to_string())),
    ])
}

/// `truth15.json` beside the cards (scorer 15's defects and credits),
/// held equal per set to the map-truth cards' columns.
fn write_truth15(
    cards_dir: &Path,
    label: &str,
    sets: &std::collections::BTreeMap<String, Vec<(CardSum, PyValue)>>,
    audits: &std::collections::BTreeMap<String, Vec<(String, EntryAudit)>>,
) -> Result<(), String> {
    let mut by_set: std::collections::BTreeMap<String, Vec<(String, &EntryAudit)>> =
        Default::default();
    for (set, es) in audits {
        let rows: Vec<(String, &EntryAudit)> = es.iter().map(|(id, a)| (id.clone(), a)).collect();
        let cards: Vec<&CardSum> = sets[set]
            .iter()
            .filter(|(c, _)| {
                es.iter()
                    .any(|(id, _)| Some(id.as_str()) == c.entry.as_deref())
            })
            .map(|(c, _)| c)
            .collect();
        let sum = |k: &str| cards.iter().map(|c| c.totals[k]).sum::<i64>();
        audit15::check_totals(
            set,
            &rows,
            (
                sum("namedAsCopyCreditBytes"),
                sum("wrongFamilyBytes"),
                sum("defectExcludedBytes"),
            ),
        )?;
        by_set.insert(set.clone(), rows);
    }
    write(
        &cards_dir.join("truth15.json"),
        &crate::json::py_text(&audit15::doc(label, &by_set), true),
    )
}

/// `wrong-families.json` beside the cards (a private corpus's stays in its
/// data dir with them), held equal to each set's charged wrong bytes.
fn write_wrong_families(
    cards_dir: &Path,
    label: &str,
    agg: &[(String, PyValue, f64)],
    wrongs: &std::collections::BTreeMap<String, Vec<(String, EntryWrongs)>>,
    sample_sets: &[String],
    fam: &Families,
) -> Result<(), String> {
    for (set, row, _) in agg {
        let Some(entries) = wrongs.get(set).filter(|_| !sample_sets.contains(set)) else {
            continue;
        };
        let want = ints_of(row).get("wrongFamilyBytes").copied().unwrap_or(0);
        wrong_families::check_total(set, entries, want)?;
    }
    let doc = wrong_families::doc(label, wrongs, sample_sets, &fam.libix);
    let dir = cards_dir.parent().unwrap_or(cards_dir);
    write(
        &dir.join("wrong-families.json"),
        &crate::json::py_text(&doc, true),
    )
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
        Value::String(x) => !x.is_empty(),
        Value::Number(n) => n.as_f64() != Some(0.0),
    }
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Every layer the summary records was frozen for this corpus manifest,
/// and the origin layers with scorer 7/10/12's rules.
fn check_layers(corpus: &Corpus, sha: &str) -> Result<(), String> {
    for (kind, what) in [
        (LayerKind::Origins, "origins.json was frozen"),
        (LayerKind::Copies2, "the copies layer was frozen"),
        (LayerKind::Chained, "the chained-sources layer was frozen"),
        (LayerKind::Chained9, "the scorer 9 chained layer was frozen"),
        (
            LayerKind::Chained11,
            "the scorer 11 chained layer was frozen",
        ),
        (LayerKind::Origins7, "the scorer 7 origin layer was frozen"),
        (
            LayerKind::Origins10,
            "the scorer 10 origin layer was frozen",
        ),
        (
            LayerKind::Origins12,
            "the scorer 12 origin layer was frozen",
        ),
        (
            LayerKind::Mapdefect14,
            "the scorer 14 map-defect layer was frozen",
        ),
    ] {
        if corpus.layer(kind)?.corpus_sha256 != sha {
            return Err(format!("REFUSED: {what} for another corpus manifest"));
        }
    }
    corpus.layer(LayerKind::Misattr13)?;
    for (kind, rules) in [
        (LayerKind::Origins7, RULES7),
        (LayerKind::Origins10, RULES10),
        (LayerKind::Origins12, RULES12),
    ] {
        if !rules_are(corpus.layer(kind)?, rules) {
            return Err(format!(
                "REFUSED: the {} layer was frozen with other rules",
                kind.prefix()
            ));
        }
    }
    if !corpus
        .layer(LayerKind::Copies2)?
        .schema
        .starts_with("exp129-copies/")
    {
        return Err("REFUSED: the copies layer is not scorer 12's v2".into());
    }
    Ok(())
}

/// score.py's per-entry progress line.
fn entry_line(id: &str, card: &PyValue) -> String {
    let t = card.get("totals").expect("totals");
    let f = |k: &str| match t.get(k) {
        Some(PyValue::Float(x)) => *x,
        _ => 0.0,
    };
    let app = match t.get("appClaimedBytes") {
        Some(PyValue::Int(d)) => d.clone(),
        _ => "0".into(),
    };
    format!(
        "{id:60} P {:.3} R {:.3} F0.5 {:.3} F1 {:.3} app {app}\n",
        f("precision"),
        f("recall"),
        f("f05"),
        f("f1")
    )
}

/// timing.json (NOT deterministic: wall clock and peak memory).
fn timing_doc(timing: &[(String, PyValue)], private: bool) -> PyValue {
    let num = |v: &PyValue, k: &str| -> f64 {
        match v.get(k) {
            Some(PyValue::Float(x)) => *x,
            Some(PyValue::Int(d)) => d.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    };
    let walls: f64 = timing
        .iter()
        .map(|(_, t)| num(t, "wallSeconds"))
        .fold(0.0, |a, b| a + b);
    let peak = timing
        .iter()
        .filter_map(|(_, t)| t.get("peakRssMb").cloned())
        .max_by(|a, b| {
            let f = |v: &PyValue| match v {
                PyValue::Int(d) => d.parse::<f64>().unwrap_or(0.0),
                PyValue::Float(x) => *x,
                _ => 0.0,
            };
            f(a).partial_cmp(&f(b)).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(int(0));
    let mut e = vec![
        kv(
            "note",
            s(
                "NOT deterministic: wall clock and peak memory of each `humanify-ag discover` run (index load included); the cards and summary.json are the deterministic part",
            ),
        ),
        kv("totalWallSeconds", flt(super::py::round_n(walls, 1))),
        kv("maxPeakRssMb", peak),
    ];
    if !private {
        e.push(kv("perEntry", PyValue::Dict(timing.to_vec())));
    }
    dict(e)
}

/// Read a written card back (determinism, leaderboard).
pub fn read_card(path: &Path) -> Result<PyValue, String> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    PyValue::parse(&t).map_err(|e| format!("{}: {e}", path.display()))
}

/// Proof (b) as a self-test: re-score `label` into `scratch` and compare
/// its cards and summary byte for byte with the committed ones (a private
/// corpus's cards: the ones beside its reports). Ok: one verdict line.
pub fn proof(
    repo: &Path,
    label: &str,
    registry: &Path,
    scratch: &Path,
) -> Result<(bool, String), String> {
    let summary = repo
        .join(crate::paths::LIBRARY_RESULTS)
        .join(label)
        .join("summary.json");
    let text =
        std::fs::read_to_string(&summary).map_err(|e| format!("{}: {e}", summary.display()))?;
    let v = PyValue::parse(&text).map_err(|e| format!("{}: {e}", summary.display()))?;
    match v.get("schema") {
        Some(PyValue::Str(x)) if x == SCORER_VERSION => {}
        other => {
            return Err(format!(
                "REFUSED: {label} was scored by {}, this harness is {SCORER_VERSION}: a proof compares one scorer version with itself (re-score the label with `library rescore --out DIR`)",
                match other {
                    Some(PyValue::Str(x)) => x.as_str(),
                    _ => "an unknown scorer",
                }
            ));
        }
    }
    let corpus_name = match v.get("config").and_then(|c| c.get("corpus")) {
        Some(PyValue::Str(c)) => c.clone(),
        _ => "dev".to_string(),
    };
    let corpus = Corpus::open(repo, &corpus_name, None)?;
    let committed_cards = if corpus.is_private() {
        corpus.data.join("results").join(label).join("cards")
    } else {
        repo.join(crate::paths::LIBRARY_RESULTS)
            .join(label)
            .join("cards")
    };
    let out = scratch.join(label);
    rescore(
        repo,
        label,
        (&corpus_name, None),
        registry,
        &Out::Dir(out.clone()),
    )?;
    let mut differ = Vec::new();
    for e in corpus.entries() {
        let name = format!("{}.json", e.id);
        let a = std::fs::read(committed_cards.join(&name)).ok();
        let b = std::fs::read(out.join("cards").join(&name)).ok();
        if a.is_none() && b.is_none() {
            continue;
        }
        if a != b {
            differ.push(e.id.clone());
        }
    }
    let same_summary = std::fs::read(&summary).ok() == std::fs::read(out.join("summary.json")).ok();
    let ok = differ.is_empty() && same_summary;
    Ok((
        ok,
        format!(
            "{label}: cards {} summary {}{}\n",
            if differ.is_empty() {
                "IDENTICAL"
            } else {
                "DIFFER"
            },
            if same_summary { "IDENTICAL" } else { "DIFFERS" },
            if differ.is_empty() {
                String::new()
            } else {
                format!(" ({})", differ.join(" "))
            }
        ),
    ))
}
