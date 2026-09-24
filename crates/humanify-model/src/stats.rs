//! The `--stats-json` record (TS: `writeEvalStats`, unified.ts:455-504) —
//! the one owner of its SHAPE in Rust.
//!
//! The eval harness reads this file (analyze.ts `determinism()`), and a
//! committed run's stats are compared across releases, so the Rust writer
//! must produce the TS bytes: `JSON.stringify(stats, null, 2)`, no
//! trailing newline, keys in the order the TS objects are BUILT (not their
//! type declarations' order — see `jsshape`).
//!
//! Two absent-is-not-zero rules are shape-load-bearing (contract 14 §5.2):
//! `vendorNaming` is OMITTED when the namer was never asked, `renameClaims`
//! is ALWAYS written (all-zero included), and `bindingResolutionStats` is
//! always present, `null` when no binding matching ran.
//!
//! Every record's key order was read from its TS construction site (cited)
//! and is proven by the byte round trip over the oracle runs' stats files.

use crate::js::JsValue;
use crate::js_record;
use crate::jsshape::{CountMap, JsType, Nullable, Prop, Schema};

js_record! {
    /// `RenameCounts` (rename/coverage.ts), built in this order.
    pub struct RenameCounts {
        total: f64 = "total",
        llm: f64 = "llm",
        library_prefix: f64 = "libraryPrefix",
        fallback: f64 = "fallback",
        not_renamed: f64 = "notRenamed",
        nothing_to_rename: f64 = "nothingToRename",
        cached: f64 = "cached",
        close_match: f64 = "closeMatch",
        already_named: f64 = "alreadyNamed",
        failed: f64 = "failed",
    }
}

js_record! {
    /// `RenameCounts & { skippedBySkipList }` — the identifiers row.
    pub struct IdentifierCounts {
        total: f64 = "total",
        llm: f64 = "llm",
        library_prefix: f64 = "libraryPrefix",
        fallback: f64 = "fallback",
        not_renamed: f64 = "notRenamed",
        nothing_to_rename: f64 = "nothingToRename",
        cached: f64 = "cached",
        close_match: f64 = "closeMatch",
        already_named: f64 = "alreadyNamed",
        failed: f64 = "failed",
        skipped_by_skip_list: f64 = "skippedBySkipList",
    }
}

js_record! {
    /// `CoverageSummary.llm` (coverage.ts:212-219).
    pub struct LlmCoverage {
        total_calls: f64 = "totalCalls",
        retries: f64 = "retries",
        avg_response_time_ms: f64 = "avgResponseTimeMs",
        total_tokens: Option<f64> = "totalTokens",
        input_tokens: Option<f64> = "inputTokens",
        output_tokens: Option<f64> = "outputTokens",
    }
}

js_record! {
    /// `Record<MintedFamily, number>` (minted-census.ts summarizeCensus).
    pub struct MintedFamilies {
        class_expr_id: f64 = "classExprId",
        fn_expr_id: f64 = "fnExprId",
        param: f64 = "param",
        fn_decl: f64 = "fnDecl",
        var_other: f64 = "varOther",
    }
}

js_record! {
    /// `MintedCensus`, in `summarizeCensus`'s return-literal order.
    pub struct MintedCensus {
        total: f64 = "total",
        decorated: Option<f64> = "decorated",
        total_bindings: Option<f64> = "totalBindings",
        free_references: Option<Vec<String>> = "freeReferences",
        by_family: MintedFamilies = "byFamily",
        derivable_expr_ids: f64 = "derivableExprIds",
        zero_ref_expr_ids: f64 = "zeroRefExprIds",
        names: Option<Vec<String>> = "names",
        decorated_names: Option<Vec<String>> = "decoratedNames",
    }
}

js_record! {
    /// `CoverageSummary` (coverage.ts): the literal, then `llm`/`elapsedMs`
    /// (when metrics exist), then `mintedCensus` (plugin.ts end of run).
    pub struct CoverageSummary {
        functions: RenameCounts = "functions",
        module_bindings: RenameCounts = "moduleBindings",
        identifiers: IdentifierCounts = "identifiers",
        llm: Option<LlmCoverage> = "llm",
        elapsed_ms: Option<f64> = "elapsedMs",
        minted_census: Option<MintedCensus> = "mintedCensus",
    }
}

