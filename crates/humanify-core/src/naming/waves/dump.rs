//! The phase-4 step-1 gate's dump (WP4.3): run parse → graph → matching →
//! the phase-3 transfer stage → the LLM waves over a TS dump's texts, with
//! the LLM answered by WARM REPLAY of a cache, and write what the TS dump
//! writes for the waves — `prompts.jsonl`, `cache-keys.jsonl` and
//! `names.json` — for `humanify-parity compare`. Migration scaffolding —
//! deleted at phase 6 with the TS core (02 §9).
//!
//! `names.json` here is the WAVE-BOUNDARY table: the strategy trail's rows
//! (every tier that settled a binding, through the LLM) merged with the
//! recorded rows (LLM applies, uniquify, identity) — the recorded row wins
//! on a span collision, as the TS writer merges. Post-wave passes
//! (reconcile, floor, sweep — WP4.4/4.5) are not run.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use humanify_model::llm::{BatchRenameRequest, CacheKeyParams, NameProvider, StrMap};
use serde_json::{Value, json};

use crate::matching::matches_dump::read_dump_texts;
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::rename::eligibility::Eligibility;
use crate::rename::transfer::rows::Rows;
use crate::rename::validated::RenameState;
use crate::trail::{Anchor, TrailEntry};

use super::generate::TextView;
use super::graph_ext::build_naming_graph;
use super::probe::graph_probe_lines;
use super::processor::{CloseContext, WaveInputs, WaveOutcome, run_waves};
use super::render::{FnPrinter, Occurrences};

/// What the dump verb does.
#[derive(Clone, Debug, Default)]
pub struct WavesDumpOptions {
    /// Write only the naming graph's bisection probe (graph-probe.jsonl),
    /// over the ORIGINAL names — no transfer, no waves.
    pub probe_graph: bool,
    /// Session ids whose full code/body the probe writes.
    pub probe_only: Vec<String>,
    /// The cache the waves replay (opened read-only).
    pub llm_cache: Option<std::path::PathBuf>,
    /// A planted order bug (the gate's red runs).
    pub plant: Option<super::processor::Plant>,
}

/// What the dump reports.
#[derive(Clone, Debug, Default)]
pub struct WavesDumpSummary {
    pub probe_rows: usize,
    pub dispatches: usize,
    pub names: usize,
    pub misses: usize,
    pub errors: usize,
    pub waves: u64,
}

/// The cache-key params of the dump's run (meta.json flags; temperature is
/// the TS's literal 0; maxTokens is not configured on the oracle runs).
pub fn cache_params_of(meta: &Value) -> CacheKeyParams {
    let flags = &meta["flags"];
    CacheKeyParams {
        model: flags["model"].as_str().unwrap_or_default().to_string(),
        temperature: Some(0.0),
        max_tokens: flags["maxTokens"].as_u64(),
        reasoning_effort: flags["reasoningEffort"].as_str().map(str::to_string),
    }
}

/// Run the stages on a TS dump's texts and write the waves' dump files.
/// `provider` builds the LLM seam from the run's cache-key params (the
/// CLI's replay-only client).
pub fn dump_waves<P: NameProvider>(
    ts_dump_dir: &Path,
    out_dir: &Path,
    options: &WavesDumpOptions,
    provider: impl FnOnce(CacheKeyParams) -> P,
) -> Result<WavesDumpSummary, String> {
    let (meta, fresh, prior) = read_dump_texts(ts_dump_dir)?;
    let flags = &meta["flags"];
    let bundler = flags["bundler"].as_str();
    let minifier = flags["minifier"].as_str();
    let input = PriorMatchInput {
        fresh: &fresh,
        prior: &prior,
        bundler,
        minifier,
        visit_optional_calls: false,
    };
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
    let params = cache_params_of(&meta);
    match_prior_version(input, |stage| {
        let semantic = stage.fresh.ingest.semantic();
        let graph = stage.fresh.graph;
        let view = TextView::build(semantic);
        let ng = build_naming_graph(semantic, graph, &view);
        let fns = FnPrinter::nodes(semantic, graph);
        if options.probe_graph {
            let state = RenameState::new(semantic, Anchor::Fresh);
            let occ = Occurrences::build(semantic, &state);
            let printer = FnPrinter {
                semantic,
                view: &view,
                graph,
                state: &state,
                occ: &occ,
                fns: &fns,
            };
            let lines =
                graph_probe_lines(semantic, graph, &ng, &view, &printer, &options.probe_only);
            let mut text = lines.join("\n");
            text.push('\n');
            fs::write(out_dir.join("graph-probe.jsonl"), text)
                .map_err(|e| format!("write graph-probe.jsonl: {e}"))?;
            return Ok(WavesDumpSummary {
                probe_rows: lines.len(),
                ..WavesDumpSummary::default()
            });
        }
        let client = provider(params.clone());
        let naming = StageNaming {
            view: &view,
            ng: &ng,
            fns: &fns,
            bundler,
            minifier,
        };
        let (waves, _private) = run_stage_waves(stage, &naming, &params, options.plant, &client)?;
        write_dump(out_dir, &view, &waves, &params)?;
        Ok(WavesDumpSummary {
            probe_rows: 0,
            dispatches: waves.dispatches.len(),
            names: waves.names.len(),
            misses: waves.misses,
            errors: waves.errors,
            waves: waves.waves,
        })
    })
}

