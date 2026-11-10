//! Scorer 15's two truth corrections on one map-truth card (exp214), and
//! the label's listing of every unit they touched (`truth15.json`):
//!
//! - DEFECT (`defect15`): a library-labelled unit whose labelled file
//!   cannot contain it is relabelled `<defect>` — out of the library
//!   bytes and out of every claim verdict that counts — and listed.
//! - COPY CREDIT (`copy_credit`): a `wrongFamily` claim whose code is the
//!   claimed package's own and whose truth package ships a copy of it is
//!   judged `copyCredit` — its own column — and listed with its evidence;
//!   every wrong claim left wrong is listed with why the credit was
//!   refused, so the listing's credited + refused bytes add back up to
//!   the wrong claims scorer 14's judgement made.

use std::collections::{BTreeMap, HashMap};

use super::copy_credit::{self, Claim, Credit, LiteralIndex, Outcome, Registry};
use super::defect15::{self, EntryTexts, Evidence, Lits, PackageText};
use super::index::{Index, Unit};
use super::py::{dict, flt, int, kv, opt_s, s};
use super::truth::{Label, Truth, is_library};
use crate::json::py::PyValue;

/// The label a defect leaves on its unit (not a library: out of both sides).
pub const DEFECT: &str = "<defect>";
/// How many rows of each kind the listing keeps per set (largest first);
/// the totals always cover all of them.
const ROWS: usize = 400;

/// What scorer 15 reads beyond scorer 14's inputs, once per label.
pub struct Ctx<'a> {
    /// The label's index literal table (None: the index has none, and no
    /// claim can be credited).
    pub lits: Option<&'a LiteralIndex>,
    pub registry: &'a Registry,
}

/// One defect unit.
pub struct DefectRow {
    pub s: i64,
    pub e: i64,
    pub bytes: i64,
    pub truth: String,
    pub file: Option<String>,
    pub claimed: Option<String>,
    pub ev: Evidence,
    pub holder: Option<(String, f64)>,
}

/// One credited claim.
pub struct CreditRow {
    pub s: i64,
    pub e: i64,
    pub bytes: i64,
    pub claimed: String,
    pub truth: String,
    pub truth_file: Option<String>,
    pub credit: Credit,
}

/// One entry's corrections.
pub struct EntryAudit {
    texts: EntryTexts,
    unit_lits: BTreeMap<usize, Option<Lits>>,
    /// label -> what that package's files in the corpus map hold.
    packages: BTreeMap<String, PackageText>,
    pub defects: Vec<DefectRow>,
    pub credits: Vec<CreditRow>,
    /// (claimed, truth) -> refusal reason -> bytes.
    pub refused: BTreeMap<(String, String), BTreeMap<&'static str, i64>>,
}

impl EntryAudit {
    pub fn new(texts: EntryTexts) -> EntryAudit {
        EntryAudit {
            texts,
            unit_lits: BTreeMap::new(),
            packages: BTreeMap::new(),
            defects: Vec::new(),
            credits: Vec::new(),
            refused: BTreeMap::new(),
        }
    }

    fn lits_of(&mut self, u: &Unit) -> Option<Lits> {
        if !self.unit_lits.contains_key(&u.i) {
            let text = self.texts.unit_text(u.s, u.e);
            self.unit_lits.insert(u.i, defect15::unit_literals(&text));
        }
        self.unit_lits.get(&u.i).cloned().flatten()
    }

    /// The literal sets of the files the truth labels [s, e) with
    /// `label` (None: no such file, or one has no readable text).
    fn label_lits(&mut self, truth: &Truth, s: i64, e: i64, label: &str) -> Option<Vec<Lits>> {
        let srcs = defect15::label_sources(truth, s, e, label);
        if srcs.is_empty() {
            return None;
        }
        self.texts.all_lits(&srcs)
    }

