//! The naming-stage driver (WP4.6) — TS `src/rename/plugin.ts`
//! (`createRenamePlugin`'s returned function): the ONE place the naming
//! stage's order lives. Every caller — the pipeline and the gate verbs
//! (`humanify naming`, `passes`, `waves`) — runs [`run_naming`]; none keeps
//! its own copy of the order.
//!
//! In plugin.ts order:
//!
//! 1. the naming era ([`era`]): parse + graph, the pre-naming freezes
//!    (eval/with taint, the wrapper IIFE, library functions), the
//!    prior-version match + transfer (only with a prior), the LLM waves,
//!    the library-prefix pass, the naming floor (+ the pre-generate sweep
//!    when it is not deferred), `generate`;
//! 2. the prior-diff reconcile — when `reconcilePriorDiff && prior &&
//!    !sourceMap && outputValid` (an Err is "did not run");
//! 3. the deferred sweep — when `isSweepDeferred && outputValid`, over the
//!    reconciled text (anchor `reconciled`) else the generated one;
//! 4. the family permute — unless `--disable family-permute`, when
//!    `reconcilePriorDiff && prior && !sourceMap && !emitRenameLedger &&
//!    outputValid`, over the last text produced;
//! 5. the minted census of the shipped text, the coverage summary.
//!
//! `isSweepDeferred` = `namingFloor && namingFloorSweep && reconcilePriorDiff
//! && prior && !sourceMap`. Source maps and the rename ledger are not
//! ported (the Rust has no source-map output; `sourceMap` / the ledger flag
//! only gate passes here, exactly as the TS conditions read them).

pub mod dump;
pub mod era;
pub mod library;
pub mod validate;

use humanify_model::llm::{CacheKeyParams, NameProvider};
use humanify_model::stats::{
    CloseMatchStats, CoverageSummary, EvalStats, NamingFloorStats, RejectionCounts,
    RenameClaimGuards, RenameClaimStats, TransferStats as StatsTransferStats, TransferStatsByTier,
};

use crate::naming::passes::census::MintedCensus;
use crate::naming::passes::census_of_text;
use crate::naming::passes::family_permute::{
    FamilyPermuteOutcome, PermutePlant, run_family_permute,
};
use crate::naming::passes::sweep::{SweepResult, run_deferred_sweep};
use crate::naming::reconcile::step::{PriorDiffOutcome, run_prior_diff_reconciliation};
use crate::naming::reconcile::{ReconcilePlant, ReconcileResult};
use crate::naming::report::coverage::{
    CoverageInputs, build_coverage_summary, census_record, format_coverage_summary,
};
use crate::naming::report::{ProcessorReport, RenameReport};
use crate::naming::waves::processor::Plant;
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::rename::eligibility::Eligibility;
use crate::rename::transfer::TransferStats;
use crate::trail::{Anchor, StrategyTrail};
use era::{EraOptions, FloorCounts, NamingEra, PriorStats, WaveRecords};
use library::RecordedName;

/// The plugin options that decide (`RenamePluginOptions`, the subset the
/// Rust honours).
#[derive(Clone, Debug)]
pub struct NamingConfig {
    pub bundler: Option<String>,
    pub minifier: Option<String>,
    /// `skipLibraries` (default true).
    pub skip_libraries: bool,
    pub reconcile_prior_diff: bool,
    pub naming_floor: bool,
    pub naming_floor_sweep: bool,
    /// `sourceMap` requested (gates the post passes off).
    pub source_map: bool,
    /// `emitRenameLedger` (gates the family permute off).
    pub emit_rename_ledger: bool,
    /// `--disable family-permute`.
    pub family_permute_disabled: bool,
    pub params: CacheKeyParams,
}

impl NamingConfig {
    /// `isSweepDeferred`.
    pub fn sweep_deferred(&self, has_prior: bool) -> bool {
        self.naming_floor
            && self.naming_floor_sweep
            && self.reconcile_prior_diff
            && has_prior
            && !self.source_map
    }
}

