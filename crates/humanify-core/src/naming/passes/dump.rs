//! The WP4.4+4.5 gate's dump (`humanify passes`): the naming driver (the
//! one orchestration, `naming::driver`) over a TS dump (warm replay) —
//! every post-wave pass in plugin order — writing what the TS probe
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

use super::MintedCensus;
use super::family_permute::{FamilyPermuteOutcome, PermutePlant};
use super::sweep::SweepDispatch;
use crate::naming::driver::dump::run_dump;
use crate::naming::driver::{NamingHooks, PostPass};
use crate::naming::reconcile::{ReconcilePlant, ReconcileResult};
use crate::trail::Anchor;

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

fn read_text(dir: &Path, name: &str) -> Option<String> {
    fs::read_to_string(dir.join(name)).ok()
}

fn write_text(out_dir: &Path, name: &str, text: &str) -> Result<(), String> {
    fs::write(out_dir.join("text").join(name), text).map_err(|e| format!("write {name}: {e}"))
}

/// Run the naming driver over a TS dump's texts and write the passes dump.
pub fn dump_passes<P: NameProvider>(
    ts_dump_dir: &Path,
    out_dir: &Path,
    options: &PassesDumpOptions,
    provider: impl FnOnce(CacheKeyParams) -> P,
) -> Result<PassesDumpSummary, String> {
    fs::create_dir_all(out_dir.join("text")).map_err(|e| format!("mkdir: {e}"))?;
    // Bisection: each post-generate pass reads the TS's input text for it.
    let ts_text = |name: &str| {
        options
            .ts_inputs
            .as_ref()
            .and_then(|d| read_text(&d.join("text"), name))
    };
    let ts_generated = options
        .ts_inputs
        .as_ref()
        .and_then(|_| read_text(&ts_dump_dir.join("text"), "generated.js"));
    let pass_input = |p: PostPass| -> Option<String> {
        options.ts_inputs.as_ref()?;
        match p {
            PostPass::Reconcile => ts_generated.clone(),
            PostPass::Sweep => ts_text("reconciled.js").or_else(|| ts_generated.clone()),
            PostPass::Permute => ts_text("swept.js")
                .or_else(|| ts_text("reconciled.js"))
                .or_else(|| ts_generated.clone()),
        }
    };
    let hooks = NamingHooks {
        reconcile_plant: (options.plant == Some(PassPlant::ReconcileFlipTier))
            .then_some(ReconcilePlant::FlipTier),
        permute_plant: (options.plant == Some(PassPlant::PermuteReverse))
            .then_some(PermutePlant::TieBreakReversed),
        pass_input: Some(&pass_input),
        ..NamingHooks::default()
    };
    let (_run, out) = run_dump(ts_dump_dir, &hooks, provider)?;
    let generated = out.generated.clone().ok_or("no generated text")?;
    write_text(out_dir, "generated.js", &generated)?;
    let floor = out.floor.unwrap_or_default();
    let mut summary = PassesDumpSummary {
        dispatches: out.waves.dispatches.len(),
        misses: out.misses,
        errors: out.errors,
        derived: floor.derived,
        undecorated: floor.undecorated,
        ..PassesDumpSummary::default()
    };
    let mut passes: Vec<Value> = vec![json!({
        "pass": "generate",
        "textSha": sha(&generated),
    })];

    // -- reconcile ----------------------------------------------------------
    let recon_code = out.reconcile.as_ref().and_then(|r| r.code.clone());
    passes.push(reconcile_row(
        out.reconcile.as_ref().map(|r| &r.result),
        recon_code.as_deref(),
    ));
    summary.reconciled = out.reconcile.as_ref().map_or(0, |r| r.result.renames.len());
    if let Some(code) = &recon_code {
        write_text(out_dir, "reconciled.js", code)?;
    }

    // -- deferred sweep -----------------------------------------------------
    let anchor = if recon_code.is_some() {
        Anchor::Reconciled
    } else {
        Anchor::Generated
    };
    let sweep = out.deferred_sweep.as_ref().map(|(_, s)| s);
    let swept_code = sweep.and_then(|s| s.code.clone());
    passes.push(json!({
        "pass": "deferred-sweep",
        "anchor": anchor.as_str(),
        "ran": sweep.is_some(),
        "named": sweep.map_or(0, |s| s.result.named),
        "skipped": sweep.map_or(0, |s| s.result.skipped),
        "textSha": swept_code.as_deref().map(sha),
        "misses": sweep.map_or(0, |s| s.result.misses),
    }));
    if let Some(s) = sweep {
        summary.swept = s.result.named;
        write_sweep_prompts(out_dir, &s.result.dispatches, anchor)?;
    }
    if let Some(code) = &swept_code {
        write_text(out_dir, "swept.js", code)?;
    }

    // -- family permute -----------------------------------------------------
    passes.push(permute_row(out.permute.as_ref()));
    summary.permuted = out.permute.as_ref().map_or(0, |p| p.applied);
    let shipped = out.code.clone().unwrap_or_default();
    write_text(out_dir, "shipped.js", &shipped)?;

    // -- census -------------------------------------------------------------
    let census = out.census.as_ref().ok_or("no census")?;
    summary.census_total = census.total;
    passes.push(json!({
        "pass": "census",
        "census": census_json(census),
        "textSha": sha(&shipped),
    }));

    // plugin.ts `floorStats`, the deferred sweep folded in
    // (`resolveFinalOutput`): what stats.json's `namingFloor` reports.
    passes.push(json!({
        "pass": "naming-floor-stats",
        "derived": floor.derived,
        "undecorated": floor.undecorated,
        "swept": floor.swept,
        "skipped": floor.skipped,
    }));

    let rows = out.trail.transfer_rows();
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
