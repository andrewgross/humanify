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
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use oxc_allocator::Allocator;
use serde_json::{Value, json};

use crate::graph::Eligibility;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::alternation::{self, GraphSide, prepare_binding_matching};
use crate::matching::cascade::{
    MatchOptions, Side, assign_interchangeable_pools, match_functions, resolve_ambiguous_by_ordinal,
};
use crate::matching::statement_context::StatementContexts;
use crate::rename::eligibility::NeverRename;
use crate::toolchain::{BundleLayout, ModuleWrapperGrammar};

/// The texts and flags the match stage runs on: the FORMATTED fresh text,
/// the prior version's code, the run's never-rename lists (the fresh
/// side's rename-eligibility skip set — the toolchain's P7 piece) and the
/// run's bundle layout (where each side's top-level statements are — P9).
#[derive(Clone, Copy)]
pub struct PriorMatchInput<'t> {
    pub fresh: &'t str,
    pub prior: &'t str,
    pub never_rename: NeverRename,
    pub layout: BundleLayout,
    /// The run's module wrapper grammar (P3): each side's bundled modules,
    /// whose functions the graph skips (spec I10).
    pub module_wrappers: ModuleWrapperGrammar,
    /// The fast schedule (the relaxed default and `--sequential` alike):
    /// build the prior side's graph on a thread of its own
    /// (from its own parse of the same text — the AST is not `Send`),
    /// beside the fresh side's. Byte-identical: a parse is deterministic.
    pub fast: bool,
    /// Run the same-program sanity check ([`assert_prior_looks_like_same_program`])
    /// on THIS call. Every file of a multi-file work dir is matched against
    /// the whole prior tree, so the check's granularity is the dump, not
    /// the call: the one caller that amortizes it across files (the match
    /// verb) turns the per-call check off and runs the same assert over
    /// the union of every file's pairs. Semantics are byte-identical for
    /// every other caller (the pipeline always passes true).
    pub same_program_check: bool,
}

/// One side of the match stage, all borrowed from the frame that built
/// it ([`match_prior_version`]'s locals, or the prior side a whole run
/// holds via [`with_prior_match_side`]): the parse, its program JSON, the
/// symbol tables, the graph and its statement contexts, the session-id
/// spans, the function fingerprint index, the alternation's graph view,
/// the wrapper, and the twins' statement inventory (with the statement
/// values the gates walk). All references — `Copy`, so a built side is
/// passed around freely within its owning frame's lifetime.
#[derive(Clone, Copy)]
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
///
/// This is the SINGLE-CALL entry: it builds BOTH sides per call. Callers
/// matching many fresh files against the SAME prior (the match verb's
/// multi-file dumps) build the prior side once instead —
/// [`with_prior_match_side`] + [`match_stage_with_prior`] — which is
/// byte-identical per call (a parse is deterministic).
pub fn match_prior_version<T>(
    input: PriorMatchInput<'_>,
    consume: impl FnOnce(&MatchStage<'_, '_>) -> Result<T, String>,
) -> Result<T, String> {
    let PriorMatchInput {
        fresh,
        prior,
        never_rename,
        layout,
        module_wrappers,
        same_program_check,
        fast,
    } = input;

    // ── both sides: parse, then each side's program JSON ONCE ───────────
    // The JSON is shared by every consumer below (the graph's row hashes
    // and features, the statement contexts, the close tier, the twin
    // inventories); the two parses are independent and run concurrently.
    use crate::profiling::phase;
    let ph = phase("prior:parse+json");
    let fresh_allocator = Allocator::default();
    let prior_allocator = Allocator::default();
    let fresh_eligibility = Eligibility::SkipSet(never_rename);
    let (fresh_ingest, prior_ingest, fresh_json, prior_json, fresh_parts, prior_parts) = if fast {
        drop(ph);
        let _ph = phase("prior:sides-parallel");
        // The prior side's graph on its own thread, from its own parse;
        // this thread builds the fresh side, then parses the prior again
        // for the AST the later stages walk.
        let (prior_side, fresh_side) = crate::par::beside(
            || prior_side_owned(prior, layout, module_wrappers),
            || -> Result<_, String> {
                let fresh_ingest = parse_side(&fresh_allocator, fresh, "input.js")?;
                let fresh_json = crate::ingest::program_estree_json(fresh_ingest.program);
                let fresh_parts = build_side_parts(
                    &fresh_ingest,
                    &fresh_json,
                    "input.js",
                    fresh_eligibility,
                    layout,
                    module_wrappers,
                );
                let prior_ingest = parse_prior(&prior_allocator, prior)?;
                Ok((fresh_ingest, fresh_json, fresh_parts, prior_ingest))
            },
        );
        let (prior_json, prior_parts) = prior_side?;
        let (fresh_ingest, fresh_json, fresh_parts, prior_ingest) = fresh_side?;
        (
            fresh_ingest,
            prior_ingest,
            fresh_json,
            prior_json,
            fresh_parts,
            prior_parts,
        )
    } else {
        let fresh_ingest = parse_side(&fresh_allocator, fresh, "input.js")?;
        let prior_ingest = parse_prior(&prior_allocator, prior)?;
        let (fresh_json, prior_json) = program_jsons(&fresh_ingest, &prior_ingest);
        drop(ph);
        let ph = phase("prior:graph-fresh");
        // ── the fresh side (the pipeline's own eligibility) ─────────────
        let fresh_parts = build_side_parts(
            &fresh_ingest,
            &fresh_json,
            "input.js",
            fresh_eligibility,
            layout,
            module_wrappers,
        );
        drop(ph);
        // ── the prior side (ALL bindings eligible — prior-version.ts:284-288)
        let _ph = phase("prior:graph-prior");
        let prior_parts = build_side_parts(
            &prior_ingest,
            &prior_json,
            "prior.js",
            Eligibility::All,
            layout,
            module_wrappers,
        );
        (
            fresh_ingest,
            prior_ingest,
            fresh_json,
            prior_json,
            fresh_parts,
            prior_parts,
        )
    };

    // ── each side's dependents (index, graph view, wrapper, twins) ───────
    // The prior side's graph/parts may come from the side thread's parse
    // while its ingest is this thread's re-parse of the same text; both
    // parses are deterministic, so the built pieces are identical either
    // way.
    let ph = phase("prior:index");
    let prior_deps = build_side_dependents(
        &prior_ingest,
        &prior_json,
        &prior_parts.graph,
        &prior_parts.tables,
        "prior",
        layout,
    )?;
    let fresh_deps = build_side_dependents(
        &fresh_ingest,
        &fresh_json,
        &fresh_parts.graph,
        &fresh_parts.tables,
        "fresh",
        layout,
    )?;
    drop(ph);
    run_match_stage(
        stage_side(&fresh_ingest, &fresh_json, &fresh_parts, &fresh_deps),
        stage_side(&prior_ingest, &prior_json, &prior_parts, &prior_deps),
        same_program_check,
        consume,
    )
}