/// The texts the stage runs on.
#[derive(Clone, Copy)]
pub struct NamingInput<'t> {
    /// The formatted (beautified) text of the file.
    pub fresh: &'t str,
    /// The prior version's humanified text.
    pub prior: Option<&'t str>,
    /// The file's library classification (None: no banner regions) —
    /// `libdetect::function_carry`, the one owner.
    pub library: Option<&'t crate::libdetect::function_carry::LibraryClassification>,
}

/// Which post-generate pass a text override feeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PostPass {
    Reconcile,
    Sweep,
    Permute,
}

/// Gate scaffolding: planted order bugs, per-pass input overrides (the
/// bisection mode — each pass reads the TS's input text), and the
/// wave-boundary stop.
#[derive(Default)]
pub struct NamingHooks<'h> {
    pub stop_after_waves: bool,
    pub wave_plant: Option<Plant>,
    pub reconcile_plant: Option<ReconcilePlant>,
    pub permute_plant: Option<PermutePlant>,
    #[allow(clippy::type_complexity)]
    pub pass_input: Option<&'h dyn Fn(PostPass) -> Option<String>>,
    /// The wall-clock elapsed the coverage reports (0 in the gate).
    pub elapsed_ms: f64,
    pub driver_plant: Option<DriverPlant>,
}

/// The driver's planted bugs (the gate's red runs): each breaks one ORDER
/// or CONDITION plugin.ts decides by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverPlant {
    /// Drop `isSweepDeferred`: the sweep runs pre-generate with a prior.
    NoDeferral,
    /// Run the family permute FIRST — over the generated text, ahead of
    /// the reconcile and the sweep (two pass calls swapped).
    PermuteFirst,
    /// Model a first version with the prior's TWO scope epochs.
    TwoEpochsWithoutPrior,
}

/// A post-generate pass that ran, with the text it produced (None when it
/// applied nothing).
pub struct PassRun<T> {
    pub result: T,
    pub code: Option<String>,
}

/// What the stage hands on (`RenamePluginResult` + the gate's records).
pub struct NamingOutcome {
    /// The generated text (None when stopped after the waves).
    pub generated: Option<String>,
    pub reconcile: Option<PassRun<ReconcileResult>>,
    /// The deferred sweep, with the anchor of the text it read.
    pub deferred_sweep: Option<(Anchor, PassRun<SweepResult>)>,
    pub permute: Option<FamilyPermuteOutcome>,
    /// The shipped code (`finalCode`).
    pub code: Option<String>,
    pub census: Option<MintedCensus>,
    pub trail: StrategyTrail,
    pub waves: WaveRecords,
    pub pre_sweep: Option<SweepResult>,
    pub floor: Option<FloorCounts>,
    pub prior: Option<PriorStats>,
    /// Every report (`allReports`: the processor's, then library prefix).
    pub reports: Vec<RenameReport>,
    pub processor: ProcessorReport,
    pub library_names: Vec<RecordedName>,
    /// The library freeze as applied (regions.json `libraryFunctions`).
    pub library_functions: Vec<crate::libdetect::function_carry::LibraryFunctionKey>,
    pub coverage: Option<CoverageSummary>,
    pub coverage_text: Option<String>,
    pub claims: crate::rename::validated::RenameClaimStats,
    pub output_valid: bool,
    /// Which invariant the generated text failed (None when stopped
    /// before generate) — the CLI's `ERROR:` blocks read it.
    pub verdict: Option<validate::Verdict>,
    pub fn_hashes: Vec<(String, String)>,
    /// `renameResult.priorCarry` — the split's tiers regime and the `-vv`
    /// `prior-match-map.json` read it (None without a prior).
    pub prior_carry: Option<crate::rename::transfer::carry::PriorCarry>,
    /// Cache misses / provider errors across every LLM pass.
    pub misses: usize,
    pub errors: usize,
    /// `renameResult.renameLedger` (`emitRenameLedger` only): the base
    /// stage over the fresh text, then one post stage per post-generate
    /// pass that applied a rename (reconcile, deferred sweep).
    pub rename_ledger: Option<crate::rename::validated::ledger::RenameLedgerBundle>,
}

