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
    WaveGaugesStats,
};

use crate::naming::passes::census::MintedCensus;
use crate::naming::passes::census_of_text;
use crate::naming::passes::family_permute::{
    FamilyPermuteOutcome, PriorMembers, run_family_permute, run_family_permute_with,
};
use crate::naming::passes::sweep::{DecidedNames, SweepResult, run_deferred_sweep};
use crate::naming::reconcile::ReconcileResult;
use crate::naming::reconcile::step::{PriorDiffOutcome, run_prior_diff_reconciliation};
use crate::naming::report::coverage::{
    CoverageInputs, binding_provenance_record, build_coverage_summary, census_record,
    format_coverage_summary, survivor_provenance_split,
};
use crate::naming::report::{ProcessorReport, RenameReport};
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::rename::eligibility::Eligibility;
use crate::rename::name_profile::NameProfile;
use crate::rename::transfer::TransferStats;
use crate::trail::{Anchor, StrategyTrail};
use era::{EraOptions, FloorCounts, NamingEra, PriorStats, WaveRecords};
use library::RecordedName;

/// The plugin options that decide (`RenamePluginOptions`, the subset the
/// Rust honours).
#[derive(Clone, Debug)]
pub struct NamingConfig {
    /// The run's never-rename lists (the toolchain's P7 piece,
    /// `crate::toolchain`).
    pub never_rename: crate::rename::eligibility::NeverRename,
    /// How many module-level names one naming request carries — the one
    /// number the run's per-bundler tuning (the toolchain's P14 piece,
    /// `BundlerTuning::module_group_size`) decides for this stage. The
    /// stage is handed the number, not the bundler-keyed type (review R23).
    pub module_group_size: usize,
    /// The run's bundle layout (the toolchain's P9 piece): where each
    /// side's top-level statements are — the match sides, the freezes, the
    /// family permute's module scope.
    pub layout: crate::toolchain::BundleLayout,
    /// The run's module wrapper grammar (the toolchain's P3 piece): which
    /// bindings are the bundler's lazy-init helper, the plumbing the
    /// pipeline names itself (`naming::plumbing`).
    pub module_wrappers: crate::toolchain::ModuleWrapperGrammar,
    /// The minifier name profile selected once from detection
    /// (`rename::name_profile::select_name_profile`) — every name-shape
    /// question of the stage is asked under it.
    pub name_profile: NameProfile,
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
    /// `--dump-artifacts` armed: take the naming era's
    /// [`era::EraCapture`] (observation only).
    pub capture_dump: bool,
    /// `batchSize` / `maxRetriesPerIdentifier` / `maxFreeRetries` /
    /// `laneThreshold` (the defaults when unset) + the Rust-run re-ask
    /// budget (`--rename-retries`, `naming::reask`'s default).
    pub tunables: crate::naming::waves::batch::WaveTunables,
    /// `--probe shingle-probe`.
    pub shingle_probe: bool,
    /// `--fast [tier]` (docs/rust-port/20-fast-mode.md, `crate::fast`).
    pub fast: crate::fast::FastTier,
    /// How many rendered prompts may be alive at once (finding #65) — the
    /// CLI sizes it over the rate limiter's `max_concurrent`
    /// [`crate::naming::waves::processor::DEFAULT_PROMPT_WINDOW`]).
    pub prompt_window: usize,
    /// The module step (`naming::module_names`): Some(the namer's prompt
    /// budget) when the run's toolchain offers module markers — the step
    /// itself still runs only when they describe the bundle; None off.
    pub module_naming: Option<crate::place::assign::namer::SplitNamerBudget>,
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
    /// The names mode's claimed placement groups (`library.matching =
    /// "names"` only, exp156): raw statement spans from the unpack, which
    /// the era converts through `era_bridge` (exp166); with no bridge
    /// (unit tests) their `statements` are taken as era ordinals. Empty =
    /// nothing was claimed.
    pub claimed_regions: &'t [crate::naming::module_names::ClaimedRegion],
    /// The raw-to-era bridge (exp166) and the formatter's statement
    /// provenance for THIS file — names mode's app file only.
    pub era_bridge: Option<EraBridgeInput<'t>>,
    /// The claimed units' captured restoration answers (exp160), keyed by
    /// unit print hash. Empty = nothing was claimed, or the index holds
    /// no names tables.
    pub restorations:
        &'t std::collections::BTreeMap<String, crate::libindex::table::RestorationRow>,
}