/// The naming graph's read-only inputs over the fresh text (built once
/// per run: the printer view, the naming graph, the function node handles).
pub struct StageNaming<'a, 's> {
    pub view: &'a TextView<'s>,
    pub ng: &'a super::graph_ext::NamingGraph,
    pub fns: &'a [Option<super::nodes::FnNode>],
    pub bundler: Option<&'a str>,
    pub minifier: Option<&'a str>,
}

/// The phase-3 transfer stage then the LLM waves over one match stage —
/// the naming-era state every post-wave pass (WP4.4/4.5) continues from,
/// with the statement twins' private-name rewrites (the render's).
pub fn run_stage_waves<P: NameProvider>(
    stage: &crate::prior::MatchStage<'_, '_>,
    naming: &StageNaming<'_, '_>,
    params: &CacheKeyParams,
    plant: Option<super::processor::Plant>,
    client: &P,
) -> Result<(WaveOutcome, Vec<crate::twins::gates::PrivateRenameSet>), String> {
    let semantic = stage.fresh.ingest.semantic();
    let graph = stage.fresh.graph;
    let (outcome, _twins) = crate::rename::transfer::apply_prior_version(stage)?;
    let occ = Occurrences::build(semantic, &outcome.rename);
    let rows = Rows::build(graph, semantic, outcome.rename.view());
    let close = close_contexts(stage, &outcome.fn_close_prior)?;
    let eligible = Eligibility::new(naming.bundler, naming.minifier);
    let inputs = WaveInputs {
        semantic,
        graph,
        ng: naming.ng,
        view: naming.view,
        occ: &occ,
        fns: naming.fns,
        rows: &rows,
        eligible: &eligible,
        transferred: &outcome.fn_transferred,
        transferred_pairs: &outcome.fn_transferred_pairs,
        close: &close,
        suggested: &outcome.binding_suggested,
        esbuild: naming.bundler == Some("esbuild"),
        params: params.clone(),
        plant,
    };
    let private_renames = outcome.private_renames;
    Ok((
        run_waves(
            &inputs,
            client,
            outcome.rename,
            outcome.fn_state,
            outcome.binding_state,
        ),
        private_renames,
    ))
}

/// The pending close-matched functions' prior-version context: the prior
/// function's code (`generate(priorFn.path.node)` on the PRIOR text), its
/// placeholder names (unique, first 40), and the pair's folded hints/snaps.
fn close_contexts(
    stage: &crate::prior::MatchStage<'_, '_>,
    close_prior: &[Option<String>],
) -> Result<Vec<Option<CloseContext>>, String> {
    let prior_semantic = stage.prior.ingest.semantic();
    let prior_view = TextView::build(prior_semantic);
    let ident_index = identifier_index(prior_semantic);
    let fn_json = function_json_index(stage.prior.json);
    let prior_fn_by_span: HashMap<(u32, u32), usize> = stage
        .prior
        .graph
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.span.start, f.span.end), i))
        .collect();
    let mut pairs: HashMap<(i64, i64), &humanify_model::dump::ClosePairRow> = HashMap::new();
    if let Some(file) = stage.close_file {
        for p in &file.pairs {
            pairs.insert((p.fresh.start, p.fresh.end), p);
        }
    }
    let fresh_graph = stage.fresh.graph;
    close_prior
        .iter()
        .enumerate()
        .map(|(f, prior_id)| {
            let Some(prior_id) = prior_id else {
                return Ok(None);
            };
            let span = *stage
                .prior
                .spans
                .get(prior_id)
                .ok_or_else(|| format!("close prior {prior_id} has no span"))?;
            let prior_code = prior_view.pretty(span, &[], true);
            let prior_names = match prior_fn_by_span.get(&(span.start, span.end)) {
                Some(&pi) => collect_prior_names(
                    &ident_index,
                    fn_json.get(&(span.start, span.end)).copied(),
                    &stage.prior.graph.functions[pi].placeholder_bindings,
                ),
                None => Vec::new(),
            };
            let fs = fresh_graph.functions[f].span;
            let row = pairs.get(&(i64::from(fs.start), i64::from(fs.end)));
            let hints = row
                .map(|r| {
                    r.hints
                        .iter()
                        .map(|h| (h.new_name.clone(), h.prior_name.clone()))
                        .collect::<Vec<_>>()
                })
                .filter(|h| !h.is_empty())
                .map(StrMap);
            let snaps = row
                .map(|r| {
                    r.snaps
                        .iter()
                        .map(|h| (h.new_name.clone(), h.prior_name.clone()))
                        .collect::<Vec<_>>()
                })
                .filter(|h| !h.is_empty());
            Ok(Some(CloseContext {
                prior_code,
                prior_names,
                hints,
                snaps,
            }))
        })
        .collect()
}