/// Build the prior side ONCE — one parse of `prior`, one graph, one
/// fingerprint index, one wrapper, one twins inventory — and hold it for
/// a whole run of any number of fresh files: `run` receives the built
/// side and calls [`match_stage_with_prior`] per file.
///
/// The amortization this owns: against a ~33MB prior the build is ~14
/// seconds, and the single-call design the match verb grew from redid it
/// per file — twice per file in the fast schedule — turning minutes of
/// actual matching into hours of rebuilding the same index on a
/// multi-file work dir. Byte-identical to per-call building: a parse is
/// deterministic, so one parse's side IS every call's side.
///
/// Rust lifetime reality, why a callback: the side's pieces borrow from
/// the frame that builds them (oxc's AST lives in the frame's allocator;
/// [`FingerprintIndex`](crate::matching::FingerprintIndex) borrows the
/// graph), so the side cannot be a self-contained struct that outlives
/// this function — `run` is the frame.
pub fn with_prior_match_side<T>(
    prior: &str,
    layout: BundleLayout,
    module_wrappers: ModuleWrapperGrammar,
    run: impl FnOnce(&StageSide<'_, '_>) -> Result<T, String>,
) -> Result<T, String> {
    use crate::profiling::phase;
    let ph = phase("prior:build-prior-side");
    let allocator = Allocator::default();
    let ingest = parse_prior(&allocator, prior)?;
    let json = crate::ingest::program_estree_json(ingest.program);
    // The prior side's eligibility is ALL bindings (prior-version.ts:284-288).
    let parts = build_side_parts(
        &ingest,
        &json,
        "prior.js",
        Eligibility::All,
        layout,
        module_wrappers,
    );
    let deps = build_side_dependents(&ingest, &json, &parts.graph, &parts.tables, "prior", layout)?;
    drop(ph);
    run(&stage_side(&ingest, &json, &parts, &deps))
}

