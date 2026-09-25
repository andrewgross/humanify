//! The naming ERA of the driver — everything `createRenamePlugin` does on
//! the fresh AST before it is released (plugin.ts, parse → generate):
//! the pre-naming freezes, the prior-version transfer (when a prior is
//! given), the LLM waves, the library-prefix pass, the naming floor (and
//! the pre-generate coverage sweep when it is not deferred), the
//! structural invariant, and `generate`. Two entry points, one body:
//!
//! - [`prior_era`] inside `match_prior_version`'s stage (the phase-3
//!   transfer stage then the waves over the TWO scope epochs the prior
//!   match's cache clear leaves — lesson 18);
//! - [`fresh_era`] for a first version: no match, no transfer, and ONE
//!   scope epoch (`clearBabelCacheAfterPriorMatch` is gated on a prior).
//!
//! Everything the era hands on is OWNED ([`NamingEra`]): the ASTs die with
//! it, as the TS releases the naming-era AST before the post-generate
//! passes re-parse their own texts.

use std::collections::HashMap;

use humanify_model::llm::{CacheKeyParams, NameProvider, StrMap};
use oxc_allocator::Allocator;
use serde_json::Value;

use super::library::{LibraryOutcome, classify_library_functions, run_library_prefix_pass};
use crate::graph::UnifiedGraph;
use crate::matching::cascade::ResolutionStats;
use crate::modules::soundness::collect_eval_with_taint;
use crate::naming::passes::floor_passes::{derive_expression_inner_names, retry_decorated_names};
use crate::naming::passes::sweep::{SweepResult, sweep_minted_names};
use crate::naming::report::ProcessorReport;
use crate::naming::waves::generate::TextView;
use crate::naming::waves::graph_ext::{NamingGraph, build_naming_graph};
use crate::naming::waves::nodes::FnNode;
use crate::naming::waves::processor::{
    CloseContext, DispatchRecord, NameRecord, Plant, WaveInputs, WaveOutcome, run_waves,
};
use crate::naming::waves::render::{
    FnPrinter, Occurrences, private_rename_edits, render_program_with,
};
use crate::prior::MatchStage;
use crate::rename::eligibility::Eligibility;
use crate::rename::transfer::rows::Rows;
use crate::rename::transfer::{PreFreeze, PriorCounts, TransferStats, pre_transfer_states};
use crate::rename::validated::{RenameClaimStats, RenameState};
use crate::trail::{Anchor, StrategyTrail};
use crate::twins::gates::PrivateRenameSet;

/// What the era is asked to do (the plugin options it reads).
pub struct EraOptions<'o> {
    pub bundler: Option<&'o str>,
    pub minifier: Option<&'o str>,
    pub params: &'o CacheKeyParams,
    /// `namingFloor`.
    pub naming_floor: bool,
    /// `namingFloorSweep && !isSweepDeferred`: the sweep runs pre-generate.
    pub pre_generate_sweep: bool,
    /// `skipLibraries` (default true) and the library stage's hook.
    pub skip_libraries: bool,
    pub library: Option<super::library::LibraryHook<'o>>,
    pub wave_plant: Option<Plant>,
    /// Stop after the waves (the `waves` verb's wave-boundary dump).
    pub stop_after_waves: bool,
}

/// The waves' own records (the dump's prompts / keys / names).
#[derive(Default)]
pub struct WaveRecords {
    pub dispatches: Vec<DispatchRecord>,
    pub names: Vec<NameRecord>,
    pub misses: usize,
    pub errors: usize,
    pub waves: u64,
}

/// `applyPriorVersionIfPresent`'s stats (stats.json / the coverage).
#[derive(Clone, Debug, Default)]
pub struct PriorStats {
    pub statement_twin: TransferStats,
    pub exact_match: TransferStats,
    pub close_match: TransferStats,
    pub retry: TransferStats,
    pub counts: PriorCounts,
    pub close_match_stats: humanify_model::dump::CloseStatsRow,
    pub resolution_stats: ResolutionStats,
    /// None when no binding matching ran.
    pub binding_resolution_stats: Option<ResolutionStats>,
}

/// The naming floor's counters so far (plugin.ts `NamingFloorResult`).
#[derive(Clone, Copy, Debug, Default)]
pub struct FloorCounts {
    pub derived: usize,
    pub undecorated: usize,
    pub swept: usize,
    pub skipped: usize,
}

