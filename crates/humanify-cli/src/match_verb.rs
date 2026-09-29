//! `humanify match` — the matching stage (pipeline stage 8) run on its
//! own, as an inspection verb: the same machinery the pipeline drives
//! (`humanify_core::prior::match_prior_version`, the one owner — stages
//! detect → unpack → format → graph → matching, then STOP before naming),
//! with every decision it made written out per function and per
//! statement: matched-to-which, the tier/strategy that resolved it, the
//! close tier's scores, or unmatched.
//!
//! NO LLM path executes — none exists here to reach: matching is fully
//! cold (matches_dump's module doc), and the verb never constructs a
//! provider. Because there is no naming, the PRIOR the verb expects is
//! the prior version's text as the pipeline would read it
//! (`--prior-version`); the ground-truth harness passes the stage-6
//! formatter's output of the old version (the closest no-LLM equivalent
//! of a humanified prior — the matcher reads structure, not names).
//!
//! Deliberate differences from a full pipeline run, both recorded in the
//! dump's `meta.differences`: the verb does not run stage 4 (library
//! detection), so it matches EVERY unpacked file where the pipeline only
//! names the files the library filter kept, and the transfer freeze is
//! empty (a leftover of the naming era that matching does not read).
//!
//! The dump is the ground-truth harness's whole input: each side's
//! function and statement inventories carry their source SLICE, so the
//! harness computes its ground truth from the same rows the matcher
//! decided on and never re-parses JavaScript.

use std::path::Path;

use humanify_model::detection::{
    BundlerType, MinifierType, SELECTABLE_BUNDLERS, SELECTABLE_MINIFIERS,
};
use serde_json::{Value, json};

use crate::pipeline_config::{build_pipeline_config, enum_name};

/// The dump's schema version — bump on a breaking shape change so a
/// harness built against the old shape can refuse loudly.
pub const MATCH_DUMP_SCHEMA_VERSION: u64 = 1;

/// `humanify match`'s inputs (mirrors the pipeline flags it reuses).
pub struct MatchVerbArgs<'a> {
    /// The new version's file — minified or bundled, as the pipeline
    /// takes it.
    pub input: &'a str,
    /// `--prior-version`: the prior release's (humanified) text.
    pub prior_version: &'a str,
    /// `--sequential`: the conservative schedule (the prior side built on
    /// this thread instead of its own). Byte-identical either way.
    pub sequential: bool,
    /// `--bundler <type>` override, as the pipeline's flag.
    pub bundler: Option<&'a str>,
    /// `--minifier <type>` override, as the pipeline's flag.
    pub minifier: Option<&'a str>,
    /// `--webcrack-shim <script>`: required to unpack webpack/browserify
    /// bundles (the same shim `humanify unpack` takes).
    pub webcrack_shim: Option<&'a str>,
    /// Where the unpack adapter writes its tree; default a temp dir that
    /// is removed afterwards.
    pub work_dir: Option<&'a str>,
    /// Keep the work dir even when it was a default temp dir.
    pub keep_work_dir: bool,
}

