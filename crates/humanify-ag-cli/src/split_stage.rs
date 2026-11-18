//! Stages 10-12 as the pipeline runs them — TS unified.ts `runSplit` /
//! `tryStableSplit`: load the prior ledger, the split
//! ([`humanify_ag_core::emit::stable_split`]), then the COMMIT into the
//! output tree (drop the consumed source, put the tree, the ledger, the
//! `-vv` match map, the humanified bundle, the stage hashes), then the
//! finishing stage
//! ([`humanify_ag_core::finish::driver`]: re-link, `using` desugar, scaffold,
//! post-split reconcile + bundle carry) and the placement stats.
//!
//! Every handoff is an in-process value: the shipped text and the prior
//! carry come from the naming stage, the ledger and the runnable file list
//! from the split. The per-stage verbs (`humanify-ag emit`, `humanify-ag
//! finish`) call the same core owners.

use std::path::Path;

use humanify_ag_core::artifact_dump::SplitSections;
use humanify_ag_core::emit::align::AlignSwitches;
use humanify_ag_core::emit::stable_split::{
    InputGate, SplitError, SplitOptions, SplitOutcome, stable_split,
};
use humanify_ag_core::finish::driver::{FinishInput, FinishReport, FinishSwitches, finish_stage};
use humanify_ag_core::place::assign::namer::{
    ProviderSplitNamer, ProviderTreeReviser, SplitNamer, SplitNamerBudget, TreeReviser,
};
use humanify_ag_core::place::ledger::{StableSplitLedger, settle_prior_hashes};
use humanify_ag_core::place::method::{MarkerOffer, method_name, method_record, not_split_record};
use humanify_ag_core::place::tiers::{PLACEMENT_TIERS, PlacementSwitches, placement_summary};
use humanify_ag_core::place::trail::PlacementTrail;
use humanify_ag_core::rename::transfer::carry::PriorCarry;
use humanify_ag_model::llm::NameProvider;
use humanify_ag_model::ordered::OrderedMap;
use humanify_ag_model::stats::SplitMethodStats;

use crate::kill_switches::{Switch, SwitchState};
use crate::log::debug_enabled;
use crate::progress::ProgressRenderer;
use crate::writers::{
    PLACEMENT_STATS_PATH, PlacementStats, SPLIT_LEDGER_PATH, STAGE_HASHES_PATH, StageHashes,
    placement_stats_text, stage_fingerprint, stage_hashes_text,
};
use humanify_ag_core::output_tree::OutputTree;

/// What the split stage is given.
/// What the stages before the split recorded for the provenance file.
pub struct ProvenanceInput<'a> {
    pub link: Option<&'a humanify_ag_core::unpack::chunk_link::LinkMap>,
    pub unpack: Option<&'a humanify_ag_core::unpack::bun::UnpackOrigins>,
    pub origins: &'a [(u32, u32)],
}

