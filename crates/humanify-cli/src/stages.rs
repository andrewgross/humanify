//! The ported pipeline stages as the driver runs them, in the TS order of
//! docs/pipeline-stages.md, and the NOT-YET stubs for the rest.
//!
//! Ported and wired: 1 detect, 2 select unpack adapter (pipeline_config),
//! 7 build the function graph, 8 match against the prior (both cascades,
//! the tail tiers, and the same-program assertion). Stages 3-6 (unpack,
//! library detection, vendor naming, format) and 9-12 (naming, placement,
//! split, emit) are NOT-YET: the run stops there with an `ERROR:` block and
//! [`EXIT_NOT_YET`] — never a silent skip, never a partial tree.
//!
//! Stages 7-8 need the FORMATTED text (stage 6's output). Until the
//! formatter is ported the binary takes it from the TS via the Rust-only
//! `--beautified-input <path>` (00-control §3, 2026-09-19).
//!
//! `match_prior` orchestrates the matching calls exactly as the WP2.1/2.2
//! gate dump does (matching/matches_dump.rs, mirroring the TS
//! matchAndApplyFunctions); that orchestration belongs in humanify-core's
//! prior-version port (WP3.2), where both callers should share it — named
//! in the WPB.4 hand-back rather than moved here, because the matching
//! files are another lane's this week.

use humanify_core::graph::{Eligibility, build_unified_graph_with_eligibility};
use humanify_core::hash::serialize::SymbolTables;
use humanify_core::ingest::Ingest;
use humanify_core::matching::alternation::{
    GraphSide, alternate_function_and_binding_matching, prepare_binding_matching,
};
use humanify_core::matching::build_fingerprint_index;
use humanify_core::matching::cascade::{
    MatchOptions, Side, assign_interchangeable_pools, match_functions, resolve_ambiguous_by_ordinal,
};
use humanify_core::matching::statement_context::StatementContexts;

/// The driver's exit code for a run that reached an unported stage:
/// distinct from 0 (success), 1 (documented failure), 2 (reserved for the
/// harness layer, contract 14 §2) and 101 (a panic).
pub const EXIT_NOT_YET: i32 = 3;

/// One pipeline stage (docs/pipeline-stages.md numbering).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    pub number: u8,
    pub name: &'static str,
    /// Who ports it (docs/rust-port/10-work-breakdown.md).
    pub owner: &'static str,
}

pub const UNPACK: Stage = Stage {
    number: 3,
    name: "unpack the bundle",
    owner: "WPB.2",
};
pub const NAMING: Stage = Stage {
    number: 9,
    name: "name identifiers",
    owner: "WP3.x/WP4.x",
};

/// Every stage the Rust driver cannot run yet, in pipeline order.
pub const NOT_YET: [Stage; 8] = [
    UNPACK,
    Stage {
        number: 4,
        name: "detect libraries",
        owner: "WPB.3",
    },
    Stage {
        number: 5,
        name: "name vendor files",
        owner: "WPB.2",
    },
    Stage {
        number: 6,
        name: "format",
        owner: "phase 5a (the TS formatter's output is ingested meanwhile)",
    },
    NAMING,
    Stage {
        number: 10,
        name: "place statements",
        owner: "WP5.x",
    },
    Stage {
        number: 11,
        name: "split",
        owner: "WP5.x",
    },
    Stage {
        number: 12,
        name: "emit + finish on disk",
        owner: "WP5.x",
    },
];

/// The `ERROR:` block for reaching an unported stage: a headline, then
/// INDENTED detail (the harness keeps it with the headline).
pub fn not_yet_block(stage: Stage) -> String {
    let list: Vec<String> = NOT_YET
        .iter()
        .map(|s| format!("{} {}", s.number, s.name))
        .collect();
    format!(
        "ERROR: stage {} ({}) is NOT YET PORTED to Rust (owner: {}) — no output was written; this run is marked failed.\n  not yet ported: {}\n  run the TS pipeline (npx tsx src/index.ts) for a complete run",
        stage.number,
        stage.name,
        stage.owner,
        list.join(", ")
    )
}

/// Stage 7 on one side: ingest the formatted text and build the graph.
pub struct GraphSummary {
    pub functions: usize,
    pub module_bindings: usize,
}

/// What stage 8 decided, as counts.
pub struct MatchSummary {
    pub prior_functions: usize,
    pub matched: usize,
    pub ambiguous: usize,
    pub unmatched: usize,
    pub binding_matched: Option<usize>,
}

fn parse<'a>(
    allocator: &'a oxc_allocator::Allocator,
    text: &'a str,
    name: &str,
) -> Result<Ingest<'a>, String> {
    let ingest = Ingest::parse(allocator, text, name);
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc failed to parse {name}: {} diagnostic(s) — {}",
            ingest.errors.len(),
            ingest.errors[0]
        ));
    }
    Ok(ingest)
}