/// The whole dump: one `files[]` entry per unpacked file the verb
/// matched. Deterministic: inventory rows are in build order and pair
/// rows are sorted by prior span (matches_dump's dump order).
pub fn match_dump(args: &MatchVerbArgs<'_>) -> Result<Value, String> {
    let code = read_text(args.input)?;
    let prior = read_text(args.prior_version)?;
    if humanify_model::js::trim(&prior).is_empty() {
        return Err(format!(
            "--prior-version file is empty: {}",
            args.prior_version
        ));
    }

    // Stages 1-2: detect, then the same config the pipeline builds
    // (the bundler/minifier names feed the fresh side's eligibility).
    let detection = humanify_core::detect::detect_bundle(&code);
    let config = build_pipeline_config(
        &detection,
        parse_override::<BundlerType>(args.bundler, &SELECTABLE_BUNDLERS, "bundler")?,
        parse_override::<MinifierType>(args.minifier, &SELECTABLE_MINIFIERS, "minifier")?,
    );
    let adapter = humanify_core::unpack::select_unpack_adapter(config.unpack_adapter_name)?;
    let bundler = enum_name(config.bundler_type);
    let minifier = enum_name(config.minifier_type);

    // Stage 3: unpack into the work dir (a default temp dir is removed
    // afterwards; the dump embeds everything the harness needs).
    let (work_dir, owned) = match args.work_dir {
        Some(dir) => (dir.to_string(), false),
        None => (
            std::env::temp_dir()
                .join(format!("humanify-match-{}", std::process::id()))
                .display()
                .to_string(),
            true,
        ),
    };
    if owned {
        std::fs::create_dir_all(&work_dir).map_err(|e| format!("mkdir {work_dir}: {e}"))?;
    }
    let shim = args
        .webcrack_shim
        .map(|s| crate::unminify::webcrack_shim(Path::new(s)));
    let result = humanify_core::unpack::run_adapter(
        adapter,
        &code,
        Path::new(&work_dir),
        humanify_core::unpack::bun::BunUnpackOptions::default(),
        shim.as_ref(),
    )?;

    // Stages 6-8 per unpacked file: format, then the match stage, then
    // stop — no naming, no LLM.
    let mut files = Vec::new();
    for file in &result.files {
        if file.path.extension().is_some_and(|e| e != "js") {
            continue;
        }
        // The dump's path is relative to the work dir: the absolute
        // location is not a decision and must not reach the dump (a
        // default temp dir would make two identical runs differ).
        let path = file
            .path
            .strip_prefix(&work_dir)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file.path.display().to_string());
        let text = read_text(&file.path.display().to_string())?;
        if humanify_model::js::trim(&text).is_empty() {
            continue;
        }
        let fresh = humanify_core::format::format_file(
            &text,
            &humanify_core::format::FormatOptions::default(),
            &[],
        )?
        .text;
        let section = humanify_core::prior::match_prior_version(
            humanify_core::prior::PriorMatchInput {
                fresh: &fresh,
                prior: &prior,
                bundler: Some(&bundler),
                minifier: Some(&minifier),
                fast: !args.sequential,
            },
            |stage| {
                let freeze = humanify_core::rename::transfer::library_freeze(stage, None, false)?;
                let twins = humanify_core::rename::transfer::statement_twins(stage, &freeze)?;
                file_section(stage, &twins, &fresh, &prior, &path)
            },
        )?;
        files.push(section);
    }
    if owned && !args.keep_work_dir {
        let _ = std::fs::remove_dir_all(&work_dir);
    }

    Ok(json!({
        "schemaVersion": MATCH_DUMP_SCHEMA_VERSION,
        "tool": "humanify match",
        "meta": {
            "input": args.input,
            "prior": args.prior_version,
            "bundler": bundler,
            "minifier": minifier,
            "adapter": config.unpack_adapter_name,
            "differences": [
                "no library detection: every unpacked file is matched, where the pipeline only names the files the library filter kept",
                "no naming: the transfer freeze is empty; matching does not read it"
            ],
        },
        "files": files,
    }))
}

/// The verb action: build the dump, write it to `out` (pretty) or stdout.
pub fn run_match(args: &MatchVerbArgs<'_>, out: Option<&str>) -> Result<(), String> {
    let dump = match_dump(args)?;
    let text = serde_json::to_string_pretty(&dump).expect("the dump serializes") + "\n";
    match out {
        Some(path) => std::fs::write(path, text).map_err(|e| format!("write {path}: {e}"))?,
        None => {
            use std::io::Write;
            std::io::stdout()
                .write_all(text.as_bytes())
                .map_err(|e| format!("stdout: {e}"))?;
        }
    }
    Ok(())
}

/// A `--bundler`/`--minifier` override: the enum's serialized name, with
/// the valid set listed on a bad name (the verb fails louder than the
/// pipeline, which silently drops an unknown override).
fn parse_override<T>(value: Option<&str>, valid: &[T], what: &str) -> Result<Option<T>, String>
where
    T: serde::de::DeserializeOwned + serde::Serialize + Copy,
{
    let Some(name) = value else {
        return Ok(None);
    };
    if !valid.iter().any(|v| enum_name(*v) == name) {
        let names: Vec<String> = valid.iter().map(|v| enum_name(*v)).collect();
        return Err(format!(
            "unknown {what} \"{name}\" (valid: {})",
            names.join(", ")
        ));
    }
    Ok(serde_json::from_value(json!(name)).ok())
}

fn read_text(path: &str) -> Result<String, String> {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .map_err(|e| format!("cannot read {path}: {e}"))
}

/// Slice a side's text by byte span (oxc spans need no conversion).
fn slice_at(text: &str, span: oxc_span::Span) -> &str {
    &text[span.start as usize..span.end as usize]
}

/// One file's section: the function cascade (both cascades' pairs,
/// unmatched, ambiguous, rejections, the stats bags), the close tier,
/// and the statement twins.
fn file_section(
    stage: &humanify_core::prior::MatchStage<'_, '_>,
    twins: &humanify_core::twins::gates::TwinGateOutput,
    fresh_text: &str,
    prior_text: &str,
    path: &str,
) -> Result<Value, String> {
    Ok(json!({
        "path": path,
        "freshText": fresh_text,
        "functions": functions_section(stage, fresh_text, prior_text),
        "close": stage
            .close_file
            .map(|f| serde_json::to_value(f).expect("the close file serializes"))
            .unwrap_or(Value::Null),
        "twins": twins_section(stage, twins, fresh_text, prior_text),
    }))
}