/// ONE fresh file's match stage against a pre-built prior side — the
/// per-call half of [`match_prior_version`], for callers that hold the
/// prior side across files via [`with_prior_match_side`]. Byte-identical
/// to `match_prior_version` on the same texts; `same_program_check`
/// keeps its per-call semantics (callers amortizing the check across
/// files against one prior — the match verb's multi-file dumps — pass
/// false and run the identical assert over the union of their pairs).
pub fn match_stage_with_prior<T>(
    fresh: &str,
    never_rename: NeverRename,
    layout: BundleLayout,
    module_wrappers: ModuleWrapperGrammar,
    prior: StageSide<'_, '_>,
    same_program_check: bool,
    consume: impl FnOnce(&MatchStage<'_, '_>) -> Result<T, String>,
) -> Result<T, String> {
    use crate::profiling::phase;
    let ph = phase("prior:build-fresh-side");
    let allocator = Allocator::default();
    let ingest = parse_side(&allocator, fresh, "input.js")?;
    let json = crate::ingest::program_estree_json(ingest.program);
    let parts = build_side_parts(
        &ingest,
        &json,
        "input.js",
        Eligibility::SkipSet(never_rename),
        layout,
        module_wrappers,
    );
    let deps = build_side_dependents(&ingest, &json, &parts.graph, &parts.tables, "fresh", layout)?;
    drop(ph);
    run_match_stage(
        stage_side(&ingest, &json, &parts, &deps),
        prior,
        same_program_check,
        consume,
    )
}

/// The match stage's shared tail — everything after both sides are fully
/// built (the per-side builds are [`build_side_dependents`]). The flow
/// mirrors the TS matchAndApplyFunctions (prior-version.ts :524-596):
/// the initial function cascade with propagation, the alternation with
/// the prepared binding setup, the tail tiers on the FUNCTION result
/// only, the close tier, the same-program sanity check.
fn run_match_stage<T>(
    fresh: StageSide<'_, '_>,
    prior: StageSide<'_, '_>,
    same_program_check: bool,
    consume: impl FnOnce(&MatchStage<'_, '_>) -> Result<T, String>,
) -> Result<T, String> {
    // ── matchAndApplyFunctions (prior-version.ts:524-596) ────────────────
    // The initial function cascade (propagation on), the alternation with
    // the prepared binding setup, then the tail tiers on the FUNCTION
    // result; both cascades' final rows are captured (:584-592).
    use crate::profiling::phase;
    let ph = phase("prior:match-cascade");
    let setup = prepare_binding_matching(prior.graph, fresh.graph);
    let initial = {
        let _ph = phase("match:functions");
        match_functions(
            prior.index,
            fresh.index,
            prior.ctx,
            fresh.ctx,
            MatchOptions {
                enable_propagation: true,
                ..MatchOptions::default()
            },
        )
    };
    let outcome = alternation::alternate_function_and_binding_matching(
        initial,
        prior.index,
        fresh.index,
        prior.ctx,
        fresh.ctx,
        prior.graph_side,
        fresh.graph_side,
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
    let old_side = Side::new(prior.index, prior.ctx);
    let new_side = Side::new(fresh.index, fresh.ctx);
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
            prior_graph: prior.graph,
            fresh_graph: fresh.graph,
            prior_semantic: prior.ingest.semantic(),
            fresh_semantic: fresh.ingest.semantic(),
            prior_tables: prior.tables,
            fresh_tables: fresh.tables,
            prior_index: prior.index,
            fresh_index: fresh.index,
            fn_matches: &fn_matches_for_close,
        },
        prior.json,
        fresh.json,
    )?;
    // The TS's same-program sanity check (:624) — a prior sharing nearly no
    // structural hashes with the new version is a wrong file, not an
    // aggressive refactor. Fails the dump loudly instead of transferring
    // nothing (the pipeline throws; the dump cannot proceed past it).
    // Callers amortizing the check across many files against the SAME
    // prior (the match verb's multi-file dumps) opt out per call and run
    // the identical assert at their own granularity.
    if same_program_check {
        crate::prior::assert_prior_looks_like_same_program(
            prior.graph.functions.len(),
            function_result.unmatched.len(),
        )?;
    }

    let stage = MatchStage {
        fresh,
        prior,
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

/// The parse-count pin's counter: how many parses of a PRIOR text this
/// process has run. Observation only — no decision reads it — but the
/// match verb's multi-file pin asserts it stays at ONE per run: the prior
/// side is the run's shared state, not per-call work to redo per file.
static PRIOR_PARSES: AtomicU64 = AtomicU64::new(0);

/// How many prior parses ran so far ([`PRIOR_PARSES`] is private; tests
/// and diagnostics read this).
pub fn prior_parse_count() -> u64 {
    PRIOR_PARSES.load(Relaxed)
}

/// Zero the parse counter. Tests call this just before a run whose prior
/// parse count they pin.
pub fn reset_prior_parse_count() {
    PRIOR_PARSES.store(0, Relaxed);
}

/// Parse the PRIOR side's text — every prior parse routes here, so the
/// counter above counts them all (the name in errors is `prior.js`).
fn parse_prior<'a>(allocator: &'a Allocator, prior: &'a str) -> Result<Ingest<'a>, String> {
    PRIOR_PARSES.fetch_add(1, Relaxed);
    parse_side(allocator, prior, "prior.js")
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

/// The prior side's program JSON and built parts from a parse of its own
/// (everything returned is plain data: the parse dies here).
fn prior_side_owned(
    prior: &str,
    layout: BundleLayout,
    module_wrappers: ModuleWrapperGrammar,
) -> Result<(Value, SideParts), String> {
    let allocator = Allocator::default();
    let ingest = parse_prior(&allocator, prior)?;
    let json = crate::ingest::program_estree_json(ingest.program);
    let parts = build_side_parts(
        &ingest,
        &json,
        "prior.js",
        Eligibility::All,
        layout,
        module_wrappers,
    );
    Ok((json, parts))
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

/// The pieces of one side that depend ONLY on that side — nothing a
/// fresh file changes: the function fingerprint index and the
/// alternation's graph view (both borrowing the side's graph), the
/// wrapper function, and the twins' statement inventory (with the
/// statement values the gates walk). Built beside [`SideParts`], these
/// are the prior side's cacheable half: hold them once per run and every
/// fresh file's stage borrows them.
struct SideDependents<'g> {
    index: crate::matching::FingerprintIndex<'g>,
    graph_side: GraphSide<'g>,
    wrapper: Option<crate::modules::wrapper::WrapperFunction>,
    inventory: crate::twins::SideInventory,
    inventory_values: Vec<Value>,
}

/// Build one side's dependents (see [`SideDependents`]) from its parse,
/// program JSON and built parts. `anchor` names the side in the twins'
/// inventory ("prior" / "fresh"); `layout` is the run's bundle layout,
/// which finds the side's wrapper.
fn build_side_dependents<'g>(
    ingest: &Ingest<'_>,
    program_json: &Value,
    graph: &'g crate::graph::UnifiedGraph,
    tables: &SymbolTables,
    anchor: &'static str,
    layout: BundleLayout,
) -> Result<SideDependents<'g>, String> {
    let index = crate::matching::build_fingerprint_index(graph, ingest.semantic(), tables);
    let graph_side = GraphSide::build(graph, ingest.semantic());
    let wrapper = layout.find_wrapper(ingest.program, ingest.semantic());
    let (inventory, inventory_values) = crate::twins::statement_inventory_from_json(
        program_json,
        wrapper.as_ref().map(|w| w.body_span),
        anchor,
        Some(graph),
        true,
    )?;
    Ok(SideDependents {
        index,
        graph_side,
        wrapper,
        inventory,
        inventory_values,
    })
}

