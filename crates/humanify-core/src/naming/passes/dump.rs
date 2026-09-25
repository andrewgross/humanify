//! The WP4.4+4.5 gate's dump (`humanify passes`): run the match stage,
//! the transfer stage and the LLM waves (warm replay, as `humanify waves`),
//! then every post-wave pass in plugin order, and write what the TS probe
//! (test/parity/wp445-pass-probe.ts) and the oracle dump record:
//!
//! - `passes.json` — each pass's decisions and the sha256 of the text after
//!   it (the probe's schema, plus the floor/generate rows the oracle's
//!   transfers.json and meta.json carry);
//! - `transfers.json` — the FINAL strategy trail (waves + floor + reconcile
//!   + sweep rows, each span-keyed in its own anchored text);
//! - `prompts-sweep.jsonl` — the sweep's dispatches (the oracle's
//!   `site: "sweep"` rows);
//! - `text/{generated,reconciled,swept,shipped}.js`.
//!
//! `ts_inputs` makes each post-generate pass read the TS's own input text
//! for it (the oracle's generated.js, the probe's reconciled/swept texts)
//! instead of the Rust chain's — the per-pass bisection mode. Migration
//! scaffolding — deleted at phase 6 with the TS core (02 §9).

use std::fs;
use std::path::{Path, PathBuf};

use humanify_model::llm::{CacheKeyParams, NameProvider};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::family_permute::{FamilyPermuteOutcome, PermutePlant, run_family_permute};
use super::floor_passes::{derive_expression_inner_names, retry_decorated_names};
use super::sweep::{DeferredSweepOutcome, SweepDispatch, run_deferred_sweep};
use super::{MintedCensus, census_of_text};
use crate::matching::matches_dump::read_dump_texts;
use crate::modules::soundness::collect_eval_with_taint;
use crate::naming::reconcile::step::{PriorDiffOutcome, run_prior_diff_reconciliation};
use crate::naming::reconcile::{ReconcilePlant, ReconcileResult};
use crate::naming::waves::dump::{StageNaming, cache_params_of, run_stage_waves};
use crate::naming::waves::generate::TextView;
use crate::naming::waves::graph_ext::build_naming_graph;
use crate::naming::waves::render::{FnPrinter, private_rename_edits, render_program_with};
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::rename::eligibility::Eligibility;
use crate::trail::{Anchor, StrategyTrail};

/// A planted bug (the gate's red runs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PassPlant {
    /// Route the first descriptive reconcile survivor as asymmetric.
    ReconcileFlipTier,
    /// Take equal-support permute candidates in reverse fresh order.
    PermuteReverse,
}

/// What the verb does.
#[derive(Clone, Debug, Default)]
pub struct PassesDumpOptions {
    /// The cache the waves and the sweep replay (opened read-only).
    pub llm_cache: Option<PathBuf>,
    /// The TS probe's output dir: feed each post-generate pass the TS's
    /// input text for it (bisection).
    pub ts_inputs: Option<PathBuf>,
    pub plant: Option<PassPlant>,
}

/// What the verb reports.
#[derive(Clone, Debug, Default)]
pub struct PassesDumpSummary {
    pub dispatches: usize,
    pub misses: usize,
    pub errors: usize,
    pub derived: usize,
    pub undecorated: usize,
    pub reconciled: usize,
    pub swept: usize,
    pub permuted: usize,
    pub census_total: usize,
    pub trail_rows: usize,
}

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The naming era's result, carried out of the match stage.
struct NamingEra {
    generated: String,
    trail: StrategyTrail,
    derived: usize,
    undecorated: usize,
    /// class-id skips + decoration skips (the floor's `skipped` so far).
    floor_skipped: usize,
    dispatches: usize,
    misses: usize,
    errors: usize,
}

fn read_text(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok()
}

fn write_text(out_dir: &Path, name: &str, text: &str) -> Result<(), String> {
    fs::write(out_dir.join("text").join(name), text).map_err(|e| format!("write {name}: {e}"))
}