    /// What the package `label`'s files in the corpus map hold, read once.
    fn package(&mut self, truth: &Truth, label: &str) -> &PackageText {
        if !self.packages.contains_key(label) {
            let srcs: Vec<usize> = (0..truth.sources.len())
                .filter(|&i| truth.sources[i].label == label)
                .collect();
            let text = self.texts.package_text(&srcs);
            self.packages.insert(label.to_string(), text);
        }
        &self.packages[label]
    }

    /// The defect rule for one library-labelled unit: its labelled files
    /// have text, and neither the labelled PACKAGE's files in the corpus
    /// map nor (when the index has a literal table) any one of its
    /// published files hold more than `LACK` of its evidence. A unit filed
    /// under the wrong file of the RIGHT package is no defect.
    fn defect_of(&mut self, ctx: &Ctx, truth: &Truth, u: &Unit, l: &Label) -> Option<Evidence> {
        if !is_library(&l.label) || l.misattributed || l.mapdefect {
            return None;
        }
        let srcs = defect15::label_sources(truth, u.s, u.e, &l.label);
        if srcs.is_empty() || !srcs.iter().all(|&si| self.texts.has_text(si)) {
            return None;
        }
        let unit = self.lits_of(u)?;
        let pkg = self.package(truth, &l.label);
        let ev = defect15::judge_with(&unit, |x| pkg.holds(x));
        if !ev.is_defect() {
            return None;
        }
        if let Some(lits) = ctx.lits {
            let d = defect15::distinctive(&unit);
            let held = lits.owner_best_held(&l.label, &d);
            if held as f64 > defect15::LACK * d.len() as f64 {
                return None;
            }
        }
        // The label's package as installed for the corpus app: one of its
        // files holding the unit keeps the label; no installed copy on
        // this host means the defect cannot be confirmed (no verdict).
        let pkg_srcs: Vec<usize> = (0..truth.sources.len())
            .filter(|&i| truth.sources[i].label == l.label)
            .collect();
        let roots = self.texts.roots(&pkg_srcs);
        match self.texts.installed_best(&roots, &unit) {
            Some(share) if share <= defect15::LACK => Some(ev),
            _ => None,
        }
    }

    /// The defect check over every scored unit: a defect's label becomes
    /// `<defect>` (its claim, if any, is recorded with it).
    pub fn defects(
        &mut self,
        ctx: &Ctx,
        truth: &Truth,
        (every, labels): (&[&Unit], &mut HashMap<usize, Label>),
        claimed: &HashMap<usize, &str>,
    ) {
        for u in every {
            let l = labels[&u.i].clone();
            let Some(ev) = self.defect_of(ctx, truth, u, &l) else {
                continue;
            };
            let srcs = defect15::label_sources(truth, u.s, u.e, &l.label);
            let unit = self.lits_of(u).unwrap_or_default();
            let holder = self.texts.best_holder(&unit, &srcs);
            let l = &l;
            self.defects.push(DefectRow {
                s: u.s,
                e: u.e,
                bytes: u.b,
                truth: l.label.clone(),
                file: l.file.clone(),
                claimed: claimed.get(&u.i).map(|c| c.to_string()),
                ev,
                holder,
            });
            labels.insert(
                u.i,
                Label {
                    label: DEFECT.into(),
                    file: l.file.clone(),
                    ..Label::default()
                },
            );
        }
    }