/// `RenameRejectionReason` (validated-rename.ts) — the vocabulary of
/// `TransferStats.rejected`'s keys.
pub const REJECTION_REASONS: [&str; 8] = [
    "invalid-target",
    "no-binding",
    "target-in-scope",
    "target-visible",
    "capture-in-subtree",
    "target-free-name",
    "shadows-child",
    "stale-binding",
];

/// `Partial<Record<RenameRejectionReason, number>>`: keys appear in the
/// order the reasons were FIRST counted during the run (an insertion-order
/// map), each one of [`REJECTION_REASONS`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RejectionCounts(pub CountMap);

impl JsType for RejectionCounts {
    fn to_js(&self) -> JsValue {
        self.0.to_js_map()
    }
    fn from_js(v: &JsValue, at: &str) -> Result<Self, String> {
        CountMap::from_js_map(v, at, Some(&REJECTION_REASONS)).map(RejectionCounts)
    }
    fn schema() -> Schema {
        Schema::Object(
            REJECTION_REASONS
                .iter()
                .map(|r| Prop {
                    name: r.to_string(),
                    optional: true,
                    nullable: false,
                    schema: Schema::Number,
                })
                .collect(),
        )
    }
}

js_record! {
    /// `TransferStats` (prior-transfer.ts).
    pub struct TransferStats {
        attempted: f64 = "attempted",
        applied: f64 = "applied",
        skipped: f64 = "skipped",
        rejected: Option<RejectionCounts> = "rejected",
    }
}

js_record! {
    /// `TransferStatsByTier` (prior-transfer.ts).
    pub struct TransferStatsByTier {
        exact_match: TransferStats = "exactMatch",
        close_match: TransferStats = "closeMatch",
        statement_twin: Option<TransferStats> = "statementTwin",
        retry: Option<TransferStats> = "retry",
    }
}

js_record! {
    /// `RenamePluginResult.namingFloor` (plugin.ts floorStats).
    pub struct NamingFloorStats {
        derived: f64 = "derived",
        undecorated: f64 = "undecorated",
        swept: f64 = "swept",
        skipped: f64 = "skipped",
    }
}

js_record! {
    /// `CloseMatchStats` (prior-version.ts).
    pub struct CloseMatchStats {
        corroborated_by_alignment: f64 = "corroboratedByAlignment",
        corroborated_by_shingles: f64 = "corroboratedByShingles",
        uncorroborated: f64 = "uncorroborated",
    }
}

js_record! {
    /// `ResolutionStats.propagationByRung`.
    pub struct PropagationByRung {
        matched_callee: f64 = "matchedCallee",
        matched_caller: f64 = "matchedCaller",
        scope_parent: f64 = "scopeParent",
        external_refs: f64 = "externalRefs",
        scope_ordinal: f64 = "scopeOrdinal",
    }
}

js_record! {
    /// `EnclosingStmtAbstainCounts` (analysis/types.ts), in
    /// `emptyResolutionStats`' order; `reachedSpanBuckets` is keyed by the
    /// span-bucket labels in bucket order.
    pub struct EnclosingStmtAbstain {
        no_hash_is_statement: f64 = "noHashIsStatement",
        no_hash_too_long: f64 = "noHashTooLong",
        no_hash_other: f64 = "noHashOther",
        no_new_holders: f64 = "noNewHolders",
        count_mismatch: f64 = "countMismatch",
        partner_filtered: f64 = "partnerFiltered",
        reached: f64 = "reached",
        resolved_local: f64 = "resolvedLocal",
        resolved_spanning: f64 = "resolvedSpanning",
        count_mismatch_local: f64 = "countMismatchLocal",
        count_mismatch_spanning: f64 = "countMismatchSpanning",
        spanning_parent_agrees: f64 = "spanningParentAgrees",
        spanning_parent_disagrees: f64 = "spanningParentDisagrees",
        spanning_parent_unknown: f64 = "spanningParentUnknown",
        reached_span_buckets: CountMap = "reachedSpanBuckets",
    }
}

