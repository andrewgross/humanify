//! Prior-version orchestration helpers (WP2.4) — TS original:
//! `src/prior-version/prior-version.ts`. Only the pipeline-called input
//! contract lives here; the rename-time application surface (exact-match
//! placeholder application, the ambiguity probe, the lifecycle state
//! machine) lands with WP3's rename port.

/// Minimum prior functions before the same-program sanity floor applies
/// (TS `SAME_PROGRAM_FLOOR_MIN_FUNCTIONS` :723).
pub const SAME_PROGRAM_FLOOR_MIN_FUNCTIONS: usize = 50;
/// Minimum fraction of prior functions whose hash exists in the new
/// version (TS `SAME_PROGRAM_PRESENCE_FLOOR` :724).
pub const SAME_PROGRAM_PRESENCE_FLOOR: f64 = 0.05;

/// TS `assertPriorLooksLikeSameProgram` (:732): a prior that shares
/// (nearly) no structural hashes with the new version is a wrong file, not
/// an aggressive refactor — matched AND ambiguous prior functions both
/// count as presence (only the cascade's `unmatched` are absent), so even
/// a version where nothing disambiguates passes. Fails fast instead of
/// letting a full-cost run transfer nothing.
///
/// `prior_function_count` is the prior side's function-row count; the TS
/// passes `priorFnMap.size`. `unmatched` is the FUNCTION cascade result's
/// `unmatched` length.
pub fn assert_prior_looks_like_same_program(
    prior_function_count: usize,
    unmatched: usize,
) -> Result<(), String> {
    if prior_function_count < SAME_PROGRAM_FLOOR_MIN_FUNCTIONS {
        return Ok(());
    }
    let present = prior_function_count.saturating_sub(unmatched);
    let fraction = present as f64 / prior_function_count as f64;
    if fraction < SAME_PROGRAM_PRESENCE_FLOOR {
        return Err(format!(
            "prior version does not appear to be the same program: only {present} of \
             {prior_function_count} prior functions have a matching structural hash in \
             the new version. Check the --prior-version file; drop the flag to run \
             without transfer."
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// matchPriorVersion's orchestration (prior-version.ts :237-447, :532-632)
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use oxc_allocator::Allocator;
use serde_json::{Value, json};

use crate::graph::Eligibility;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::alternation::{self, GraphSide, prepare_binding_matching};
use crate::matching::build_fingerprint_index;
use crate::matching::cascade::{
    MatchOptions, Side, assign_interchangeable_pools, match_functions, resolve_ambiguous_by_ordinal,
};
use crate::matching::statement_context::StatementContexts;

/// The texts and flags the match stage runs on: the FORMATTED fresh text,
/// the prior version's code, the detected bundler/minifier (the fresh
/// side's rename-eligibility skip set).
#[derive(Clone, Copy)]
pub struct PriorMatchInput<'t> {
    pub fresh: &'t str,
    pub prior: &'t str,
    pub bundler: Option<&'t str>,
    pub minifier: Option<&'t str>,
}

/// One side of the match stage, all borrowed from [`with_match_stage`]'s
/// locals: the parse, its program JSON, the symbol tables, the graph and
/// its statement contexts, the session-id spans, the function fingerprint
/// index, the alternation's graph view, the wrapper, and the twins'
/// statement inventory (with the statement values the gates walk).
pub struct StageSide<'a, 's> {
    pub ingest: &'a Ingest<'s>,
    pub json: &'a Value,
    pub tables: &'a SymbolTables,
    pub graph: &'a crate::graph::UnifiedGraph,
    pub ctx: &'a StatementContexts,
    pub spans: &'a HashMap<String, oxc_span::Span>,
    pub index: &'a crate::matching::FingerprintIndex<'a>,
    pub graph_side: &'a GraphSide<'a>,
    pub wrapper: Option<&'a crate::modules::wrapper::WrapperFunction>,
    pub inventory: &'a crate::twins::SideInventory,
    pub inventory_values: &'a [Value],
}

/// Everything the TS `matchPriorVersion` computed before the prior AST
/// dropped, up to (not including) the rename half: both sides, the two
/// cascades' final results (post alternation; the function result after
/// the ordinal + interchangeable tail tiers), the binding cascade's setup,
/// and the close tier's dump file + per-pair contexts (assignment order).
pub struct MatchStage<'a, 's> {
    pub fresh: StageSide<'a, 's>,
    pub prior: StageSide<'a, 's>,
    pub function_result: &'a crate::matching::cascade::MatchResult,
    pub binding_result: Option<&'a crate::matching::cascade::MatchResult>,
    pub binding_setup: Option<&'a crate::matching::alternation::BindingMatchSetup<'a>>,
    pub close_file: Option<&'a humanify_model::dump::MatchesCloseFile>,
    pub close_pairs: &'a [crate::matching::close_dump::ClosePairContext],
    /// The WP2.1 probe's tracing surface (evidence.json), Null when the
    /// alternation built none.
    pub evidence: &'a Value,
}

/// Run the whole cold match stage over a TS dump's two texts and hand the
/// result to `consume` — the matches dump (WP2.x gates) and the transfer
/// dump (WP3.2's gate) share it, so the two cannot drift. The flow mirrors
/// the TS matchAndApplyFunctions (prior-version.ts :524-596): the initial
/// function cascade with propagation, the alternation with the prepared
/// binding setup, the tail tiers on the FUNCTION result only, the close
/// tier, the same-program sanity check.
#[allow(clippy::too_many_lines)]
pub fn match_prior_version<T>(
    input: PriorMatchInput<'_>,
    consume: impl FnOnce(&MatchStage<'_, '_>) -> Result<T, String>,
) -> Result<T, String> {
    let PriorMatchInput {
        fresh,
        prior,
        bundler,
        minifier,
    } = input;

    // ── both sides: parse, then each side's program JSON ONCE ───────────
    // The JSON is shared by every consumer below (the graph's row hashes
    // and features, the statement contexts, the close tier, the twin
    // inventories); the two parses are independent and run concurrently.
    use crate::profiling::phase;
    let ph = phase("prior:parse+json");
    let fresh_allocator = Allocator::default();
    let fresh_ingest = parse_side(&fresh_allocator, fresh, "input.js")?;
    let prior_allocator = Allocator::default();
    let prior_ingest = parse_side(&prior_allocator, prior, "prior.js")?;
    let (fresh_json, prior_json) = program_jsons(&fresh_ingest, &prior_ingest);
    drop(ph);
    let ph = phase("prior:graph-fresh");

    // ── the fresh side (the pipeline's own eligibility) ─────────────────
    let SideParts {
        tables: fresh_tables,
        graph: fresh_graph,
        ctx: fresh_ctx,
        spans: fresh_spans,
    } = build_side_parts(
        &fresh_ingest,
        &fresh_json,
        "input.js",
        Eligibility::SkipSet { bundler, minifier },
    );

    // ── the prior side (ALL bindings eligible — prior-version.ts:284-288) ─
    let SideParts {
        tables: prior_tables,
        graph: prior_graph,
        ctx: prior_ctx,
        spans: prior_spans,
    } = {
        drop(ph);
        let _ph = phase("prior:graph-prior");
        build_side_parts(&prior_ingest, &prior_json, "prior.js", Eligibility::All)
    };
    let ph = phase("prior:index");

    // ── matchAndApplyFunctions (prior-version.ts:524-596) ────────────────
    // The initial function cascade (propagation on), the alternation with
    // the prepared binding setup, then the tail tiers on the FUNCTION
    // result; both cascades' final rows are captured (:584-592).
    let prior_index = build_fingerprint_index(&prior_graph, prior_ingest.semantic(), &prior_tables);
    let fresh_index = build_fingerprint_index(&fresh_graph, fresh_ingest.semantic(), &fresh_tables);
    let prior_side = GraphSide::build(&prior_graph, prior_ingest.semantic());
    let fresh_side = GraphSide::build(&fresh_graph, fresh_ingest.semantic());
    let setup = prepare_binding_matching(&prior_graph, &fresh_graph);
    drop(ph);
    let ph = phase("prior:match-cascade");
    let initial = match_functions(
        &prior_index,
        &fresh_index,
        &prior_ctx,
        &fresh_ctx,
        MatchOptions {
            enable_propagation: true,
            ..MatchOptions::default()
        },
    );
    let outcome = alternation::alternate_function_and_binding_matching(
        initial,
        &prior_index,
        &fresh_index,
        &prior_ctx,
        &fresh_ctx,
        &prior_side,
        &fresh_side,
        setup.as_ref(),
    );
    // The WP2.1 probe's tracing surface: the last reference-identity
    // evidence the alternation built — which propagation rung failed for
    // WHICH pair is bisectable offline.
    let evidence = outcome
        .final_evidence
        .as_ref()
        .map(|e| {
            json!({
                "oldRefs": e.old_refs,
                "newRefs": e.new_refs,
                "refMatches": e.ref_matches,
            })
        })
        .unwrap_or(Value::Null);
    let mut function_result = outcome.function_result;
    // The last tiers (prior-version.ts:566-573): ordinal pairing, then the
    // certified interchangeable pools — after every evidence source.
    let old_side = Side::new(&prior_index, &prior_ctx);
    let new_side = Side::new(&fresh_index, &fresh_ctx);
    resolve_ambiguous_by_ordinal(&mut function_result, &old_side, &new_side);
    assign_interchangeable_pools(&mut function_result, &old_side, &new_side);

    // ── the close-match tier (WP2.2's gate) ──────────────────────────────
    // The TS runs buildCloseMatchContext here (:632-640, after the tail
    // tiers) with the cascade's final matches — the close dump's rows are
    // the tier's candidates, assignment outcomes and corroboration verdicts.
    drop(ph);
    let ph = phase("prior:close-dump");
    let fn_matches_for_close = function_result.matches.to_hash_map();
    let (close_file, close_pairs) = crate::matching::close_dump::close_dump_with_context(
        &crate::matching::close_dump::CloseDumpSides {
            prior_graph: &prior_graph,
            fresh_graph: &fresh_graph,
            prior_semantic: prior_ingest.semantic(),
            fresh_semantic: fresh_ingest.semantic(),
            prior_tables: &prior_tables,
            fresh_tables: &fresh_tables,
            prior_index: &prior_index,
            fresh_index: &fresh_index,
            fn_matches: &fn_matches_for_close,
        },
        &prior_json,
        &fresh_json,
    )?;
    // The TS's same-program sanity check (:624) — a prior sharing nearly no
    // structural hashes with the new version is a wrong file, not an
    // aggressive refactor. Fails the dump loudly instead of transferring
    // nothing (the pipeline throws; the dump cannot proceed past it).
    crate::prior::assert_prior_looks_like_same_program(
        prior_graph.functions.len(),
        function_result.unmatched.len(),
    )?;

    drop(ph);
    let ph = phase("prior:twin-inventories");
    // ── the twins' inventories (WP2.3) ───────────────────────────────────
    let prior_wrapper = crate::modules::wrapper::find_wrapper_function(
        prior_ingest.program,
        prior_ingest.semantic(),
    );
    let fresh_wrapper = crate::modules::wrapper::find_wrapper_function(
        fresh_ingest.program,
        fresh_ingest.semantic(),
    );
    let (prior_inventory, prior_values) = crate::twins::statement_inventory_from_json(
        &prior_json,
        prior_wrapper.as_ref().map(|w| w.body_span),
        "prior",
        Some(&prior_graph),
        true,
    )?;
    let (fresh_inventory, fresh_values) = crate::twins::statement_inventory_from_json(
        &fresh_json,
        fresh_wrapper.as_ref().map(|w| w.body_span),
        "fresh",
        Some(&fresh_graph),
        true,
    )?;

    let stage = MatchStage {
        fresh: StageSide {
            ingest: &fresh_ingest,
            json: &fresh_json,
            tables: &fresh_tables,
            graph: &fresh_graph,
            ctx: &fresh_ctx,
            spans: &fresh_spans,
            index: &fresh_index,
            graph_side: &fresh_side,
            wrapper: fresh_wrapper.as_ref(),
            inventory: &fresh_inventory,
            inventory_values: &fresh_values,
        },
        prior: StageSide {
            ingest: &prior_ingest,
            json: &prior_json,
            tables: &prior_tables,
            graph: &prior_graph,
            ctx: &prior_ctx,
            spans: &prior_spans,
            index: &prior_index,
            graph_side: &prior_side,
            wrapper: prior_wrapper.as_ref(),
            inventory: &prior_inventory,
            inventory_values: &prior_values,
        },
        function_result: &function_result,
        binding_result: outcome.binding_result.as_ref(),
        binding_setup: setup.as_ref(),
        close_file: close_file.as_ref(),
        close_pairs: &close_pairs,
        evidence: &evidence,
    };
    drop(ph);
    let _ph = phase("naming-era");
    consume(&stage)
}

impl<'a, 's> StageSide<'a, 's> {
    /// The twins' gate view of this side (GateSide borrows the stage).
    pub fn gate_side(&self) -> crate::twins::gates::GateSide<'a, 's> {
        crate::twins::gates::GateSide::build(
            self.graph,
            self.ingest.semantic(),
            self.tables,
            self.inventory,
            self.inventory_values,
            self.graph_side,
            self.wrapper.map(|w| w.span),
        )
    }
}

/// Parse one side's text (`name` — `input.js` / `prior.js` — names the
/// source in errors). Babel's `sourceType: "unambiguous"`, as the TS
/// `parseSourceAst` parses both sides: an ESM text (`import.meta`) that a
/// script parse rejects is a module (the zustand fixture's regime).
pub fn parse_side<'a>(
    allocator: &'a Allocator,
    text: &'a str,
    name: &str,
) -> Result<Ingest<'a>, String> {
    let ingest = Ingest::parse_unambiguous(allocator, text);
    if let Some(first) = ingest.errors.first() {
        return Err(format!(
            "oxc failed to parse {name}: {} diagnostic(s) — {first}",
            ingest.errors.len()
        ));
    }
    Ok(ingest)
}

/// The two sides' program JSON ([`crate::ingest::program_estree_json`]):
/// serialized here (the AST is not thread-safe), parsed concurrently.
pub(crate) fn program_jsons(fresh: &Ingest<'_>, prior: &Ingest<'_>) -> (Value, Value) {
    let fresh_text = fresh.program.to_estree_json(false, true);
    let prior_text = prior.program.to_estree_json(false, true);
    crate::par::join(
        || crate::ingest::parse_estree_json(&fresh_text),
        || crate::ingest::parse_estree_json(&prior_text),
    )
}

/// One side's built state: the tables, graph, statement contexts and
/// session-id spans.
pub(crate) struct SideParts {
    pub(crate) tables: SymbolTables,
    pub(crate) graph: crate::graph::UnifiedGraph,
    pub(crate) ctx: StatementContexts,
    /// session id → row span, functions then bindings.
    pub(crate) spans: HashMap<String, oxc_span::Span>,
}

/// Build one side: the Bun classification, the unified graph, the
/// statement contexts and the session-id spans — all over the side's one
/// program JSON.
pub(crate) fn build_side_parts(
    ingest: &Ingest<'_>,
    program_json: &Value,
    file_name: &str,
    eligibility: Eligibility<'_>,
) -> SideParts {
    let wrapper = crate::modules::wrapper::find_wrapper_function(ingest.program, ingest.semantic());
    let tables = SymbolTables::build(ingest.semantic());
    let factories = crate::modules::classify_bun_modules(
        ingest.text,
        ingest.program,
        ingest.semantic(),
        wrapper.as_ref().map(|w| w.body_span),
        &tables,
    )
    .map(|c| c.factories)
    .unwrap_or_default();
    let graph = crate::graph::build_unified_graph_with_json(
        ingest.semantic(),
        ingest.program,
        program_json,
        file_name,
        &factories,
        eligibility,
    );
    let ctx = StatementContexts::build_with_json(
        &graph,
        ingest.semantic(),
        &tables,
        program_json,
        ingest.text,
    );
    let mut spans: HashMap<String, oxc_span::Span> = HashMap::new();
    for f in &graph.functions {
        spans.insert(f.session_id.clone(), f.span);
    }
    for mb in &graph.module_bindings {
        spans.insert(mb.session_id.clone(), mb.span);
    }
    SideParts {
        tables,
        graph,
        ctx,
        spans,
    }
}
