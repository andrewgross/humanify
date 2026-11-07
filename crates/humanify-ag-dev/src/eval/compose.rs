//! Diff composition (TS: `037/diff-composition.ts`): the on-disk diff of a
//! split tree decomposed into real change and each noise mechanism, in GIT
//! LINE units.
//!
//! Per common file, top-level statements are matched in tiers:
//!
//! 1. exact (hash + text) on both sides: unchanged content; off the LCS of
//!    the exact-matched sequence it is REORDER (delete + add);
//! 2. same hash, different text: NAMING churn, charged the line-level LCS
//!    churn of the pair (FIFO among same-hash candidates); ALIAS when both
//!    sides are `const x = require("...")` header lines;
//! 3. a hash present on one side only is first offered as an EDITED
//!    version of a removed statement (same masked head, >= 50% token
//!    overlap: charged the set-based edited lines); else REAL whole mass.
//!
//! Files on one side only are charged whole to `fileAddRemove`.
//!
//! SOFT NOISE (advisory): among the statements about to be charged as
//! real whole mass, a fresh and a removed statement that are the same code
//! modulo WRAPPER SPELLING — equal under the split's `STATEMENT` form (the
//! ONE rule, `hash::wrapper_spelling`) though not under the KPI form — are
//! reported in `spelling_identical_lines`. Their charge STAYS inside
//! `real` (the raw charge); the clean diff subtracts it.
//!
//! Scorer v2 definition change (README): the TS detector paired only a
//! wrapper HEAD spelled `(0, ns.m)(<arrow|function> {` and re-parsed the
//! normalised text; v2 reads the one Rust rule, which unifies every SAFE
//! arrow in the statement. Same advisory meaning, one rule.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::stmts::{FileStmt, file_statements};
use super::text::{EditPairer, edited_line_counts, line_churn, list_js, on_lcs, read};

/// Git-line churn of a split tree diff, split by what caused each line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub real: usize,
    pub naming: usize,
    pub alias: usize,
    pub reorder: usize,
    pub file_add_remove: usize,
    /// Charged INSIDE `real` (advisory): wrapper-spelling pairs' mass.
    pub spelling_identical_lines: usize,
}

impl Tally {
    fn add(&mut self, o: &Tally) {
        self.real += o.real;
        self.naming += o.naming;
        self.alias += o.alias;
        self.reorder += o.reorder;
        self.file_add_remove += o.file_add_remove;
        self.spelling_identical_lines += o.spelling_identical_lines;
    }
}

/// What a sampled instance was charged as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleKind {
    Reorder,
    Naming,
    Alias,
    Real,
    Spelling,
}

/// One charged instance, kept so the diff can be READ (never touches a
/// tally).
#[derive(Clone, Debug)]
pub struct Sample {
    pub kind: SampleKind,
    pub file: String,
    pub lines: usize,
    pub prior_text: Option<String>,
    pub fresh_text: Option<String>,
}

fn take_first(bucket: &mut Vec<usize>) -> Option<usize> {
    if bucket.is_empty() {
        None
    } else {
        Some(bucket.remove(0))
    }
}

/// One file pair's classification in progress: the statements, the tally
/// it charges and the samples it keeps.
struct Classifier<'a, 's> {
    prior: &'a [FileStmt],
    fresh: &'a [FileStmt],
    file: &'a str,
    tally: &'a mut Tally,
    samples: Option<&'s mut Vec<Sample>>,
}

fn exact_key(s: &FileStmt) -> String {
    format!("{}\u{0}{}", s.hash, s.text)
}

impl Classifier<'_, '_> {
    fn keep(&mut self, kind: SampleKind, lines: usize, prior: Option<usize>, fresh: Option<usize>) {
        if let Some(out) = self.samples.as_deref_mut() {
            out.push(Sample {
                kind,
                file: self.file.to_string(),
                lines,
                prior_text: prior.map(|i| self.prior[i].text.clone()),
                fresh_text: fresh.map(|i| self.fresh[i].text.clone()),
            });
        }
    }

    /// 1. exact (hash + text) pairing, FIFO by multiset: (fresh matched,
    ///    fresh rest, prior matched, prior rest). The leftover prior copies
    ///    are each key's FIRST copies, as the TS consumed them.
    fn exact_tier(&self) -> [Vec<usize>; 4] {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for s in self.prior {
            *counts.entry(exact_key(s)).or_default() += 1;
        }
        let split = |stmts: &[FileStmt], counts: &mut HashMap<String, usize>| {
            let (mut hit, mut miss) = (Vec::new(), Vec::new());
            for (i, s) in stmts.iter().enumerate() {
                match counts.get_mut(&exact_key(s)) {
                    Some(n) if *n > 0 => {
                        *n -= 1;
                        hit.push(i);
                    }
                    _ => miss.push(i),
                }
            }
            (hit, miss)
        };
        let (fresh_matched, fresh_rest) = split(self.fresh, &mut counts);
        // Here a "hit" is a leftover copy (the count nothing matched).
        let (prior_rest, prior_matched) = split(self.prior, &mut counts);
        [fresh_matched, fresh_rest, prior_matched, prior_rest]
    }