/// What the era hands on.
pub struct NamingEra {
    /// The generated text (None when stopped after the waves).
    pub generated: Option<String>,
    pub trail: StrategyTrail,
    pub claims: RenameClaimStats,
    pub waves: WaveRecords,
    pub processor: ProcessorReport,
    pub library: LibraryOutcome,
    pub floor: Option<FloorCounts>,
    /// The pre-generate coverage sweep (fresh-anchored), when it ran.
    pub pre_sweep: Option<SweepResult>,
    pub prior: Option<PriorStats>,
    pub function_count: usize,
    /// sessionId → the function's structural hash (the report's).
    pub fn_hashes: Vec<(String, String)>,
}

/// The naming graph's read-only inputs over the fresh text.
struct Naming<'a, 's> {
    view: TextView<'s>,
    ng: NamingGraph,
    fns: Vec<Option<FnNode>>,
    graph: &'a UnifiedGraph,
    semantic: &'a oxc_semantic::Semantic<'s>,
}

impl<'a, 's> Naming<'a, 's> {
    fn build(semantic: &'a oxc_semantic::Semantic<'s>, graph: &'a UnifiedGraph) -> Self {
        let view = TextView::build(semantic);
        let ng = build_naming_graph(semantic, graph, &view);
        let fns = FnPrinter::nodes(semantic, graph);
        Naming {
            view,
            ng,
            fns,
            graph,
            semantic,
        }
    }
}

/// The wave-stage state the era runs the waves over (a transfer outcome,
/// or the bare freezes of a first version).
struct WaveStart {
    rename: RenameState,
    fn_state: Vec<crate::rename::transfer::lifecycle::Lifecycle>,
    binding_state: Vec<crate::rename::transfer::lifecycle::Lifecycle>,
    transferred: Vec<std::collections::HashSet<String>>,
    transferred_pairs: Vec<Option<Vec<(String, String)>>>,
    close: Vec<Option<CloseContext>>,
    suggested: Vec<Option<String>>,
    private: Vec<PrivateRenameSet>,
    single_epoch: bool,
}

/// The era over a prior-version match stage.
pub fn prior_era<P: NameProvider>(
    stage: &MatchStage<'_, '_>,
    opts: &EraOptions<'_>,
    provider: &P,
) -> Result<NamingEra, String> {
    let semantic = stage.fresh.ingest.semantic();
    let graph = stage.fresh.graph;
    let wrapper = stage.fresh.wrapper.map(|w| w.span);
    let library = classify_library_functions(
        stage.fresh.ingest.text,
        graph,
        wrapper.is_some(),
        opts.skip_libraries,
        opts.library,
    );
    let freeze = PreFreeze {
        library: library.iter().map(|(f, _)| *f).collect(),
    };
    let (outcome, _twins) = crate::rename::transfer::apply_prior_version_with(stage, &freeze)?;
    let close = close_contexts(stage, &outcome.fn_close_prior)?;
    let prior = PriorStats {
        statement_twin: outcome.stats_twin.clone(),
        exact_match: outcome.stats_exact.clone(),
        close_match: outcome.stats_close.clone(),
        retry: outcome.stats_retry.clone(),
        counts: outcome.counts,
        close_match_stats: stage
            .close_file
            .map(|f| f.stats.clone())
            .unwrap_or_default(),
        resolution_stats: stage.function_result.resolution_stats.clone(),
        binding_resolution_stats: stage.binding_result.map(|r| r.resolution_stats.clone()),
    };
    let start = WaveStart {
        rename: outcome.rename,
        fn_state: outcome.fn_state,
        binding_state: outcome.binding_state,
        transferred: outcome.fn_transferred,
        transferred_pairs: outcome.fn_transferred_pairs,
        close,
        suggested: outcome.binding_suggested,
        private: outcome.private_renames,
        single_epoch: false,
    };
    let naming = Naming::build(semantic, graph);
    Ok(run_era(
        &naming,
        start,
        library,
        Some(prior),
        opts,
        provider,
    ))
}

