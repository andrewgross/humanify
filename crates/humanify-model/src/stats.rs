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
//! always present, `null` when no binding matching ran. The `reask` block
//! (2026-09-29) is always written by the current writer and ABSENT on every
//! recorded pre-2026-09-29 scorecard — strictly additive, so those files
//! still strict-parse and re-emit byte-identically. The `waveGauges` block
//! (2026-10-02, finding #66's instrumentation) and the `toolchain` block
//! (2026-10-04, the run's plugin pieces) follow the same rule.
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
    /// The survivors' provenance split (2026-09-30, Andrew's provenance
    /// decision): of the minted leftovers the census counts, which have a
    /// RECORDED decision behind them — `modelChosen` (a tier deliberately
    /// APPLIED the surviving name; carried, protected from re-rolling),
    /// `askedKept` (asked, terminal keep), `exhausted` (the retry budget
    /// died still-unrenamed — the sweep keeps them targets) — and which
    /// have no record at all (`neverAsked`, the real gap class finding
    /// #64 named). The tree-walk split is joined by NAME against the
    /// run's outcome records (`report::coverage`'s
    /// `survivor_provenance_split`), so `neverAsked` is a lower bound
    /// there; the in-stage per-binding block (`bindingProvenance`) is
    /// exact. The `singleLetters` block is the SINGLE-LETTER SLICE of
    /// this split — finding #62's monitor.
    pub struct ProvenanceSplit {
        total: f64 = "total",
        model_chosen: f64 = "modelChosen",
        asked_kept: f64 = "askedKept",
        exhausted: f64 = "exhausted",
        never_asked: f64 = "neverAsked",
    }
}

