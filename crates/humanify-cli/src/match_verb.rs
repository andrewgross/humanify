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
//! One raw-tree question the pipeline never faces (its library filter
//! skips `vendor/`, so its formatter never parses a bare factory body):
//! a Bun bundle's vendor files are the RAW CJS-factory expressions the
//! unpack extracted — `function (…) {…}` at statement level, which does
//! not parse standalone ("Expected function name"). The verb wraps every
//! vendor file the way the split ships it (`finish::relink`, the one
//! owner of that wrap: `exports.f = __commonJS(F)` with the interop
//! helpers bound from the shim), so the dump describes the same matching
//! surface a humanified prior tree carries — every wrapped file is
//! listed in `meta.differences` (and `meta.wrappedFactoryFiles`); a
//! vendor file that still fails to parse is skipped and reported in
//! `meta.skippedFiles`, never silently dropped.
//!
//! A raw bundle's work dir is also MULTI-file (a runtime plus hundreds
//! of vendor files), and two things that are right for a single file
//! are wrong at that granularity: the prior inventories are hoisted to
//! the dump's top level ONCE instead of per section
//! ([`MATCH_DUMP_SCHEMA_VERSION_MULTI_FILE`]), and the same-program
//! sanity check runs once over the union of every file's pairs — a
//! per-call check compares one vendor file's sliver against the whole
//! prior tree and would reject even a perfectly matched one (the
//! pipeline never hits this: the library filter hands it the whole
//! program's files).
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

/// The dump's schema version for the SINGLE-FILE shape (every exp092
/// corpus dump): each `files[]` section embeds the full prior
/// inventories next to its own. Bump on a breaking shape change so a
/// harness built against the old shape can refuse loudly.
pub const MATCH_DUMP_SCHEMA_VERSION: u64 = 1;

/// The MULTI-FILE shape (a raw bundle's work dir: a runtime plus its
/// vendor files). The prior side is byte-identical in every per-file
/// call (the same prior text, the same parse), so it is hoisted to the
/// dump's top level ONCE (`prior.functions` / `prior.statements`); the
/// per-file sections carry only their own fresh side and the decisions
/// (whose prior indices reference the shared inventory). Repeating the
/// prior inventories per section would embed ~64k rows per vendor file
/// — hundreds of GB on the walk corpus.
pub const MATCH_DUMP_SCHEMA_VERSION_MULTI_FILE: u64 = 2;

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

    // A MULTI-file tree (>= 2 candidates) is the raw-bundle regime: the
    // prior side is hoisted to the dump's top level once, and the
    // same-program check runs over the union of every file's pairs
    // instead of within each call.
    let candidates = collect_candidates(&result, &work_dir)?;
    let multi = candidates.len() > 1;

    // Stages 6-8 per candidate: format, then the match stage, then stop —
    // no naming, no LLM.
    let ctx = MatchContext {
        args,
        prior: &prior,
        bundler: &bundler,
        minifier: &minifier,
        multi,
    };
    let mut run = MatchRun::default();
    for (path, text) in &candidates {
        run.add_file(&ctx, path, text)?;
    }
    if owned && !args.keep_work_dir {
        let _ = std::fs::remove_dir_all(&work_dir);
    }
    let prior_block = multi.then(|| run.hoisted_prior_block()).transpose()?;
    let meta = dump_meta(
        &run,
        args,
        &bundler,
        &minifier,
        config.unpack_adapter_name,
        multi,
    );
    let mut dump = json!({
        "schemaVersion": if multi {
            MATCH_DUMP_SCHEMA_VERSION_MULTI_FILE
        } else {
            MATCH_DUMP_SCHEMA_VERSION
        },
        "tool": "humanify match",
        "meta": meta,
        "files": run.files,
    });
    if let Some(block) = prior_block {
        dump["prior"] = block;
    }
    Ok(dump)
}

/// The unpack tree's candidate files (js-extension, non-empty), each with
/// its dump-relative path (the absolute location is not a decision and
/// must not reach the dump: a default temp dir would make two identical
/// runs differ).
fn collect_candidates(
    result: &humanify_core::unpack::UnpackResult,
    work_dir: &str,
) -> Result<Vec<(String, String)>, String> {
    let mut candidates = Vec::new();
    for file in &result.files {
        if file.path.extension().is_some_and(|e| e != "js") {
            continue;
        }
        let path = file
            .path
            .strip_prefix(work_dir)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file.path.display().to_string());
        let text = read_text(&file.path.display().to_string())?;
        if humanify_model::js::trim(&text).is_empty() {
            continue;
        }
        candidates.push((path, text));
    }
    Ok(candidates)
}

/// The per-file matching loop's accumulation: the sections built, the
/// vendor files wrapped (the raw-bundle regime), the ones skipped with
/// their errors, the hoisted prior block, and the union of every file's
/// matched prior indices (the multi-file same-program check's input).
#[derive(Default)]
struct MatchRun {
    files: Vec<Value>,
    wrapped: Vec<String>,
    skipped: Vec<Value>,
    shared_prior: Option<Value>,
    matched_prior: std::collections::HashSet<u64>,
}

