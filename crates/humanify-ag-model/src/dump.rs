//! The decision records the stages share with the artifact dump: the
//! close tier's file, the strategy trail's transfer rows and the
//! placement trail's rows. Each derives `Serialize`; the dump writes them
//! as they are (`humanify_ag_core::artifact_dump::DumpSink`).

/// A span key: which anchored text a span indexes into,
/// half-open, in UTF-8 byte offsets.
#[derive(
    serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug,
)]
pub struct SpanKey {
    /// Which anchored text — "fresh", "prior", or "minified".
    pub text: String,
    pub start: i64,
    pub end: i64,
}

impl SpanKey {
    pub fn display(&self) -> String {
        format!("{}[{}..{})", self.text, self.start, self.end)
    }
}

impl std::fmt::Display for SpanKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}[{}..{})", self.text, self.start, self.end)
    }
}

/// One scored candidate's fate. `score` is the cosine f64 (the two legs
/// compute identical bits; the `scoreBits` column is the tie identity the
/// diff reads first — a tie abstains on EXACT float equality).
#[derive(serde::Serialize, Clone, PartialEq, Debug)]
pub struct CloseCandidateRow {
    pub prior: SpanKey,
    pub fresh: SpanKey,
    pub score: f64,
    #[serde(rename = "scoreBits")]
    pub score_bits: String,
    pub rank: u64,
    pub outcome: String,
}

/// One name-transfer pair (oldName = the minified NEW name).
#[derive(serde::Serialize, Clone, PartialEq, Eq, Debug)]
pub struct CloseNamePair {
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "newName")]
    pub new_name: String,
}

/// One folded per-identifier hint (a minified name the transfer gate did
/// not cover, with the prior name it resolved to).
#[derive(serde::Serialize, Clone, PartialEq, Eq, Debug)]
pub struct CloseHintRow {
    #[serde(rename = "newName")]
    pub new_name: String,
    #[serde(rename = "priorName")]
    pub prior_name: String,
    #[serde(rename = "snapEligible")]
    pub snap_eligible: bool,
}

/// One WON close pair's corroboration verdict — the context map's row.
#[derive(serde::Serialize, Clone, PartialEq, Debug)]
pub struct ClosePairRow {
    pub prior: SpanKey,
    pub fresh: SpanKey,
    pub verdict: String,
    #[serde(rename = "alignedStatements")]
    pub aligned_statements: u64,
    #[serde(rename = "totalNewStatements")]
    pub total_new_statements: u64,
    pub transfers: Vec<CloseNamePair>,
    pub hints: Vec<CloseHintRow>,
    pub snaps: Vec<CloseHintRow>,
}

#[derive(serde::Serialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct CloseStatsRow {
    #[serde(rename = "corroboratedByAlignment")]
    pub corroborated_by_alignment: u64,
    #[serde(rename = "corroboratedByShingles")]
    pub corroborated_by_shingles: u64,
    pub uncorroborated: u64,
}

/// The close tier's decision record: every scored candidate's fate, the
/// won pairs' corroboration and the skip counters.
#[derive(serde::Serialize, Clone, PartialEq, Debug)]
pub struct MatchesCloseFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub candidates: Vec<CloseCandidateRow>,
    pub pairs: Vec<ClosePairRow>,
    pub stats: CloseStatsRow,
    /// Ids that could not be scored at all (no features) — NOT ids scored
    /// and found dissimilar.
    #[serde(rename = "skippedOld")]
    pub skipped_old: u64,
    #[serde(rename = "skippedNew")]
    pub skipped_new: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TransferAttempt {
    pub tier: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "proposedName")]
    pub proposed_name: Option<String>,
    /// The model's answer key when the answer-key owner matched it
    /// tolerantly (finding #85); absent for every exact key, so a run
    /// without one writes the bytes it always wrote.
    #[serde(rename = "answerKey", default, skip_serializing_if = "Option::is_none")]
    pub answer_key: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct TransferRow {
    pub target: SpanKey,
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "finalName")]
    pub final_name: Option<String>,
    /// Absent when nothing settled the binding (the TS writes
    /// `entry.settledBy`, undefined then).
    #[serde(rename = "settledBy", default, skip_serializing_if = "Option::is_none")]
    pub settled_by: Option<String>,
    pub attempts: Vec<TransferAttempt>,
}

#[derive(serde::Serialize, Clone, PartialEq, Debug)]
pub struct PlacementRow {
    pub key: SpanKey,
    pub index: u64,
    pub names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "nameCount")]
    pub name_count: Option<u64>,
    #[serde(rename = "placedBy")]
    pub placed_by: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "priorFile")]
    pub prior_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "priorFileFrom")]
    pub prior_file_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "hashMiss")]
    pub hash_miss: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<serde_json::Value>,
    pub evidence: serde_json::Value,
}
