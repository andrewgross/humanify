//! The `--stats-json` record: the one owner of its shape. Each record
//! derives `Serialize`; the file is serde's pretty JSON (indent 2, no
//! trailing newline), keys in field order.
//!
//! The eval harness reads this file (`eval summarize`, `eval leaderboard`,
//! the walk report): a field it reads keeps its name and meaning.
//!
//! Absent is not zero: `vendorNaming` is OMITTED when the vendor namer was
//! not asked, `renameClaims` is always written (all-zero included), and
//! `bindingResolutionStats` is always present, `null` when no binding
//! matching ran. Every other `Option` block is omitted when the run did
//! not produce it.

use crate::ordered::OrderedMap;

/// `RenameCounts` (rename/coverage.ts), built in this order.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameCounts {
    pub total: u64,
    pub llm: u64,
    pub library_prefix: u64,
    pub fallback: u64,
    pub not_renamed: u64,
    pub nothing_to_rename: u64,
    pub cached: u64,
    pub close_match: u64,
    pub already_named: u64,
    pub failed: u64,
}

/// `RenameCounts & { skippedBySkipList }` — the identifiers row.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifierCounts {
    pub total: u64,
    pub llm: u64,
    pub library_prefix: u64,
    pub fallback: u64,
    pub not_renamed: u64,
    pub nothing_to_rename: u64,
    pub cached: u64,
    pub close_match: u64,
    pub already_named: u64,
    pub failed: u64,
    pub skipped_by_skip_list: u64,
}

/// `CoverageSummary.llm` (coverage.ts:212-219).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmCoverage {
    pub total_calls: u64,
    pub retries: u64,
    pub avg_response_time_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
}

/// `Record<MintedFamily, number>` (minted-census.ts summarizeCensus).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MintedFamilies {
    pub class_expr_id: u64,
    pub fn_expr_id: u64,
    pub param: u64,
    pub fn_decl: u64,
    pub var_other: u64,
}

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
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvenanceSplit {
    pub total: u64,
    pub model_chosen: u64,
    pub asked_kept: u64,
    pub exhausted: u64,
    pub never_asked: u64,
}

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
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingProvenance {
    pub join: String,
    pub total: u64,
    pub renamed: u64,
    pub model_chosen: u64,
    pub asked_kept: u64,
    pub exhausted: u64,
    pub never_asked: u64,
}

/// `MintedCensus`, in `summarizeCensus`'s return-literal order.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MintedCensus {
    pub total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decorated: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_bindings: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_references: Option<Vec<String>>,
    pub by_family: MintedFamilies,
    pub derivable_expr_ids: u64,
    pub zero_ref_expr_ids: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decorated_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ProvenanceSplit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub single_letters: Option<ProvenanceSplit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding_provenance: Option<BindingProvenance>,
}

/// `CoverageSummary` (coverage.ts): the literal, then `llm`/`elapsedMs`
/// (when metrics exist), then `mintedCensus` (plugin.ts end of run).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageSummary {
    pub functions: RenameCounts,
    pub module_bindings: RenameCounts,
    pub identifiers: IdentifierCounts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm: Option<LlmCoverage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minted_census: Option<MintedCensus>,
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
pub type RejectionCounts = OrderedMap<u64>;

/// `TransferStats` (prior-transfer.ts).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferStats {
    pub attempted: u64,
    pub applied: u64,
    pub skipped: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected: Option<RejectionCounts>,
}

/// `TransferStatsByTier` (prior-transfer.ts).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferStatsByTier {
    pub exact_match: TransferStats,
    pub close_match: TransferStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub statement_twin: Option<TransferStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<TransferStats>,
}

/// The `privateNames` block (exp224): the class-private member namer
/// (`naming::private_members`), per member — kept (not minted),
/// derived from the class, or numbered by kind.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivateNameStats {
    pub members: u64,
    pub kept: u64,
    pub derived: u64,
    pub numbered: u64,
}

/// `RenamePluginResult.namingFloor` (plugin.ts floorStats).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamingFloorStats {
    pub derived: u64,
    pub undecorated: u64,
    pub swept: u64,
    pub skipped: u64,
}

/// `CloseMatchStats` (prior-version.ts).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseMatchStats {
    pub corroborated_by_alignment: u64,
    pub corroborated_by_shingles: u64,
    pub uncorroborated: u64,
}

/// `ResolutionStats.propagationByRung`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropagationByRung {
    pub matched_callee: u64,
    pub matched_caller: u64,
    pub scope_parent: u64,
    pub external_refs: u64,
    pub scope_ordinal: u64,
}