pub struct SplitStageInput<'a> {
    pub provenance: ProvenanceInput<'a>,
    /// The embedded files the linked input reads by path (exp217): the
    /// runnable tree's `embedded/`.
    pub embedded_files: &'a [humanify_ag_core::unpack::chunk_link::EmbeddedFile],
    /// The run's input bundle (`filename`).
    pub input_file: &'a Path,
    /// The working directory, read once by the shell: relative paths
    /// resolve against it (`node_path_resolve`).
    pub working_dir: &'a Path,
    /// The unpacked file the naming stage processed (removed once the tree
    /// supersedes it).
    pub processed_source: Option<&'a Path>,
    /// The prior release's tree (`--prior-version`), read once by the
    /// shell (`crate::prior_tree`).
    pub prior: Option<&'a humanify_ag_core::prior_tree::PriorTree>,
    /// The prior split ledger in force (`--split-ledger`, else the one
    /// beside the prior), read once with the tree.
    pub ledger: Option<&'a crate::prior_tree::PriorLedger>,
    pub split_pure: bool,
    /// Whether the toolchain offers the module markers (decided once at
    /// detection); the split itself picks the method from the bundle.
    pub markers: MarkerOffer,
    pub switches: &'a SwitchState,
    pub provider: &'a dyn NameProvider,
    /// The run's per-dispatch recorder (finding #65): the `folders` site's
    /// rows stream at commit.
    pub log: &'a mut humanify_ag_core::artifact_dump::DispatchLog,
    /// How big one split-namer prompt may get (`--context-tokens`, `--max-tokens`).
    pub namer_budget: SplitNamerBudget,
    /// The rendered-prompt window (finding #65).
    pub prompt_window: usize,
    /// The naming stage's input text (the beautified, pre-rename runtime) —
    /// the vendor bridge's (finding #60) raw-name anchor. None when no
    /// naming outcome exists.
    pub fresh: Option<&'a str>,
    /// The factories the unpack kept as the app's own CommonJS modules
    /// (exp172), by raw name — the split groups them with the app.
    pub app_modules: &'a [String],
    /// The run's ORIGINAL input bundle — the text the unpack stage saw,
    /// before its vendor extraction spliced the CJS factories out. The
    /// split's ≥50 wrapper-binding gate reads the INPUT (being a bundled
    /// app is a property of the input, not of the post-extraction runtime
    /// the split is handed); the wrapper GRAMMAR still reads the shipped
    /// text.
    pub input_bundle: Option<&'a str>,
    /// The run's toolchain: the finish reads its name profile and
    /// never-rename lists (the post-split reconcile) and its interop
    /// helpers (the relink).
    pub toolchain: humanify_ag_core::toolchain::Toolchain,
    /// The vendor record THIS run's unpack adapter wrote (None when the
    /// adapter writes none, or fell back to a single file). The split's
    /// vendor captures and the finish's re-link read it — never whatever
    /// record happens to sit in the output folder (toolchain review R6).
    pub vendor_record: Option<&'a humanify_ag_core::modules::vendor_names::BunModulesManifest>,
    /// The names mode's placed modules (library-names plan step 4,
    /// exp156): `(module index, vendor path)` from the naming stage's
    /// layout — the fossil split pre-fills those paths. Empty in every
    /// other mode.
    pub library: &'a [(usize, String)],
}

/// `loadPriorSplitLedger`: the ledger the shell read (`--split-ledger`
/// wins, else the ledger beside `--prior-version`), settled onto this
/// binary's statement hashes.
fn load_prior_split_ledger(
    input: &mut SplitStageInput<'_>,
    renderer: &mut dyn ProgressRenderer,
) -> Option<StableSplitLedger> {
    let prior = input.ledger?;
    let mut ledger = prior.ledger.clone();
    renderer.message(&format!(
        "Split ledger: inheriting assignments from {}",
        prior.path.display()
    ));
    // A stale-era ledger's statement hashes are not this binary's (an older
    // `hashVersion`): re-key them from the
    // prior text they were written from, or refuse them LOUDLY (WP5.6e) —
    // never a silent mis-join.
    renderer.message(
        &settle_prior_hashes(
            &mut ledger,
            input.prior.map(|t| t.code.as_str()),
            input.toolchain.layout.piece,
        )
        .describe(),
    );
    Some(ledger)
}

/// `removeConsumedSourceFile`: never outside the output dir, never the
/// run's own input.
fn remove_consumed_source_file(
    tree: &mut OutputTree,
    source: &Path,
    input_file: &Path,
    working_dir: &Path,
) {
    let abs = |p: &Path| humanify_ag_model::js::node_path_resolve(working_dir, p);
    let resolved = abs(source);
    if resolved == abs(input_file) {
        return;
    }
    if resolved.starts_with(abs(tree.root())) {
        tree.remove(source);
    }
}

fn placement_switches(s: &SwitchState) -> PlacementSwitches {
    PlacementSwitches {
        content_anchor: s.switch_on(Switch::ContentAnchor),
        anchor_preempt: s.switch_on(Switch::AnchorPreempt),
        anchor_nearident: s.switch_on(Switch::AnchorNearIdent),
        allsame_vote: s.switch_on(Switch::AllSameVote),
        empty_decl_hash_guard: s.switch_on(Switch::EmptyDeclHashGuard),
    }
}

/// The tiers' carry from the naming stage's (`renameResult.priorCarry`).
fn tiers_carry(carry: Option<&PriorCarry>) -> Option<humanify_ag_core::place::tiers::PriorCarry> {
    carry.map(|c| humanify_ag_core::place::tiers::PriorCarry {
        statement_texts: c.matcher.statement_texts.clone(),
        match_map: c.match_map.iter().cloned().collect(),
    })
}

