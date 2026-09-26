//! Stages 10-12 as the pipeline runs them — TS unified.ts `runSplit` /
//! `tryStableSplit`: load the prior ledger, the split
//! ([`humanify_core::emit::stable_split`]), then the COMMIT (drop the
//! consumed source, write the tree, the ledger, the `-vv` match map, the
//! humanified bundle, the stage hashes), then the finishing stage
//! ([`humanify_core::finish::driver`]: re-link, `using` desugar, scaffold,
//! post-split reconcile + bundle carry) and the placement stats.
//!
//! Every handoff is an in-process value: the shipped text and the prior
//! carry come from the naming stage, the ledger and the runnable file list
//! from the split. The per-stage verbs (`humanify emit`, `humanify
//! finish`) call the same core owners.

use std::path::Path;

use humanify_core::artifact_dump::SplitSections;
use humanify_core::emit::align::AlignSwitches;
use humanify_core::emit::stable_split::{SplitOptions, SplitOutcome, stable_split};
use humanify_core::finish::driver::{FinishInput, FinishReport, FinishSwitches, finish_stage};
use humanify_core::place::assign::namer::{
    ProviderSplitNamer, ProviderTreeReviser, SplitNamer, SplitNamerBudget, TreeReviser,
};
use humanify_core::place::layout::find_split_ledger_path;
use humanify_core::place::ledger::{StableSplitLedger, read_ledger, settle_prior_hashes};
use humanify_core::place::placement_dump::Regime;
use humanify_core::place::tiers::{PLACEMENT_TIERS, PlacementSwitches, placement_summary};
use humanify_core::place::trail::PlacementTrail;
use humanify_core::rename::transfer::carry::PriorCarry;
use humanify_model::js::{JsObject, JsValue, stringify};
use humanify_model::jsshape::CountMap;
use humanify_model::llm::NameProvider;

use crate::kill_switches::{Switch, SwitchState};
use crate::log::debug_enabled;
use crate::progress::ProgressRenderer;
use crate::writers::{
    PlacementStats, StageHashes, stage_fingerprint, write_placement_stats, write_split_ledger,
    write_stage_hashes,
};

/// What the split stage is given.
pub struct SplitStageInput<'a> {
    pub output_dir: &'a Path,
    /// The run's input bundle (`filename`).
    pub input_file: &'a Path,
    /// The unpacked file the naming stage processed (removed once the tree
    /// supersedes it).
    pub processed_source: Option<&'a Path>,
    pub prior_version: Option<&'a Path>,
    /// `--split-ledger`.
    pub split_ledger: Option<&'a str>,
    pub split_pure: bool,
    /// Decided once at detection (`fossilSplit`).
    pub fossil: bool,
    pub switches: &'a SwitchState,
    pub provider: &'a dyn NameProvider,
    /// How big one split-namer prompt may get (`--context-tokens`, `--max-tokens`).
    pub namer_budget: SplitNamerBudget,
}

/// `loadPriorSplitLedger`: `--split-ledger` wins, else the ledger beside
/// `--prior-version`.
fn load_prior_split_ledger(
    input: &SplitStageInput<'_>,
    renderer: &mut dyn ProgressRenderer,
) -> Result<Option<StableSplitLedger>, String> {
    let discovered = input.prior_version.and_then(find_split_ledger_path);
    let path = match input.split_ledger {
        Some(p) => std::path::PathBuf::from(p),
        None => match discovered {
            Some(p) => p,
            None => return Ok(None),
        },
    };
    let mut ledger = read_ledger(&path)?;
    renderer.message(&format!(
        "Split ledger: inheriting assignments from {}",
        path.display()
    ));
    // A TS-era ledger's statement hashes are not this binary's: re-key them
    // from the prior text they were written from, or refuse them LOUDLY
    // (WP5.6e) — never a silent mis-join.
    let prior_text = input
        .prior_version
        .and_then(|p| std::fs::read_to_string(p).ok());
    renderer.message(&settle_prior_hashes(&mut ledger, prior_text.as_deref()).describe());
    Ok(Some(ledger))
}