    /// Scorer 15's verdict on a claim scorer 14 judged `wrongFamily`:
    /// true when credited (recorded either way).
    pub fn copy_credit(
        &mut self,
        ctx: &Ctx,
        ix: &Index,
        truth: &Truth,
        (u, claimed, label): (&Unit, &str, &Label),
    ) -> bool {
        let outcome = match ctx.lits {
            None => Outcome::Refused("no-literal-table"),
            Some(lits) => {
                let unit = self.lits_of(u);
                let tl = self.label_lits(truth, u.s, u.e, &label.label);
                let judge = copy_credit::Judge {
                    ix,
                    lits,
                    registry: ctx.registry,
                };
                judge.judge(&Claim {
                    claimed,
                    truth: &label.label,
                    truth_version: label.version.as_deref(),
                    truth_file: label.file.as_deref(),
                    print: u.w.as_ref().map(|w| w.0.h),
                    unit_lits: unit.as_ref(),
                    truth_lits: tl.as_deref(),
                })
            }
        };
        match outcome {
            Outcome::Credited(credit) => {
                self.credits.push(CreditRow {
                    s: u.s,
                    e: u.e,
                    bytes: u.b,
                    claimed: claimed.to_string(),
                    truth: label.label.clone(),
                    truth_file: label.file.clone(),
                    credit,
                });
                true
            }
            Outcome::Refused(why) => {
                *self
                    .refused
                    .entry((claimed.to_string(), label.label.clone()))
                    .or_default()
                    .entry(why)
                    .or_insert(0) += u.b;
                false
            }
        }
    }

    /// Drop the entry's texts once its card is scored (the listing keeps
    /// only the rows).
    pub fn release(&mut self) {
        self.texts = EntryTexts::new(Vec::new(), "", Vec::new(), Vec::new());
        self.unit_lits.clear();
    }

    pub fn credit_bytes(&self) -> i64 {
        self.credits.iter().map(|c| c.bytes).sum()
    }

    pub fn refused_bytes(&self) -> i64 {
        self.refused.values().flat_map(|r| r.values()).sum()
    }

    pub fn defect_bytes(&self) -> i64 {
        self.defects.iter().map(|d| d.bytes).sum()
    }
}

/// The listing must hold exactly the bytes the cards count: per set, the
/// credits are the cards' `namedAsCopyCreditBytes`, the refusals their
/// `wrongFamilyBytes` and the defects their `defectExcludedBytes`.
pub fn check_totals(
    set: &str,
    entries: &[(String, &EntryAudit)],
    (credit, wrong, defect): (i64, i64, i64),
) -> Result<(), String> {
    let got = |f: fn(&EntryAudit) -> i64| entries.iter().map(|(_, a)| f(a)).sum::<i64>();
    let have = (
        got(EntryAudit::credit_bytes),
        got(EntryAudit::refused_bytes),
        got(EntryAudit::defect_bytes),
    );
    if have != (credit, wrong, defect) {
        return Err(format!(
            "truth15 listing of set {set} holds (credit, wrong, defect) {have:?} bytes, the cards count {:?}",
            (credit, wrong, defect)
        ));
    }
    Ok(())
}

// ------------------------------------------------------------ the listing

#[derive(Default)]
struct Agg {
    bytes: i64,
    units: i64,
    claimed: i64,
    entries: Vec<String>,
    by: BTreeMap<String, i64>,
}

impl Agg {
    fn add(&mut self, id: &str, bytes: i64, claimed: bool, key: &str) {
        self.bytes += bytes;
        self.units += 1;
        if claimed {
            self.claimed += bytes;
        }
        if self.entries.last().map(String::as_str) != Some(id) {
            self.entries.push(id.to_string());
        }
        *self.by.entry(key.to_string()).or_insert(0) += bytes;
    }
}

fn by_block(m: &BTreeMap<String, i64>) -> PyValue {
    let mut v: Vec<_> = m.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    dict(v.into_iter().map(|(k, b)| (k.clone(), int(*b))).collect())
}

fn desc<K: Ord + Clone>(m: &BTreeMap<K, Agg>) -> Vec<(K, &Agg)> {
    let mut v: Vec<_> = m.iter().map(|(k, a)| (k.clone(), a)).collect();
    v.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then(a.0.cmp(&b.0)));
    v
}