    /// REORDER: exact-matched statements off the LCS (delete + add).
    fn reorder(&mut self, prior_matched: &[usize], fresh_matched: &[usize]) {
        let pk: Vec<String> = prior_matched
            .iter()
            .map(|&i| exact_key(&self.prior[i]))
            .collect();
        let fk: Vec<String> = fresh_matched
            .iter()
            .map(|&i| exact_key(&self.fresh[i]))
            .collect();
        let pkr: Vec<&str> = pk.iter().map(String::as_str).collect();
        let fkr: Vec<&str> = fk.iter().map(String::as_str).collect();
        let in_order = on_lcs(&pkr, &fkr);
        for (j, &i) in fresh_matched.iter().enumerate() {
            if !in_order.contains(&j) {
                let lines = self.fresh[i].lines().len() * 2;
                self.tally.reorder += lines;
                self.keep(SampleKind::Reorder, lines, None, Some(i));
            }
        }
    }

    /// 2. same hash, different text: NAMING churn, FIFO among the
    ///    candidates. Returns the novel fresh statements and the removed
    ///    prior ones (first-seen-hash order, the TS Map's).
    fn naming_tier(
        &mut self,
        prior_rest: &[usize],
        fresh_rest: &[usize],
    ) -> (Vec<usize>, Vec<usize>) {
        let mut by_hash: HashMap<&str, Vec<usize>> = HashMap::new();
        let mut order: Vec<&str> = Vec::new();
        for &i in prior_rest {
            let h = self.prior[i].hash.as_str();
            by_hash
                .entry(h)
                .or_insert_with(|| {
                    order.push(h);
                    Vec::new()
                })
                .push(i);
        }
        let mut novel = Vec::new();
        for &i in fresh_rest {
            let Some(t) = by_hash
                .get_mut(self.fresh[i].hash.as_str())
                .and_then(take_first)
            else {
                novel.push(i);
                continue;
            };
            let (s, twin) = (&self.fresh[i], &self.prior[t]);
            let churn = line_churn(&twin.lines(), &s.lines());
            let alias = s.is_require && twin.is_require;
            if alias {
                self.tally.alias += churn;
            } else {
                self.tally.naming += churn;
            }
            if churn > 0 {
                let kind = if alias {
                    SampleKind::Alias
                } else {
                    SampleKind::Naming
                };
                self.keep(kind, churn, Some(t), Some(i));
            }
        }
        let removed = order
            .iter()
            .flat_map(|h| by_hash.get(h).cloned().unwrap_or_default())
            .collect();
        (novel, removed)
    }

    /// 3. an edited version of a removed statement (`text::EditPairer`):
    ///    charged the set-based edited lines. Returns the unpaired novel
    ///    statements and the removed ones no edit used.
    fn edited_tier(&mut self, novel: &[usize], removed: &[usize]) -> (Vec<usize>, Vec<usize>) {
        let mut pairer = EditPairer::new(
            removed
                .iter()
                .map(|&r| self.prior[r].text.as_str())
                .collect(),
        );
        let mut unpaired = Vec::new();
        for &i in novel {
            match pairer.take(&self.fresh[i].text) {
                Some(k) => {
                    let c = removed[k];
                    let (f, p) = edited_line_counts(&self.fresh[i].text, &self.prior[c].text);
                    self.tally.real += f + p;
                    self.keep(SampleKind::Real, f + p, Some(c), Some(i));
                }
                None => unpaired.push(i),
            }
        }
        let leftover = (0..removed.len())
            .filter(|&k| !pairer.used(k))
            .map(|k| removed[k])
            .collect();
        (unpaired, leftover)
    }

    /// 4. one-sided statements: REAL whole mass; the wrapper-spelling
    ///    pairs among them reported alongside (their charge stays in real).
    fn one_sided(&mut self, unpaired: &[usize], leftover: &[usize]) {
        for &i in unpaired {
            let lines = self.fresh[i].lines().len();
            self.tally.real += lines;
            self.keep(SampleKind::Real, lines, None, Some(i));
        }
        for &r in leftover {
            let lines = self.prior[r].lines().len();
            self.tally.real += lines;
            self.keep(SampleKind::Real, lines, Some(r), None);
        }
        for (f, r) in spelling_pairs(self.prior, self.fresh, unpaired, leftover) {
            let lines = self.fresh[f].lines().len() + self.prior[r].lines().len();
            self.tally.spelling_identical_lines += lines;
            self.keep(SampleKind::Spelling, lines, Some(r), Some(f));
        }
    }
}