/// `removeConsumedSourceFile`: never outside the output dir, never the
/// run's own input.
fn remove_consumed_source_file(output_dir: &Path, source: &Path, input_file: &Path) {
    let abs = |p: &Path| humanify_model::js::node_path_resolve(p);
    let resolved = abs(source);
    if resolved == abs(input_file) {
        return;
    }
    if resolved.starts_with(abs(output_dir)) {
        let _ = std::fs::remove_file(source);
    }
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
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
fn tiers_carry(carry: Option<&PriorCarry>) -> Option<humanify_core::place::tiers::PriorCarry> {
    carry.map(|c| humanify_core::place::tiers::PriorCarry {
        statement_texts: c.matcher.statement_texts.clone(),
        match_map: c.match_map.iter().cloned().collect(),
    })
}

/// How the split ended for the caller: complete, or a post-commit step
/// failed (the tree stays on disk). A pre-commit failure is an Err.
pub enum SplitEnded {
    Complete,
    TreeWrittenPostFailure,
}

/// What the split hands the run's recorders (`--diagnostics`,
/// `--dump-artifacts`): how it ended, the placement trail, and the dump's
/// split-era sections.
pub struct SplitRecords {
    pub ended: SplitEnded,
    /// The per-statement placement trail (`placementTrail`).
    pub trail: PlacementTrail,
    pub dump: SplitSections,
    /// What the finishing stage's passes recorded into the run's reports:
    /// the post-split reconcile's trail rows (per split file) and the
    /// claims of it and the bundle carry.
    pub post_split: PostSplitRecords,
}

/// The finishing passes' contribution to the run-wide recorders.
#[derive(Default)]
pub struct PostSplitRecords {
    pub trail: Vec<(String, Vec<humanify_core::trail::TrailEntry>)>,
    pub claims: humanify_core::rename::validated::RenameClaimStats,
}

fn split_dump(
    code: &str,
    outcome: &SplitOutcome,
    trail: &PlacementTrail,
    prompts: Vec<(&'static str, humanify_model::llm::LlmCall)>,
) -> SplitSections {
    let hashes: Vec<String> = outcome
        .ledger
        .as_object()
        .and_then(|o| o.get("hashes"))
        .and_then(|h| match h {
            JsValue::Array(items) => Some(
                items
                    .iter()
                    .map(|v| v.as_str().unwrap_or_default().to_string())
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    let aliases: std::collections::HashMap<&str, &str> = outcome
        .aliases
        .iter()
        .map(|(f, a)| (f.as_str(), a.as_str()))
        .collect();
    SplitSections {
        statement_family: outcome.spans.iter().copied().zip(hashes).collect(),
        placement: trail.placement_json(),
        shipped: code.to_string(),
        emit: humanify_core::emit::emit_dump::layout_rows(&outcome.layout, &outcome.spans, |p| {
            aliases.get(p).map(|a| a.to_string())
        }),
        prompts,
    }
}

/// `runSplit` over the naming stage's shipped text.
pub fn run_split(
    code: &str,
    prior_carry: Option<&PriorCarry>,
    input: &SplitStageInput<'_>,
    renderer: &mut dyn ProgressRenderer,
) -> Result<SplitRecords, String> {
    let mut trail = PlacementTrail::default();
    let (outcome, prior_present, prompts) =
        split_before_commit(code, prior_carry, input, &mut trail, renderer)
            .map_err(|e| format!("stable split failed before any tree was written: {e}"))?;
    let mut post_split = PostSplitRecords::default();
    let ended = match commit_and_finish(
        code,
        prior_carry,
        &outcome,
        prior_present,
        input,
        &mut post_split,
        renderer,
    ) {
        Ok(()) => SplitEnded::Complete,
        Err(Committed(false, e)) => {
            return Err(format!(
                "stable split failed before any tree was written: {e}"
            ));
        }
        Err(Committed(true, e)) => {
            renderer.message(&format!(
                "Post-split step failed ({e}); the split tree is already written"
            ));
            SplitEnded::TreeWrittenPostFailure
        }
    };
    match ended {
        SplitEnded::Complete => renderer.message(&format!(
            "Split complete: written to {}",
            input.output_dir.display()
        )),
        SplitEnded::TreeWrittenPostFailure => {
            renderer.message("Split tree already written; a post-split step failed after commit")
        }
    }
    let dump = split_dump(code, &outcome, &trail, prompts);
    Ok(SplitRecords {
        ended,
        trail,
        dump,
        post_split,
    })
}

/// The prior ledger + the split itself (nothing written yet).
fn split_before_commit(
    code: &str,
    prior_carry: Option<&PriorCarry>,
    input: &SplitStageInput<'_>,
    trail: &mut PlacementTrail,
    renderer: &mut dyn ProgressRenderer,
) -> Result<(SplitOutcome, bool, NamerCalls), String> {
    let prior = load_prior_split_ledger(input, renderer)?;
    // Fresh release: LLM-named folders/files; warm fossil hops: LLM-named
    // fresh module mints; inherited layout is never renamed.
    let mut namer = ProviderSplitNamer::with_budget(input.provider, input.namer_budget);
    let mut reviser = ProviderTreeReviser::new(input.provider);
    let regime = if input.fossil {
        Regime::Fossil
    } else if prior.is_some() {
        Regime::Tiers
    } else {
        Regime::Cluster
    };
    let (use_namer, use_reviser) = match regime {
        Regime::Fossil => (prior.is_some(), false),
        Regime::Tiers => (false, false),
        Regime::Cluster => (true, true),
    };
    if prior.is_none() {
        renderer.message("Split naming: LLM-naming folders and files");
    } else {
        renderer.message("Split naming: LLM-naming fresh module mints");
    }
    let switches = input.switches;
    let outcome = stable_split(
        code,
        SplitOptions {
            regime,
            prior: prior.as_ref(),
            carry: tiers_carry(prior_carry),
            namer: use_namer.then_some(&mut namer as &mut dyn SplitNamer),
            reviser: use_reviser.then_some(&mut reviser as &mut dyn TreeReviser),
            placement: placement_switches(switches),
            align: AlignSwitches {
                emit_align_disabled: switches.switch_on(Switch::EmitAlign),
                name_align_disabled: switches.switch_on(Switch::NameAlign),
            },
            registrar_exemption_disabled: switches.switch_on(Switch::RegistrarExemption),
            split_pure: input.split_pure,
            trail: Some(trail),
        },
    )?;
    report_namer(&namer, renderer);
    if let Some(reason) = &outcome.declined {
        renderer.message(&format!(
            "Runnable emit declined: {reason} — writing byte-exact review tree instead"
        ));
    }
    // The `folders` prompt site in dispatch order: the file/folder namer's
    // calls, then the tree reviser's one (it runs last).
    let prompts = namer
        .dispatched
        .into_iter()
        .map(|c| ("split-namer", c))
        .chain(reviser.dispatched.into_iter().map(|c| ("tree-reviser", c)))
        .collect();
    Ok((outcome, prior.is_some(), prompts))
}

/// The split namer's batch outcome (finding #39: its failures used to be
/// debug-log only, which hid that it had never named a fossil mint).
fn report_namer(namer: &ProviderSplitNamer<'_>, renderer: &mut dyn ProgressRenderer) {
    if namer.dispatched.is_empty() {
        return;
    }
    renderer.message(&format!(
        "Split naming: {} prompts, {} failed, {} names proposed",
        namer.dispatched.len(),
        namer.failed_batches,
        namer.proposals
    ));
}

/// The split namers' calls with their functionId, dispatch order.
type NamerCalls = Vec<(&'static str, humanify_model::llm::LlmCall)>;

/// A failure after (`true`) or before (`false`) the commit point.
struct Committed(bool, String);

/// Commit the tree, then finish it (tryStableSplit after the split).
fn commit_and_finish(
    code: &str,
    prior_carry: Option<&PriorCarry>,
    outcome: &SplitOutcome,
    prior_present: bool,
    input: &SplitStageInput<'_>,
    post_split: &mut PostSplitRecords,
    renderer: &mut dyn ProgressRenderer,
) -> Result<(), Committed> {
    let before = |e: String| Committed(false, e);
    let after = |e: String| Committed(true, e);
    let out = input.output_dir;
    if let Some(source) = input.processed_source {
        remove_consumed_source_file(out, source, input.input_file);
    }
    for (path, content) in &outcome.files {
        write_file(&out.join(path), content).map_err(before)?;
    }
    write_split_ledger(out, &outcome.ledger).map_err(|e| before(e.to_string()))?;
    if debug_enabled()
        && let Some(carry) = prior_carry.filter(|c| !c.match_map.is_empty())
    {
        let map = JsObject::from_entries(
            carry
                .match_map
                .iter()
                .map(|(k, v)| (k.clone(), JsValue::str(v.as_str())))
                .collect(),
        );
        write_file(
            &out.join(".humanify").join("prior-match-map.json"),
            &stringify(&JsValue::Object(map)),
        )
        .map_err(before)?;
    }
    write_file(
        &out.join(humanify_core::place::layout::HUMANIFIED_SOURCE_PATH),
        code,
    )
    .map_err(before)?;
    write_stage_hashes(
        out,
        &StageHashes {
            after_naming: stage_fingerprint(code),
            after_placement: stage_fingerprint(&stringify(&outcome.ledger)),
        },
    )
    .map_err(|e| before(e.to_string()))?;

    // -- committed: the tree, ledger and source are on disk -------------
    let finish_switches = FinishSwitches {
        vendor_inherit_disabled: input.switches.switch_on(Switch::VendorInherit),
        post_split_reconcile_disabled: input.switches.switch_on(Switch::PostSplitReconcile),
    };
    let finish_input = FinishInput {
        output_dir: out,
        runnable: outcome.runnable.as_deref(),
        prior_version: input.prior_version,
        input_file: input.input_file,
        switches: finish_switches,
    };
    let mut report = FinishReport::default();
    let finished = finish_stage(&finish_input, &mut report);
    for m in &report.messages {
        renderer.message(m);
    }
    let (relinked, reconciled) = finished.map_err(after)?;
    if let Some(r) = reconciled {
        post_split.trail = r.result.trail;
        post_split.claims = r.result.claims;
        if let Some(carry) = &r.carry {
            humanify_core::naming::driver::add_claims(&mut post_split.claims, &carry.claims);
        }
    }
    let stats = &outcome.stats;
    let by_tier = CountMap(
        PLACEMENT_TIERS
            .iter()
            .zip(stats.tiers.by_tier)
            .map(|(t, n)| (t.name().to_string(), n as f64))
            .collect(),
    );
    write_placement_stats(
        out,
        &PlacementStats {
            statements: stats.statements as f64,
            files: stats.files as f64,
            folders: stats.folders as f64,
            inherited: stats.tiers.inherited as f64,
            residue_locality: stats.tiers.residue_locality as f64,
            by_tier,
        },
    )
    .map_err(|e| after(e.to_string()))?;
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
    renderer.message(&format!(
        "Next release: --prior-version {}",
        out.join(humanify_core::place::layout::HUMANIFIED_SOURCE_PATH)
            .display()
    ));
    Ok(())
}
