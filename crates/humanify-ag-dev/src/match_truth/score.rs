//! The scorer (exp092; ported from experiments/lib/match-truth/score.ts):
//! one `humanify-ag match` dump in, one deterministic scorecard out —
//! precision-like and recall-like numbers for the matching machinery
//! against the harness's ground-truth rule.
//!
//! GROUND TRUTH. A function/statement whose canonical form
//! ([`super::canonical`]) appears EXACTLY ONCE on each side is a must-match
//! pair: the same code, provably, modulo wrapper spelling and identifier
//! renaming. Canonical forms that repeat on a side (duplicate helpers) are
//! excluded — the matcher may pair those in any order — and counted
//! separately (`duplicateClass`).
//!
//! WHAT EACH NUMBER IS.
//! - `recall` — of the must-match pairs, the fraction the matcher reported.
//!   THE primary metric.
//! - `reportedClasses` — every reported pair classified by what the two
//!   slices actually are: `identical` (canonical-equal), `near` (line
//!   distance within the near threshold), `far`. `far` is NOT proof of a
//!   wrong match — it is a LEAD list.
//! - `statements.recall` — the same rule over the statement twins
//!   (proposals, not bridging: `abstained:no-candidacy` is by design).
//!
//! The scorecard's JSON ([`scorecard_json`]) is score.ts's object, key for
//! key, in the JS number spelling.

use std::collections::{HashMap, HashSet};

use humanify_ag_model::js::{JsObject, JsValue};
use serde::Deserialize;
use serde_json::Value;

use super::canonical::{canonical, canonical_loose, line_diff_mass};

pub const SCORECARD_SCHEMA_VERSION: u32 = 1;

const FAR_SLICE_CHARS: usize = 400;
const NEAR_ABSOLUTE_LINES: usize = 3;
const NEAR_FRACTION: f64 = 0.2;

// --- the dump (only the fields the scorer reads) --------------------------------

#[derive(Deserialize)]
struct InventoryRow {
    name: String,
    slice: String,
}

#[derive(Deserialize)]
struct StatementRow {
    start: u64,
    end: u64,
    slice: String,
}

#[derive(Deserialize)]
struct PairRow {
    prior: usize,
    fresh: usize,
    tier: String,
}

#[derive(Deserialize)]
struct Span {
    start: u64,
    end: u64,
}

#[derive(Deserialize)]
struct GatesRow {
    prior: Span,
    fresh: Span,
    outcome: String,
}

#[derive(Deserialize)]
struct Gates {
    rows: Vec<GatesRow>,
}

#[derive(Deserialize)]
struct Functions {
    prior: Vec<InventoryRow>,
    fresh: Vec<InventoryRow>,
    pairs: Vec<PairRow>,
}

#[derive(Deserialize)]
struct Twins {
    prior: Vec<StatementRow>,
    fresh: Vec<StatementRow>,
    gates: Gates,
}

#[derive(Deserialize)]
struct DumpFile {
    path: String,
    functions: Functions,
    twins: Twins,
}

