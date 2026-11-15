//! The per-statement FILE PLACEMENT trail — TS `src/split/placement-trail.ts`.
//!
//! Observation only: every placement strategy records each statement it
//! placed, and the recorder never feeds a decision. The TS singleton
//! (`placementTrail`, armed by `--diagnostics`) is a value here — the
//! caller owns one per run and hands it to the strategy.
//!
//! Recording rules ported exactly (`record`): `names` truncated at
//! [`MAX_NAMES`] with `nameCount` carrying the real total only when cut;
//! `alternatives` reduced to the DISSENTING tiers (absent when all agree);
//! the bulky `evidence` kept only where it explains something
//! (`keepsEvidence`), else `{}`.

use humanify_ag_model::dump::{PlacementRow, SpanKey};
use humanify_ag_model::ordered::OrderedMap;
use serde::Serialize;
use serde_json::{Map, Value};

/// Names beyond this are noise in a trail (`MAX_NAMES`).
pub const MAX_NAMES: usize = 32;

/// Tiers whose individual decisions always keep their full evidence
/// (`DETAILED_TIERS`).
const DETAILED_TIERS: [&str; 7] = [
    "conflict",
    "novote",
    "allsame",
    "anchor",
    "anchorPreempt",
    "preempt",
    "fill",
];

/// `PlacementEvidence`: the vote arrays and the anchor verdict. Field
/// order is the TS object's (`votes`, `allSame`, `anchor`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlacementEvidence {
    pub votes: Option<Vec<String>>,
    pub all_same: Option<Vec<String>>,
    pub anchor: Option<String>,
}

impl PlacementEvidence {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        if let Some(v) = &self.votes {
            m.insert("votes".into(), Value::from(v.clone()));
        }
        if let Some(v) = &self.all_same {
            m.insert("allSame".into(), Value::from(v.clone()));
        }
        if let Some(a) = &self.anchor {
            m.insert("anchor".into(), Value::from(a.clone()));
        }
        Value::Object(m)
    }
}

/// `PlacementTrailEntry` as a strategy hands it to [`PlacementTrail::record`].
#[derive(Clone, Debug, Default)]
pub struct TrailEntry {
    /// Bundle-order index of the top-level statement.
    pub index: usize,
    /// The statement's span in the shipped text (UTF-8 bytes); `None` is
    /// the TS's missing-offset case (written as -1/-1).
    pub span: Option<(u32, u32)>,
    pub names: Vec<String>,
    pub placed_by: String,
    pub file: String,
    pub prior_file: Option<String>,
    pub prior_file_from: Option<&'static str>,
    pub hash_miss: Option<&'static str>,
    /// Tier → file for every non-winning, non-locality tier that had an
    /// opinion, in registry order (`tierVerdicts`).
    pub alternatives: Option<Vec<(String, String)>>,
    pub evidence: PlacementEvidence,
}

/// The run's trail (`PlacementTrailRecorder`): tier counts + one row per
/// statement, in record order.
#[derive(Debug, Default)]
pub struct PlacementTrail {
    /// tier → statements placed, in first-seen order.
    pub tiers: Vec<(String, usize)>,
    pub rows: Vec<PlacementRow>,
    /// Per row, the dissenting alternatives in the tiers' own order (the
    /// row's `alternatives` is a sorted map; `--diagnostics` writes the
    /// recorded order).
    dissent_order: Vec<Option<Vec<(String, String)>>>,
}