/// The era of a first version (no prior): parse, graph, the freezes.
pub fn fresh_era<P: NameProvider>(
    fresh: &str,
    opts: &EraOptions<'_>,
    provider: &P,
) -> Result<NamingEra, String> {
    let allocator = Allocator::default();
    let ingest = crate::prior::parse_side(&allocator, fresh, "input.js")?;
    let json = crate::ingest::program_estree_json(ingest.program);
    let parts = crate::prior::build_side_parts(
        &ingest,
        &json,
        "input.js",
        crate::graph::Eligibility::SkipSet {
            bundler: opts.bundler,
            minifier: opts.minifier,
        },
        false,
    );
    let semantic = ingest.semantic();
    let graph = &parts.graph;
    let wrapper =
        crate::modules::wrapper::find_wrapper_function(ingest.program, semantic).map(|w| w.span);
    let library = classify_library_functions(
        fresh,
        graph,
        wrapper.is_some(),
        opts.skip_libraries,
        opts.library,
    );
    let freeze = PreFreeze {
        library: library.iter().map(|(f, _)| *f).collect(),
    };
    let (fn_state, binding_state) = pre_transfer_states(graph, semantic, wrapper, &freeze);
    let n_fns = graph.functions.len();
    let n_bindings = graph.module_bindings.len();
    let start = WaveStart {
        rename: RenameState::new(semantic, Anchor::Fresh),
        fn_state,
        binding_state,
        transferred: vec![Default::default(); n_fns],
        transferred_pairs: vec![None; n_fns],
        close: vec![None; n_fns],
        suggested: vec![None; n_bindings],
        private: Vec::new(),
        single_epoch: true,
    };
    let naming = Naming::build(semantic, graph);
    Ok(run_era(&naming, start, library, None, opts, provider))
}

/// The shared body: waves, library prefix, floor, generate.
fn run_era<P: NameProvider>(
    naming: &Naming<'_, '_>,
    start: WaveStart,
    library: Vec<(usize, String)>,
    prior: Option<PriorStats>,
    opts: &EraOptions<'_>,
    provider: &P,
) -> NamingEra {
    let semantic = naming.semantic;
    let graph = naming.graph;
    let eligible = Eligibility::new(opts.bundler, opts.minifier);
    let occ = Occurrences::build(semantic, &start.rename);
    let rows = Rows::build(graph, semantic, start.rename.view());
    let inputs = WaveInputs {
        semantic,
        graph,
        ng: &naming.ng,
        view: &naming.view,
        occ: &occ,
        fns: &naming.fns,
        rows: &rows,
        eligible: &eligible,
        transferred: &start.transferred,
        transferred_pairs: &start.transferred_pairs,
        close: &start.close,
        suggested: &start.suggested,
        esbuild: opts.bundler == Some("esbuild"),
        params: opts.params.clone(),
        single_epoch: start.single_epoch,
        plant: opts.wave_plant,
    };
    let fn_hashes: Vec<(String, String)> = graph
        .functions
        .iter()
        .map(|f| (f.session_id.clone(), f.structural_hash.clone()))
        .collect();
    let has_nodes = !graph.functions.is_empty() || !graph.module_bindings.is_empty();
    let WaveOutcome {
        mut state,
        dispatches,
        names,
        misses,
        errors,
        waves,
        processor,
        ..
    } = if has_nodes {
        run_waves(
            &inputs,
            provider,
            start.rename,
            start.fn_state,
            start.binding_state,
        )
    } else {
        WaveOutcome::idle(start.rename, start.fn_state, start.binding_state)
    };
    let records = WaveRecords {
        dispatches,
        names,
        misses,
        errors,
        waves,
    };
    let library = run_library_prefix_pass(&mut state, &rows, graph, &library, &eligible);
    let mut era = NamingEra {
        generated: None,
        trail: StrategyTrail::default(),
        claims: RenameClaimStats::default(),
        waves: records,
        processor,
        library,
        floor: None,
        pre_sweep: None,
        prior,
        function_count: graph.functions.len(),
        fn_hashes,
    };
    if !opts.stop_after_waves {
        if opts.naming_floor {
            let taint = collect_eval_with_taint(semantic);
            let derivation = derive_expression_inner_names(semantic, &mut state, &eligible, &taint);
            let decoration = retry_decorated_names(semantic, &mut state, &eligible, &taint);
            let sweep = opts.pre_generate_sweep.then(|| {
                sweep_minted_names(
                    semantic,
                    &mut state,
                    &eligible,
                    &taint,
                    provider,
                    opts.params,
                )
            });
            era.floor = Some(FloorCounts {
                derived: derivation.derived,
                undecorated: decoration.undecorated,
                swept: sweep.as_ref().map_or(0, |s| s.named),
                skipped: derivation.skipped.len()
                    + decoration.skipped
                    + sweep.as_ref().map_or(0, |s| s.skipped),
            });
            era.pre_sweep = sweep;
        }
        let privates = private_rename_edits(semantic.source_text(), &start.private);
        era.generated = Some(render_program_with(semantic, &state, &privates));
    }
    era.claims = state.claim_stats();
    era.trail = state.finish().trail;
    era
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