// --- the scorecard ---------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Classes {
    pub identical: usize,
    pub near: usize,
    pub far: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Missed {
    pub prior_name: String,
    pub fresh_name: String,
    pub prior_slice: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FarPair {
    pub prior_name: String,
    pub fresh_name: String,
    pub tier: String,
    pub prior_slice: String,
    pub fresh_slice: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FunctionScore {
    pub prior_count: usize,
    pub fresh_count: usize,
    pub prior_names: Vec<String>,
    pub must_match: usize,
    pub matched_of_must: usize,
    pub recall: Option<f64>,
    pub duplicate_class: usize,
    pub should_match: usize,
    pub matched_of_should: usize,
    pub should_recall: Option<f64>,
    pub reported: usize,
    pub reported_classes: Classes,
    /// Sorted by tier name.
    pub tier_counts: Vec<(String, usize)>,
    pub missed_must: Vec<Missed>,
    pub missed_should: Vec<Missed>,
    pub far_pairs: Vec<FarPair>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StatementScore {
    pub prior_count: usize,
    pub fresh_count: usize,
    pub must_match: usize,
    pub proposed_of_must: usize,
    pub bridged_of_must: usize,
    pub recall: Option<f64>,
    pub duplicate_class: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scorecard {
    pub path: String,
    pub functions: FunctionScore,
    pub statements: StatementScore,
}

fn ratio(num: usize, den: usize) -> Option<f64> {
    (den != 0).then(|| num as f64 / den as f64)
}

/// Canonical forms, memoized by slice text.
struct Memo<'a> {
    form: fn(&str) -> String,
    seen: HashMap<&'a str, String>,
}

impl<'a> Memo<'a> {
    fn new(form: fn(&str) -> String) -> Memo<'a> {
        Memo {
            form,
            seen: HashMap::new(),
        }
    }

    fn of(&mut self, slice: &'a str) -> String {
        let form = self.form;
        self.seen
            .entry(slice)
            .or_insert_with(|| form(slice))
            .clone()
    }
}

fn counts_of(values: &[String]) -> HashMap<&str, usize> {
    let mut m = HashMap::new();
    for v in values {
        *m.entry(v.as_str()).or_insert(0) += 1;
    }
    m
}

fn first_index_of(values: &[String]) -> HashMap<&str, usize> {
    let mut m = HashMap::new();
    for (i, v) in values.iter().enumerate() {
        m.entry(v.as_str()).or_insert(i);
    }
    m
}

/// must-pairs: canonical forms unique on BOTH sides; plus the rows excluded
/// as the duplicate class.
fn must_pairs(prior: &[String], fresh: &[String]) -> (Vec<(usize, usize)>, usize) {
    let pc = counts_of(prior);
    let fc = counts_of(fresh);
    let first_fresh = first_index_of(fresh);
    let count = |m: &HashMap<&str, usize>, c: &str| m.get(c).copied().unwrap_or(0);
    let in_duplicate_class = |c: &str| count(&pc, c) > 1 || count(&fc, c) > 1;
    let mut must = Vec::new();
    let mut duplicate_class = 0;
    for (i, c) in prior.iter().enumerate() {
        if count(&pc, c) == 1 && count(&fc, c) == 1 {
            must.push((i, first_fresh[c.as_str()]));
        } else if in_duplicate_class(c) {
            duplicate_class += 1;
        }
    }
    duplicate_class += fresh.iter().filter(|c| in_duplicate_class(c)).count();
    (must, duplicate_class)
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    Identical,
    Near,
    Far,
}

fn classify(prior: &str, fresh: &str) -> Class {
    if prior == fresh {
        return Class::Identical;
    }
    let lines = |s: &str| s.matches('\n').count() + 1;
    let max_lines = lines(prior).max(lines(fresh));
    let mass = line_diff_mass(prior, fresh);
    let near_bar = NEAR_ABSOLUTE_LINES.max((NEAR_FRACTION * max_lines as f64).floor() as usize);
    if mass <= near_bar {
        Class::Near
    } else {
        Class::Far
    }
}

/// score.ts's `slicePreview`: the first 400 UTF-16 units and `…`.
fn slice_preview(s: &str) -> String {
    let u: Vec<u16> = s.encode_utf16().collect();
    if u.len() > FAR_SLICE_CHARS {
        // A cut through a surrogate pair reads U+FFFD here (JSON.stringify
        // wrote the lone surrogate as an escape).
        format!("{}…", String::from_utf16_lossy(&u[..FAR_SLICE_CHARS]))
    } else {
        s.to_string()
    }
}

fn missed(prior: &InventoryRow, fresh: &InventoryRow) -> Missed {
    Missed {
        prior_name: prior.name.clone(),
        fresh_name: fresh.name.clone(),
        prior_slice: slice_preview(&prior.slice),
    }
}

fn span_index(rows: &[StatementRow]) -> HashMap<(u64, u64), usize> {
    let mut m = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        // A later row with the same span wins, as in the JS Map.
        m.insert((r.start, r.end), i);
    }
    m
}

/// Statement twins: the must-set, which of it was proposed as a twin, and
/// how much of that bridged slots.
fn score_statements<'a>(twins: &'a Twins, memo: &mut Memo<'a>) -> StatementScore {
    let prior: Vec<String> = twins.prior.iter().map(|s| memo.of(&s.slice)).collect();
    let fresh: Vec<String> = twins.fresh.iter().map(|s| memo.of(&s.slice)).collect();
    let pc = counts_of(&prior);
    let fc = counts_of(&fresh);
    let prior_idx = span_index(&twins.prior);
    let fresh_idx = span_index(&twins.fresh);
    let mut proposed = HashSet::new();
    let mut bridged = HashSet::new();
    for row in &twins.gates.rows {
        let p = prior_idx.get(&(row.prior.start, row.prior.end));
        let f = fresh_idx.get(&(row.fresh.start, row.fresh.end));
        let (Some(&p), Some(&f)) = (p, f) else {
            continue;
        };
        proposed.insert((p, f));
        if row.outcome == "bridged" {
            bridged.insert((p, f));
        }
    }
    let first_fresh = first_index_of(&fresh);
    let (mut must_match, mut proposed_of_must, mut bridged_of_must, mut duplicate_class) =
        (0, 0, 0, 0);
    for (i, c) in prior.iter().enumerate() {
        let p_count = pc.get(c.as_str()).copied().unwrap_or(0);
        let f_count = fc.get(c.as_str()).copied().unwrap_or(0);
        if p_count != 1 || f_count != 1 {
            if p_count > 1 || f_count > 1 {
                duplicate_class += 1;
            }
            continue;
        }
        must_match += 1;
        let key = (i, first_fresh[c.as_str()]);
        if proposed.contains(&key) {
            proposed_of_must += 1;
            if bridged.contains(&key) {
                bridged_of_must += 1;
            }
        }
    }
    StatementScore {
        prior_count: twins.prior.len(),
        fresh_count: twins.fresh.len(),
        must_match,
        proposed_of_must,
        bridged_of_must,
        recall: ratio(proposed_of_must, must_match),
        duplicate_class,
    }
}