/// Run the naming stage.
pub fn run_naming<P: NameProvider>(
    input: &NamingInput<'_>,
    config: &NamingConfig,
    hooks: &NamingHooks<'_>,
    provider: &P,
) -> Result<NamingOutcome, String> {
    let has_prior = input.prior.is_some();
    let deferred =
        config.sweep_deferred(has_prior) && hooks.driver_plant != Some(DriverPlant::NoDeferral);
    let opts = EraOptions {
        bundler: config.bundler.as_deref(),
        minifier: config.minifier.as_deref(),
        params: &config.params,
        naming_floor: config.naming_floor,
        pre_generate_sweep: config.naming_floor_sweep && !deferred,
        skip_libraries: config.skip_libraries,
        library: input.library,
        wave_plant: hooks.wave_plant,
        stop_after_waves: hooks.stop_after_waves,
        two_epochs_without_prior: hooks.driver_plant == Some(DriverPlant::TwoEpochsWithoutPrior),
        rename_ledger: config.emit_rename_ledger,
    };
    let era = match input.prior {
        Some(prior) => match_prior_version(
            PriorMatchInput {
                fresh: input.fresh,
                prior,
                bundler: opts.bundler,
                minifier: opts.minifier,
                visit_optional_calls: false,
            },
            |stage| era::prior_era(stage, &opts, provider),
        )?,
        None => era::fresh_era(input.fresh, &opts, provider)?,
    };
    let NamingEra {
        generated,
        trail,
        waves,
        processor,
        library,
        library_functions,
        floor,
        pre_sweep,
        prior,
        function_count,
        fn_hashes,
        prior_carry,
        ledger,
        ..
    } = era;
    let mut reports = processor.reports.clone();
    reports.extend(library.reports.clone());
    let mut out = NamingOutcome {
        generated,
        reconcile: None,
        deferred_sweep: None,
        permute: None,
        code: None,
        census: None,
        trail,
        misses: waves.misses + pre_sweep.as_ref().map_or(0, |s| s.misses),
        errors: waves.errors + pre_sweep.as_ref().map_or(0, |s| s.errors),
        waves,
        pre_sweep,
        floor,
        prior,
        reports,
        processor,
        library_names: library.names.clone(),
        library_functions,
        coverage: None,
        coverage_text: None,
        claims: Default::default(),
        output_valid: true,
        verdict: None,
        fn_hashes,
        prior_carry,
        rename_ledger: None,
    };
    // `buildLedgerPostStages`: (input text, the pass's ledger) per pass
    // that produced code — reconcile over the generated text, the sweep
    // over the reconciled text (else the generated one).
    let mut ledger_stages: Vec<(String, crate::rename::validated::ledger::RenameLedger)> =
        Vec::new();
    let Some(generated) = out.generated.clone() else {
        out.claims = out.trail.claims;
        return Ok(out);
    };
    // `captureSemanticBaseline` + the invariant checks on the generated
    // text: the post-generate passes need a valid output.
    let verdict = match validate::baseline_of(input.fresh) {
        Some(b) => validate::verdict(&generated, &b),
        // The fresh text itself does not parse: nothing can be validated.
        None => validate::Verdict::ParseFailed,
    };
    out.output_valid = verdict == validate::Verdict::Valid;
    out.verdict = Some(verdict);
    let eligible = Eligibility::new(opts.bundler, opts.minifier);
    let over = |p: PostPass| hooks.pass_input.and_then(|f| f(p));
    let trail = std::mem::take(&mut out.trail);

    // -- the prior-diff reconcile --------------------------------------
    let run_reconcile = config.reconcile_prior_diff && !config.source_map && out.output_valid;
    let trail = match input.prior.filter(|_| run_reconcile) {
        None => trail,
        Some(prior) => {
            let text = over(PostPass::Reconcile).unwrap_or_else(|| generated.clone());
            match run_prior_diff_reconciliation(
                &text,
                prior,
                &eligible,
                trail,
                hooks.reconcile_plant,
                config.emit_rename_ledger,
            ) {
                Ok(PriorDiffOutcome {
                    result,
                    code,
                    trail,
                    ledger,
                }) => {
                    ledger_stages.extend(ledger.map(|l| (text, l)));
                    out.reconcile = Some(PassRun { result, code });
                    trail
                }
                Err((_, trail)) => trail,
            }
        }
    };
    let recon_code = out.reconcile.as_ref().and_then(|r| r.code.clone());

    // -- the deferred sweep -----------------------------------------------
    let trail = if deferred && out.output_valid {
        let text = over(PostPass::Sweep)
            .unwrap_or_else(|| recon_code.clone().unwrap_or_else(|| generated.clone()));
        let anchor = if recon_code.is_some() {
            Anchor::Reconciled
        } else {
            Anchor::Generated
        };
        match run_deferred_sweep(
            &text,
            anchor,
            &eligible,
            provider,
            &config.params,
            trail,
            config.emit_rename_ledger,
        ) {
            Ok(o) => {
                ledger_stages.extend(o.ledger.map(|l| (text.clone(), l)));
                out.misses += o.sweep.misses;
                out.errors += o.sweep.errors;
                if let Some(f) = out.floor.as_mut() {
                    f.swept += o.sweep.named;
                    f.skipped += o.sweep.skipped;
                }
                out.deferred_sweep = Some((
                    anchor,
                    PassRun {
                        result: o.sweep,
                        code: o.code,
                    },
                ));
                o.trail
            }
            Err((_, trail)) => trail,
        }
    } else {
        trail
    };
    let swept_code = out
        .deferred_sweep
        .as_ref()
        .and_then(|(_, s)| s.code.clone());
    let resolved = swept_code
        .or_else(|| recon_code.clone())
        .unwrap_or_else(|| generated.clone());
    out.claims = trail.claims;
    out.trail = trail;
    out.rename_ledger = ledger.map(|mut base| {
        let (stage_sources, stages): (Vec<String>, Vec<_>) = ledger_stages
            .into_iter()
            .map(|(text, l)| {
                (
                    text,
                    crate::rename::validated::ledger::LedgerStage {
                        source_sha256: l.source_sha256,
                        entries: l.entries,
                    },
                )
            })
            .unzip();
        base.post = (!stages.is_empty()).then_some(stages);
        crate::rename::validated::ledger::RenameLedgerBundle {
            ledger: base,
            source: input.fresh.to_string(),
            stage_sources,
        }
    });

    // -- the family permute ------------------------------------------------
    let permute_eligible = config.reconcile_prior_diff
        && !config.source_map
        && !config.emit_rename_ledger
        && out.output_valid;
    let mut shipped = resolved.clone();
    if !config.family_permute_disabled
        && permute_eligible
        && let Some(prior) = input.prior
    {
        let text = if hooks.driver_plant == Some(DriverPlant::PermuteFirst) {
            generated.clone()
        } else {
            over(PostPass::Permute).unwrap_or(resolved)
        };
        if let Ok(p) = run_family_permute(&text, prior, &eligible, hooks.permute_plant) {
            add_claims(&mut out.claims, &p.claims);
            shipped = p.code.clone().unwrap_or(text);
            out.permute = Some(p);
        } else {
            shipped = text;
        }
    }

    // -- the census + coverage ----------------------------------------------
    let census = census_of_text(&shipped, &eligible)?;
    let mut coverage = build_coverage_summary(
        &out.reports,
        &coverage_inputs(&out, function_count, &library, hooks),
    );
    coverage.minted_census = Some(census_record(&census));
    out.coverage_text = Some(format_coverage_summary(&coverage));
    out.coverage = Some(coverage);
    out.census = Some(census);
    out.code = Some(shipped);
    Ok(out)
}