/// How the split ended for the caller: complete, a post-commit step
/// failed (the tree is still written), or the bundle's layout is not one the
/// split reads — the named output was written unsplit (a WARNING, never
/// an error after naming: finding C3). A pre-commit failure is an Err.
pub enum SplitEnded {
    Complete,
    TreeWrittenPostFailure,
    NotSplit,
}

/// What the split hands the run's recorders (`--diagnostics`,
/// `--dump-artifacts`): how it ended, the placement trail, and the dump's
/// split-era sections.
pub struct SplitRecords {
    pub ended: SplitEnded,
    /// The per-statement placement trail (`placementTrail`).
    pub trail: PlacementTrail,
    /// The dump's split-era sections (None: not split).
    pub dump: Option<SplitSections>,
    /// The split method and why (`--stats-json`'s `splitMethod`).
    pub method: SplitMethodStats,
    /// What the finishing stage's passes recorded into the run's reports:
    /// the post-split reconcile's trail rows (per split file) and the
    /// claims of it and the bundle carry.
    pub post_split: PostSplitRecords,
    /// The split's LLM calls that failed after every retry: (file/folder
    /// namer, tree reviser) — the run's `providerErrors`.
    pub failed_calls: SplitFailedCalls,
    /// Why the runnable emit declined (the review tree was written in
    /// its place); the run's closing report fails the run on it.
    pub declined: Option<String>,
}

/// The split namer's and the tree reviser's failed calls.
#[derive(Clone, Copy, Debug, Default)]
pub struct SplitFailedCalls {
    pub namer: usize,
    pub tree_reviser: usize,
}

/// The finishing passes' contribution to the run-wide recorders.
#[derive(Default)]
pub struct PostSplitRecords {
    pub trail: Vec<humanify_ag_core::naming::report::diagnostics::ExtraText>,
    pub claims: humanify_ag_core::rename::validated::RenameClaimStats,
}

fn split_dump(code: &str, outcome: &SplitOutcome, trail: &PlacementTrail) -> SplitSections {
    let hashes: Vec<String> = outcome.ledger.hashes.clone();
    let files: Vec<serde_json::Value> = outcome
        .layout
        .iter()
        .map(|(path, statements)| serde_json::json!({"path": path, "statements": statements}))
        .collect();
    let aliases: Vec<serde_json::Value> = outcome
        .aliases
        .iter()
        .map(|(importer, module, alias)| {
            serde_json::json!({"importer": importer, "module": module, "alias": alias})
        })
        .collect();
    SplitSections {
        statement_family: outcome.spans.iter().copied().zip(hashes).collect(),
        placement: serde_json::to_value(&trail.rows).expect("the placement rows serialize"),
        shipped: code.to_string(),
        emit: serde_json::json!({"files": files, "aliases": aliases}),
    }
}

/// The split failed before any tree was written.
#[derive(Debug, thiserror::Error)]
#[error("stable split failed before any tree was written: {0}")]
pub struct SplitFailed(pub SplitError);

/// `runSplit` over the naming stage's shipped text.
pub fn run_split(
    code: &str,
    prior_carry: Option<&PriorCarry>,
    input: &mut SplitStageInput<'_>,
    tree: &mut OutputTree,
    renderer: &mut dyn ProgressRenderer,
) -> Result<SplitRecords, SplitFailed> {
    // The layout gate on the run's ORIGINAL input, once: a bundle whose
    // layout the split does not read finishes UNSPLIT (finding C3).
    let gate = match input.input_bundle {
        Some(original) => match input
            .toolchain
            .layout
            .piece
            .original_bundle_binding_count(original)
        {
            Ok(count) => InputGate::Settled(count),
            Err(why) => {
                return Ok(finish_unsplit(
                    code,
                    input,
                    tree,
                    &why.to_string(),
                    renderer,
                ));
            }
        },
        None => InputGate::TextAtHand,
    };
    let mut trail = PlacementTrail::default();
    let ph = humanify_ag_core::profiling::phase("split:compute");
    let (outcome, prior_present, failed_calls) =
        split_before_commit(code, prior_carry, input, gate, &mut trail, renderer)
            .map_err(SplitFailed)?;
    drop(ph);
    let mut post_split = PostSplitRecords::default();
    let ended = match commit_and_finish(
        code,
        prior_carry,
        &outcome,
        prior_present,
        input,
        tree,
        &mut post_split,
        renderer,
    ) {
        Ok(()) => SplitEnded::Complete,
        Err(PostCommit(e)) => {
            renderer.message(&format!(
                "Post-split step failed ({e}); the split tree is already written"
            ));
            SplitEnded::TreeWrittenPostFailure
        }
    };
    match ended {
        SplitEnded::Complete => renderer.message(&format!(
            "Split complete: written to {}",
            tree.root().display()
        )),
        SplitEnded::TreeWrittenPostFailure => {
            renderer.message("Split tree already written; a post-split step failed after commit")
        }
        SplitEnded::NotSplit => {}
    }
    let dump = split_dump(code, &outcome, &trail);
    Ok(SplitRecords {
        declined: outcome.declined.as_ref().map(ToString::to_string),
        ended,
        trail,
        dump: Some(dump),
        method: method_record(&outcome.method),
        post_split,
        failed_calls,
    })
}