/// `dissenters`: the subset of `alternatives` that disagrees with `file`,
/// or `None` when all agree (or there were none).
fn dissenters(alternatives: Option<&[(String, String)]>, file: &str) -> Option<Map<String, Value>> {
    let mut out = Map::new();
    for (tier, candidate) in alternatives? {
        if candidate != file {
            out.insert(tier.clone(), Value::from(candidate.clone()));
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The tier a statement moved by the load-time cycle guard is counted
/// under.
pub const CYCLE_GUARD_TIER: &str = "load-time-cycle-guard";

impl PlacementTrail {
    /// `record(entry)`.
    pub fn record(&mut self, entry: TrailEntry) {
        match self.tiers.iter_mut().find(|(t, _)| *t == entry.placed_by) {
            Some((_, n)) => *n += 1,
            None => self.tiers.push((entry.placed_by.clone(), 1)),
        }
        let alternatives = dissenters(entry.alternatives.as_deref(), &entry.file);
        self.dissent_order.push(alternatives.as_ref().map(|_| {
            entry
                .alternatives
                .iter()
                .flatten()
                .filter(|(_, candidate)| *candidate != entry.file)
                .cloned()
                .collect()
        }));
        // keepsEvidence: a detailed tier, a dissent, or a MOVE.
        let keeps = DETAILED_TIERS.contains(&entry.placed_by.as_str())
            || alternatives.is_some()
            || entry.prior_file.as_ref().is_some_and(|p| *p != entry.file);
        let (start, end) = entry
            .span
            .map_or((-1, -1), |(s, e)| (i64::from(s), i64::from(e)));
        let name_count = (entry.names.len() > MAX_NAMES).then_some(entry.names.len() as u64);
        let mut names = entry.names;
        names.truncate(MAX_NAMES);
        self.rows.push(PlacementRow {
            key: SpanKey {
                text: "shipped".to_string(),
                start,
                end,
            },
            index: entry.index as u64,
            names,
            name_count,
            placed_by: entry.placed_by,
            file: entry.file,
            prior_file: entry.prior_file,
            prior_file_from: entry.prior_file_from.map(str::to_string),
            hash_miss: entry.hash_miss.map(str::to_string),
            alternatives: alternatives.map(Value::Object),
            evidence: if keeps {
                entry.evidence.to_json()
            } else {
                Value::Object(Map::new())
            },
        });
    }

    /// The load-time cycle guard moved statement `index` into `file`
    /// ([`crate::emit::cycle_guard`]): its row says so, the move counted
    /// under the guard's tier instead of the one that placed it.
    pub fn moved_by_cycle_guard(&mut self, index: usize, file: &str) {
        let Some(row) = self.rows.iter_mut().find(|r| r.index == index as u64) else {
            return;
        };
        if let Some((_, n)) = self.tiers.iter_mut().find(|(t, _)| *t == row.placed_by) {
            *n = n.saturating_sub(1);
        }
        row.placed_by = CYCLE_GUARD_TIER.to_string();
        row.file = file.to_string();
        match self.tiers.iter_mut().find(|(t, _)| t == CYCLE_GUARD_TIER) {
            Some((_, n)) => *n += 1,
            None => self.tiers.push((CYCLE_GUARD_TIER.to_string(), 1)),
        }
    }

    /// The `--diagnostics` placement report: `tiers` in first-seen order,
    /// then every row in record order with its span as JS string indexes
    /// into `shipped` (absent without one) and its dissenting tiers in
    /// their own order.
    pub fn diagnostics_report(&self, shipped: &str) -> TrailReport<'_> {
        let offsets = humanify_ag_model::js::Utf16Offsets::new(shipped);
        let trails = self
            .rows
            .iter()
            .zip(&self.dissent_order)
            .map(|(r, dissent)| TrailReportRow {
                index: r.index,
                span: (r.key.start >= 0).then(|| ReportSpan {
                    start: offsets.at(r.key.start as u32),
                    end: offsets.at(r.key.end as u32),
                }),
                names: &r.names,
                placed_by: &r.placed_by,
                file: &r.file,
                prior_file: r.prior_file.as_deref(),
                prior_file_from: r.prior_file_from.as_deref(),
                hash_miss: r.hash_miss.as_deref(),
                alternatives: dissent.clone().map(OrderedMap::from),
                evidence: &r.evidence,
                name_count: r.name_count,
            })
            .collect();
        TrailReport {
            tiers: OrderedMap::from(self.tiers.clone()),
            trails,
        }
    }
}

/// The `--diagnostics` placement report ([`PlacementTrail::diagnostics_report`]).
#[derive(Debug, Serialize)]
pub struct TrailReport<'t> {
    pub tiers: OrderedMap<usize>,
    pub trails: Vec<TrailReportRow<'t>>,
}

/// A span as JS string indexes.
#[derive(Debug, Serialize)]
pub struct ReportSpan {
    pub start: u32,
    pub end: u32,
}

/// One placed statement in the `--diagnostics` report.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrailReportRow<'t> {
    pub index: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<ReportSpan>,
    pub names: &'t [String],
    pub placed_by: &'t str,
    pub file: &'t str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_file: Option<&'t str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_file_from: Option<&'t str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_miss: Option<&'t str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<OrderedMap<String>>,
    pub evidence: &'t Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_count: Option<u64>,
}

#[cfg(test)]
mod trail_test;
