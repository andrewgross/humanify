//! The run identity record and the ONE comparison guard
//! (docs/dev-harness.md §3).
//!
//! Every card — eval pair, library entry, neutrality leg, walk hop, e2e
//! run — has one [`Identity`] in one schema. A LABEL's identity is derived
//! from its cards ([`label_cards`]), never written over: today run.sh
//! rewrites `pipeline.json` on every run, so a forced partial re-run
//! silently replaced the record of the earlier cards.
//!
//! [`comparable`] is the one guard: one function and one table
//! ([`Purpose::fields`]) of the fields each purpose requires to be equal.
//! It replaces eval.ts's `guardLabel` / `guardPipeline` / `guardBaseMode`,
//! the leaderboard's base- and naming-mode comparisons, leaderboard.py's
//! corpus / scorer refusal and pipeline-bin's commit refusal — none of
//! which checked the library index. A field nobody recorded is
//! [`F::Unknown`], and UNKNOWN NEVER EQUALS ANYTHING (stricter than today,
//! on purpose: a missing record is unknown, not passing); `--force-mixed`
//! is the only override, and it prints every mismatch by field name.

pub mod backfill;

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The identity schema version.
pub const SCHEMA: u32 = 1;

/// A field's value, or why it is not known.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum F<T> {
    Known(T),
    Unknown(UnknownWhy),
}

/// Why a field is unknown (serialized `{"unknown": "<why>"}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnknownWhy {
    pub unknown: String,
}

impl<T> F<T> {
    pub fn unknown(why: &str) -> F<T> {
        F::Unknown(UnknownWhy {
            unknown: why.to_string(),
        })
    }

    pub fn known(&self) -> Option<&T> {
        match self {
            F::Known(v) => Some(v),
            F::Unknown(_) => None,
        }
    }

    /// `Some(v)` known, `None` unknown for `why`.
    pub fn from_option(v: Option<T>, why: &str) -> F<T> {
        v.map_or_else(|| F::unknown(why), F::Known)
    }
}

/// What a card is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CardKind {
    EvalPair,
    LibraryEntry,
    NeutralityLeg,
    WalkHop,
    E2e,
}

/// The commit `cargo dev` was built from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Harness {
    pub commit: F<String>,
    pub dirty: F<bool>,
}

/// The pipeline binary that produced the card.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Binary {
    pub path: F<String>,
    pub sha256: F<String>,
    pub build_commit: F<String>,
    pub dirty: F<bool>,
    /// `humanify` (before exp186) or `humanify-ag`.
    pub name: F<String>,
}

/// The library index a names-mode run read.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LibraryIndex {
    /// The index's manifest sha (its directory name in the store).
    pub id: F<String>,
    pub dir: F<String>,
    pub bytes: F<u64>,
}

/// The pipeline settings every launch of the card carried.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PipelineSettings {
    /// The `--pipeline-set` entries, in order.
    pub sets: F<Vec<String>>,
    /// The launch's `--config` TOML sha (None: a launch with no file).
    pub config_toml_sha256: F<Option<String>>,
}

/// The LLM side of the card.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Llm {
    pub model: F<String>,
    pub reasoning_effort: F<Option<String>>,
    pub concurrency: F<u64>,
    /// sha256 of the endpoint URL — never the URL (a private address in a
    /// public repo).
    pub endpoint_id: F<String>,
    /// None: no cache (every prompt reached the model).
    pub cache: F<Option<LlmCache>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LlmCache {
    pub dir: F<String>,
    pub entries_before: F<u64>,
    pub entries_after: F<u64>,
}

/// What the card measured: the corpus, its layers, this card's entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Corpus {
    /// `034-pairs`, or a library corpus name.
    pub name: F<String>,
    /// The corpus manifest's identity (library); `pairs.json`'s sha (eval).
    pub manifest_sha256: F<String>,
    /// Each truth layer's identity by key (library); None for the eval.
    pub layers: F<Option<BTreeMap<String, String>>>,
    /// This card's pair (`2.1.85->2.1.86`) or library entry id.
    pub entry: String,
    /// The card's input files' shas, by path (eval: the scored input).
    pub inputs: F<BTreeMap<String, String>>,
}

/// The scorer that wrote the card.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Scorer {
    pub name: String,
    pub version: F<String>,
}