fn add_claims(
    total: &mut crate::rename::validated::RenameClaimStats,
    more: &crate::rename::validated::RenameClaimStats,
) {
    total.ledger_only_rejections += more.ledger_only_rejections;
    total.by_guard.target_in_scope += more.by_guard.target_in_scope;
    total.by_guard.target_visible += more.by_guard.target_visible;
    total.by_guard.shadows_child += more.by_guard.shadows_child;
    total.claims_recorded += more.claims_recorded;
}

fn coverage_inputs(
    out: &NamingOutcome,
    function_count: usize,
    library: &library::LibraryOutcome,
    hooks: &NamingHooks<'_>,
) -> CoverageInputs {
    let counts = out.prior.as_ref().map(|p| p.counts).unwrap_or_default();
    CoverageInputs {
        total_functions: function_count,
        skipped_by_skip_list: out.processor.skipped_by_skip_list,
        skip_reasons: out.processor.skip_reasons,
        library_no_minified: library.no_minified,
        prior_version_applied: counts.functions_matched,
        prior_version_already_named: counts.functions_already_named,
        prior_version_bindings_applied: counts.bindings_applied,
        prior_version_close_match: counts.close_match_count,
        llm_calls: out.processor.completed_calls,
        llm_retries: 0,
        avg_response_time_ms: 0.0,
        elapsed_ms: hooks.elapsed_ms,
    }
}