/// Check the dump's envelope the way score.ts did, then read its one file.
fn dump_file(dump: &Value) -> Result<DumpFile, String> {
    let Some(obj) = dump.as_object() else {
        return Err("the match dump is not a JSON object".into());
    };
    let version = obj.get("schemaVersion");
    if version.and_then(Value::as_f64) != Some(1.0) {
        let shown = version.map_or_else(|| "undefined".to_string(), Value::to_string);
        return Err(format!(
            "unsupported match dump schemaVersion: {shown} (expected 1)"
        ));
    }
    let files = obj.get("files").and_then(Value::as_array);
    let count = files.map_or(0, Vec::len);
    if count != 1 {
        return Err(format!(
            "expected exactly one file section in the dump, got {count} (multi-file unpacks are not scored yet)"
        ));
    }
    let file = &files.expect("one file")[0];
    DumpFile::deserialize(file).map_err(|e| format!("the match dump's file section: {e}"))
}

/// Score one `humanify-ag match` dump. Errs loudly on a foreign shape.
pub fn score_match_dump(dump: &Value) -> Result<Scorecard, String> {
    let file = dump_file(dump)?;
    let mut memo = Memo::new(canonical);
    let mut loose = Memo::new(canonical_loose);
    let fns = &file.functions;
    let prior: Vec<String> = fns.prior.iter().map(|r| memo.of(&r.slice)).collect();
    let fresh: Vec<String> = fns.fresh.iter().map(|r| memo.of(&r.slice)).collect();
    let (must, duplicate_class) = must_pairs(&prior, &fresh);

    // Tier 2 (should-match, advisory): loose-canonical pairs outside the
    // tier-1 must set.
    let prior_loose: Vec<String> = fns.prior.iter().map(|r| loose.of(&r.slice)).collect();
    let fresh_loose: Vec<String> = fns.fresh.iter().map(|r| loose.of(&r.slice)).collect();
    let tier1: HashSet<(usize, usize)> = must.iter().copied().collect();
    let should: Vec<(usize, usize)> = must_pairs(&prior_loose, &fresh_loose)
        .0
        .into_iter()
        .filter(|k| !tier1.contains(k))
        .collect();

    let reported: HashSet<(usize, usize)> = fns.pairs.iter().map(|p| (p.prior, p.fresh)).collect();
    // A later pair for the same key wins, as in the JS Map.
    let tier_of: HashMap<(usize, usize), &str> = fns
        .pairs
        .iter()
        .map(|p| ((p.prior, p.fresh), p.tier.as_str()))
        .collect();

    let mut tiers: HashMap<&str, usize> = HashMap::new();
    let mut missed_must = Vec::new();
    for &(p, f) in &must {
        match tier_of.get(&(p, f)) {
            Some(tier) => *tiers.entry(tier).or_insert(0) += 1,
            None => missed_must.push(missed(&fns.prior[p], &fns.fresh[f])),
        }
    }
    let matched_of_must = must.len() - missed_must.len();
    let mut tier_counts: Vec<(String, usize)> =
        tiers.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    tier_counts.sort_by(|a, b| humanify_ag_model::js::cmp_utf16(&a.0, &b.0));

    let mut classes = Classes::default();
    let mut far_pairs = Vec::new();
    for pair in &fns.pairs {
        match classify(&prior[pair.prior], &fresh[pair.fresh]) {
            Class::Identical => classes.identical += 1,
            Class::Near => classes.near += 1,
            Class::Far => {
                classes.far += 1;
                far_pairs.push(FarPair {
                    prior_name: fns.prior[pair.prior].name.clone(),
                    fresh_name: fns.fresh[pair.fresh].name.clone(),
                    tier: pair.tier.clone(),
                    prior_slice: slice_preview(&fns.prior[pair.prior].slice),
                    fresh_slice: slice_preview(&fns.fresh[pair.fresh].slice),
                });
            }
        }
    }

    let missed_should: Vec<Missed> = should
        .iter()
        .filter(|k| !reported.contains(k))
        .map(|&(p, f)| missed(&fns.prior[p], &fns.fresh[f]))
        .collect();
    let matched_of_should = should.len() - missed_should.len();

    let statements = score_statements(&file.twins, &mut memo);
    Ok(Scorecard {
        path: file.path.clone(),
        functions: FunctionScore {
            prior_count: fns.prior.len(),
            fresh_count: fns.fresh.len(),
            prior_names: fns.prior.iter().map(|r| r.name.clone()).collect(),
            must_match: must.len(),
            matched_of_must,
            recall: ratio(matched_of_must, must.len()),
            duplicate_class,
            should_match: should.len(),
            matched_of_should,
            should_recall: ratio(matched_of_should, should.len()),
            reported: fns.pairs.len(),
            reported_classes: classes,
            tier_counts,
            missed_must,
            missed_should,
            far_pairs,
        },
        statements,
    })
}