/// One card's identity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Identity {
    pub schema: u32,
    pub kind: CardKind,
    pub harness: Harness,
    pub binary: Binary,
    /// scratch / seeded / archive; None outside the eval.
    pub base_mode: F<Option<String>>,
    /// names / off: the EFFECTIVE mode (the binary's own record); None
    /// where naming does not apply (a library entry).
    pub naming_mode: F<Option<String>>,
    /// None in off mode.
    pub library_index: F<Option<LibraryIndex>>,
    pub pipeline_settings: PipelineSettings,
    /// None where no LLM is involved (a library entry).
    pub llm: F<Option<Llm>>,
    pub corpus: Corpus,
    pub scorer: Scorer,
    pub boot_gate_model: F<Option<String>>,
    pub run_config_sha256: F<Option<String>>,
}

impl Identity {
    /// The record as written beside a card (`identity.json`, JS spelling).
    pub fn to_text(&self) -> String {
        let compact = serde_json::to_string(self).expect("an identity serializes");
        let value =
            humanify_ag_model::js::JsValue::parse(&compact).expect("serde_json writes JSON");
        crate::json::js_text(&value, true)
    }

    pub fn from_text(text: &str) -> Result<Identity, String> {
        serde_json::from_str(text).map_err(|e| format!("identity.json: {e}"))
    }
}

/// What a comparison is for; each purpose has its own equal-field list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Adding a card to a label: everything but what differs card to card.
    AddCard,
    /// The eval leaderboard: compare labels' KPIs.
    Leaderboard,
    /// The library leaderboard.
    LibraryLeaderboard,
    /// Noise bands from same-commit repeats.
    Bands,
    /// The two legs of a neutrality run.
    NeutralityLegs,
}

/// Fields that differ card to card within one label by construction.
const PER_CARD: &[&str] = &[
    "corpus.entry",
    "corpus.inputs",
    "binary.path",
    "llm.cache.entriesBefore",
    "llm.cache.entriesAfter",
    // A launch's TOML names its own output paths, so it differs per pair.
    "pipelineSettings.configTomlSha256",
    // A forced partial re-run (`--pairs`) writes its own run config; what
    // it SET is compared through pipelineSettings.sets.
    "runConfigSha256",
];

const LEADERBOARD: &[&str] = &[
    "baseMode",
    "namingMode",
    "libraryIndex.id",
    "scorer",
    "llm.model",
    "llm.reasoningEffort",
    "corpus.entry",
    "corpus.inputs",
];

impl Purpose {
    /// The fields (dotted paths; a path covers everything under it) that
    /// must be equal. `None` = every field except [`PER_CARD`].
    pub fn fields(self) -> Option<Vec<&'static str>> {
        match self {
            Purpose::AddCard => None,
            Purpose::Leaderboard => Some(LEADERBOARD.to_vec()),
            Purpose::LibraryLeaderboard => Some(vec![
                "corpus.name",
                "corpus.manifestSha256",
                "corpus.layers",
                "corpus.entry",
                "scorer",
            ]),
            Purpose::Bands => {
                let mut f = vec!["binary.sha256"];
                f.extend_from_slice(LEADERBOARD);
                Some(f)
            }
            Purpose::NeutralityLegs => Some(vec![
                "llm.model",
                "llm.reasoningEffort",
                "llm.concurrency",
                "llm.endpointId",
                "llm.cache.dir",
                "pipelineSettings.sets",
                "libraryIndex.id",
                "namingMode",
            ]),
        }
    }
}

/// One field that differs (or is unknown on either side).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    pub field: String,
    pub a: String,
    pub b: String,
    /// Neither side recorded it.
    pub both_unknown: bool,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} vs {}", self.field, self.a, self.b)
    }
}

/// How much of a path a purpose compares.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Need {
    /// Everything at and under the path.
    All,
    /// Only some fields under it: descend.
    Some,
    No,
}

fn under(path: &str, field: &str) -> bool {
    path == field || path.starts_with(&format!("{field}."))
}

fn need(purpose: Purpose, path: &str) -> Need {
    match purpose.fields() {
        Some(fields) => {
            if fields.iter().any(|f| under(path, f)) {
                Need::All
            } else if fields.iter().any(|f| path.is_empty() || under(f, path)) {
                Need::Some
            } else {
                Need::No
            }
        }
        None => {
            if PER_CARD.iter().any(|f| under(path, f)) {
                Need::No
            } else if PER_CARD.iter().any(|f| path.is_empty() || under(f, path)) {
                Need::Some
            } else {
                Need::All
            }
        }
    }
}

/// The `why` of an unknown field's record, if `v` is one.
fn unknown_why(v: &Value) -> Option<&str> {
    match v {
        Value::Object(m) if m.len() == 1 => m.get("unknown").and_then(Value::as_str),
        _ => None,
    }
}