fn transfer_stats(s: &TransferStats) -> StatsTransferStats {
    StatsTransferStats {
        attempted: s.attempted as f64,
        applied: s.applied as f64,
        skipped: s.skipped as f64,
        rejected: (!s.rejected.is_empty()).then(|| {
            RejectionCounts(humanify_model::jsshape::CountMap(
                s.rejected
                    .iter()
                    .map(|(k, n)| (k.clone(), *n as f64))
                    .collect(),
            ))
        }),
    }
}

/// `TransferStatsByTier` of a prior-carrying run.
pub fn transfer_stats_by_tier(p: &PriorStats) -> TransferStatsByTier {
    TransferStatsByTier {
        exact_match: transfer_stats(&p.exact_match),
        close_match: transfer_stats(&p.close_match),
        statement_twin: Some(transfer_stats(&p.statement_twin)),
        retry: Some(transfer_stats(&p.retry)),
    }
}

fn resolution_stats(
    s: &crate::matching::cascade::ResolutionStats,
) -> humanify_model::stats::ResolutionStats {
    use crate::matching::statement_context::STMT_SPAN_BUCKETS;
    use humanify_model::stats as m;
    let r = &s.propagation_by_rung;
    let a = &s.enclosing_stmt_abstain;
    let n = |x: usize| x as f64;
    m::ResolutionStats {
        structural_hash_unique: n(s.structural_hash_unique),
        identity_resolved: n(s.identity_resolved),
        member_key_resolved: n(s.member_key_resolved),
        enclosing_statement_resolved: n(s.enclosing_statement_resolved),
        callee_shapes_resolved: n(s.callee_shapes_resolved),
        caller_shapes_resolved: n(s.caller_shapes_resolved),
        callee_hashes_resolved: n(s.callee_hashes_resolved),
        two_hop_shapes_resolved: n(s.two_hop_shapes_resolved),
        shingle_similarity_resolved: n(s.shingle_similarity_resolved),
        shingle_unconsultable: n(s.shingle_unconsultable),
        ordinal_resolved: n(s.ordinal_resolved),
        interchangeable_resolved: n(s.interchangeable_resolved),
        injectivity_demoted: n(s.injectivity_demoted),
        singleton_rejected: n(s.singleton_rejected),
        singleton_unguarded: n(s.singleton_unguarded),
        still_ambiguous: n(s.still_ambiguous),
        unmatched: n(s.unmatched),
        propagation_resolved: n(s.propagation_resolved),
        propagation_by_rung: m::PropagationByRung {
            matched_callee: n(r.matched_callee),
            matched_caller: n(r.matched_caller),
            scope_parent: n(r.scope_parent),
            external_refs: n(r.external_refs),
            scope_ordinal: n(r.scope_ordinal),
        },
        crossed_container_revoked: n(s.crossed_container_revoked),
        enclosing_stmt_abstain: m::EnclosingStmtAbstain {
            no_hash_is_statement: n(a.no_hash_is_statement),
            no_hash_too_long: n(a.no_hash_too_long),
            no_hash_other: n(a.no_hash_other),
            no_new_holders: n(a.no_new_holders),
            count_mismatch: n(a.count_mismatch),
            partner_filtered: n(a.partner_filtered),
            reached: n(a.reached),
            resolved_local: n(a.resolved_local),
            resolved_spanning: n(a.resolved_spanning),
            count_mismatch_local: n(a.count_mismatch_local),
            count_mismatch_spanning: n(a.count_mismatch_spanning),
            spanning_parent_agrees: n(a.spanning_parent_agrees),
            spanning_parent_disagrees: n(a.spanning_parent_disagrees),
            spanning_parent_unknown: n(a.spanning_parent_unknown),
            reached_span_buckets: humanify_model::jsshape::CountMap(
                STMT_SPAN_BUCKETS
                    .iter()
                    .zip(a.reached_span_buckets)
                    .map(|(k, v)| (k.to_string(), n(v)))
                    .collect(),
            ),
        },
    }
}