/// The raw-to-era bridge (exp166, [`crate::naming::module_names::EraBridge`])
/// with the formatter's statement provenance for the file being named.
#[derive(Clone, Copy)]
pub struct EraBridgeInput<'t> {
    pub bridge: &'t crate::naming::module_names::EraBridge,
    pub origins: &'t [(u32, u32)],
}

/// The empty restorations map — every call site with no captured answers
/// (tests, every non-names mode).
pub fn no_restorations()
-> &'static std::collections::BTreeMap<String, crate::libindex::table::RestorationRow> {
    static EMPTY: std::sync::LazyLock<
        std::collections::BTreeMap<String, crate::libindex::table::RestorationRow>,
    > = std::sync::LazyLock::new(std::collections::BTreeMap::new);
    &EMPTY
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
    /// Why the prior-diff reconcile did not run although it was due (its
    /// `diff` failed or ran past its deadline) — see
    /// [`NamingOutcome::reconcile_skipped_warning`].
    pub reconcile_error: Option<String>,
    /// Cache misses / provider errors across every LLM pass.
    pub misses: usize,
    pub errors: usize,
    /// `renameResult.renameLedger` (`emitRenameLedger` only): the base
    /// stage over the fresh text, then one post stage per post-generate
    /// pass that applied a rename (reconcile, deferred sweep).
    pub rename_ledger: Option<crate::rename::validated::ledger::RenameLedgerBundle>,
    /// The naming era's artifact-dump capture (`capture_dump` only).
    pub capture: Option<era::EraCapture>,
    /// `--probe shingle-probe`'s debug lines (the CLI logs them).
    pub probe_lines: Vec<String>,
    /// The names the pipeline chose for the bundler's plumbing (the
    /// lazy-init helper, `naming::plumbing`) — on the trail too.
    pub plumbing: crate::naming::plumbing::PlumbingNames,
    /// The names the pipeline chose for library imports
    /// (`naming::plumbing::name_library_imports`) — on the trail too.
    pub library_imports: crate::naming::plumbing::PlumbingNames,
    /// What the module step did (`naming::module_names`).
    pub module_names: crate::naming::module_names::ModuleNamingReport,
    /// What the names mode placed (`naming::module_names`, exp156): the
    /// moved modules' vendor paths (the split reads them) and their
    /// spans (already applied as a freeze). Empty when nothing moved.
    pub library_layout: crate::naming::module_names::LibraryLayout,
    /// The restoration's measurement evidence (exp164; diagnostics only,
    /// never a decision input). None outside the names mode's claims.
    pub era_dump: Option<crate::naming::restore::EraDump>,
}

