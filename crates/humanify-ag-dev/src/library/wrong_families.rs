//! The wrong-family breakdown of a library label (exp210): which package a
//! wrong claim named, which package the truth says the bytes are, per
//! corpus entry and per set. A DIAGNOSTIC written beside the cards
//! (`wrong-families.json`), never part of a card or the summary: those
//! stay byte for byte score.py's (`library proof`). Its total per set is
//! held equal to the cards' `wrongFamilyBytes` (`check_total`), so it is
//! the same bytes the precision column charges, split up.
//!
//! Only map-truth cards are broken down: a sample-truth set (Claude Code)
//! has estimated bytes per stratum, not per claim.

use std::collections::{BTreeMap, HashMap};

use super::families::Libix;
use super::py::{dict, int, kv, s};
use crate::json::py::PyValue;

/// How many of a pair's truth files the doc lists (largest first).
const TOP_FILES: usize = 5;

/// One claim the scorer judged `wrongFamily`.
pub struct WrongClaim<'a> {
    pub claimed: &'a str,
    pub truth: &'a str,
    /// The truth's source file of the unit, when it has one.
    pub file: Option<&'a str>,
    /// The claim's rule (`seed`, `grow`, ...), when the report gives one.
    pub rule: Option<&'a str>,
    pub bytes: i64,
}

/// One (claimed package, true package) pair's bytes.
#[derive(Default)]
pub struct Pair {
    pub bytes: i64,
    pub units: i64,
    pub files: BTreeMap<String, i64>,
    pub rules: BTreeMap<String, i64>,
}

impl Pair {
    fn add(&mut self, bytes: i64, units: i64) {
        self.bytes += bytes;
        self.units += units;
    }

    fn absorb(&mut self, o: &Pair) {
        self.add(o.bytes, o.units);
        for (f, b) in &o.files {
            *self.files.entry(f.clone()).or_insert(0) += b;
        }
        for (r, b) in &o.rules {
            *self.rules.entry(r.clone()).or_insert(0) += b;
        }
    }
}

/// One corpus entry's wrong claims, by (claimed, true) package.
#[derive(Default)]
pub struct EntryWrongs {
    pairs: BTreeMap<(String, String), Pair>,
}

impl EntryWrongs {
    pub fn add(&mut self, w: WrongClaim) {
        let p = self
            .pairs
            .entry((w.claimed.to_string(), w.truth.to_string()))
            .or_default();
        p.add(w.bytes, 1);
        if let Some(f) = w.file {
            *p.files.entry(f.to_string()).or_insert(0) += w.bytes;
        }
        *p.rules
            .entry(w.rule.unwrap_or("-").to_string())
            .or_insert(0) += w.bytes;
    }

    pub fn bytes(&self) -> i64 {
        self.pairs.values().map(|p| p.bytes).sum()
    }

    pub fn units(&self) -> i64 {
        self.pairs.values().map(|p| p.units).sum()
    }

    /// Pairs, largest first (ties by key).
    pub fn pairs(&self) -> Vec<(&(String, String), &Pair)> {
        let mut v: Vec<_> = self.pairs.iter().collect();
        v.sort_by(|a, b| b.1.bytes.cmp(&a.1.bytes).then(a.0.cmp(b.0)));
        v
    }
}

/// The breakdown must charge exactly the bytes the cards charge.
pub fn check_total(set: &str, entries: &[(String, EntryWrongs)], want: i64) -> Result<(), String> {
    let got: i64 = entries.iter().map(|(_, e)| e.bytes()).sum();
    if got != want {
        return Err(format!(
            "wrong-family breakdown of set {set} sums to {got} bytes, the cards charge {want}"
        ));
    }
    Ok(())
}

fn top_files(files: &BTreeMap<String, i64>) -> PyValue {
    let mut v: Vec<_> = files.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    PyValue::List(
        v.into_iter()
            .take(TOP_FILES)
            .map(|(f, b)| dict(vec![kv("file", s(f)), kv("bytes", int(*b))]))
            .collect(),
    )
}

fn rules_block(rules: &BTreeMap<String, i64>) -> PyValue {
    dict(rules.iter().map(|(r, b)| (r.clone(), int(*b))).collect())
}

fn pair_row(claimed: &str, truth: &str, p: &Pair) -> PyValue {
    dict(vec![
        kv("claimed", s(claimed)),
        kv("truth", s(truth)),
        kv("bytes", int(p.bytes)),
        kv("units", int(p.units)),
        kv("files", int(p.files.len() as i64)),
        kv("rules", rules_block(&p.rules)),
        kv("topFiles", top_files(&p.files)),
    ])
}

