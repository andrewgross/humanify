//! The dump schema (docs/rust-port/07-differential-validation.md §2): the
//! serde structs the artifact dump writes and the parity comparer reads.
//!
//! Every struct mirrors the TS writer's JSON exactly (camelCase via
//! `rename_all`); the comparer joins rows by their primary span key and
//! compares whole values. This schema doubles as the production
//! version-record format (12 §2) — it is the one parity artifact that
//! survives phase 6.
//!
//! Number types: spans are i64 (the -1 no-position sentinel); counts are
//! u64; `round`/`slotIndex`/`bundleIndex` are u64. Every struct is
//! `PartialEq + Eq` so row comparison is derived, not written twice.

/// Bump on any schema change; the comparer refuses mismatched sets.
pub const DUMP_SCHEMA_VERSION: u64 = 1;

/// The join key (07 §1): which anchored text a span indexes into,
/// half-open, in UTF-8 BYTE offsets (already converted by the TS writer).
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

// ---------------------------------------------------------------------------
// meta.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug, Default)]
pub struct MetaTexts {
    pub fresh: Option<String>,
    pub prior: Option<String>,
    pub minified: Option<String>,
    /// The SPLIT's input text — the shipped code after every naming/
    /// reconcile/permute pass; split-era spans anchor here (WP0.2).
    pub shipped: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct MetaFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    #[serde(rename = "generatedAt")]
    pub generated_at: String,
    pub commit: String,
    /// The run's resolved flags (free-form record; compared by the differ
    /// only as anchors — flags themselves are recorded, not gated).
    pub flags: serde_json::Value,
    pub texts: MetaTexts,
}

// ---------------------------------------------------------------------------
// functions.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct FnBindingSlot {
    pub slot: String,
    pub span: SpanKey,
    pub name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct FunctionRow {
    pub key: SpanKey,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub kind: String,
    pub name: String,
    #[serde(rename = "nameBinding")]
    pub name_binding: Option<SpanKey>,
    #[serde(rename = "structuralHash", default)]
    pub structural_hash: String,
    #[serde(rename = "internalCallees")]
    pub internal_callees: Vec<SpanKey>,
    #[serde(rename = "scopeParent")]
    pub scope_parent: Option<SpanKey>,
    pub bindings: Vec<FnBindingSlot>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct FunctionsFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub functions: Vec<FunctionRow>,
}

// ---------------------------------------------------------------------------
// partitions.json — compared as partitions, never as hash bytes (07 §4)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PartitionMember {
    pub member: SpanKey,
    pub hash: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PartitionFamily {
    pub family: String,
    pub members: Vec<PartitionMember>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PartitionsFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub families: Vec<PartitionFamily>,
}

// ---------------------------------------------------------------------------
// matches.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct MatchPair {
    pub cascade: String,
    pub prior: SpanKey,
    pub fresh: SpanKey,
    pub tier: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct MatchRejection {
    pub cascade: String,
    pub prior: SpanKey,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<SpanKey>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct MatchesFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub pairs: Vec<MatchPair>,
    pub rejections: Vec<MatchRejection>,
    /// The WP2.1 gate's "resolutionStats identical": the two stat bags are
    /// compared as whole values — absent on both sides is agreement.
    #[serde(rename = "resolutionStats", default)]
    pub resolution_stats: Option<serde_json::Value>,
    #[serde(rename = "bindingResolutionStats", default)]
    pub binding_resolution_stats: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// matches-close.json (WP2.2's gate — the close tier's decision record)
// ---------------------------------------------------------------------------

/// One scored candidate's fate. `score` is the cosine f64 (the two legs
/// compute identical bits; the `scoreBits` column is the tie identity the
/// diff reads first — a tie abstains on EXACT float equality).
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
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
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct CloseNamePair {
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "newName")]
    pub new_name: String,
}

/// One folded per-identifier hint (a minified name the transfer gate did
/// not cover, with the prior name it resolved to).
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct CloseHintRow {
    #[serde(rename = "newName")]
    pub new_name: String,
    #[serde(rename = "priorName")]
    pub prior_name: String,
    #[serde(rename = "snapEligible")]
    pub snap_eligible: bool,
}

/// One WON close pair's corroboration verdict — the context map's row
/// (minus the prompt-material fields, WP4's surface).
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
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

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug, Default)]
pub struct CloseStatsRow {
    #[serde(rename = "corroboratedByAlignment")]
    pub corroborated_by_alignment: u64,
    #[serde(rename = "corroboratedByShingles")]
    pub corroborated_by_shingles: u64,
    pub uncorroborated: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
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

// ---------------------------------------------------------------------------
// transfers.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TransferAttempt {
    pub tier: String,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "proposedName")]
    pub proposed_name: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct TransferRow {
    pub target: SpanKey,
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "finalName")]
    pub final_name: Option<String>,
    #[serde(rename = "settledBy")]
    pub settled_by: Option<String>,
    pub attempts: Vec<TransferAttempt>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct TransfersFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub transfers: Vec<TransferRow>,
}

// ---------------------------------------------------------------------------
// votes.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct VoteTally {
    pub name: String,
    pub total: u64,
    pub exact: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct VoteWitness {
    #[serde(rename = "sourceFunctionId")]
    pub source_function_id: String,
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "exactSlot")]
    pub exact_slot: bool,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct VoteRow {
    pub target: SpanKey,
    #[serde(rename = "targetKind")]
    pub target_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    pub tally: Vec<VoteTally>,
    pub witnesses: Vec<VoteWitness>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct VotesFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub votes: Vec<VoteRow>,
}