/// The function half: both sides' inventories (with slices), the
/// function cascade's pairs/unmatched/ambiguous/rejections and stats,
/// then the same for the binding cascade.
fn functions_section(
    stage: &humanify_core::prior::MatchStage<'_, '_>,
    fresh_text: &str,
    prior_text: &str,
) -> Value {
    let prior_rows = function_rows(stage.prior.graph, prior_text);
    let fresh_rows = function_rows(stage.fresh.graph, fresh_text);
    let prior_ids = id_index(&prior_rows);
    let fresh_ids = id_index(&fresh_rows);
    let result = stage.function_result;
    let (pairs, unmatched, ambiguous, rejections) = cascade_rows(result, &prior_ids, &fresh_ids);
    let (binding_pairs, binding_unmatched, binding_ambiguous, _) = match stage.binding_result {
        Some(binding) => cascade_rows(binding, &prior_ids, &fresh_ids),
        None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
    };
    json!({
        "prior": prior_rows,
        "fresh": fresh_rows,
        "pairs": pairs,
        "unmatched": unmatched,
        "ambiguous": ambiguous,
        "rejections": rejections,
        "stats": result.resolution_stats.to_ts_value(),
        "bindingPairs": binding_pairs,
        "bindingUnmatched": binding_unmatched,
        "bindingAmbiguous": binding_ambiguous,
        "bindingStats": stage
            .binding_result
            .map(|r| r.resolution_stats.to_ts_value())
            .unwrap_or(Value::Null),
    })
}

/// One side's function inventory in row order, each row with its session
/// id, span, name and source slice (the harness's ground-truth input).
fn function_rows(graph: &humanify_core::graph::UnifiedGraph, text: &str) -> Vec<Value> {
    graph
        .functions
        .iter()
        .map(|f| {
            json!({
                "id": f.session_id,
                "start": f.span.start,
                "end": f.span.end,
                "name": f.name,
                "slice": slice_at(text, f.span),
            })
        })
        .collect()
}

/// id → inventory index.
fn id_index(rows: &[Value]) -> std::collections::HashMap<String, usize> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| (r["id"].as_str().expect("an id").to_string(), i))
        .collect()
}

/// One cascade's decision rows as (pairs, unmatched, ambiguous,
/// rejections), indices into the function inventories. Pairs sort by
/// prior span — matches_dump's dump order; the ids ride along so the
/// harness never depends on the index.
fn cascade_rows(
    result: &humanify_core::matching::cascade::MatchResult,
    prior_ids: &std::collections::HashMap<String, usize>,
    fresh_ids: &std::collections::HashMap<String, usize>,
) -> (Vec<Value>, Vec<Value>, Vec<Value>, Vec<Value>) {
    let mut pairs: Vec<Value> = result
        .pair_resolutions
        .iter()
        .filter_map(|p| {
            Some(json!({
                "prior": prior_ids.get(&p.prior)?,
                "fresh": fresh_ids.get(&p.fresh)?,
                "priorId": p.prior,
                "freshId": p.fresh,
                "tier": p.tier,
            }))
        })
        .collect();
    pairs.sort_by_key(|p| {
        (
            p["prior"].as_u64().unwrap_or(0),
            p["fresh"].as_u64().unwrap_or(0),
        )
    });
    let unmatched: Vec<Value> = result
        .unmatched
        .iter()
        .filter_map(|id| prior_ids.get(id))
        .map(|i| json!(i))
        .collect();
    let ambiguous: Vec<Value> = result
        .ambiguous
        .iter()
        .filter_map(|(id, candidates)| {
            Some(json!({
                "prior": prior_ids.get(id)?,
                "candidates": candidates
                    .iter()
                    .filter_map(|c| fresh_ids.get(c))
                    .collect::<Vec<_>>(),
            }))
        })
        .collect();
    let rejections: Vec<Value> = result
        .pair_rejections
        .iter()
        .filter_map(|r| {
            Some(json!({
                "prior": prior_ids.get(&r.prior)?,
                "kind": r.kind.as_str(),
                "candidates": r
                    .candidates
                    .as_ref()
                    .map(|c| c.iter().filter_map(|x| fresh_ids.get(x)).collect::<Vec<_>>()),
            }))
        })
        .collect();
    (pairs, unmatched, ambiguous, rejections)
}

/// The twins half: both sides' statement inventories (span, hash,
/// slice — the statement-level ground truth) plus the gates' dump rows
/// (`humanify_core::twins::gates::gate_dump`, the one owner of that shape).
fn twins_section(
    stage: &humanify_core::prior::MatchStage<'_, '_>,
    twins: &humanify_core::twins::gates::TwinGateOutput,
    fresh_text: &str,
    prior_text: &str,
) -> Value {
    let statements = |inventory: &humanify_core::twins::SideInventory, text: &str| -> Vec<Value> {
        inventory
            .statements
            .iter()
            .map(|s| {
                json!({
                    "start": s.span.start,
                    "end": s.span.end,
                    "hash": s.hash,
                    "slice": slice_at(text, s.span),
                })
            })
            .collect()
    };
    json!({
        "prior": statements(stage.prior.inventory, prior_text),
        "fresh": statements(stage.fresh.inventory, fresh_text),
        "gates": humanify_core::twins::gates::gate_dump(twins, &stage.prior.gate_side(), &stage.fresh.gate_side()),
    })
}