js_record! {
    /// `ResolutionStats` (analysis/types.ts:515) in `emptyResolutionStats`
    /// (fingerprint-index.ts:43) order — NOT the interface's order, which
    /// lists `interchangeableResolved` first.
    pub struct ResolutionStats {
        structural_hash_unique: f64 = "structuralHashUnique",
        identity_resolved: f64 = "identityResolved",
        member_key_resolved: f64 = "memberKeyResolved",
        enclosing_statement_resolved: f64 = "enclosingStatementResolved",
        callee_shapes_resolved: f64 = "calleeShapesResolved",
        caller_shapes_resolved: f64 = "callerShapesResolved",
        callee_hashes_resolved: f64 = "calleeHashesResolved",
        two_hop_shapes_resolved: f64 = "twoHopShapesResolved",
        shingle_similarity_resolved: f64 = "shingleSimilarityResolved",
        shingle_unconsultable: f64 = "shingleUnconsultable",
        ordinal_resolved: f64 = "ordinalResolved",
        interchangeable_resolved: f64 = "interchangeableResolved",
        injectivity_demoted: f64 = "injectivityDemoted",
        singleton_rejected: f64 = "singletonRejected",
        singleton_unguarded: f64 = "singletonUnguarded",
        still_ambiguous: f64 = "stillAmbiguous",
        unmatched: f64 = "unmatched",
        propagation_resolved: f64 = "propagationResolved",
        propagation_by_rung: PropagationByRung = "propagationByRung",
        crossed_container_revoked: f64 = "crossedContainerRevoked",
        enclosing_stmt_abstain: EnclosingStmtAbstain = "enclosingStmtAbstain",
    }
}

js_record! {
    /// `VendorNamingStats` (unpack/vendor-namer.ts).
    pub struct VendorNamingStats {
        named: f64 = "named",
        declined: f64 = "declined",
        echoed: f64 = "echoed",
        batches_failed: f64 = "batchesFailed",
    }
}

impl VendorNamingStats {
    /// `vendorNamingAttempted`: did the namer receive any request at all?
    pub fn attempted(&self) -> bool {
        self.named + self.declined + self.echoed + self.batches_failed > 0.0
    }
}

js_record! {
    /// `RenameClaimStats.byGuard`.
    pub struct RenameClaimGuards {
        target_in_scope: f64 = "targetInScope",
        target_visible: f64 = "targetVisible",
        shadows_child: f64 = "shadowsChild",
    }
}

js_record! {
    /// `RenameClaimStats` (validated-rename.ts).
    pub struct RenameClaimStats {
        ledger_only_rejections: f64 = "ledgerOnlyRejections",
        by_guard: RenameClaimGuards = "byGuard",
        claims_recorded: f64 = "claimsRecorded",
    }
}

pub use crate::pipeline::PipelineSelectionRecord;

js_record! {
    /// The whole `--stats-json` object (writeEvalStats' literal).
    pub struct EvalStats {
        coverage: Option<CoverageSummary> = "coverage",
        transfer_stats: Option<TransferStatsByTier> = "transferStats",
        prior_version_applied: Option<f64> = "priorVersionApplied",
        prior_version_already_named: Option<f64> = "priorVersionAlreadyNamed",
        prior_version_bindings_applied: Option<f64> = "priorVersionBindingsApplied",
        naming_floor: Option<NamingFloorStats> = "namingFloor",
        close_match_stats: Option<CloseMatchStats> = "closeMatchStats",
        resolution_stats: Option<ResolutionStats> = "resolutionStats",
        binding_resolution_stats: Nullable<ResolutionStats> = "bindingResolutionStats",
        vendor_naming: Option<VendorNamingStats> = "vendorNaming",
        rename_claims: RenameClaimStats = "renameClaims",
        selection: Option<PipelineSelectionRecord> = "selection",
    }
}

impl EvalStats {
    /// The file bytes: `JSON.stringify(stats, null, 2)`, no trailing newline.
    pub fn to_file_text(&self) -> String {
        crate::js::stringify_pretty(&self.to_js(), 2)
    }

    /// Strict parse of a `--stats-json` file.
    pub fn parse(text: &str) -> Result<EvalStats, String> {
        EvalStats::from_js(&JsValue::parse(text)?, "stats")
    }
}