/// What every per-file matching call reads from the run: the verb's
/// flags and the shared prior facts (multi = the raw-bundle regime).
struct MatchContext<'a> {
    args: &'a MatchVerbArgs<'a>,
    prior: &'a str,
    bundler: &'a str,
    minifier: &'a str,
    multi: bool,
}

impl MatchRun {
    /// Stages 6-8 for ONE candidate: format (a RAW bun-bundle tree's
    /// vendor files are bare factory bodies — wrapped the way the split
    /// ships them, the dump's matching surface — with a wrap failure
    /// skipped and reported, never silently dropped), then the match
    /// stage, stopped before naming — no LLM.
    fn add_file(&mut self, ctx: &MatchContext<'_>, path: &str, text: &str) -> Result<(), String> {
        let MatchContext {
            args,
            prior,
            bundler,
            minifier,
            multi,
        } = *ctx;
        let fresh = if path.starts_with("vendor/") {
            match wrapped_factory_text(text, path) {
                Ok(wrapped) => {
                    self.wrapped.push(path.to_string());
                    wrapped
                }
                Err(error) => {
                    self.skipped.push(json!({ "path": path, "error": error }));
                    return Ok(());
                }
            }
        } else {
            humanify_core::format::format_file(
                text,
                &humanify_core::format::FormatOptions::default(),
                &[],
            )?
            .text
        };
        let section = humanify_core::prior::match_prior_version(
            humanify_core::prior::PriorMatchInput {
                fresh: &fresh,
                prior,
                bundler: Some(bundler),
                minifier: Some(minifier),
                fast: !args.sequential,
                same_program_check: !multi,
            },
            |stage| {
                let freeze = humanify_core::rename::transfer::library_freeze(stage, None, false)?;
                let twins = humanify_core::rename::transfer::statement_twins(stage, &freeze)?;
                // The prior side is one owner's deterministic build over
                // the same text in every call: capture it once, hoisted.
                if self.shared_prior.is_none() {
                    self.shared_prior = Some(json!({
                        "functions": function_rows(stage.prior.graph, prior),
                        "statements": statement_rows(stage.prior.inventory, prior),
                    }));
                }
                let section = file_section(stage, &twins, &fresh, prior, path, !multi)?;
                if multi {
                    for pair in section["functions"]["pairs"].as_array().expect("pairs") {
                        self.matched_prior
                            .insert(pair["prior"].as_u64().expect("a prior index"));
                    }
                }
                Ok(section)
            },
        )?;
        self.files.push(section);
        Ok(())
    }

    /// The multi-file shape's hoisted prior block, after the same-program
    /// sanity check at the DUMP's granularity: the union of every file's
    /// pairs against the shared prior inventory — the same owner formula
    /// and message (humanify_core::prior), ONCE instead of per call.
    fn hoisted_prior_block(&self) -> Result<Value, String> {
        let block = self.shared_prior.clone().ok_or_else(|| {
            "every unpacked file failed to parse; no matching ran at all — \
             see the parse errors loading them"
                .to_string()
        })?;
        let prior_count = block["functions"].as_array().map_or(0, |f| f.len());
        humanify_core::prior::assert_prior_looks_like_same_program(
            prior_count,
            prior_count - self.matched_prior.len(),
        )?;
        Ok(block)
    }
}

/// The dump's `meta`: the deliberate deviations from a pipeline run and,
/// whenever the raw-tree handling kicked in, the wrapped/skipped file
/// lists — never silent.
fn dump_meta(
    run: &MatchRun,
    args: &MatchVerbArgs<'_>,
    bundler: &str,
    minifier: &str,
    adapter: &str,
    multi: bool,
) -> Value {
    let mut differences = vec![
        "no library detection: every unpacked file is matched, where the pipeline only names the files the library filter kept".to_string(),
        "no naming: the transfer freeze is empty; matching does not read it".to_string(),
    ];
    if multi {
        differences.push(
            "multi-file raw-bundle dump (schemaVersion 2): the prior side is \
             hoisted to the top level once; per-file sections carry only their \
             own fresh side, with prior indices referencing the shared inventory"
                .to_string(),
        );
    }
    if !run.wrapped.is_empty() {
        differences.push(format!(
            "{} vendor factory file(s) wrapped the way the split ships them \
             (exports.f = __commonJS(body)) — a raw unpack tree's bare factory \
             bodies do not parse standalone; listed in meta.wrappedFactoryFiles",
            run.wrapped.len()
        ));
    }
    if !run.skipped.is_empty() {
        differences.push(format!(
            "{} unpacked vendor file(s) failed to parse even after wrapping and \
             are NOT in the dump; listed in meta.skippedFiles, never silently dropped",
            run.skipped.len()
        ));
    }
    let mut meta = json!({
        "input": args.input,
        "prior": args.prior_version,
        "bundler": bundler,
        "minifier": minifier,
        "adapter": adapter,
        "differences": differences,
    });
    if !run.wrapped.is_empty() {
        meta["wrappedFactoryFiles"] = json!(&run.wrapped);
    }
    if !run.skipped.is_empty() {
        meta["skippedFiles"] = json!(&run.skipped);
    }
    meta
}