/// Assemble the stage's view of one side from the frame that owns its
/// pieces (the ingest, the program JSON, the built parts and the built
/// dependents — all must share the frame, since the index borrows the
/// graph and the ingest borrows the frame's allocator).
fn stage_side<'a, 's>(
    ingest: &'a Ingest<'s>,
    json: &'a Value,
    parts: &'a SideParts,
    deps: &'a SideDependents<'a>,
) -> StageSide<'a, 's> {
    StageSide {
        ingest,
        json,
        tables: &parts.tables,
        graph: &parts.graph,
        ctx: &parts.ctx,
        spans: &parts.spans,
        index: &deps.index,
        graph_side: &deps.graph_side,
        wrapper: deps.wrapper.as_ref(),
        inventory: &deps.inventory,
        inventory_values: &deps.inventory_values,
    }
}

/// Build one side: the Bun classification, the unified graph, the
/// statement contexts and the session-id spans — all over the side's one
/// program JSON. `layout` is the run's bundle layout (the toolchain's P9
/// piece): the classification's container and the graph's module scope;
/// `module_wrappers` is the run's module grammar (P3): the classification.
pub(crate) fn build_side_parts(
    ingest: &Ingest<'_>,
    program_json: &Value,
    file_name: &str,
    eligibility: Eligibility,
    layout: BundleLayout,
    module_wrappers: ModuleWrapperGrammar,
) -> SideParts {
    let wrapper = layout.find_wrapper(ingest.program, ingest.semantic());
    let tables = SymbolTables::build(ingest.semantic());
    // The naming stage's third-party skip (spec I10, review R2): the run's
    // module grammar re-run on this side's text, unchanged in behaviour.
    let factories = module_wrappers
        .classify_factories(
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
        layout,
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