fn factories_of(
    text: &str,
    ingest: &Ingest<'_>,
    tables: &SymbolTables,
) -> Vec<humanify_core::modules::FactoryRecord> {
    let wrapper =
        humanify_core::modules::wrapper::find_wrapper_function(ingest.program, &ingest.semantic);
    humanify_core::modules::classify_bun_modules(
        text,
        ingest.program,
        &ingest.semantic,
        wrapper.as_ref().map(|w| w.body_span),
        tables,
    )
    .map(|c| c.factories)
    .unwrap_or_default()
}

/// Stage 7 alone (no prior): the fresh side's graph.
pub fn build_graph(
    text: &str,
    bundler: Option<&str>,
    minifier: Option<&str>,
) -> Result<GraphSummary, String> {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = parse(&allocator, text, "input.js")?;
    let tables = SymbolTables::build(&ingest.semantic);
    let factories = factories_of(text, &ingest, &tables);
    let graph = build_unified_graph_with_eligibility(
        &ingest.semantic,
        ingest.program,
        "input.js",
        &factories,
        Eligibility::SkipSet { bundler, minifier },
    );
    Ok(GraphSummary {
        functions: graph.functions.len(),
        module_bindings: graph.module_bindings.len(),
    })
}

/// Stages 7-8 with a prior: both graphs, the function cascade with
/// propagation, the function/binding alternation, the tail tiers, then the
/// TS same-program assertion (prior-version.ts:621) — a prior sharing
/// (nearly) no structural hashes is a wrong file and fails the run.
pub fn match_prior(
    fresh: &str,
    prior: &str,
    bundler: Option<&str>,
    minifier: Option<&str>,
) -> Result<MatchSummary, String> {
    let fresh_alloc = oxc_allocator::Allocator::default();
    let prior_alloc = oxc_allocator::Allocator::default();
    let fresh_ingest = parse(&fresh_alloc, fresh, "input.js")?;
    let prior_ingest = parse(&prior_alloc, prior, "prior.js")?;
    let fresh_tables = SymbolTables::build(&fresh_ingest.semantic);
    let prior_tables = SymbolTables::build(&prior_ingest.semantic);
    let fresh_factories = factories_of(fresh, &fresh_ingest, &fresh_tables);
    let prior_factories = factories_of(prior, &prior_ingest, &prior_tables);
    let fresh_graph = build_unified_graph_with_eligibility(
        &fresh_ingest.semantic,
        fresh_ingest.program,
        "input.js",
        &fresh_factories,
        Eligibility::SkipSet { bundler, minifier },
    );
    // The prior side: ALL bindings eligible (prior-version.ts:284-288).
    let prior_graph = build_unified_graph_with_eligibility(
        &prior_ingest.semantic,
        prior_ingest.program,
        "prior.js",
        &prior_factories,
        Eligibility::All,
    );
    let fresh_ctx = StatementContexts::build(
        &fresh_graph,
        &fresh_ingest.semantic,
        &fresh_tables,
        fresh_ingest.program,
        fresh,
    );
    let prior_ctx = StatementContexts::build(
        &prior_graph,
        &prior_ingest.semantic,
        &prior_tables,
        prior_ingest.program,
        prior,
    );
    let prior_index = build_fingerprint_index(&prior_graph, &prior_ingest.semantic, &prior_tables);
    let fresh_index = build_fingerprint_index(&fresh_graph, &fresh_ingest.semantic, &fresh_tables);
    let prior_side = GraphSide::build(&prior_graph, &prior_ingest.semantic);
    let fresh_side = GraphSide::build(&fresh_graph, &fresh_ingest.semantic);
    let setup = prepare_binding_matching(
        &prior_graph,
        &prior_ingest.semantic,
        &prior_tables,
        &fresh_graph,
        &fresh_ingest.semantic,
        &fresh_tables,
    );
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
    let outcome = alternate_function_and_binding_matching(
        initial,
        &prior_index,
        &fresh_index,
        &prior_ctx,
        &fresh_ctx,
        &prior_side,
        &fresh_side,
        setup.as_ref(),
    );
    let mut result = outcome.function_result;
    let old_side = Side::new(&prior_index, &prior_ctx);
    let new_side = Side::new(&fresh_index, &fresh_ctx);
    resolve_ambiguous_by_ordinal(&mut result, &old_side, &new_side);
    assign_interchangeable_pools(&mut result, &old_side, &new_side);
    humanify_core::prior::assert_prior_looks_like_same_program(
        prior_graph.functions.len(),
        result.unmatched.len(),
    )?;
    Ok(MatchSummary {
        prior_functions: prior_graph.functions.len(),
        matched: result.matches.len(),
        ambiguous: result.ambiguous.len(),
        unmatched: result.unmatched.len(),
        binding_matched: outcome.binding_result.as_ref().map(|b| b.matches.len()),
    })
}