/// `EnclosingStmtAbstainCounts` (analysis/types.ts), in
/// `emptyResolutionStats`' order; `reachedSpanBuckets` is keyed by the
/// span-bucket labels in bucket order.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnclosingStmtAbstain {
    pub no_hash_is_statement: u64,
    pub no_hash_too_long: u64,
    pub no_hash_other: u64,
    pub no_new_holders: u64,
    pub count_mismatch: u64,
    pub partner_filtered: u64,
    pub reached: u64,
    pub resolved_local: u64,
    pub resolved_spanning: u64,
    pub count_mismatch_local: u64,
    pub count_mismatch_spanning: u64,
    pub spanning_parent_agrees: u64,
    pub spanning_parent_disagrees: u64,
    pub spanning_parent_unknown: u64,
    pub reached_span_buckets: OrderedMap<u64>,
}

/// `ResolutionStats` (analysis/types.ts:515) in `emptyResolutionStats`
/// (fingerprint-index.ts:43) order — NOT the interface's order, which
/// lists `interchangeableResolved` first.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolutionStats {
    pub structural_hash_unique: u64,
    pub identity_resolved: u64,
    pub member_key_resolved: u64,
    pub enclosing_statement_resolved: u64,
    pub callee_shapes_resolved: u64,
    pub caller_shapes_resolved: u64,
    pub callee_hashes_resolved: u64,
    pub two_hop_shapes_resolved: u64,
    pub shingle_similarity_resolved: u64,
    pub shingle_unconsultable: u64,
    pub ordinal_resolved: u64,
    pub interchangeable_resolved: u64,
    pub injectivity_demoted: u64,
    pub singleton_rejected: u64,
    pub singleton_unguarded: u64,
    pub still_ambiguous: u64,
    pub unmatched: u64,
    pub propagation_resolved: u64,
    pub propagation_by_rung: PropagationByRung,
    pub crossed_container_revoked: u64,
    pub enclosing_stmt_abstain: EnclosingStmtAbstain,
}

/// `VendorNamingStats` (unpack/vendor-namer.ts).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorNamingStats {
    pub named: u64,
    pub declined: u64,
    pub echoed: u64,
    pub batches_failed: u64,
}

impl VendorNamingStats {
    /// `vendorNamingAttempted`: did the namer receive any request at all?
    pub fn attempted(&self) -> bool {
        self.named + self.declined + self.echoed + self.batches_failed > 0
    }
}

/// `RenameClaimStats.byGuard`.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameClaimGuards {
    pub target_in_scope: u64,
    pub target_visible: u64,
    pub shadows_child: u64,
}

/// `RenameClaimStats` (validated-rename.ts).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameClaimStats {
    pub ledger_only_rejections: u64,
    pub by_guard: RenameClaimGuards,
    pub claims_recorded: u64,
}

pub use crate::pipeline::{PipelineSelectionRecord, ToolchainPieceRecord};

/// Which split method the bundle got (`place::method`, 2026-10-05):
/// `module-markers`, `prior-layout`, `fresh-grouping`, or `not-split`
/// (a bundle layout the split does not read — the named output was
/// written unsplit), the reason, and the module markers' coverage of
/// the app code when it was measured.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitMethodStats {
    pub method: String,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_coverage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marker_modules: Option<u64>,
    pub threshold: f64,
}

/// The 2026-09-28 collision-retry fixes' counters (ProcessorReport's
/// lane half + SweepResult's re-ask half, summed over the sweeps) —
/// added 2026-09-29 (the deliberate schema bump, fix/small-leftovers):
/// counters that exist but nothing could read are a floor nobody can
/// audit (rule 8).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaskStats {
    pub unrecoverable_rejections: u64,
    pub late_rejections: u64,
    pub invalid_suggestion_finishes: u64,
    pub all_failed_windows: u64,
    pub sweep_reasked: u64,
    pub sweep_reask_applied: u64,
    pub sweep_reask_dropped: u64,
    /// The BACKUPS the lanes took after a rejected answer (exp206,
    /// 2026-11-08 — counted in the processor report since 2026-10-04
    /// but never exported): colliding answers an all-failed window
    /// cut off before their round-2, handed to the barrier's
    /// disclosed re-ask. ABSENT on earlier scorecards (the `reask`
    /// precedent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collision_handoffs: Option<u64>,
    /// Answers whose lane round-2 collided again, handed to the
    /// barrier's disclosed re-ask.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane_end_handoffs: Option<u64>,
    /// Collisions the lane tail repaired with the suffix ladder (the
    /// processor's `remaining` contention events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ladder_at_lane_end: Option<u64>,
    /// Collisions the barrier repaired with the suffix ladder once its
    /// re-ask budget was spent (the `wave` contention events).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ladder_at_barrier: Option<u64>,
}