/// Every identifier occurrence of a text, (start, symbol's declaration
/// span, name), sorted by start — the prior names' walk-order index.
type IdentIndex<'n> = Vec<(u32, (u32, u32), oxc_semantic::SymbolId, &'n str)>;

fn identifier_index<'n>(semantic: &'n oxc_semantic::Semantic<'_>) -> IdentIndex<'n> {
    use oxc_ast::AstKind;
    use oxc_span::GetSpan;
    let nodes = semantic.nodes();
    let scoping = semantic.scoping();
    let mut out: IdentIndex<'n> = Vec::new();
    for node in nodes.iter() {
        let (symbol, name, start) = match node.kind() {
            AstKind::IdentifierReference(r) => (
                r.reference_id
                    .get()
                    .and_then(|id| scoping.get_reference(id).symbol_id()),
                r.name.as_str(),
                r.span.start,
            ),
            AstKind::BindingIdentifier(b) => (b.symbol_id.get(), b.name.as_str(), b.span.start),
            _ => continue,
        };
        let Some(symbol) = symbol else { continue };
        let decl = nodes.get_node(scoping.symbol_declaration(symbol)).span();
        out.push((start, (decl.start, decl.end), symbol, name));
    }
    out.sort_by_key(|o| o.0);
    out
}

/// Function-like nodes of a program's ESTree JSON by span (the first in
/// pre-order — the row node, as the graph's own JSON index finds it).
fn function_json_index(root: &Value) -> HashMap<(u32, u32), &Value> {
    const TYPES: [&str; 5] = [
        "FunctionDeclaration",
        "FunctionExpression",
        "ArrowFunctionExpression",
        "MethodDefinition",
        "Property",
    ];
    fn walk<'v>(v: &'v Value, out: &mut HashMap<(u32, u32), &'v Value>) {
        match v {
            Value::Object(fields) => {
                let ty = fields.get("type").and_then(Value::as_str).unwrap_or("");
                if TYPES.contains(&ty)
                    && let (Some(s), Some(e)) = (
                        fields.get("start").and_then(Value::as_u64),
                        fields.get("end").and_then(Value::as_u64),
                    )
                {
                    out.entry((s as u32, e as u32)).or_insert(v);
                }
                for (_, c) in fields {
                    walk(c, out);
                }
            }
            Value::Array(items) => items.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut out = HashMap::new();
    walk(root, &mut out);
    out
}

/// `collectPriorNames(priorFn)`: the placeholder mapping's names in slot
/// order — the TS assigns slots by first occurrence in its `Object.keys`
/// walk of the babel node (babel's PARSED field order: a SwitchCase's
/// `consequent` before its `test`), which the Rust canonical serializer's
/// slot order does not follow (its bytes differ by design, 02 §4a). The
/// walk here follows [`ordered_child_keys`] over the function's ESTree
/// subtree; unique names, at most 40.
fn collect_prior_names(
    index: &IdentIndex<'_>,
    fn_json: Option<&Value>,
    slots: &[(String, oxc_span::Span, String)],
) -> Vec<String> {
    use crate::matching::statement_align::ordered_child_keys;
    let slot_decls: std::collections::HashSet<(u32, u32)> =
        slots.iter().map(|(_, s, _)| (s.start, s.end)).collect();
    let by_start: HashMap<u32, usize> = index.iter().enumerate().map(|(i, o)| (o.0, i)).collect();
    let mut seen_symbols = std::collections::HashSet::new();
    let mut names: Vec<String> = Vec::new();
    let mut stack: Vec<&Value> = fn_json.into_iter().collect();
    // An explicit pre-order walk (children pushed in reverse).
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(map) => {
                let ty = map.get("type").and_then(Value::as_str).unwrap_or("");
                if ty == "Identifier"
                    && let Some(start) = map.get("start").and_then(Value::as_u64)
                    && let Some(&i) = by_start.get(&(start as u32))
                {
                    let (_, decl, symbol, name) = &index[i];
                    if slot_decls.contains(decl)
                        && seen_symbols.insert(*symbol)
                        && !names.iter().any(|n| n == name)
                    {
                        names.push(name.to_string());
                        if names.len() >= 40 {
                            break;
                        }
                    }
                }
                let keys = ordered_child_keys(ty, map);
                for k in keys.into_iter().rev() {
                    if matches!(k, "type" | "start" | "end" | "range" | "loc") {
                        continue;
                    }
                    stack.push(&map[k]);
                }
            }
            Value::Array(items) => {
                for c in items.iter().rev() {
                    stack.push(c);
                }
            }
            _ => {}
        }
    }
    names
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