// --- JSON (score.ts's object shapes) ----------------------------------------------

fn num(n: usize) -> JsValue {
    JsValue::Number(n as f64)
}

fn opt(r: Option<f64>) -> JsValue {
    r.map_or(JsValue::Null, JsValue::Number)
}

fn obj(entries: Vec<(&str, JsValue)>) -> JsValue {
    JsValue::Object(JsObject::from_entries(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    ))
}

fn classes_json(c: &Classes) -> JsValue {
    obj(vec![
        ("identical", num(c.identical)),
        ("near", num(c.near)),
        ("far", num(c.far)),
    ])
}

fn missed_json(list: &[Missed]) -> JsValue {
    JsValue::Array(
        list.iter()
            .map(|m| {
                obj(vec![
                    ("priorName", JsValue::str(&m.prior_name)),
                    ("freshName", JsValue::str(&m.fresh_name)),
                    ("priorSlice", JsValue::str(&m.prior_slice)),
                ])
            })
            .collect(),
    )
}

fn far_json(list: &[FarPair]) -> JsValue {
    JsValue::Array(
        list.iter()
            .map(|m| {
                obj(vec![
                    ("priorName", JsValue::str(&m.prior_name)),
                    ("freshName", JsValue::str(&m.fresh_name)),
                    ("tier", JsValue::str(&m.tier)),
                    ("priorSlice", JsValue::str(&m.prior_slice)),
                    ("freshSlice", JsValue::str(&m.fresh_slice)),
                ])
            })
            .collect(),
    )
}