/// Run the naming stage.
pub fn run_naming<P: NameProvider>(
    input: &NamingInput<'_>,
    config: &NamingConfig,
    provider: &P,
    log: &mut crate::artifact_dump::DispatchLog,
) -> Result<NamingOutcome, String> {
    let has_prior = input.prior.is_some();
    let deferred = config.sweep_deferred(has_prior);
    let opts = EraOptions {
        never_rename: config.never_rename,
        module_group_size: config.module_group_size,
        layout: config.layout,
        module_wrappers: config.module_wrappers,
        name_profile: config.name_profile,
        params: &config.params,
        naming_floor: config.naming_floor,
        pre_generate_sweep: config.naming_floor_sweep && !deferred,
        skip_libraries: config.skip_libraries,
        library: input.library,
        rename_ledger: config.emit_rename_ledger,
        capture: config.capture_dump,
        tunables: config.tunables,
        shingle_probe: config.shingle_probe,
        fast: config.fast,
        prompt_window: config.prompt_window,
        module_naming: config.module_naming,
        claimed_regions: input.claimed_regions,
        era_bridge: input.era_bridge,
        restorations: input.restorations,
    };
    let mut run_era = || match input.prior {
        Some(prior) => match_prior_version(
            PriorMatchInput {
                fresh: input.fresh,
                prior,
                never_rename: opts.never_rename,
                layout: config.layout,
                fast: config.fast.on(),
                same_program_check: true,
            },
            |stage| era::prior_era(stage, &opts, provider, log),
        ),
        None => era::fresh_era(input.fresh, &opts, provider, log),
    };
    // `captureSemanticBaseline` reads only the fresh text, and the family
    // permute's prior index only the prior text: the fast schedule (the
    // relaxed default and `--sequential` alike) builds both on
    // a thread of their own while the era runs.
    let permute_may_run = !config.family_permute_disabled
        && config.reconcile_prior_diff
        && !config.source_map
        && !config.emit_rename_ledger;
    let ((baseline, mut prior_members), era) = if config.fast.on() {
        crate::par::beside(
            || {
                let prior_members = input
                    .prior
                    .filter(|_| permute_may_run)
                    .map(|p| PriorMembers::of(p, config.name_profile, config.layout));
                (Some(validate::baseline_of(input.fresh)), prior_members)
            },
            run_era,
        )
    } else {
        let era = run_era();
        ((None, None), era)
    };
    let era = era?;
    let NamingEra {
        generated,
        era_dump,
        trail,
        waves,
        processor,
        library,
        library_functions,
        floor,
        pre_sweep,
        exhausted_names: era_exhausted,
        prior,
        function_count,
        fn_hashes,
        prior_carry,
        ledger,
        capture,
        probe_lines,
        stems,
        plumbing,
        library_imports,
        module_names,
        library_layout,
        ..
    } = era;
    // The names mode's placed modules, in the GENERATED text's
    // coordinates: the prior-diff reconcile re-parses that text and
    // builds its own bindings, so it needs its own hold over the same
    // regions (era coordinates would point at the wrong bytes there).
    // The placed modules' generated spans, and the restoration's beside
    // them (exp160): the passes that re-parse the generated text hold both
    // — the prior's names must not overwrite the index's.
    let library_generated_spans: Vec<(u32, u32)> = library_layout
        .generated_spans
        .iter()
        .chain(&library_layout.restored_generated_spans)
        .cloned()
        .collect();
    let mut reports = processor.reports.clone();
    reports.extend(library.reports.clone());
    let mut out = NamingOutcome {
        generated,
        reconcile: None,
        reconcile_error: None,
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
        capture,
        probe_lines,
        plumbing,
        library_imports,
        module_names,
        library_layout,
        era_dump,
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
    let ph = crate::profiling::phase("naming:validate+reconcile");
    let verdict_of = |baseline: Option<validate::Baseline>| match baseline {
        Some(b) => validate::verdict(&generated, &b),
        // The fresh text itself does not parse: nothing can be validated.
        None => validate::Verdict::ParseFailed,
    };
    let eligible = Eligibility::new(opts.never_rename);
    let trail = std::mem::take(&mut out.trail);
    // -- the prior-diff reconcile (on a valid output only) -----------------
    let reconcile_gates = config.reconcile_prior_diff && !config.source_map;
    let ledger_walk = config.emit_rename_ledger.then_some(if deferred {
        crate::naming::reconcile::step::LedgerWalk::AfterLaterParse
    } else {
        crate::naming::reconcile::step::LedgerWalk::Live
    });
    let reconcile = |prior: &str, trail: StrategyTrail| {
        reconcile_pass(
            &generated,
            prior,
            &eligible,
            config.name_profile,
            trail,
            ledger_walk,
            &library_generated_spans,
        )
    };
    let (verdict, reconciled, trail) = match input.prior.filter(|_| reconcile_gates) {
        // The fast schedule: the verdict (a re-parse of the generated text) runs
        // beside a SPECULATIVE reconcile on a copy of the trail; an
        // invalid output discards the speculation — the parity outcome.
        Some(prior) if config.fast.on() => {
            let (verdict, (spec_trail, spec)) = crate::par::beside(
                || verdict_of(baseline.unwrap_or_else(|| validate::baseline_of(input.fresh))),
                || reconcile(prior, trail.clone()),
            );
            if verdict == validate::Verdict::Valid {
                (verdict, Some(spec), spec_trail)
            } else {
                (verdict, None, trail)
            }
        }
        prior => {
            let verdict =
                verdict_of(baseline.unwrap_or_else(|| validate::baseline_of(input.fresh)));
            match prior.filter(|_| verdict == validate::Verdict::Valid) {
                Some(prior) => {
                    let (trail, done) = reconcile(prior, trail);
                    (verdict, Some(done), trail)
                }
                None => (verdict, None, trail),
            }
        }
    };
    out.output_valid = verdict == validate::Verdict::Valid;
    out.verdict = Some(verdict);
    let mut recon_library_spans: Vec<(u32, u32)> = Vec::new();
    match reconciled {
        Some(Ok((run, stage, library_spans))) => {
            ledger_stages.extend(stage.map(|l| (generated.clone(), l)));
            recon_library_spans = library_spans;
            out.reconcile = Some(run);
        }
        Some(Err(error)) => out.reconcile_error = Some(error),
        None => {}
    }
    let recon_code = out.reconcile.as_ref().and_then(|r| r.code.clone());

    drop(ph);
    let ph = crate::profiling::phase("naming:deferred-sweep");
    // -- the deferred sweep -----------------------------------------------
    let trail = if deferred && out.output_valid {
        let text = recon_code.clone().unwrap_or_else(|| generated.clone());
        let anchor = if recon_code.is_some() {
            Anchor::Reconciled
        } else {
            Anchor::Generated
        };
        // The run's decision ledger joined BY NAME: per-binding identity
        // does not cross the generate/reconcile text boundary, so the
        // deferred sweep's provenance targeting consults the name join —
        // the declared approximation (naming::passes::sweep::DecidedNames).
        let decided = DecidedNames::of(&trail, &out.reports, &era_exhausted);
        // This parse builds its own bindings, so the hold must be armed
        // over the placed modules again (a sweep ask would name
        // library-placed code the mode exists to leave alone).
        let sweep_library_spans = deferred_library_spans(
            recon_code.is_some(),
            &recon_library_spans,
            &library_generated_spans,
        );
        match run_deferred_sweep(
            &text,
            anchor,
            &eligible,
            provider,
            log,
            &config.params,
            config.prompt_window,
            trail,
            config.emit_rename_ledger,
            &config.tunables,
            &decided,
            &stems,
            sweep_library_spans,
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
                        edits: l.edits,
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

    drop(ph);
    let ph = crate::profiling::phase("naming:family-permute");
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
        let text = resolved;
        let permuted = match prior_members.take() {
            Some(members) => run_family_permute_with(
                &text,
                members,
                &eligible,
                config.name_profile,
                config.layout,
            ),
            None => run_family_permute(&text, prior, &eligible, config.name_profile, config.layout),
        };
        if let Ok(p) = permuted {
            add_claims(&mut out.claims, &p.claims);
            shipped = p.code.clone().unwrap_or(text);
            out.permute = Some(p);
        } else {
            shipped = text;
        }
    }

    drop(ph);
    let _ph = crate::profiling::phase("naming:census");
    // -- the census + coverage ----------------------------------------------
    let census = census_of_text(&shipped, &eligible, config.name_profile)?;
    let mut coverage = build_coverage_summary(
        &out.reports,
        &coverage_inputs(&out, function_count, &library),
    );
    let mut record = census_record(&census);
    // The provenance meter (2026-09-30, Andrew's decision). The outcome
    // records ARE the provenance. Two halves, declared different:
    // - `survivor_provenance_split` — the TREE-WALK join of the census's
    //   shipped-text survivors against the run's records, BY NAME (spans
    //   do not cross the text boundary): the printed approximation.
    // - `binding_provenance` — the sweep's own IN-STAGE classification,
    //   per binding where identity exists; the deferred sweep's variant
    //   flags `joined: true` for the by-name rows.
    let mut exhausted: Vec<String> = era_exhausted.clone();
    exhausted.extend(
        out.pre_sweep
            .as_ref()
            .into_iter()
            .flat_map(|s| s.exhausted_names.iter().cloned()),
    );
    exhausted.extend(
        out.deferred_sweep
            .as_ref()
            .into_iter()
            .flat_map(|(_, run)| run.result.exhausted_names.iter().cloned()),
    );
    let (survivors, letters) =
        survivor_provenance_split(&census, &out.reports, &out.trail, &exhausted);
    record.provenance = Some(survivors);
    record.single_letters = Some(letters);
    record.binding_provenance = out
        .deferred_sweep
        .as_ref()
        .and_then(|(_, run)| run.result.provenance.as_ref())
        .or_else(|| out.pre_sweep.as_ref().and_then(|s| s.provenance.as_ref()))
        .map(binding_provenance_record);
    coverage.minted_census = Some(record);
    out.coverage_text = Some(format_coverage_summary(&coverage));
    out.coverage = Some(coverage);
    out.census = Some(census);
    // The ledger pins the text its whole chain must reproduce.
    if let Some(bundle) = out.rename_ledger.as_mut() {
        bundle.ledger.output_sha256 = Some(crate::rename::validated::ledger::sha256_hex(&shipped));
    }
    out.code = Some(shipped);
    Ok(out)
}

/// [`NamingOutcome::reconcile_skipped_warning`]'s line for one error.
fn reconcile_skipped_warning(error: &str) -> String {
    format!(
        "WARNING: the prior-diff reconcile did not run ({error}) — names it would have \
restored from the prior are missing from this output; re-run it"
    )
}

/// The placed modules' spans in the DEFERRED SWEEP's text: the
/// prior-diff reconcile's folds when it re-rendered, else the era's
/// generated-text coordinates (the texts are the same, so the same
/// spans apply).
fn deferred_library_spans<'a>(
    reconciled: bool,
    recon: &'a [(u32, u32)],
    generated: &'a [(u32, u32)],
) -> &'a [(u32, u32)] {
    if reconciled { recon } else { generated }
}

