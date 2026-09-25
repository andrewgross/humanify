//! The WP4.6 gate's verb (`humanify naming <ts-dump> <out>`): run the whole
//! naming stage through [`run_naming`] on a TS dump's texts (warm replay)
//! and write what the TS run records for it — `names.json`,
//! `transfers.json`, `prompts.jsonl` + `cache-keys.jsonl` (the naming and
//! sweep sites), `stats.json` (the naming half of `--stats-json`),
//! `diag.json` (`--diagnostics` minus the split's placement trail),
//! `coverage.txt` (the printed summary) and the texts. Plus the shared
//! writers the `waves` / `passes` verbs use. Migration scaffolding —
//! deleted at phase 6 with the TS core (02 §9).

use std::fs;
use std::path::Path;

use humanify_model::dump::SpanKey;
use humanify_model::js::{JsObject, JsValue, stringify_pretty};
use humanify_model::llm::{BatchRenameRequest, CacheKeyParams, NameProvider, StrMap};
use serde_json::{Value, json};

use super::library::RecordedName;
use super::{NamingConfig, NamingHooks, NamingInput, NamingOutcome, run_naming};
use crate::babel_view::BabelLines;
use crate::libdetect::function_carry::{LibraryClassification, regions_json};
use crate::naming::passes::sweep::SweepDispatch;
use crate::naming::report::diagnostics::{
    AnchorTexts, DiagnosticsInputs, build_diagnostics_report,
};
use crate::naming::waves::dump::cache_params_of;
use crate::naming::waves::processor::{DispatchRecord, NameRecord};
use crate::trail::{Anchor, StrategyTrail};

/// A TS dump's run, as the driver reads it.
pub struct DumpRun {
    pub meta: Value,
    pub fresh: String,
    pub prior: Option<String>,
    pub minified: Option<String>,
    /// The library freeze the TS applied (regions.json `libraryFunctions`,
    /// the classification the Rust consumes while the text is
    /// TS-beautified).
    pub library: Option<LibraryClassification>,
}

/// Read `meta.json`, the texts and `regions.json` (prior absent = a
/// first-version run).
pub fn read_dump_run(dir: &Path) -> Result<DumpRun, String> {
    let meta_text =
        fs::read_to_string(dir.join("meta.json")).map_err(|e| format!("meta.json: {e}"))?;
    let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;
    let text = |name: &str| fs::read_to_string(dir.join("text").join(name)).ok();
    let fresh = text("fresh.js").ok_or("text/fresh.js missing")?;
    Ok(DumpRun {
        prior: text("prior.js"),
        minified: text("minified.js"),
        fresh,
        meta,
        library: LibraryClassification::from_dump_dir(dir)?,
    })
}

/// The run's plugin options (meta.json flags).
pub fn naming_config_of(meta: &Value) -> NamingConfig {
    let flags = &meta["flags"];
    let flag = |k: &str, default: bool| flags[k].as_bool().unwrap_or(default);
    let known = |k: &str| flags[k].as_str().map(str::to_string);
    NamingConfig {
        bundler: known("bundler"),
        minifier: known("minifier"),
        skip_libraries: flag("skipLibraries", true),
        reconcile_prior_diff: flag("reconcilePriorDiff", false),
        naming_floor: flag("namingFloor", false),
        naming_floor_sweep: flag("namingFloorSweep", false),
        source_map: false,
        emit_rename_ledger: false,
        family_permute_disabled: false,
        params: cache_params_of(meta),
    }
}

/// Run the stage over a dump.
pub fn run_dump<P: NameProvider>(
    dir: &Path,
    hooks: &NamingHooks<'_>,
    provider: impl FnOnce(CacheKeyParams) -> P,
) -> Result<(DumpRun, NamingOutcome), String> {
    let run = read_dump_run(dir)?;
    let config = naming_config_of(&run.meta);
    let client = provider(config.params.clone());
    let input = NamingInput {
        fresh: &run.fresh,
        prior: run.prior.as_deref(),
        library: run.library.as_ref(),
    };
    let outcome = run_naming(&input, &config, hooks, &client)?;
    Ok((run, outcome))
}