/// The bundle's layout is not one the split reads (an ES module's or
/// esbuild `--format=cjs`'s top level, a program under the ≥50-name
/// bundle gate): write the NAMED output unsplit — what a run without
/// `--split` writes — and say so loudly. The naming work is never lost to
/// a layout the split cannot cut (finding C3; docs/plugin-spec.md P9).
fn finish_unsplit(
    code: &str,
    input: &SplitStageInput<'_>,
    tree: &mut OutputTree,
    why: &str,
    renderer: &mut dyn ProgressRenderer,
) -> SplitRecords {
    let written = match input.processed_source {
        Some(path) => {
            tree.put(path.to_path_buf(), code);
            path.display().to_string()
        }
        None => String::from("(no processed file)"),
    };
    renderer.message(&format!(
        "WARNING: --split skipped: {why}. The split reads one bundle layout, a single \
wrapper function holding the program (docs/plugin-spec.md P9, I25); the named output \
is written unsplit to {written}"
    ));
    renderer.message(&format!("Next release: --prior-version {written}"));
    SplitRecords {
        ended: SplitEnded::NotSplit,
        trail: PlacementTrail::default(),
        dump: None,
        method: not_split_record(why),
        post_split: PostSplitRecords::default(),
        failed_calls: SplitFailedCalls::default(),
        declined: None,
    }
}

/// The prior ledger + the split itself (nothing written yet).
fn split_before_commit(
    code: &str,
    prior_carry: Option<&PriorCarry>,
    input: &mut SplitStageInput<'_>,
    gate: InputGate<'_>,
    trail: &mut PlacementTrail,
    renderer: &mut dyn ProgressRenderer,
) -> Result<(SplitOutcome, bool, SplitFailedCalls), SplitError> {
    let ph = humanify_ag_core::profiling::phase("split:load-prior-ledger");
    let prior = load_prior_split_ledger(input, renderer);
    drop(ph);
    // The fresh grouping LLM-names its folders/files; the module markers
    // name files by the module names the naming stage gave the wrappers
    // (docs/design/module-naming.md); inherited layout is never renamed.
    let mut namer = ProviderSplitNamer::with_budget(input.provider, input.log, input.namer_budget);
    namer.window = input.prompt_window;
    // The tree reviser keeps its (single) call and records it AFTER the
    // split — it runs last, so its rows land in dispatch order either way.
    let mut reviser = ProviderTreeReviser::retaining(input.provider);
    let switches = input.switches;
    let vendor_captures = humanify_ag_core::unpack::bun::vendor_captures(input.vendor_record);
    let outcome = stable_split(
        code,
        SplitOptions {
            markers: input.markers,
            prior: prior.as_ref(),
            prior_tree: input.prior.map(|t| &t.files),
            carry: tiers_carry(prior_carry),
            namer: Some(&mut namer as &mut dyn SplitNamer),
            reviser: Some(&mut reviser as &mut dyn TreeReviser),
            placement: placement_switches(switches),
            align: AlignSwitches {
                emit_align_disabled: switches.switch_on(Switch::EmitAlign),
                name_align_disabled: switches.switch_on(Switch::NameAlign),
            },
            registrar_exemption_disabled: switches.switch_on(Switch::RegistrarExemption),
            split_pure: input.split_pure,
            trail: Some(trail),
            vendor_captures: &vendor_captures,
            vendor_fresh: input.fresh,
            input_gate: gate,
            layout: input.toolchain.layout.piece,
            module_wrappers: input.toolchain.module_wrappers.piece,
            library: input.library,
            app_modules: input.app_modules,
        },
    )?;
    renderer.message(&format!(
        "Split method: {} — {}",
        method_name(outcome.method.regime),
        outcome.method.reason
    ));
    report_namer(&namer, renderer);
    report_cycle_moves(&outcome.cycle_moves, renderer);
    let failed_calls = SplitFailedCalls {
        namer: namer.failed_batches,
        tree_reviser: reviser.failed_calls,
    };
    for call in reviser.take_recorded() {
        input
            .log
            .record(&humanify_ag_core::artifact_dump::Dispatch::Plain {
                function_id: "tree-reviser",
                site: "folders",
                call: &call,
            });
    }
    Ok((outcome, prior.is_some(), failed_calls))
}