/// Run the stages over a TS dump's texts and write the passes dump.
pub fn dump_passes<P: NameProvider>(
    ts_dump_dir: &Path,
    out_dir: &Path,
    options: &PassesDumpOptions,
    provider: impl FnOnce(CacheKeyParams) -> P,
) -> Result<PassesDumpSummary, String> {
    let (meta, fresh, prior) = read_dump_texts(ts_dump_dir)?;
    let flags = &meta["flags"];
    let bundler = flags["bundler"].as_str();
    let minifier = flags["minifier"].as_str();
    fs::create_dir_all(out_dir.join("text")).map_err(|e| format!("mkdir: {e}"))?;
    let params = cache_params_of(&meta);
    let eligible = Eligibility::new(bundler, minifier);
    let client = provider(params.clone());
    let input = PriorMatchInput {
        fresh: &fresh,
        prior: &prior,
        bundler,
        minifier,
        visit_optional_calls: false,
    };
    let era = match_prior_version(input, |stage| {
        let semantic = stage.fresh.ingest.semantic();
        let view = TextView::build(semantic);
        let ng = build_naming_graph(semantic, stage.fresh.graph, &view);
        let fns = FnPrinter::nodes(semantic, stage.fresh.graph);
        let naming = StageNaming {
            view: &view,
            ng: &ng,
            fns: &fns,
            bundler,
            minifier,
        };
        let (mut waves, private) = run_stage_waves(stage, &naming, &params, None, &client)?;
        let taint = collect_eval_with_taint(semantic);
        let derivation =
            derive_expression_inner_names(semantic, &mut waves.state, &eligible, &taint);
        let decoration = retry_decorated_names(semantic, &mut waves.state, &eligible, &taint);
        let privates = private_rename_edits(semantic.source_text(), &private);
        let generated = render_program_with(semantic, &waves.state, &privates);
        Ok(NamingEra {
            generated,
            derived: derivation.derived,
            undecorated: decoration.undecorated,
            floor_skipped: derivation.skipped.len() + decoration.skipped,
            dispatches: waves.dispatches.len(),
            misses: waves.misses,
            errors: waves.errors,
            trail: waves.state.finish().trail,
        })
    })?;
    write_text(out_dir, "generated.js", &era.generated)?;
    let mut summary = PassesDumpSummary {
        dispatches: era.dispatches,
        misses: era.misses,
        errors: era.errors,
        derived: era.derived,
        undecorated: era.undecorated,
        ..PassesDumpSummary::default()
    };
    let mut passes: Vec<Value> = vec![json!({
        "pass": "generate",
        "textSha": sha(&era.generated),
    })];

    // -- reconcile ----------------------------------------------------------
    let ts_text = |name: &str| {
        options
            .ts_inputs
            .as_ref()
            .and_then(|d| read_text(&d.join("text"), name))
    };
    let generated = match &options.ts_inputs {
        Some(_) => read_text(&ts_dump_dir.join("text"), "generated.js").ok_or("TS generated.js")?,
        None => era.generated.clone(),
    };
    let reconcile_plant =
        (options.plant == Some(PassPlant::ReconcileFlipTier)).then_some(ReconcilePlant::FlipTier);
    let (recon, trail) = match run_prior_diff_reconciliation(
        &generated,
        &prior,
        &eligible,
        era.trail,
        reconcile_plant,
    ) {
        Ok(PriorDiffOutcome {
            result,
            code,
            trail,
        }) => ((Some(result), code), trail),
        Err((e, trail)) => {
            eprintln!("reconcile skipped: {e}");
            ((None, None), trail)
        }
    };
    let (recon_result, recon_code) = recon;
    passes.push(reconcile_row(recon_result.as_ref(), recon_code.as_deref()));
    summary.reconciled = recon_result.as_ref().map_or(0, |r| r.renames.len());
    if let Some(code) = &recon_code {
        write_text(out_dir, "reconciled.js", code)?;
    }

    // -- deferred sweep -----------------------------------------------------
    let sweep_input = match &options.ts_inputs {
        Some(_) => ts_text("reconciled.js").unwrap_or_else(|| generated.clone()),
        None => recon_code.clone().unwrap_or_else(|| generated.clone()),
    };
    let anchor = if recon_code.is_some() {
        Anchor::Reconciled
    } else {
        Anchor::Generated
    };
    let (sweep, trail) =
        match run_deferred_sweep(&sweep_input, anchor, &eligible, &client, &params, trail) {
            Ok(DeferredSweepOutcome { sweep, code, trail }) => (Some((sweep, code)), trail),
            Err((e, trail)) => {
                eprintln!("sweep skipped: {e}");
                (None, trail)
            }
        };
    let swept_code = sweep.as_ref().and_then(|(_, c)| c.clone());
    passes.push(json!({
        "pass": "deferred-sweep",
        "anchor": anchor.as_str(),
        "ran": sweep.is_some(),
        "named": sweep.as_ref().map_or(0, |(s, _)| s.named),
        "skipped": sweep.as_ref().map_or(0, |(s, _)| s.skipped),
        "textSha": swept_code.as_deref().map(sha),
        "misses": sweep.as_ref().map_or(0, |(s, _)| s.misses),
    }));
    if let Some((s, _)) = &sweep {
        summary.swept = s.named;
        summary.misses += s.misses;
        summary.errors += s.errors;
        write_sweep_prompts(out_dir, &s.dispatches, anchor)?;
    }
    if let Some(code) = &swept_code {
        write_text(out_dir, "swept.js", code)?;
    }

    // -- family permute -----------------------------------------------------
    let permute_input = match &options.ts_inputs {
        Some(_) => ts_text("swept.js")
            .or_else(|| ts_text("reconciled.js"))
            .unwrap_or_else(|| generated.clone()),
        None => swept_code
            .clone()
            .or_else(|| recon_code.clone())
            .unwrap_or_else(|| generated.clone()),
    };
    let permute_plant = (options.plant == Some(PassPlant::PermuteReverse))
        .then_some(PermutePlant::TieBreakReversed);
    let permuted = run_family_permute(&permute_input, &prior, &eligible, permute_plant)
        .map_err(|e| eprintln!("family permute skipped: {e}"))
        .ok();
    passes.push(permute_row(permuted.as_ref()));
    let shipped = permuted
        .as_ref()
        .and_then(|p| p.code.clone())
        .unwrap_or(permute_input);
    summary.permuted = permuted.as_ref().map_or(0, |p| p.applied);
    write_text(out_dir, "shipped.js", &shipped)?;

    // -- census -------------------------------------------------------------
    let census = census_of_text(&shipped, &eligible)?;
    summary.census_total = census.total;
    passes.push(json!({
        "pass": "census",
        "census": census_json(&census),
        "textSha": sha(&shipped),
    }));

    // plugin.ts `floorStats`, the deferred sweep folded in
    // (`resolveFinalOutput`): what stats.json's `namingFloor` reports.
    let (swept, sweep_skipped) = sweep.as_ref().map_or((0, 0), |(s, _)| (s.named, s.skipped));
    passes.push(json!({
        "pass": "naming-floor-stats",
        "derived": era.derived,
        "undecorated": era.undecorated,
        "swept": swept,
        "skipped": era.floor_skipped + sweep_skipped,
    }));

    let rows = trail.transfer_rows();
    summary.trail_rows = rows.len();
    fs::write(
        out_dir.join("transfers.json"),
        json!({"schemaVersion": 1, "transfers": rows}).to_string(),
    )
    .map_err(|e| format!("transfers: {e}"))?;
    fs::write(
        out_dir.join("passes.json"),
        json!({"schemaVersion": 1, "passes": passes}).to_string(),
    )
    .map_err(|e| format!("passes: {e}"))?;
    Ok(summary)
}