/// The wave-era retention gauges (finding #66's instrumentation,
/// 2026-10-02 — docs/perf-inventory.md item 1): what the naming waves'
/// `Run` held at era end, per owner, in estimated deep heap bytes —
/// the numbers that split a fresh run's ~58 GB between the candidate
/// owners. Deterministic estimates (a String is its buffer plus its
/// header; container slack is not counted), so run-to-run deltas are
/// real.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveGaugesStats {
    /// The #56 observable: name strings the function contexts'
    /// used-identifier Sets hold at run end (shared layers counted
    /// once).
    pub context_set_names: u64,
    /// Finding #65's window gauge: the most rendered prompts alive at
    /// once.
    pub peak_live_dispatches: u64,
    /// [`Self::peak_live_dispatches`]' byte total.
    pub peak_live_prompt_bytes: u64,
    /// The stored strategy material: every function pass's retained
    /// context (binding infos, callee signatures, callsites, context
    /// vars, taken-name sets). The SUM of the six `strategy*Bytes`
    /// constituents below (2026-10-02 taken-set sub-gauge, additive
    /// the same `reask` way — absent on every pre-bump scorecard).
    pub strategy_bytes: u64,
    /// The split of `strategyBytes`: the phase's binding infos.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_bindings_bytes: Option<u64>,
    /// The split of `strategyBytes`: the taken-name snapshots the
    /// strategies hold, each distinct snapshot counted once by
    /// pointer (the sub-gauge that split finding #66's ~31 GB).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_taken_bytes: Option<u64>,
    /// The split of `strategyBytes`: the callee signature snippets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_callee_bytes: Option<u64>,
    /// The split of `strategyBytes`: the callsites.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_callsite_bytes: Option<u64>,
    /// The split of `strategyBytes`: the capped context vars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_context_var_bytes: Option<u64>,
    /// The split of `strategyBytes`: the module strategies' batch and
    /// windowed-name lists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy_module_bytes: Option<u64>,
    /// The taken sets' `contextSetNames`-style count (the #56
    /// observable): name strings the strategies' retained taken
    /// snapshots hold, distinct snapshots counted once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taken_set_names: Option<u64>,
    /// The per-node contexts: binding maps, phase orders, applied-name
    /// records, the nodes' reports.
    pub ctx_bytes: u64,
    /// The used-identifier layers (shared `Arc`s, counted once) plus
    /// every context's own barrier-edit sets and the renamed-name
    /// layers.
    pub used_set_bytes: u64,
    /// The recorded names.
    pub name_record_bytes: u64,
    /// The run's small maps: the winners, the per-functionId round
    /// counter, the module used-names Set, the graph-era tables.
    pub bookkeeping_bytes: u64,
}

/// The prompt guard's counters (`naming::shown`, 2026-10-04): per
/// prompt site, identifiers ASKED and those the shown code did NOT
/// contain (`*Unshown` — every site windows its subjects by
/// construction, so non-zero is a finding), plus the sweep's
/// refusals (targets never asked because their window lacked them).
/// Counts sum over the run's sweeps.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptGuardStats {
    pub fn_asked: u64,
    pub fn_unshown: u64,
    pub retry_asked: u64,
    pub retry_unshown: u64,
    pub module_asked: u64,
    pub module_unshown: u64,
    pub sweep_asked: u64,
    pub sweep_unshown: u64,
    pub sweep_refused: u64,
    /// The first unshown / refused identifiers, `site:name`.
    pub examples: Vec<String>,
}

/// The `providerErrors` block (2026-10-12, concurrency audit R2 +
/// recommendation 1c): LLM calls that failed after every retry, the
/// run's total and per namer. Each one is a hole in the names — the run
/// is fail-soft and still exits 0 — so the eval refuses a card whose
/// run lost any (experiments/lib/invariants.ts). Counts are CALLS
/// (a batch is one call).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderErrorStats {
    pub total: u64,
    /// The function/identifier waves and the sweeps (`outcome.errors`).
    pub naming: u64,
    /// The module namer (`ModuleNamingReport::failed_calls`).
    pub modules: u64,
    /// The vendor namer (`VendorNamingStats::batches_failed`).
    pub vendor: u64,
    /// The split's file/folder namer (`failed_batches`).
    pub split: u64,
    /// The split's tree reviser (one call per split).
    pub tree_reviser: u64,
}