/// The prior-diff reconcile over the generated text: the trail it hands
/// on, and — when it ran — its result, its rename-ledger stage, and the
/// placed modules' spans in the text it re-rendered (the deferred sweep
/// parses that text next and needs its own hold over them).
type ReconcileRun = (
    PassRun<ReconcileResult>,
    Option<crate::rename::validated::ledger::RenameLedger>,
    Vec<(u32, u32)>,
);

fn reconcile_pass(
    generated: &str,
    prior: &str,
    eligible: &Eligibility,
    profile: NameProfile,
    trail: StrategyTrail,
    ledger_walk: Option<crate::naming::reconcile::step::LedgerWalk>,
    library_spans: &[(u32, u32)],
) -> (StrategyTrail, Result<ReconcileRun, String>) {
    match run_prior_diff_reconciliation(
        generated,
        prior,
        eligible,
        profile,
        trail,
        ledger_walk,
        library_spans,
    ) {
        Ok(PriorDiffOutcome {
            result,
            code,
            library_spans,
            trail,
            ledger,
        }) => (trail, Ok((PassRun { result, code }, ledger, library_spans))),
        // Kept, never dropped: the CLI prints it (audit R3 — a failed
        // `diff` used to skip the whole pass in silence).
        Err((error, trail)) => (trail, Err(error)),
    }
}