fn sorted_desc<K: Ord + Clone, V>(m: &BTreeMap<K, V>, bytes: impl Fn(&V) -> i64) -> Vec<(K, &V)> {
    let mut v: Vec<_> = m.iter().map(|(k, x)| (k.clone(), x)).collect();
    v.sort_by(|a, b| bytes(b.1).cmp(&bytes(a.1)).then(a.0.cmp(&b.0)));
    v
}

/// One set's block: totals, by family pair, by package pair, by entry.
fn set_block(entries: &[(String, EntryWrongs)], fam: &Libix) -> PyValue {
    let mut by_pkg: BTreeMap<(String, String), Pair> = BTreeMap::new();
    let mut by_fam: BTreeMap<(String, String), (Pair, Vec<String>)> = BTreeMap::new();
    let mut fam_of: HashMap<String, String> = HashMap::new();
    let mut family = |p: &str| {
        fam_of
            .entry(p.to_string())
            .or_insert_with(|| fam.family(p))
            .clone()
    };
    for (id, e) in entries {
        for ((c, t), p) in &e.pairs {
            by_pkg.entry((c.clone(), t.clone())).or_default().absorb(p);
            let slot = by_fam.entry((family(c), family(t))).or_default();
            slot.0.absorb(p);
            if slot.1.last() != Some(id) {
                slot.1.push(id.clone());
            }
        }
    }
    let fam_rows = sorted_desc(&by_fam, |x| x.0.bytes)
        .into_iter()
        .map(|((cf, tf), (p, ids))| {
            dict(vec![
                kv("claimedFamily", s(&cf)),
                kv("truthFamily", s(&tf)),
                kv("bytes", int(p.bytes)),
                kv("units", int(p.units)),
                kv("files", int(p.files.len() as i64)),
                kv("entries", int(ids.len() as i64)),
            ])
        })
        .collect();
    let pkg_rows = sorted_desc(&by_pkg, |p| p.bytes)
        .into_iter()
        .map(|((c, t), p)| pair_row(&c, &t, p))
        .collect();
    let mut with: Vec<&(String, EntryWrongs)> =
        entries.iter().filter(|(_, e)| e.bytes() > 0).collect();
    with.sort_by(|a, b| b.1.bytes().cmp(&a.1.bytes()).then(a.0.cmp(&b.0)));
    let entry_rows = with
        .into_iter()
        .map(|(id, e)| {
            dict(vec![
                kv("entry", s(id)),
                kv("bytes", int(e.bytes())),
                kv("units", int(e.units())),
                kv(
                    "pairs",
                    PyValue::List(
                        e.pairs()
                            .into_iter()
                            .map(|((c, t), p)| pair_row(c, t, p))
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    dict(vec![
        kv(
            "wrongFamilyBytes",
            int(entries.iter().map(|(_, e)| e.bytes()).sum()),
        ),
        kv("units", int(entries.iter().map(|(_, e)| e.units()).sum())),
        kv("byFamily", PyValue::List(fam_rows)),
        kv("byPackage", PyValue::List(pkg_rows)),
        kv("byEntry", PyValue::List(entry_rows)),
    ])
}

/// The label's `wrong-families.json`.
pub fn doc(
    label: &str,
    sets: &BTreeMap<String, Vec<(String, EntryWrongs)>>,
    sample_sets: &[String],
    fam: &Libix,
) -> PyValue {
    let note = || {
        kv(
            "notBrokenDown",
            s("sample truth: bytes are stratified estimates, not per claim"),
        )
    };
    let mut out: Vec<(String, PyValue)> = sets
        .iter()
        .map(|(k, v)| {
            let mut block = set_block(v, fam);
            if let (true, PyValue::Dict(es)) = (sample_sets.contains(k), &mut block) {
                // a mixed set: its sample-truth entries are left out
                es.push(note());
            }
            (k.clone(), block)
        })
        .collect();
    for k in sample_sets.iter().filter(|k| !sets.contains_key(*k)) {
        out.push(kv(k, dict(vec![note()])));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    dict(vec![
        kv("schema", s("exp210-wrong-families/1")),
        kv("label", s(label)),
        kv(
            "note",
            s(
                "DIAGNOSTIC: the cards' wrongFamilyBytes split by (claimed package, true package); families are the scorer's (library::families). Not a KPI; the summary's precision is the number",
            ),
        ),
        kv("sets", dict(out)),
    ])
}

#[cfg(test)]
#[path = "wrong_families_test.rs"]
mod wrong_families_test;
