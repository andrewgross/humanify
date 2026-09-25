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

use humanify_model::dump::{PlacementFile, PlacementRow, SpanKey};
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

impl PlacementTrail {
    /// `record(entry)`.
    pub fn record(&mut self, entry: TrailEntry) {
        match self.tiers.iter_mut().find(|(t, _)| *t == entry.placed_by) {
            Some((_, n)) => *n += 1,
            None => self.tiers.push((entry.placed_by.clone(), 1)),
        }
        let alternatives = dissenters(entry.alternatives.as_deref(), &entry.file);
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

    /// The dump's `placement.json` (`writePlacement`): rows sorted by span
    /// key (`spanKeyOrder`: start, then end).
    pub fn to_placement_file(&self) -> PlacementFile {
        let mut placements = self.rows.clone();
        placements.sort_by_key(|r| (r.key.start, r.key.end));
        PlacementFile {
            schema_version: humanify_model::dump::DUMP_SCHEMA_VERSION,
            placements,
        }
    }
}

#[cfg(test)]
mod trail_test;