fn write_dump(
    out_dir: &Path,
    view: &TextView<'_>,
    waves: &WaveOutcome,
    params: &CacheKeyParams,
) -> Result<(), String> {
    let mut prompts = String::new();
    let mut keys = String::new();
    for d in &waves.dispatches {
        let row = json!({
            "seq": d.seq,
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
        prompts.push_str(&row.to_string());
        prompts.push('\n');
        let key = json!({
            "seq": d.seq,
            "params": params_json(params),
            "request": request_material(&d.request),
            "cacheKey": d.cache_key,
        });
        keys.push_str(&key.to_string());
        keys.push('\n');
    }
    fs::write(out_dir.join("prompts.jsonl"), prompts).map_err(|e| format!("prompts: {e}"))?;
    fs::write(out_dir.join("cache-keys.jsonl"), keys).map_err(|e| format!("keys: {e}"))?;

    // transfers.json at the wave boundary (bisection: the llm attempts).
    let rows = waves.state.trail().transfer_rows();
    fs::write(
        out_dir.join("transfers-waves.json"),
        json!({"schemaVersion": 1, "transfers": rows}).to_string(),
    )
    .map_err(|e| format!("transfers: {e}"))?;

    // names.json: trail rows, then recorded rows (the recorded row wins).
    let mut by_span: BTreeMap<(u32, u32), Value> = BTreeMap::new();
    for e in waves.state.trail().entries() {
        if let Some(row) = trail_name_row(view, e) {
            by_span.insert((e.target.decl_span.start, e.target.decl_span.end), row);
        }
    }
    for r in &waves.names {
        by_span.insert(
            (r.span.start, r.span.end),
            json!({
                "target": {"text": "fresh", "start": r.span.start, "end": r.span.end},
                "oldName": r.old_name,
                "newName": r.new_name,
                "kind": if r.module { "module-binding" } else { "function" },
                "classified": "renamed",
                "functionId": r.function_id,
            }),
        );
    }
    let names: Vec<Value> = by_span.into_values().collect();
    let file = json!({"schemaVersion": 1, "names": names});
    fs::write(out_dir.join("names.json"), file.to_string()).map_err(|e| format!("names: {e}"))
}

/// A trail entry's names.json row (`writeNames`' trailRows).
fn trail_name_row(view: &TextView<'_>, e: &TrailEntry) -> Option<Value> {
    let final_name = e.final_name.as_ref()?;
    if e.target.anchor != Anchor::Fresh {
        return None;
    }
    let (line, col) = view.loc_of(e.target.decl_span.start);
    let tier = e
        .terminal_by
        .or(e.settled_by)
        .map(|t| t.as_str())
        .unwrap_or("?");
    Some(json!({
        "target": {"text": "fresh", "start": e.target.decl_span.start, "end": e.target.decl_span.end},
        "oldName": e.old_name,
        "newName": final_name,
        "kind": "function",
        "classified": "renamed",
        "functionId": format!("{line}:{col} ({tier})"),
    }))
}