/// The comment regions library detection (stage 4, run only under
/// `skipLibraries`) hands the naming stage for a single-file input: the
/// default detector's layer 3 — none when the detector selected for the
/// text's unpack adapter is Bun's (it never yields mixed files) or the
/// header layer already made the whole file a library.
fn mixed_file_regions(minified: &str) -> Vec<crate::libdetect::CommentRegion> {
    use crate::libdetect::{LibraryDetector, find_comment_regions, select_library_detector};
    let adapter = crate::unpack::select_adapter(&crate::detect::detect_bundle(minified), None);
    if select_library_detector(adapter.name()) != LibraryDetector::Default {
        return Vec::new();
    }
    let regions = find_comment_regions(minified);
    let header = crate::detect::js_text::js_prefix(minified, 1024);
    if find_comment_regions(header).is_empty() {
        regions
    } else {
        Vec::new()
    }
}

/// What the `naming` verb reports.
#[derive(Clone, Debug, Default)]
pub struct NamingDumpSummary {
    pub dispatches: usize,
    pub misses: usize,
    pub errors: usize,
    pub trail_rows: usize,
    pub names: usize,
    pub reports: usize,
    pub output_valid: bool,
}

/// `humanify naming`: run + write every gate file.
pub fn dump_naming<P: NameProvider>(
    dir: &Path,
    out_dir: &Path,
    hooks: &NamingHooks<'_>,
    provider: impl FnOnce(CacheKeyParams) -> P,
) -> Result<NamingDumpSummary, String> {
    let (run, out) = run_dump(dir, hooks, provider)?;
    fs::create_dir_all(out_dir.join("text")).map_err(|e| format!("mkdir: {e}"))?;
    let params = cache_params_of(&run.meta);
    let write = |name: &str, text: &str| {
        fs::write(out_dir.join(name), text).map_err(|e| format!("write {name}: {e}"))
    };
    let texts = AnchorTexts {
        fresh: &run.fresh,
        generated: out.generated.as_deref(),
        reconciled: out.reconcile.as_ref().and_then(|r| r.code.as_deref()),
        shipped: out.code.as_deref(),
    };
    for (name, text) in [
        ("generated.js", out.generated.as_deref()),
        ("reconciled.js", texts.reconciled),
        (
            "swept.js",
            out.deferred_sweep
                .as_ref()
                .and_then(|(_, s)| s.code.as_deref()),
        ),
        ("final.js", out.code.as_deref()),
    ] {
        if let Some(t) = text {
            write(&format!("text/{name}"), t)?;
        }
    }
    let sweeps: Vec<(Anchor, &SweepDispatch)> = out
        .pre_sweep
        .iter()
        .flat_map(|s| s.dispatches.iter().map(|d| (Anchor::Fresh, d)))
        .chain(
            out.deferred_sweep
                .iter()
                .flat_map(|(a, s)| s.result.dispatches.iter().map(move |d| (*a, d))),
        )
        .collect();
    let (prompts, keys) = dispatch_rows(&out.waves.dispatches, &sweeps, &params);
    write("prompts.jsonl", &prompts)?;
    write("cache-keys.jsonl", &keys)?;
    let rows = out.trail.transfer_rows();
    write(
        "transfers.json",
        &json!({"schemaVersion": 1, "transfers": rows}).to_string(),
    )?;
    let names = names_table(&out.trail, &out.waves.names, &out.library_names, &texts);
    let n_names = names.len();
    write(
        "names.json",
        &json!({"schemaVersion": 1, "names": names}).to_string(),
    )?;
    write("stats.json", &out.eval_stats().to_file_text())?;
    // regions.json: the Rust's own banner regions over the minified text
    // and the library freeze as the naming stage applied it (the Rust
    // records no Bun banner classification).
    let comment_regions = run
        .minified
        .as_deref()
        .filter(|_| naming_config_of(&run.meta).skip_libraries)
        .map(mixed_file_regions)
        .unwrap_or_default();
    write(
        "regions.json",
        &humanify_model::js::stringify(&regions_json(
            &comment_regions,
            &out.library_functions,
            Vec::new(),
        )),
    )?;
    write(
        "outcome.json",
        &json!({"outputValid": out.output_valid}).to_string(),
    )?;
    if let Some(coverage) = &out.coverage {
        let transfer = out.prior.as_ref().map(super::transfer_stats_by_tier);
        let diag = build_diagnostics_report(&DiagnosticsInputs {
            timestamp: iso_now(),
            reports: &out.reports,
            coverage,
            transfer_stats: transfer.as_ref(),
            trail: &out.trail,
            texts,
            contention: &out.processor.contention,
        });
        write("diag.json", &format!("{}\n", stringify_pretty(&diag, 2)))?;
    }
    if let Some(text) = &out.coverage_text {
        write("coverage.txt", text)?;
    }
    let mut hashes = JsObject::new();
    for (sid, h) in &out.fn_hashes {
        hashes.insert(sid.clone(), JsValue::str(h));
    }
    write(
        "function-hashes.json",
        &humanify_model::js::stringify(&JsValue::Object(hashes)),
    )?;
    Ok(NamingDumpSummary {
        dispatches: out.waves.dispatches.len() + sweeps.len(),
        misses: out.misses,
        errors: out.errors,
        trail_rows: rows.len(),
        names: n_names,
        reports: out.reports.len(),
        output_valid: out.output_valid,
    })
}