impl NamingOutcome {
    /// The `--stats-json` record's naming half (`writeEvalStats`, minus
    /// `vendorNaming` and `selection`, which other stages own).
    pub fn eval_stats(&self) -> EvalStats {
        let p = self.prior.as_ref();
        let c = &self.claims;
        EvalStats {
            coverage: self.coverage.clone(),
            transfer_stats: p.map(transfer_stats_by_tier),
            prior_version_applied: Some(p.map_or(0.0, |p| p.counts.functions_matched as f64)),
            prior_version_already_named: Some(
                p.map_or(0.0, |p| p.counts.functions_already_named as f64),
            ),
            prior_version_bindings_applied: Some(
                p.map_or(0.0, |p| p.counts.bindings_applied as f64),
            ),
            naming_floor: self.floor.map(|f| NamingFloorStats {
                derived: f.derived as f64,
                undecorated: f.undecorated as f64,
                swept: f.swept as f64,
                skipped: f.skipped as f64,
            }),
            close_match_stats: p.map(|p| CloseMatchStats {
                corroborated_by_alignment: p.close_match_stats.corroborated_by_alignment as f64,
                corroborated_by_shingles: p.close_match_stats.corroborated_by_shingles as f64,
                uncorroborated: p.close_match_stats.uncorroborated as f64,
            }),
            resolution_stats: p.map(|p| resolution_stats(&p.resolution_stats)),
            binding_resolution_stats: humanify_model::jsshape::Nullable(
                p.and_then(|p| p.binding_resolution_stats.as_ref())
                    .map(resolution_stats),
            ),
            vendor_naming: None,
            rename_claims: RenameClaimStats {
                ledger_only_rejections: c.ledger_only_rejections as f64,
                by_guard: RenameClaimGuards {
                    target_in_scope: c.by_guard.target_in_scope as f64,
                    target_visible: c.by_guard.target_visible as f64,
                    shadows_child: c.by_guard.shadows_child as f64,
                },
                claims_recorded: c.claims_recorded as f64,
            },
            selection: None,
        }
    }
}

#[cfg(test)]
mod driver_test;