fn reconcile_row(result: Option<&ReconcileResult>, code: Option<&str>) -> Value {
    let Some(r) = result else {
        return json!({"pass": "reconcile", "ran": false});
    };
    let mut row = json!({
        "pass": "reconcile",
        "priorTooDissimilar": r.prior_too_dissimilar,
        "hunks": {
            "changed": r.hunks.changed,
            "noise": r.hunks.noise,
            "genuine": r.hunks.genuine,
            "oversized": r.hunks.oversized,
            "tainted": r.hunks.tainted,
            "mixed": r.hunks.mixed,
        },
        "renames": r.renames.iter().map(|x| json!({
            "fromName": x.from_name,
            "toName": x.to_name,
            "votes": x.votes,
            "kind": x.kind.as_str(),
            "declLine": x.decl_line,
            "applied": x.applied,
        })).collect::<Vec<_>>(),
        "skipped": r.skipped.iter().map(|x| json!({
            "fromName": x.from_name,
            "toName": x.to_name,
            "reason": x.reason,
            "votes": x.votes,
        })).collect::<Vec<_>>(),
        "textSha": code.map(sha),
    });
    if !r.renames.is_empty() {
        // The TS checks the pure-rename invariant here; it holds by
        // construction in the Rust (see `reconcile::step`).
        row["invariantFailure"] = Value::Null;
    }
    row
}