fn defect_row(id: &str, d: &DefectRow) -> PyValue {
    dict(vec![
        kv("entry", s(id)),
        kv("start", int(d.s)),
        kv("end", int(d.e)),
        kv("bytes", int(d.bytes)),
        kv("truth", s(&d.truth)),
        kv("file", opt_s(d.file.as_deref())),
        kv("claimed", opt_s(d.claimed.as_deref())),
        kv("evidence", int(d.ev.evidence as i64)),
        kv("held", int(d.ev.held as i64)),
        kv(
            "absent",
            PyValue::List(d.ev.absent.iter().map(|a| s(a)).collect()),
        ),
        kv(
            "holder",
            match &d.holder {
                Some((p, sh)) => dict(vec![kv("source", s(p)), kv("share", flt(*sh))]),
                None => PyValue::Null,
            },
        ),
    ])
}

fn defects_block(entries: &[(String, &EntryAudit)]) -> PyValue {
    let mut by: BTreeMap<(String, String), Agg> = BTreeMap::new();
    let mut rows: Vec<(&str, &DefectRow)> = Vec::new();
    for (id, a) in entries {
        for d in &a.defects {
            let file = d.file.clone().unwrap_or_default();
            by.entry((d.truth.clone(), file)).or_default().add(
                id,
                d.bytes,
                d.claimed.is_some(),
                d.claimed.as_deref().unwrap_or("<unclaimed>"),
            );
            rows.push((id, d));
        }
    }
    rows.sort_by(|a, b| {
        b.1.bytes
            .cmp(&a.1.bytes)
            .then(a.0.cmp(b.0))
            .then(a.1.s.cmp(&b.1.s))
    });
    let groups = desc(&by)
        .into_iter()
        .map(|((t, f), a)| {
            dict(vec![
                kv("truth", s(&t)),
                kv("file", s(&f)),
                kv("bytes", int(a.bytes)),
                kv("units", int(a.units)),
                kv("claimedBytes", int(a.claimed)),
                kv("claimedAs", by_block(&a.by)),
                kv(
                    "entries",
                    PyValue::List(a.entries.iter().map(|e| s(e)).collect()),
                ),
            ])
        })
        .collect();
    dict(vec![
        kv("bytes", int(rows.iter().map(|r| r.1.bytes).sum())),
        kv("units", int(rows.len() as i64)),
        kv(
            "claimedBytes",
            int(rows
                .iter()
                .filter(|r| r.1.claimed.is_some())
                .map(|r| r.1.bytes)
                .sum()),
        ),
        kv("byTruthFile", PyValue::List(groups)),
        kv(
            "largest",
            PyValue::List(
                rows.iter()
                    .take(ROWS)
                    .map(|(id, d)| defect_row(id, d))
                    .collect(),
            ),
        ),
    ])
}

/// One credited (claimed, truth, link)'s bytes and its claimed files.
type CreditAgg = (Agg, BTreeMap<String, i64>);