fn functions_json(f: &FunctionScore) -> JsValue {
    obj(vec![
        ("priorCount", num(f.prior_count)),
        ("freshCount", num(f.fresh_count)),
        ("priorNames", JsValue::str_array(&f.prior_names)),
        ("mustMatch", num(f.must_match)),
        ("matchedOfMust", num(f.matched_of_must)),
        ("recall", opt(f.recall)),
        ("duplicateClass", num(f.duplicate_class)),
        ("shouldMatch", num(f.should_match)),
        ("matchedOfShould", num(f.matched_of_should)),
        ("shouldRecall", opt(f.should_recall)),
        ("reported", num(f.reported)),
        ("reportedClasses", classes_json(&f.reported_classes)),
        (
            "tierCounts",
            JsValue::Object(
                f.tier_counts
                    .iter()
                    .map(|(k, v)| (k.clone(), num(*v)))
                    .collect(),
            ),
        ),
        ("missedMust", missed_json(&f.missed_must)),
        ("missedShould", missed_json(&f.missed_should)),
        ("farPairs", far_json(&f.far_pairs)),
    ])
}

fn statements_json(s: &StatementScore) -> JsValue {
    obj(vec![
        ("priorCount", num(s.prior_count)),
        ("freshCount", num(s.fresh_count)),
        ("mustMatch", num(s.must_match)),
        ("proposedOfMust", num(s.proposed_of_must)),
        ("bridgedOfMust", num(s.bridged_of_must)),
        ("recall", opt(s.recall)),
        ("duplicateClass", num(s.duplicate_class)),
    ])
}

/// The scorecard as score.ts's `Scorecard` object.
pub fn scorecard_json(card: &Scorecard) -> JsValue {
    obj(vec![
        ("schemaVersion", num(SCORECARD_SCHEMA_VERSION as usize)),
        ("path", JsValue::str(&card.path)),
        ("functions", functions_json(&card.functions)),
        ("statements", statements_json(&card.statements)),
    ])
}

/// Totals over per-package scorecards (exp092's baseline row).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Aggregate {
    pub packages: usize,
    pub must_match: usize,
    pub matched_of_must: usize,
    pub should_match: usize,
    pub matched_of_should: usize,
    pub reported: usize,
    pub reported_classes: Classes,
    pub statement_must_match: usize,
    pub statement_proposed_of_must: usize,
    pub statement_bridged_of_must: usize,
}

impl Aggregate {
    pub fn recall(&self) -> Option<f64> {
        ratio(self.matched_of_must, self.must_match)
    }

    pub fn of<'a>(cards: impl IntoIterator<Item = &'a Scorecard>) -> Aggregate {
        let mut t = Aggregate::default();
        for c in cards {
            let f = &c.functions;
            t.packages += 1;
            t.must_match += f.must_match;
            t.matched_of_must += f.matched_of_must;
            t.should_match += f.should_match;
            t.matched_of_should += f.matched_of_should;
            t.reported += f.reported;
            t.reported_classes.identical += f.reported_classes.identical;
            t.reported_classes.near += f.reported_classes.near;
            t.reported_classes.far += f.reported_classes.far;
            t.statement_must_match += c.statements.must_match;
            t.statement_proposed_of_must += c.statements.proposed_of_must;
            t.statement_bridged_of_must += c.statements.bridged_of_must;
        }
        t
    }

    /// score.ts's `Aggregate` object.
    pub fn to_json(&self) -> JsValue {
        obj(vec![
            ("packages", num(self.packages)),
            ("mustMatch", num(self.must_match)),
            ("matchedOfMust", num(self.matched_of_must)),
            ("recall", opt(self.recall())),
            ("shouldMatch", num(self.should_match)),
            ("matchedOfShould", num(self.matched_of_should)),
            (
                "shouldRecall",
                opt(ratio(self.matched_of_should, self.should_match)),
            ),
            ("reported", num(self.reported)),
            ("reportedClasses", classes_json(&self.reported_classes)),
            ("statementMustMatch", num(self.statement_must_match)),
            (
                "statementProposedOfMust",
                num(self.statement_proposed_of_must),
            ),
            (
                "statementBridgedOfMust",
                num(self.statement_bridged_of_must),
            ),
            (
                "statementRecall",
                opt(ratio(
                    self.statement_proposed_of_must,
                    self.statement_must_match,
                )),
            ),
        ])
    }
}

#[cfg(test)]
#[path = "score_test.rs"]
mod score_test;