/// The statements the load-time cycle guard moved so the runnable tree
/// loads (silent when it moved none).
fn report_cycle_moves(moves: &[(usize, String, String)], renderer: &mut dyn ProgressRenderer) {
    if moves.is_empty() {
        return;
    }
    let listed: Vec<String> = moves
        .iter()
        .take(10)
        .map(|(s, from, to)| format!("statement {s} {from} -> {to}"))
        .collect();
    renderer.message(&format!(
        "Load-time cycle guard: moved {} statement(s) so no two files read each other at load: {}{}",
        moves.len(),
        listed.join(", "),
        if moves.len() > 10 { ", ..." } else { "" }
    ));
}

/// The split namer's batch outcome (finding #39: its failures used to be
/// debug-log only, which hid that it had never named a fossil mint).
fn report_namer(namer: &ProviderSplitNamer<'_>, renderer: &mut dyn ProgressRenderer) {
    if namer.calls == 0 {
        return;
    }
    renderer.message(&format!(
        "Split naming: {} prompts, {} failed, {} names proposed",
        namer.calls, namer.failed_batches, namer.proposals
    ));
}

fn write_provenance(
    code: &str,
    outcome: &SplitOutcome,
    input: &SplitStageInput<'_>,
    tree: &mut OutputTree,
) -> Result<(), humanify_ag_core::provenance::ProvenanceError> {
    use humanify_ag_core::provenance::{PROVENANCE_PATH, Sources, provenance_json};
    let (Some(fresh), false) = (input.fresh, outcome.lines.is_empty()) else {
        return Ok(());
    };
    let input_name = input
        .input_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let sources = Sources {
        link: input.provenance.link,
        input_name: &input_name,
        unpack: input.provenance.unpack,
        fresh,
        origins: input.provenance.origins,
        shipped: code,
        spans: &outcome.spans,
    };
    let root = tree.root().to_path_buf();
    let line_count = |path: &str| {
        tree.read(&root.join(path)).map_or(0, |bytes| {
            bytes.iter().filter(|b| **b == b'\n').count() as u32
        })
    };
    let text = provenance_json(&sources, &outcome.lines, &line_count)?;
    tree.put_rel(PROVENANCE_PATH, text);
    Ok(())
}

/// A failure after the commit point (committing into the in-memory tree
/// cannot fail).
struct PostCommit(String);