/// `new Date().toISOString()` (UTC, millisecond precision).
fn iso_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        now.subsec_millis()
    )
}

/// `writeNames`: the trail rows (every anchored text; functionId =
/// `line:col (tier)` in the row's own text) merged with the recorded rows
/// (LLM applies, uniquify, identity, library prefix) — the recorded row
/// wins on a span collision; sorted by span key.
pub fn names_table(
    trail: &StrategyTrail,
    waves: &[NameRecord],
    library: &[RecordedName],
    texts: &AnchorTexts<'_>,
) -> Vec<Value> {
    let lines: Vec<(Anchor, BabelLines<'_>)> = [
        Anchor::Fresh,
        Anchor::Generated,
        Anchor::Reconciled,
        Anchor::Shipped,
    ]
    .into_iter()
    .map(|a| {
        let text = match a {
            Anchor::Fresh => Some(texts.fresh),
            Anchor::Generated => texts.generated,
            Anchor::Reconciled => texts.reconciled,
            Anchor::Shipped => texts.shipped,
        };
        (a, BabelLines::new(text.unwrap_or(texts.fresh)))
    })
    .collect();
    let mut rows: Vec<(SpanKey, Value)> = Vec::new();
    let mut index: std::collections::HashMap<(String, i64, i64), usize> =
        std::collections::HashMap::new();
    let mut put = |key: SpanKey, row: Value| {
        let k = (key.text.clone(), key.start, key.end);
        match index.get(&k) {
            Some(&i) => rows[i].1 = row,
            None => {
                index.insert(k, rows.len());
                rows.push((key, row));
            }
        }
    };
    for e in trail.entries() {
        let Some(final_name) = &e.final_name else {
            continue;
        };
        let lines = &lines
            .iter()
            .find(|(a, _)| *a == e.target.anchor)
            .expect("anchor")
            .1;
        let (line, col) = lines.loc(e.target.decl_span.start);
        let tier = e
            .terminal_by
            .or(e.settled_by)
            .map(|t| t.as_str())
            .unwrap_or("?");
        let key = SpanKey {
            text: e.target.anchor.as_str().to_string(),
            start: i64::from(e.target.decl_span.start),
            end: i64::from(e.target.decl_span.end),
        };
        let row = json!({
            "target": key_json(&key),
            "oldName": e.old_name,
            "newName": final_name,
            "kind": "function",
            "classified": "renamed",
            "functionId": format!("{line}:{col} ({tier})"),
        });
        put(key, row);
    }
    let recorded = waves
        .iter()
        .map(|r| RecordedName {
            span: Some(r.span),
            old_name: r.old_name.clone(),
            new_name: Some(r.new_name.clone()),
            module: r.module,
            classified: "renamed",
            function_id: r.function_id.clone(),
        })
        .chain(library.iter().cloned());
    for r in recorded {
        let key = SpanKey {
            text: "fresh".to_string(),
            start: r.span.map_or(-1, |s| i64::from(s.start)),
            end: r.span.map_or(-1, |s| i64::from(s.end)),
        };
        let row = json!({
            "target": key_json(&key),
            "oldName": r.old_name,
            "newName": r.new_name,
            "kind": if r.module { "module-binding" } else { "function" },
            "classified": r.classified,
            "functionId": r.function_id,
        });
        put(key, row);
    }
    rows.sort_by(|a, b| {
        (a.0.text.as_str(), a.0.start, a.0.end).cmp(&(b.0.text.as_str(), b.0.start, b.0.end))
    });
    rows.into_iter().map(|(_, v)| v).collect()
}

fn key_json(k: &SpanKey) -> Value {
    json!({"text": k.text, "start": k.start, "end": k.end})
}