// ---------------------------------------------------------------------------
// names.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct NameRecord {
    pub target: SpanKey,
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "newName")]
    pub new_name: Option<String>,
    pub kind: String,
    pub classified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round: Option<u64>,
    #[serde(rename = "functionId")]
    pub function_id: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct NamesFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub names: Vec<NameRecord>,
}

// ---------------------------------------------------------------------------
// placement.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
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

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct PlacementFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub placements: Vec<PlacementRow>,
}

// ---------------------------------------------------------------------------
// emit.json
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct EmitStatement {
    pub span: SpanKey,
    #[serde(rename = "slotIndex")]
    pub slot_index: u64,
    #[serde(rename = "bundleIndex")]
    pub bundle_index: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct EmitFileRow {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub statements: Vec<EmitStatement>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct EmitLayoutFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub files: Vec<EmitFileRow>,
}

// ---------------------------------------------------------------------------
// prompts.jsonl — one JSON object per line, dispatch order
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PromptTarget {
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub start: i64,
    pub end: i64,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct PromptRecord {
    pub seq: u64,
    #[serde(rename = "functionId")]
    pub function_id: String,
    pub site: String,
    pub round: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave: Option<u64>,
    #[serde(rename = "isRetry")]
    pub is_retry: bool,
    #[serde(rename = "cacheKey")]
    pub cache_key: String,
    #[serde(rename = "systemPrompt")]
    pub system_prompt: String,
    #[serde(rename = "userPrompt")]
    pub user_prompt: String,
    pub identifiers: Vec<String>,
    pub targets: Vec<PromptTarget>,
}

// ---------------------------------------------------------------------------
// modules.json — the Bun CJS classification (WP1.5; the graph's own
// classification, anchored on fresh)
// ---------------------------------------------------------------------------

/// One classification site's data (helper var + wrapper + factory rows).
/// `unpack` anchors the minified text; `graph` anchors fresh — the two
/// sites the pipeline classifies at (the graph one is null on every real
/// Bun bundle: the beautifier splits the `{exports:{}}` marker across
/// lines, so the scan misses — ported behavior, not an accident).
#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ModulesData {
    #[serde(rename = "helperVar")]
    pub helper_var: String,
    pub wrapper: Option<ModulesWrapper>,
    pub factories: Vec<ModulesFactoryRow>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ModulesFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    #[serde(default)]
    pub unpack: Option<ModulesData>,
    #[serde(default)]
    pub graph: Option<ModulesData>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ModulesFactoryRow {
    pub key: SpanKey,
    #[serde(rename = "factoryVar")]
    pub factory_var: String,
    /// 1-indexed [startLine, endLine] of the declarator.
    #[serde(rename = "lineRange")]
    pub line_range: (i64, i64),
    #[serde(rename = "contentHash")]
    pub content_hash: String,
    #[serde(rename = "structuralHash")]
    pub structural_hash: String,
    /// The banner's stripped, trimmed text — absent when none (the TS
    /// omits the field; `default` makes absent and null the same None).
    #[serde(rename = "bannerText", default)]
    pub banner_text: Option<String>,
    #[serde(rename = "bannerPackage", default)]
    pub banner_package: Option<String>,
    #[serde(rename = "bannerVersion", default)]
    pub banner_version: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ModulesWrapper {
    pub span: SpanKey,
    #[serde(rename = "bodySpan")]
    pub body_span: SpanKey,
    #[serde(rename = "bindingCount")]
    pub binding_count: i64,
}

// ---------------------------------------------------------------------------
// twins.json — the statement-twin unique-tier proposals (WP2.3)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TwinInventory {
    pub statements: i64,
    #[serde(rename = "distinctHashes")]
    pub distinct_hashes: i64,
    #[serde(rename = "uniqueHashes")]
    pub unique_hashes: i64,
    #[serde(rename = "maxBucket")]
    pub max_bucket: i64,
    /// bucket-size -> count of hash classes.
    #[serde(rename = "bucketHistogram")]
    pub bucket_histogram: std::collections::BTreeMap<String, i64>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TwinProposalPair {
    pub prior: SpanKey,
    pub fresh: SpanKey,
    /// The shared statementHash (informational — bytes are serializer
    /// artifacts; the gate compares spans/counts, never digest strings).
    pub hash: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TwinsFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub inventories: std::collections::BTreeMap<String, TwinInventory>,
    #[serde(rename = "uniqueTier")]
    pub unique_tier: TwinTier,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TwinTier {
    #[serde(rename = "uniqueTwins")]
    pub unique_twins: i64,
    pub pairs: Vec<TwinProposalPair>,
}

// ---------------------------------------------------------------------------
// twin-gates.json — the twins' per-proposal gate outcomes (WP2.3)
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TwinGateRow {
    pub tier: String,
    pub fresh: SpanKey,
    pub prior: SpanKey,
    pub outcome: String,
    /// Bridged slot-pair count (bridged rows only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slots: Option<i64>,
    /// The bridged transfer pairs' names (bridged rows only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairs: Option<Vec<TwinGatePair>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TwinGatePair {
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "newName")]
    pub new_name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct TwinGateConflict {
    #[serde(rename = "oldName")]
    pub old_name: String,
    #[serde(rename = "cascadeName")]
    pub cascade_name: String,
    #[serde(rename = "twinName")]
    pub twin_name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, PartialEq, Debug)]
pub struct TwinsGatesFile {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u64,
    pub stats: std::collections::BTreeMap<String, serde_json::Value>,
    pub rows: Vec<TwinGateRow>,
    #[serde(default)]
    pub conflicts: Option<Vec<TwinGateConflict>>,
}