/// A RAW bun-bundle work tree's vendor file — the bare CJS-factory body
/// the unpack wrote (`function (…) {…}` or `(…) => {…}` at statement
/// level, which does not parse standalone) — as the shipped tree's text:
/// the split's own wrapping (`finish::relink::wrap_extracted_factory`,
/// the one owner of that shape: `exports.f = __commonJS(F)`, the interop
/// helpers bound from the shim), then formatted. The dump then describes
/// the same matching surface a humanified prior tree carries for its
/// vendor modules.
fn wrapped_factory_text(raw: &str, from_file: &str) -> Result<String, String> {
    let (wrapped, _) = humanify_core::finish::relink::wrap_extracted_factory(
        humanify_model::js::trim(raw),
        from_file,
        &std::collections::BTreeMap::new(),
        &[],
    )?;
    Ok(humanify_core::format::format_file(
        &wrapped,
        &humanify_core::format::FormatOptions::default(),
        &[],
    )?
    .text)
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
/// and the statement twins. With `include_prior` (the single-file shape)
/// the section embeds the prior inventories; the multi-file shape hoists
/// them to the dump's top level and its sections carry only their own
/// side.
fn file_section(
    stage: &humanify_core::prior::MatchStage<'_, '_>,
    twins: &humanify_core::twins::gates::TwinGateOutput,
    fresh_text: &str,
    prior_text: &str,
    path: &str,
    include_prior: bool,
) -> Result<Value, String> {
    Ok(json!({
        "path": path,
        "freshText": fresh_text,
        "functions": functions_section(stage, fresh_text, prior_text, include_prior),
        "close": stage
            .close_file
            .map(|f| serde_json::to_value(f).expect("the close file serializes"))
            .unwrap_or(Value::Null),
        "twins": twins_section(stage, twins, fresh_text, prior_text, include_prior),
    }))
}

/// The function half: both sides' inventories (with slices), the
/// function cascade's pairs/unmatched/ambiguous/rejections and stats,
/// then the same for the binding cascade.
fn functions_section(
    stage: &humanify_core::prior::MatchStage<'_, '_>,
    fresh_text: &str,
    prior_text: &str,
    include_prior: bool,
) -> Value {
    let prior_rows = if include_prior {
        function_rows(stage.prior.graph, prior_text)
    } else {
        Vec::new()
    };
    let fresh_rows = function_rows(stage.fresh.graph, fresh_text);
    let prior_ids = session_positions(stage.prior.graph);
    let fresh_ids = id_index(&fresh_rows);
    let result = stage.function_result;
    let (pairs, unmatched, ambiguous, rejections) = cascade_rows(result, &prior_ids, &fresh_ids);
    let (binding_pairs, binding_unmatched, binding_ambiguous, _) = match stage.binding_result {
        Some(binding) => cascade_rows(binding, &prior_ids, &fresh_ids),
        None => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
    };
    let mut functions = json!({
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
    });
    if !include_prior {
        // The multi-file shape: the prior inventory lives ONCE at the
        // dump's top level, never repeated per section.
        functions
            .as_object_mut()
            .expect("functions is an object")
            .remove("prior");
    }
    functions
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

/// session id → build-order index — the inventory position WITHOUT
/// materializing the rows. `function_rows` numbers its rows in the same
/// graph order, so this is the same map the hoisted (multi-file) prior
/// inventory's indices resolve against.
fn session_positions(
    graph: &humanify_core::graph::UnifiedGraph,
) -> std::collections::HashMap<String, usize> {
    graph
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.session_id.clone(), i))
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
/// (`humanify_core::twins::gates::gate_dump`, the one owner of that
/// shape). The multi-file shape hoists the prior statements to the
/// dump's top level.
fn twins_section(
    stage: &humanify_core::prior::MatchStage<'_, '_>,
    twins: &humanify_core::twins::gates::TwinGateOutput,
    fresh_text: &str,
    prior_text: &str,
    include_prior: bool,
) -> Value {
    let mut twins = json!({
        "fresh": statement_rows(stage.fresh.inventory, fresh_text),
        "gates": humanify_core::twins::gates::gate_dump(twins, &stage.prior.gate_side(), &stage.fresh.gate_side()),
    });
    if include_prior {
        twins["prior"] = json!(statement_rows(stage.prior.inventory, prior_text));
    }
    twins
}

/// One side's statement inventory rows (span, hash, slice).
fn statement_rows(inventory: &humanify_core::twins::SideInventory, text: &str) -> Vec<Value> {
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
}