/// prompts.jsonl + cache-keys.jsonl: the waves' dispatches (`site:
/// "naming"`) then the sweep's (`site: "sweep"`, rounds counted per
/// functionId from 1), seq in that order.
pub fn dispatch_rows(
    waves: &[DispatchRecord],
    sweeps: &[(Anchor, &SweepDispatch)],
    params: &CacheKeyParams,
) -> (String, String) {
    let mut prompts = String::new();
    let mut keys = String::new();
    let mut seq = 0u64;
    let mut push = |prompt: Value, request: &BatchRenameRequest, cache_key: &str| {
        let mut prompt = prompt;
        prompt["seq"] = json!(seq);
        prompts.push_str(&prompt.to_string());
        prompts.push('\n');
        let key = json!({
            "seq": seq,
            "params": params_json(params),
            "request": request_material(request),
            "cacheKey": cache_key,
        });
        keys.push_str(&key.to_string());
        keys.push('\n');
        seq += 1;
    };
    for d in waves {
        let row = json!({
            "seq": 0,
            "functionId": d.function_id,
            "site": "naming",
            "round": d.round,
            "wave": d.wave,
            "isRetry": d.request.is_retry == Some(true),
            "cacheKey": d.cache_key,
            "systemPrompt": d.system_prompt,
            "userPrompt": d.user_prompt,
            "identifiers": d.request.identifiers,
            "targets": d.targets.iter().map(|(sid, s)| json!({
                "sessionId": sid, "start": s.start, "end": s.end, "text": "fresh"
            })).collect::<Vec<_>>(),
        });
        push(row, &d.request, &d.cache_key);
    }
    for (i, (anchor, d)) in sweeps.iter().enumerate() {
        let row = json!({
            "seq": 0,
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
        push(row, &d.request, &d.cache_key);
    }
    (prompts, keys)
}

/// The waves' own names table at the wave boundary (`waves` verb).
pub fn wave_boundary_names(
    trail: &StrategyTrail,
    waves: &[NameRecord],
    library: &[RecordedName],
    fresh: &str,
) -> Vec<Value> {
    names_table(
        trail,
        waves,
        library,
        &AnchorTexts {
            fresh,
            ..AnchorTexts::default()
        },
    )
}

fn str_map(m: &StrMap) -> Value {
    Value::Object(
        m.0.iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect(),
    )
}

/// `cacheKeyMaterialRow`'s request: the typed request flattened, Sets in
/// their actual order, the callee `snippet` DROPPED (the oracle dump's
/// shape — 16-findings #7), undefined fields absent.
fn request_material(r: &BatchRenameRequest) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("code".into(), json!(r.code));
    o.insert("identifiers".into(), json!(r.identifiers));
    o.insert("usedNames".into(), json!(r.used_names));
    o.insert(
        "calleeSignatures".into(),
        Value::Array(
            r.callee_signatures
                .iter()
                .map(|c| json!({"name": c.name, "params": c.params}))
                .collect(),
        ),
    );
    o.insert("callsites".into(), json!(r.callsites));
    if let Some(v) = &r.context_vars {
        o.insert("contextVars".into(), json!(v));
    }
    if let Some(v) = &r.prior_version_code {
        o.insert("priorVersionCode".into(), json!(v));
    }
    if let Some(v) = &r.prior_version_names {
        o.insert("priorVersionNames".into(), json!(v));
    }
    if let Some(v) = &r.prior_name_hints {
        o.insert("priorNameHints".into(), str_map(v));
    }
    if let Some(v) = &r.already_renamed {
        o.insert("alreadyRenamed".into(), str_map(v));
    }
    if let Some(v) = r.is_retry {
        o.insert("isRetry".into(), json!(v));
    }
    if let Some(v) = &r.previous_attempt {
        o.insert("previousAttempt".into(), str_map(v));
    }
    if let Some(f) = &r.failures {
        o.insert(
            "failures".into(),
            json!({
                "duplicates": f.duplicates,
                "invalid": f.invalid,
                "missing": f.missing,
                "unchanged": f.unchanged,
            }),
        );
    }
    if let Some(v) = &r.prompt_body {
        o.insert("promptBody".into(), json!(v));
    }
    if let Some(v) = &r.user_prompt {
        o.insert("userPrompt".into(), json!(v));
    }
    if let Some(v) = &r.system_prompt {
        o.insert("systemPrompt".into(), json!(v));
    }
    Value::Object(o)
}

fn params_json(p: &CacheKeyParams) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("model".into(), json!(p.model));
    if let Some(t) = p.temperature {
        o.insert("temperature".into(), json!(t as i64));
    }
    if let Some(m) = p.max_tokens {
        o.insert("maxTokens".into(), json!(m));
    }
    if let Some(e) = &p.reasoning_effort {
        o.insert("reasoningEffort".into(), json!(e));
    }
    Value::Object(o)
}