js_record! {
    /// The in-stage, PER-BINDING provenance classification
    /// (`naming::passes::sweep::classify_bindings`) — the exact half of
    /// the meter, computed inside the sweep where binding identity
    /// exists: of every eligible, non-carve-out, non-frozen binding,
    /// how many were renamed / carried (model-chosen below-floor
    /// applies) / asked-and-kept / retry-exhausted / never asked at all.
    /// `join` declares how unmarked bindings were classified:
    /// `"per-binding"` (the in-era sweep — every class from its own
    /// ledger) or `"by-name"` (the deferred sweep's DECLARED
    /// approximation — identity does not cross the generate/reconcile
    /// text boundary, so its unmarked rows read the by-name join).
    pub struct BindingProvenance {
        join: String = "join",
        total: f64 = "total",
        renamed: f64 = "renamed",
        model_chosen: f64 = "modelChosen",
        asked_kept: f64 = "askedKept",
        exhausted: f64 = "exhausted",
        never_asked: f64 = "neverAsked",
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
        provenance: Option<ProvenanceSplit> = "provenance",
        single_letters: Option<ProvenanceSplit> = "singleLetters",
        binding_provenance: Option<BindingProvenance> = "bindingProvenance",
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

pub use crate::pipeline::{PipelineSelectionRecord, ToolchainPieceRecord};

js_record! {
    /// The 2026-09-28 collision-retry fixes' counters (ProcessorReport's
    /// lane half + SweepResult's re-ask half, summed over the sweeps) —
    /// added 2026-09-29 (the deliberate schema bump, fix/small-leftovers):
    /// counters that exist but nothing could read are a floor nobody can
    /// audit (rule 8).
    pub struct ReaskStats {
        unrecoverable_rejections: f64 = "unrecoverableRejections",
        late_rejections: f64 = "lateRejections",
        invalid_suggestion_finishes: f64 = "invalidSuggestionFinishes",
        all_failed_windows: f64 = "allFailedWindows",
        sweep_reasked: f64 = "sweepReasked",
        sweep_reask_applied: f64 = "sweepReaskApplied",
        sweep_reask_dropped: f64 = "sweepReaskDropped",
    }
}

js_record! {
    /// The wave-era retention gauges (finding #66's instrumentation,
    /// 2026-10-02 — docs/perf-inventory.md item 1): what the naming waves'
    /// `Run` held at era end, per owner, in estimated deep heap bytes —
    /// the numbers that split a fresh run's ~58 GB between the candidate
    /// owners. Deterministic estimates (a String is its buffer plus its
    /// header; container slack is not counted), so run-to-run deltas are
    /// real.
    pub struct WaveGaugesStats {
        /// The #56 observable: name strings the function contexts'
        /// used-identifier Sets hold at run end (shared layers counted
        /// once).
        context_set_names: f64 = "contextSetNames",
        /// Finding #65's window gauge: the most rendered prompts alive at
        /// once.
        peak_live_dispatches: f64 = "peakLiveDispatches",
        /// [`Self::peak_live_dispatches`]' byte total.
        peak_live_prompt_bytes: f64 = "peakLivePromptBytes",
        /// The stored strategy material: every function pass's retained
        /// context (binding infos, callee signatures, callsites, context
        /// vars, taken-name sets). The SUM of the six `strategy*Bytes`
        /// constituents below (2026-10-02 taken-set sub-gauge, additive
        /// the same `reask` way — absent on every pre-bump scorecard).
        strategy_bytes: f64 = "strategyBytes",
        /// The split of `strategyBytes`: the phase's binding infos.
        strategy_bindings_bytes: Option<f64> = "strategyBindingsBytes",
        /// The split of `strategyBytes`: the taken-name snapshots the
        /// strategies hold, each distinct snapshot counted once by
        /// pointer (the sub-gauge that split finding #66's ~31 GB).
        strategy_taken_bytes: Option<f64> = "strategyTakenBytes",
        /// The split of `strategyBytes`: the callee signature snippets.
        strategy_callee_bytes: Option<f64> = "strategyCalleeBytes",
        /// The split of `strategyBytes`: the callsites.
        strategy_callsite_bytes: Option<f64> = "strategyCallsiteBytes",
        /// The split of `strategyBytes`: the capped context vars.
        strategy_context_var_bytes: Option<f64> = "strategyContextVarBytes",
        /// The split of `strategyBytes`: the module strategies' batch and
        /// windowed-name lists.
        strategy_module_bytes: Option<f64> = "strategyModuleBytes",
        /// The taken sets' `contextSetNames`-style count (the #56
        /// observable): name strings the strategies' retained taken
        /// snapshots hold, distinct snapshots counted once.
        taken_set_names: Option<f64> = "takenSetNames",
        /// The per-node contexts: binding maps, phase orders, applied-name
        /// records, the nodes' reports.
        ctx_bytes: f64 = "ctxBytes",
        /// The used-identifier layers (shared `Arc`s, counted once) plus
        /// every context's own barrier-edit sets and the renamed-name
        /// layers.
        used_set_bytes: f64 = "usedSetBytes",
        /// The recorded names.
        name_record_bytes: f64 = "nameRecordBytes",
        /// The run's small maps: the winners, the per-functionId round
        /// counter, the module used-names Set, the graph-era tables.
        bookkeeping_bytes: f64 = "bookkeepingBytes",
    }
}

js_record! {
    /// The prompt guard's counters (`naming::shown`, 2026-10-04): per
    /// prompt site, identifiers ASKED and those the shown code did NOT
    /// contain (`*Unshown` — every site windows its subjects by
    /// construction, so non-zero is a finding), plus the sweep's
    /// refusals (targets never asked because their window lacked them).
    /// Counts sum over the run's sweeps.
    pub struct PromptGuardStats {
        fn_asked: f64 = "fnAsked",
        fn_unshown: f64 = "fnUnshown",
        retry_asked: f64 = "retryAsked",
        retry_unshown: f64 = "retryUnshown",
        module_asked: f64 = "moduleAsked",
        module_unshown: f64 = "moduleUnshown",
        sweep_asked: f64 = "sweepAsked",
        sweep_unshown: f64 = "sweepUnshown",
        sweep_refused: f64 = "sweepRefused",
        /// The first unshown / refused identifiers, `site:name`.
        examples: Vec<String> = "examples",
    }
}

js_record! {
    /// The whole `--stats-json` object (writeEvalStats' literal, plus the
    /// 2026-09-29 `reask` and 2026-10-02 `waveGauges` additions — the
    /// deliberate post-cutover bumps, nested so the top level stays
    /// byte-stable).
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
        /// The `reask` block. ALWAYS written on runs from this writer
        /// (all-zero included); ABSENT on every pre-2026-09-29 recorded
        /// scorecard, which must stay loadable — Option expresses exactly
        /// that absent-is-not-zero state (contract 14 §5.2).
        reask: Option<ReaskStats> = "reask",
        /// The `waveGauges` block (2026-10-02). ALWAYS written on runs
        /// from this writer (all-zero included); ABSENT on every earlier
        /// recorded scorecard — the `reask` precedent, verbatim.
        wave_gauges: Option<WaveGaugesStats> = "waveGauges",
        /// The `toolchain` block (2026-10-04): the plugin pieces the run
        /// used and why each was chosen. ALWAYS written on runs from this
        /// writer; ABSENT on every earlier recorded scorecard — the
        /// `reask` precedent, verbatim.
        toolchain: Option<Vec<ToolchainPieceRecord>> = "toolchain",
        /// The `promptGuard` block (2026-10-04): does each prompt show
        /// what it asks about. ALWAYS written on runs from this writer;
        /// ABSENT on every earlier recorded scorecard — the `reask`
        /// precedent, verbatim.
        prompt_guard: Option<PromptGuardStats> = "promptGuard",
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
