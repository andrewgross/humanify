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

    /// `placementTrail.report()` as `--diagnostics` writes it: `tiers` in
    /// first-seen order, then every entry in record order with its RAW
    /// span (JS string indexes into `shipped`; absent without one) — keys
    /// in the recorded object's order, `nameCount` appended by the
    /// recorder after `evidence`.
    pub fn diagnostics_report(&self, shipped: &str) -> humanify_model::js::JsValue {
        use humanify_model::js::{JsObject, JsValue, Utf16Offsets};
        let offsets = Utf16Offsets::new(shipped);
        let mut tiers = JsObject::new();
        for (t, n) in &self.tiers {
            tiers.insert(t.clone(), JsValue::Number(*n as f64));
        }
        let trails = self
            .rows
            .iter()
            .zip(&self.dissent_order)
            .map(|(r, dissent)| {
                let mut o = JsObject::new();
                o.insert("index", JsValue::Number(r.index as f64));
                if r.key.start >= 0 {
                    let mut span = JsObject::new();
                    span.insert(
                        "start",
                        JsValue::Number(f64::from(offsets.at(r.key.start as u32))),
                    );
                    span.insert(
                        "end",
                        JsValue::Number(f64::from(offsets.at(r.key.end as u32))),
                    );
                    o.insert("span", JsValue::Object(span));
                }
                o.insert("names", str_list(&r.names));
                insert_placed(&mut o, r, dissent.as_deref());
                o.insert_opt("nameCount", r.name_count.map(|n| JsValue::Number(n as f64)));
                JsValue::Object(o)
            })
            .collect();
        let mut out = JsObject::new();
        out.insert("tiers", JsValue::Object(tiers));
        out.insert("trails", JsValue::Array(trails));
        JsValue::Object(out)
    }

    /// The dump's `placement.json` as `writePlacement` writes it: rows
    /// sorted by span key (stable), each `{key, index, names, nameCount?,
    /// placedBy, file, priorFile?, priorFileFrom?, hashMiss?, alternatives?,
    /// evidence}` — the writer's literal order, the alternatives in the
    /// tiers' order and the evidence in the record site's.
    pub fn placement_json(&self) -> humanify_model::js::JsValue {
        use humanify_model::js::{JsObject, JsValue};
        let mut order: Vec<usize> = (0..self.rows.len()).collect();
        order.sort_by_key(|&i| (self.rows[i].key.start, self.rows[i].key.end));
        let placements = order
            .into_iter()
            .map(|i| {
                let r = &self.rows[i];
                let mut key = JsObject::new();
                key.insert("text", JsValue::str(r.key.text.as_str()));
                key.insert("start", JsValue::Number(r.key.start as f64));
                key.insert("end", JsValue::Number(r.key.end as f64));
                let mut o = JsObject::new();
                o.insert("key", JsValue::Object(key));
                o.insert("index", JsValue::Number(r.index as f64));
                o.insert("names", str_list(&r.names));
                o.insert_opt("nameCount", r.name_count.map(|n| JsValue::Number(n as f64)));
                insert_placed(&mut o, r, self.dissent_order[i].as_deref());
                JsValue::Object(o)
            })
            .collect();
        let mut out = JsObject::new();
        out.insert("schemaVersion", JsValue::Number(1.0));
        out.insert("placements", JsValue::Array(placements));
        JsValue::Object(out)
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

fn str_list(v: &[String]) -> humanify_model::js::JsValue {
    use humanify_model::js::JsValue;
    JsValue::Array(v.iter().map(|s| JsValue::str(s.as_str())).collect())
}

/// A row's decision fields in the recorded order: `placedBy, file,
/// priorFile?, priorFileFrom?, hashMiss?, alternatives?` (the dissenting
/// tiers, their order), then `evidence` (`votes, allSame, anchor`).
fn insert_placed(
    o: &mut humanify_model::js::JsObject,
    r: &PlacementRow,
    dissent: Option<&[(String, String)]>,
) {
    use humanify_model::js::{JsObject, JsValue};
    o.insert("placedBy", JsValue::str(r.placed_by.as_str()));
    o.insert("file", JsValue::str(r.file.as_str()));
    o.insert_opt("priorFile", r.prior_file.as_deref().map(JsValue::str));
    o.insert_opt(
        "priorFileFrom",
        r.prior_file_from.as_deref().map(JsValue::str),
    );
    o.insert_opt("hashMiss", r.hash_miss.as_deref().map(JsValue::str));
    if let Some(d) = dissent {
        let mut alt = JsObject::new();
        for (tier, file) in d {
            alt.insert(tier.clone(), JsValue::str(file.as_str()));
        }
        o.insert("alternatives", JsValue::Object(alt));
    }
    let mut evidence = JsObject::new();
    for key in ["votes", "allSame", "anchor"] {
        match r.evidence.get(key) {
            Some(serde_json::Value::Array(items)) => {
                let items: Vec<String> = items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect();
                evidence.insert(key, str_list(&items));
            }
            Some(serde_json::Value::String(s)) => {
                evidence.insert(key, JsValue::str(s.as_str()));
            }
            _ => {}
        }
    }
    o.insert("evidence", JsValue::Object(evidence));
}

#[cfg(test)]
mod trail_test;