fn permute_row(p: Option<&FamilyPermuteOutcome>) -> Value {
    json!({
        "pass": "family-permute",
        "ran": p.is_some(),
        "applied": p.map_or(0, |p| p.applied),
        "buckets": p.map_or(0, |p| p.buckets),
        "skipped": p.map_or(0, |p| p.skipped),
        "moves": p.map(|p| p.moves.iter().map(|m| json!({
            "from": m.from,
            "to": m.to,
            "support": m.support,
        })).collect::<Vec<_>>()).unwrap_or_default(),
        "textSha": p.and_then(|p| p.code.as_deref()).map(sha),
    })
}

fn census_json(c: &MintedCensus) -> Value {
    json!({
        "total": c.total,
        "decorated": c.decorated,
        "totalBindings": c.total_bindings,
        "freeReferences": c.free_references,
        "byFamily": {
            "classExprId": c.by_family[0],
            "fnExprId": c.by_family[1],
            "param": c.by_family[2],
            "fnDecl": c.by_family[3],
            "varOther": c.by_family[4],
        },
        "derivableExprIds": c.derivable_expr_ids,
        "zeroRefExprIds": c.zero_ref_expr_ids,
        "names": c.names,
        "decoratedNames": c.decorated_names,
    })
}

/// The sweep's prompt rows (the TS `recordPrompt` shape, `site: "sweep"`,
/// rounds counted per functionId from 1) and cache-key rows.
fn write_sweep_prompts(
    out_dir: &Path,
    dispatches: &[SweepDispatch],
    anchor: Anchor,
) -> Result<(), String> {
    let mut prompts = String::new();
    for (i, d) in dispatches.iter().enumerate() {
        let row = json!({
            "seq": i,
            "functionId": "coverage-sweep",
            "site": "sweep",
            "round": i + 1,
            "isRetry": false,
            "cacheKey": d.cache_key,
            "systemPrompt": d.system_prompt,
            "userPrompt": d.user_prompt,
            "identifiers": d.request.identifiers,
            "targets": d.targets.iter().map(|(name, s)| json!({
                "sessionId": name,
                "start": s.start,
                "end": s.end,
                "text": anchor.as_str(),
            })).collect::<Vec<_>>(),
            "targetsText": anchor.as_str(),
        });
        prompts.push_str(&row.to_string());
        prompts.push('\n');
    }
    fs::write(out_dir.join("prompts-sweep.jsonl"), prompts).map_err(|e| format!("prompts: {e}"))
}