fn show(v: Option<&Value>) -> String {
    match v {
        None => "(absent)".to_string(),
        Some(v) => match unknown_why(v) {
            Some(why) => format!("UNKNOWN ({why})"),
            None => v.to_string(),
        },
    }
}

/// Compare `a` and `b` at `path` in parallel: descend while both are
/// records and only part of the path is required; anywhere else the
/// values must be equal and KNOWN.
fn walk(
    purpose: Purpose,
    path: &str,
    a: Option<&Value>,
    b: Option<&Value>,
    out: &mut Vec<Mismatch>,
) {
    let n = need(purpose, path);
    if n == Need::No {
        return;
    }
    if let (Some(ma), Some(mb)) = (record(a), record(b)) {
        let mut keys: Vec<&String> = ma.keys().chain(mb.keys()).collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            let child = if path.is_empty() {
                k.clone()
            } else {
                format!("{path}.{k}")
            };
            walk(purpose, &child, ma.get(k), mb.get(k), out);
        }
        return;
    }
    let unknown = |v: Option<&Value>| v.and_then(unknown_why).is_some();
    let equal = a == b && !unknown(a) && !unknown(b) && !contains_unknown(a);
    if !equal {
        out.push(Mismatch {
            field: path.to_string(),
            a: show(a),
            b: show(b),
            both_unknown: unknown(a) && unknown(b),
        });
    }
}

/// The fields of `v` when it is a record (not an unknown marker).
fn record(v: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    match v {
        Some(Value::Object(m))
            if !(m.len() == 1 && m.get("unknown").is_some_and(Value::is_string)) =>
        {
            Some(m)
        }
        _ => None,
    }
}

/// Whether a value holds an unknown field anywhere inside it.
fn contains_unknown(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Object(m)) => {
            unknown_why(v.unwrap_or(&Value::Null)).is_some()
                || m.values().any(|c| contains_unknown(Some(c)))
        }
        Some(Value::Array(items)) => items.iter().any(|c| contains_unknown(Some(c))),
        _ => false,
    }
}

/// THE guard: may `a` and `b` be compared for `purpose`? Every required
/// field must be KNOWN on both sides and equal; each failure is one
/// [`Mismatch`], named by its field (a record replaced by `null` on one
/// side is one mismatch at the record, not one per field).
pub fn comparable(purpose: Purpose, a: &Identity, b: &Identity) -> Result<(), Vec<Mismatch>> {
    let va = serde_json::to_value(a).expect("an identity serializes");
    let vb = serde_json::to_value(b).expect("an identity serializes");
    let mut out = Vec::new();
    walk(purpose, "", Some(&va), Some(&vb), &mut out);
    if out.is_empty() { Ok(()) } else { Err(out) }
}

/// The guard's verdict for a caller: Ok with nothing to say, Ok with the
/// loud `MIXED` lines when forced, or Err with the refusal.
pub fn guard(
    purpose: Purpose,
    a: (&str, &Identity),
    b: (&str, &Identity),
    force_mixed: bool,
) -> Result<Vec<String>, String> {
    match comparable(purpose, a.1, b.1) {
        Ok(()) => Ok(Vec::new()),
        Err(m) => {
            let lines: Vec<String> = m
                .iter()
                .map(|x| format!("  {}: {} = {} vs {} = {}", x.field, a.0, x.a, b.0, x.b))
                .collect();
            if force_mixed {
                let mut out = vec![format!(
                    "!! MIXED ({purpose:?}, --force-mixed): {} and {} differ in {} field(s), shown anyway:",
                    a.0,
                    b.0,
                    m.len()
                )];
                out.extend(lines);
                Ok(out)
            } else {
                Err(format!(
                    "refusing ({purpose:?}): {} and {} differ in {} field(s) — unknown never equals \
                     anything:\n{}\nPass --force-mixed if mixing is deliberate.",
                    a.0,
                    b.0,
                    m.len(),
                    lines.join("\n")
                ))
            }
        }
    }
}

impl Purpose {
    /// Whether two labels may be compared over the pairs they share when
    /// their pair sets differ. The leaderboard may: an off-mode reference
    /// scored the four original pairs, a label scored after exp201 has
    /// five, and the comparison is still meaningful over the common four
    /// — said out loud, its totals re-folded over those pairs only. Bands
    /// may not: a band is the spread of TOTALS, and totals over different
    /// pair sets measure the pair set, not the floor.
    fn compares_common_pairs(self) -> bool {
        matches!(self, Purpose::Leaderboard)
    }
}