/// Commit the tree, then finish it (tryStableSplit after the split).
#[allow(clippy::too_many_arguments)]
fn commit_and_finish(
    code: &str,
    prior_carry: Option<&PriorCarry>,
    outcome: &SplitOutcome,
    prior_present: bool,
    input: &mut SplitStageInput<'_>,
    tree: &mut OutputTree,
    post_split: &mut PostSplitRecords,
    renderer: &mut dyn ProgressRenderer,
) -> Result<(), PostCommit> {
    let ph = humanify_ag_core::profiling::phase("split:write-tree");
    if let Some(source) = input.processed_source {
        remove_consumed_source_file(tree, source, input.input_file, input.working_dir);
    }
    for (path, content) in &outcome.files {
        tree.put_rel(path, content.as_str());
    }
    tree.put_rel(SPLIT_LEDGER_PATH, outcome.ledger.to_json());
    if debug_enabled()
        && let Some(carry) = prior_carry.filter(|c| !c.match_map.is_empty())
    {
        let map = OrderedMap(carry.match_map.clone());
        tree.put_rel(
            ".humanify/prior-match-map.json",
            serde_json::to_string(&map).expect("a name map serializes"),
        );
    }
    if let Some(carry) = prior_carry {
        tree.put_rel(
            humanify_ag_core::matching::decisions::MATCH_DECISIONS_PATH,
            serde_json::to_string(&carry.decisions).expect("match decisions serialize"),
        );
    }
    tree.put_rel(
        humanify_ag_core::place::layout::HUMANIFIED_SOURCE_PATH,
        code,
    );
    tree.put_rel(
        STAGE_HASHES_PATH,
        stage_hashes_text(&StageHashes {
            after_naming: stage_fingerprint(code),
            after_placement: stage_fingerprint(&outcome.ledger.to_json()),
        }),
    );

    // -- committed: the tree, ledger and source are in the tree ---------
    let finish_switches = FinishSwitches {
        vendor_inherit_disabled: input.switches.switch_on(Switch::VendorInherit),
        post_split_reconcile_disabled: input.switches.switch_on(Switch::PostSplitReconcile),
    };
    let finish_input = FinishInput {
        runnable: outcome.runnable.as_deref(),
        prior: input.prior,
        input_file: input.input_file,
        working_dir: input.working_dir,
        switches: finish_switches,
        // Finding #60: the vendor captures the split resolved to owner
        // files + live accessor names, for the finish's vendor bridge.
        bridges: outcome.vendor_bridges.clone(),
        name_profile: input.toolchain.name_profile.piece,
        never_rename: input.toolchain.never_rename.piece,
        interop: input.toolchain.interop.piece,
        layout: input.toolchain.layout.piece,
        vendor_record: input.vendor_record,
        split_files: outcome.files.iter().map(|(p, _)| p.as_str()).collect(),
        embedded_files: input.embedded_files,
        differ: &humanify_ag_io::diff::compute_normal_diff,
    };
    drop(ph);
    let mut report = FinishReport::default();
    let ph = humanify_ag_core::profiling::phase("split:finish");
    let finished = finish_stage(&finish_input, tree, &mut report);
    drop(ph);
    for m in &report.messages {
        renderer.message(m);
    }
    let (relinked, reconciled) = finished.map_err(|e| PostCommit(e.to_string()))?;
    write_provenance(code, outcome, input, tree).map_err(|e| PostCommit(e.to_string()))?;
    if let Some(names) = report.library_names.take() {
        post_split.trail = names.trail;
    }
    if let Some(r) = reconciled {
        post_split.trail.extend(r.result.trail);
        post_split.claims = r.result.claims;
        if let Some(carry) = &r.carry {
            humanify_ag_core::naming::driver::add_claims(&mut post_split.claims, &carry.claims);
        }
    }
    let stats = &outcome.stats;
    let by_tier = OrderedMap(
        PLACEMENT_TIERS
            .iter()
            .zip(stats.tiers.by_tier)
            .map(|(t, n)| (t.name().to_string(), n as u64))
            .collect(),
    );
    tree.put_rel(
        PLACEMENT_STATS_PATH,
        placement_stats_text(&PlacementStats {
            statements: stats.statements as u64,
            files: stats.files as u64,
            folders: stats.folders as u64,
            inherited: stats.tiers.inherited as u64,
            residue_locality: stats.tiers.residue_locality as u64,
            by_tier,
        }),
    );
    let runnable = if outcome.runnable.is_some() {
        format!(
            " [runnable CJS module graph{}]",
            if relinked { " + Bun re-link" } else { "" }
        )
    } else {
        String::new()
    };
    let tail = if prior_present {
        format!(
            " — inherited {}/{} ({})",
            stats.tiers.inherited,
            stats.statements,
            placement_summary(&stats.tiers)
        )
    } else {
        format!(" (fresh grouping, {} statements)", stats.statements)
    };
    renderer.message(&format!(
        "Stable split: {} file(s) in {} folder(s){runnable}{tail}",
        stats.files, stats.folders
    ));
    // The output FOLDER (exp179): `--prior-version` resolves it to its
    // `.humanify/humanified.js` (crate::config::PRIOR_IN_FOLDER).
    renderer.message(&format!(
        "Next release: --prior-version {}",
        tree.root().display()
    ));
    Ok(())
}