fn credits_block(entries: &[(String, &EntryAudit)]) -> PyValue {
    let mut by: BTreeMap<(String, String, String), CreditAgg> = BTreeMap::new();
    for (id, a) in entries {
        for c in &a.credits {
            let slot = by
                .entry((
                    c.claimed.clone(),
                    c.truth.clone(),
                    c.credit.link.name().to_string(),
                ))
                .or_default();
            slot.0
                .add(id, c.bytes, true, c.truth_file.as_deref().unwrap_or("-"));
            *slot.1.entry(c.credit.own_file.clone()).or_insert(0) += c.bytes;
        }
    }
    let mut v: Vec<_> = by.iter().collect();
    v.sort_by(|a, b| b.1.0.bytes.cmp(&a.1.0.bytes).then(a.0.cmp(b.0)));
    let rows = v
        .into_iter()
        .map(|((c, t, link), (a, own))| {
            dict(vec![
                kv("claimed", s(c)),
                kv("truth", s(t)),
                kv("link", s(link)),
                kv("bytes", int(a.bytes)),
                kv("units", int(a.units)),
                kv(
                    "entries",
                    PyValue::List(a.entries.iter().map(|e| s(e)).collect()),
                ),
                kv("truthFiles", by_block(&a.by)),
                kv("claimedFiles", by_block(own)),
            ])
        })
        .collect();
    let total: i64 = entries
        .iter()
        .flat_map(|(_, a)| &a.credits)
        .map(|c| c.bytes)
        .sum();
    let mut each: Vec<(&str, &CreditRow)> = entries
        .iter()
        .flat_map(|(id, a)| a.credits.iter().map(move |c| (id.as_str(), c)))
        .collect();
    each.sort_by(|a, b| {
        b.1.bytes
            .cmp(&a.1.bytes)
            .then(a.0.cmp(b.0))
            .then(a.1.s.cmp(&b.1.s))
    });
    let largest = each
        .into_iter()
        .take(ROWS)
        .map(|(id, c)| {
            dict(vec![
                kv("entry", s(id)),
                kv("start", int(c.s)),
                kv("end", int(c.e)),
                kv("bytes", int(c.bytes)),
                kv("claimed", s(&c.claimed)),
                kv("truth", s(&c.truth)),
                kv("truthFile", opt_s(c.truth_file.as_deref())),
                kv("link", s(c.credit.link.name())),
                kv("claimedFile", s(&c.credit.own_file)),
                kv("claimedShare", flt(c.credit.own_share)),
                kv("truthShare", flt(c.credit.truth_share)),
                kv("evidence", int(c.credit.evidence as i64)),
            ])
        })
        .collect();
    dict(vec![
        kv("bytes", int(total)),
        kv(
            "units",
            int(entries.iter().map(|(_, a)| a.credits.len() as i64).sum()),
        ),
        kv("byPair", PyValue::List(rows)),
        kv("largest", PyValue::List(largest)),
    ])
}

fn refused_block(entries: &[(String, &EntryAudit)]) -> PyValue {
    let mut by: BTreeMap<(String, String), BTreeMap<String, i64>> = BTreeMap::new();
    for (_, a) in entries {
        for (k, reasons) in &a.refused {
            let slot = by.entry(k.clone()).or_default();
            for (r, b) in reasons {
                *slot.entry(r.to_string()).or_insert(0) += b;
            }
        }
    }
    let mut total_by: BTreeMap<String, i64> = BTreeMap::new();
    for reasons in by.values() {
        for (r, b) in reasons {
            *total_by.entry(r.clone()).or_insert(0) += b;
        }
    }
    let mut v: Vec<_> = by
        .iter()
        .map(|(k, r)| (k, r, r.values().sum::<i64>()))
        .collect();
    v.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(b.0)));
    let rows = v
        .into_iter()
        .take(ROWS)
        .map(|((c, t), r, b)| {
            dict(vec![
                kv("claimed", s(c)),
                kv("truth", s(t)),
                kv("bytes", int(b)),
                kv("reasons", by_block(r)),
            ])
        })
        .collect();
    dict(vec![
        kv("bytes", int(total_by.values().sum())),
        kv("reasons", by_block(&total_by)),
        kv("byPair", PyValue::List(rows)),
    ])
}

/// The label's `truth15.json`: per set, the defects, the credits and the
/// refused credits (map-truth sets only).
pub fn doc(label: &str, sets: &BTreeMap<String, Vec<(String, &EntryAudit)>>) -> PyValue {
    let out = sets
        .iter()
        .map(|(k, es)| {
            (
                k.clone(),
                dict(vec![
                    kv("defects", defects_block(es)),
                    kv("copyCredits", credits_block(es)),
                    kv("wrongStill", refused_block(es)),
                ]),
            )
        })
        .collect();
    dict(vec![
        kv("schema", s("exp214-truth15/1")),
        kv("label", s(label)),
        kv(
            "note",
            s(
                "scorer 15's truth corrections: `defects` are units whose labelled file cannot hold them (out of both sides of precision and recall); `copyCredits` are wrong-family claims credited as copies (the cards' namedAsCopyCreditBytes); `wrongStill` are the wrong-family claims left wrong, by why no credit applied (the cards' wrongFamilyBytes)",
            ),
        ),
        kv("sets", dict(out)),
    ])
}

#[cfg(test)]
#[path = "audit15_test.rs"]
mod audit15_test;