/// Add one pass's validated-rename claim counters to a run total.
pub fn add_claims(
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
        // The run's wall-clock is not part of the recorded coverage.
        elapsed_ms: 0.0,
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

/// The stats record's `reask` block (the 2026-09-29 schema bump): the
/// 2026-09-28 collision-retry counters — the processor's lane half direct,
/// the sweep's re-ask half summed over the pre-generate and deferred sweeps
/// (both are sweeps; an absent one contributes zero).
fn reask_stats(
    processor: &ProcessorReport,
    pre_sweep: Option<&SweepResult>,
    deferred_sweep: Option<&SweepResult>,
) -> humanify_model::stats::ReaskStats {
    let mut reask = humanify_model::stats::ReaskStats {
        unrecoverable_rejections: processor.unrecoverable_rejections as f64,
        late_rejections: processor.late_rejections as f64,
        invalid_suggestion_finishes: processor.invalid_suggestion_finishes as f64,
        all_failed_windows: processor.all_failed_windows as f64,
        sweep_reasked: 0.0,
        sweep_reask_applied: 0.0,
        sweep_reask_dropped: 0.0,
    };
    for sweep in [pre_sweep, deferred_sweep].into_iter().flatten() {
        reask.sweep_reasked += sweep.reasked as f64;
        reask.sweep_reask_applied += sweep.reask_applied as f64;
        reask.sweep_reask_dropped += sweep.reask_dropped as f64;
    }
    reask
}

/// The stats record's `promptGuard` block (2026-10-04): the waves'
/// per-site tally plus the sweeps' asked / shown / refused counts.
fn prompt_guard_stats(
    processor: &ProcessorReport,
    pre_sweep: Option<&SweepResult>,
    deferred_sweep: Option<&SweepResult>,
) -> humanify_model::stats::PromptGuardStats {
    let g = &processor.prompt_guard;
    let mut out = humanify_model::stats::PromptGuardStats {
        fn_asked: g.fn_asked as f64,
        fn_unshown: g.fn_unshown as f64,
        retry_asked: g.retry_asked as f64,
        retry_unshown: g.retry_unshown as f64,
        module_asked: g.module_asked as f64,
        module_unshown: g.module_unshown as f64,
        sweep_asked: 0.0,
        sweep_unshown: 0.0,
        sweep_refused: 0.0,
        examples: g.examples.clone(),
    };
    for sweep in [pre_sweep, deferred_sweep].into_iter().flatten() {
        out.sweep_asked += sweep.targets_asked as f64;
        out.sweep_unshown += (sweep.targets_asked - sweep.targets_shown) as f64;
        out.sweep_refused += sweep.not_shown as f64;
        out.examples.extend(
            sweep
                .not_shown_examples
                .iter()
                .map(|n| format!("sweep:{n}")),
        );
    }
    out
}

impl NamingOutcome {
    /// The `--stats-json` record's naming half (`writeEvalStats`, minus
    /// `vendorNaming` and `selection`, which other stages own).
    pub fn eval_stats(&self) -> EvalStats {
        self.eval_stats_with(&Default::default())
    }

    /// [`NamingOutcome::eval_stats`] with the claims of the passes that ran
    /// after the naming stage (the post-split reconcile, the bundle carry)
    /// — the TS's `renameClaimStats()` is one run-wide counter.
    pub fn eval_stats_with(
        &self,
        later_claims: &crate::rename::validated::RenameClaimStats,
    ) -> EvalStats {
        let p = self.prior.as_ref();
        let mut claims = self.claims;
        add_claims(&mut claims, later_claims);
        let c = &claims;
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
            toolchain: None,
            // The split's, set by the pipeline after it runs.
            split_method: None,
            // Run-wide: the CLI fills it from every namer (unified.rs).
            provider_errors: None,
            // Set by the CLI from the run's Config (its record).
            config: None,
            prompt_guard: Some(prompt_guard_stats(
                &self.processor,
                self.pre_sweep.as_ref(),
                self.deferred_sweep.as_ref().map(|(_, run)| &run.result),
            )),
            reask: Some(reask_stats(
                &self.processor,
                self.pre_sweep.as_ref(),
                self.deferred_sweep.as_ref().map(|(_, run)| &run.result),
            )),
            wave_gauges: Some(WaveGaugesStats {
                context_set_names: self.waves.context_set_names as f64,
                peak_live_dispatches: self.waves.peak_live_dispatches as f64,
                peak_live_prompt_bytes: self.waves.peak_live_prompt_bytes as f64,
                strategy_bytes: self.waves.gauges.strategy_bytes as f64,
                strategy_bindings_bytes: Some(self.waves.gauges.strategy_bindings_bytes as f64),
                strategy_taken_bytes: Some(self.waves.gauges.strategy_taken_bytes as f64),
                strategy_callee_bytes: Some(self.waves.gauges.strategy_callee_bytes as f64),
                strategy_callsite_bytes: Some(self.waves.gauges.strategy_callsite_bytes as f64),
                strategy_context_var_bytes: Some(
                    self.waves.gauges.strategy_context_var_bytes as f64,
                ),
                strategy_module_bytes: Some(self.waves.gauges.strategy_module_bytes as f64),
                taken_set_names: Some(self.waves.gauges.taken_set_names as f64),
                ctx_bytes: self.waves.gauges.ctx_bytes as f64,
                used_set_bytes: self.waves.gauges.used_set_bytes as f64,
                name_record_bytes: self.waves.gauges.name_record_bytes as f64,
                bookkeeping_bytes: self.waves.gauges.bookkeeping_bytes as f64,
            }),
        }
    }

    /// The broken-prior WARNING (the useless-prior incident,
    /// docs/perf-inventory.md item 4): a loaded prior that bound NOTHING
    /// — no functions matched, none already named, no close matches, no
    /// binding renames, nothing from the twin or retry tiers — means the
    /// run silently degraded to a full fresh pass (the incident's
    /// multipliers: 3.7x wall, 4.8x memory, exit 0). The evidence is
    /// exactly what [`PriorStats`] already carries at that point; None
    /// when no prior ran or the prior bound anything at all. Observation
    /// only — nothing decides on this.
    /// The loud line for a prior-diff reconcile that was due and did not
    /// run (audit R3): the prior's names it would have restored are
    /// missing, so the output differs from a run where it ran. Print-only.
    pub fn reconcile_skipped_warning(&self) -> Option<String> {
        self.reconcile_error
            .as_deref()
            .map(reconcile_skipped_warning)
    }

    pub fn broken_prior_warning(&self) -> Option<String> {
        let p = self.prior.as_ref()?;
        let c = p.counts;
        let bound_something = c.functions_matched > 0
            || c.functions_already_named > 0
            || c.close_match_count > 0
            || c.bindings_applied > 0
            || p.exact_match.applied > 0
            || p.close_match.applied > 0
            || p.statement_twin.applied > 0
            || p.retry.applied > 0;
        if bound_something {
            return None;
        }
        Some(format!(
            "WARNING: the prior version bound nothing — 0 of {} function(s) matched, 0 binding \
             renames applied — so this run silently degraded to a full fresh pass (~3.7x the \
             wall, ~4.8x the memory of a bound prior; docs/perf-inventory.md, item 4). Check \
             the --prior-version file.",
            self.fn_hashes.len()
        ))
    }
}

#[cfg(test)]
mod driver_test;

#[cfg(test)]
mod fast_test;