/// The whole `--stats-json` object (writeEvalStats' literal, plus the
/// 2026-09-29 `reask` and 2026-10-02 `waveGauges` additions — the
/// deliberate post-cutover bumps, nested so the top level stays
/// byte-stable).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalStats {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<CoverageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfer_stats: Option<TransferStatsByTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_version_applied: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_version_already_named: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prior_version_bindings_applied: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub naming_floor: Option<NamingFloorStats>,
    /// The `privateNames` block (exp224). ALWAYS written on runs from
    /// this writer; ABSENT on every earlier recorded scorecard — the
    /// `reask` precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_names: Option<PrivateNameStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_match_stats: Option<CloseMatchStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_stats: Option<ResolutionStats>,
    pub binding_resolution_stats: Option<ResolutionStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vendor_naming: Option<VendorNamingStats>,
    pub rename_claims: RenameClaimStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<PipelineSelectionRecord>,
    /// The `reask` block. ALWAYS written on runs from this writer
    /// (all-zero included); ABSENT on every pre-2026-09-29 recorded
    /// scorecard, which must stay loadable — Option expresses exactly
    /// that absent-is-not-zero state (contract 14 §5.2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reask: Option<ReaskStats>,
    /// The `waveGauges` block (2026-10-02). ALWAYS written on runs
    /// from this writer (all-zero included); ABSENT on every earlier
    /// recorded scorecard — the `reask` precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wave_gauges: Option<WaveGaugesStats>,
    /// The `toolchain` block (2026-10-04): the plugin pieces the run
    /// used and why each was chosen. ALWAYS written on runs from this
    /// writer; ABSENT on every earlier recorded scorecard — the
    /// `reask` precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<Vec<ToolchainPieceRecord>>,
    /// The `promptGuard` block (2026-10-04): does each prompt show
    /// what it asks about. ALWAYS written on runs from this writer;
    /// ABSENT on every earlier recorded scorecard — the `reask`
    /// precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_guard: Option<PromptGuardStats>,
    /// The `splitMethod` block (2026-10-05): which split method the
    /// bundle got, from what it contains, and why. Written on `--split`
    /// runs; ABSENT on runs without `--split` and on every earlier
    /// recorded scorecard — the `reask` precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_method: Option<SplitMethodStats>,
    /// The `providerErrors` block (2026-10-12). ALWAYS written on runs
    /// from this writer (all-zero included); ABSENT on every earlier
    /// recorded scorecard — the `reask` precedent, verbatim. Absent is
    /// UNKNOWN to the eval, never zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_errors: Option<ProviderErrorStats>,
    /// The `config` block (2026-10-13, config sweep step 4): the run's
    /// whole resolved configuration, the API key redacted — the
    /// CLI's `Config::record`, verbatim. ALWAYS written on runs from
    /// this writer; ABSENT on every earlier recorded scorecard — the
    /// `reask` precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_json::Value>,
    /// The `libraryMatching` block (2026-11-01, exp175): what the
    /// library decision came to — `used` (an explicit or stored index),
    /// `built`, `skipped` (no index for the bundle's kind) or `off` —
    /// with the bundle's kind and the index id, so a run that skipped
    /// library names cannot pass for a names run. ALWAYS written on
    /// runs from this writer; ABSENT on every earlier recorded
    /// scorecard — the `reask` precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library_matching: Option<serde_json::Value>,
    /// The `llmCalls` block (2026-11-08, exp206): every LLM call's
    /// fate per namer — the call ledger's `outcomes` (answered /
    /// failed / per-identifier answer verdicts; byte-identical with
    /// the model held fixed), `transport` (served / sent / retries by
    /// reason / failures by kind / tokens) and `timing` (latency
    /// percentiles — wall clock). DIAGNOSTICS, never a KPI. ALWAYS
    /// written on runs from this writer; ABSENT on every earlier
    /// recorded scorecard — the `reask` precedent, verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_calls: Option<crate::call_ledger::CallStats>,
    /// The `inputLink` block (2026-11-12, exp216): how a Bun
    /// code-split input was linked into one program (chunks linked,
    /// eager / lazy, inits, lazy loads, names renamed apart). Written
    /// ONLY when the input was such a layout; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_link: Option<serde_json::Value>,
}

impl EvalStats {
    /// The file bytes: pretty JSON (indent 2), no trailing newline.
    pub fn to_file_text(&self) -> String {
        serde_json::to_string_pretty(self).expect("the stats record serializes")
    }
}