/// Which pairs a two-label comparison covers, and what to say about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelScope {
    /// The pairs both labels scored, in the first label's order.
    pub common: Vec<String>,
    /// The loud lines (`!! COMMON PAIRS`, `!! MIXED`); empty when the two
    /// labels scored the same pairs and agree on every field.
    pub lines: Vec<String>,
}

/// The pairs (card entries) a label's cards cover, in card order.
pub fn pairs_of(cards: &[Identity]) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for i in cards {
        if !v.contains(&i.corpus.entry) {
            v.push(i.corpus.entry.clone());
        }
    }
    v
}

/// THE guard over two LABELS (each a list of per-pair cards): the pair
/// sets first, then [`guard`] on every common pair. Different pair sets
/// are compared over their common pairs with a `!! COMMON PAIRS` line when
/// the purpose allows it ([`Purpose::compares_common_pairs`]), and refused
/// otherwise — `--force-mixed` never forces a pair-set mismatch, since a
/// total over different pairs is not a comparison of anything. Two labels
/// with no pair in common are always refused.
pub fn guard_labels(
    purpose: Purpose,
    a: (&str, &[Identity]),
    b: (&str, &[Identity]),
    force_mixed: bool,
) -> Result<LabelScope, String> {
    let (ea, eb) = (pairs_of(a.1), pairs_of(b.1));
    let common: Vec<String> = ea.iter().filter(|e| eb.contains(e)).cloned().collect();
    let only = |x: &[String], y: &[String]| -> Vec<String> {
        x.iter().filter(|e| !y.contains(e)).cloned().collect()
    };
    let (only_a, only_b) = (only(&ea, &eb), only(&eb, &ea));
    let mut lines = Vec::new();
    if !only_a.is_empty() || !only_b.is_empty() {
        let side = |name: &str, v: &[String]| {
            (!v.is_empty()).then(|| format!("only {name}: {}", v.join(",")))
        };
        let diff: Vec<String> = [side(a.0, &only_a), side(b.0, &only_b)]
            .into_iter()
            .flatten()
            .collect();
        let diff = diff.join("; ");
        if common.is_empty() {
            return Err(format!(
                "refusing ({purpose:?}): {} and {} have no pair in common ({diff}).",
                a.0, b.0
            ));
        }
        if !purpose.compares_common_pairs() {
            return Err(format!(
                "refusing ({purpose:?}): {} and {} scored different pairs ({diff}) — totals over \
                 different pair sets measure the pair set, not this comparison.",
                a.0, b.0
            ));
        }
        lines.push(format!(
            "!! COMMON PAIRS: {} and {} scored different pairs ({diff}); every total is over the {} \
             common pairs ({}) — the other pairs are left out of BOTH sides.",
            a.0,
            b.0,
            common.len(),
            common.join(",")
        ));
    }
    for e in &common {
        let (Some(ia), Some(ib)) = (
            a.1.iter().find(|i| &i.corpus.entry == e),
            b.1.iter().find(|i| &i.corpus.entry == e),
        ) else {
            continue;
        };
        lines.extend(guard(
            purpose,
            (&format!("{}[{e}]", a.0), ia),
            (&format!("{}[{e}]", b.0), ib),
            force_mixed,
        )?);
    }
    Ok(LabelScope { common, lines })
}

/// A label is the set of its cards' identities: they must agree on
/// everything but the per-card fields ([`Purpose::AddCard`]). Err lists
/// each card that disagrees with the first. A field NO card of the label
/// recorded is not a disagreement: a label cannot contradict itself about
/// what it never wrote down (a legacy label's gaps), while a new card
/// added to it — known where the label is unknown — is refused.
pub fn label_cards(cards: &[Identity]) -> Result<(), Vec<String>> {
    let Some(first) = cards.first() else {
        return Ok(());
    };
    let errors: Vec<String> = cards[1..]
        .iter()
        .filter_map(|c| {
            let m: Vec<Mismatch> = comparable(Purpose::AddCard, first, c)
                .err()
                .unwrap_or_default()
                .into_iter()
                .filter(|m| !m.both_unknown)
                .collect();
            (!m.is_empty()).then(|| {
                let fields: Vec<String> = m.iter().map(Mismatch::to_string).collect();
                format!(
                    "card {} disagrees with card {}: {}",
                    c.corpus.entry,
                    first.corpus.entry,
                    fields.join("; ")
                )
            })
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
#[path = "identity_test.rs"]
mod identity_test;