/// Classify ONE common file pair's statements into `tally`; samples (when
/// asked) go to `samples`.
pub fn classify_statements(
    prior: &[FileStmt],
    fresh: &[FileStmt],
    file: &str,
    tally: &mut Tally,
    samples: Option<&mut Vec<Sample>>,
) {
    let mut c = Classifier {
        prior,
        fresh,
        file,
        tally,
        samples,
    };
    let [fresh_matched, fresh_rest, prior_matched, prior_rest] = c.exact_tier();
    c.reorder(&prior_matched, &fresh_matched);
    let (novel, removed) = c.naming_tier(&prior_rest, &fresh_rest);
    let (unpaired, leftover) = c.edited_tier(&novel, &removed);
    c.one_sided(&unpaired, &leftover);
}

/// The soft-noise pairing, FIFO: a leftover fresh statement and a leftover
/// removed one equal under the spelling-unified form but not under the KPI
/// form (so they differ only by wrapper spelling). Pure: touches no tally.
fn spelling_pairs(
    prior: &[FileStmt],
    fresh: &[FileStmt],
    unpaired: &[usize],
    leftover: &[usize],
) -> Vec<(usize, usize)> {
    let mut by_spelling: HashMap<&str, Vec<usize>> = HashMap::new();
    for &r in leftover {
        by_spelling
            .entry(prior[r].spelling_hash.as_str())
            .or_default()
            .push(r);
    }
    let mut out = Vec::new();
    for &f in unpaired {
        let s = &fresh[f];
        let Some(bucket) = by_spelling.get_mut(s.spelling_hash.as_str()) else {
            continue;
        };
        if let Some(pos) = bucket.iter().position(|&r| prior[r].hash != s.hash) {
            out.push((f, bucket.remove(pos)));
        }
    }
    out
}

/// The tally for ONE file pair (texts).
pub fn compose_file(prior: &str, fresh: &str, file: &str) -> Result<Tally, String> {
    let mut tally = Tally::default();
    classify_statements(
        &file_statements(prior, &format!("prior {file}"))?,
        &file_statements(fresh, &format!("fresh {file}"))?,
        file,
        &mut tally,
        None,
    );
    Ok(tally)
}

/// Whole-file charge of a one-sided file: its text's line count.
fn whole(text: &str) -> usize {
    text.split('\n').count()
}

/// A tree composition: the totals, each common file's `real` charge (the
/// app/vendor split caps a move's take at it), and the samples when asked.
#[derive(Clone, Debug, Default)]
pub struct Composition {
    pub tally: Tally,
    pub real_by_file: HashMap<String, usize>,
    pub samples: Vec<Sample>,
}

/// Decompose the on-disk diff between two split trees. With
/// `want_samples`, also collect every charged instance (`eval diff`'s
/// real-column ledger).
pub fn compose_diff(
    prior_dir: &Path,
    fresh_dir: &Path,
    want_samples: bool,
) -> Result<Composition, String> {
    let prior_files: HashSet<String> = list_js(prior_dir)?.into_iter().collect();
    let fresh_files = list_js(fresh_dir)?;
    let fresh_set: HashSet<&String> = fresh_files.iter().collect();
    let per_file = humanify_ag_core::par::map_ordered(
        &fresh_files,
        |f| -> Result<(Tally, Vec<Sample>), String> {
            let mut tally = Tally::default();
            let mut samples = Vec::new();
            let fresh = read(&fresh_dir.join(f))?;
            if prior_files.contains(f) {
                let prior = read(&prior_dir.join(f))?;
                if prior != fresh {
                    let p = file_statements(&prior, &format!("prior {f}"))?;
                    let q = file_statements(&fresh, &format!("fresh {f}"))?;
                    classify_statements(
                        &p,
                        &q,
                        f,
                        &mut tally,
                        want_samples.then_some(&mut samples),
                    );
                }
            } else {
                tally.file_add_remove += whole(&fresh);
            }
            Ok((tally, samples))
        },
    );
    let mut out = Composition::default();
    for (f, r) in fresh_files.iter().zip(per_file) {
        let (t, s) = r?;
        out.tally.add(&t);
        if prior_files.contains(f) {
            out.real_by_file.insert(f.clone(), t.real);
        }
        out.samples.extend(s);
    }
    let mut removed: Vec<&String> = prior_files
        .iter()
        .filter(|f| !fresh_set.contains(f))
        .collect();
    removed.sort();
    for f in removed {
        out.tally.file_add_remove += whole(&read(&prior_dir.join(f))?);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "compose_test.rs"]
mod compose_test;
