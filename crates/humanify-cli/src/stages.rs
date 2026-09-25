//! The ported pipeline stages as the driver runs them, in the TS order of
//! docs/pipeline-stages.md, and the NOT-YET stubs for the rest.
//!
//! Ported and wired: 1 detect, 2 select unpack adapter (pipeline_config),
//! 3 unpack (humanify_core::unpack's registry: bun, the webcrack shim,
//! passthrough), 4 library detection (humanify_core::libdetect), 5 vendor
//! naming (inside the Bun adapter: the deterministic cascade, then the LLM
//! pass over the fallback names), 7 build the function graph, 8 match
//! against the prior (both cascades, the tail tiers, and the same-program
//! assertion). Stages 6 (format) and 9-12 (naming, placement, split, emit)
//! are NOT-YET: the run stops there with an `ERROR:` block and
//! [`EXIT_NOT_YET`] — never a silent skip. What stages 3-5 wrote (the
//! unpacked tree) stays on disk; nothing downstream of the stop is written.
//!
//! Stages 7-8 need the FORMATTED text (stage 6's output). Until the
//! formatter is ported the binary takes it from the TS via the Rust-only
//! `--beautified-input <path>` (00-control §3, 2026-09-19).
//!
//! `match_prior` calls humanify-core's `prior::match_prior_version` — the
//! ONE owner of the TS matchPriorVersion orchestration, shared with the
//! WP2.x/WP3.2 gate dumps (moved there by WP3.2, 2026-09-25).

use humanify_core::graph::{Eligibility, build_unified_graph_with_eligibility};
use humanify_core::hash::serialize::SymbolTables;
use humanify_core::ingest::Ingest;

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

pub const FORMAT: Stage = Stage {
    number: 6,
    name: "format",
    owner: "phase 5a (the TS formatter's output is ingested meanwhile: --beautified-input)",
};
pub const NAMING: Stage = Stage {
    number: 9,
    name: "name identifiers",
    owner: "WP3.x/WP4.x",
};

/// Every stage the Rust driver cannot run yet, in pipeline order.
pub const NOT_YET: [Stage; 5] = [
    FORMAT,
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
        "ERROR: stage {} ({}) is NOT YET PORTED to Rust (owner: {}) — the output holds only what the stages before it wrote; this run is marked failed.\n  not yet ported: {}\n  run the TS pipeline (npx tsx src/index.ts) for a complete run",
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
        humanify_core::modules::wrapper::find_wrapper_function(ingest.program, ingest.semantic());
    humanify_core::modules::classify_bun_modules(
        text,
        ingest.program,
        ingest.semantic(),
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
    let tables = SymbolTables::build(ingest.semantic());
    let factories = factories_of(text, &ingest, &tables);
    let graph = build_unified_graph_with_eligibility(
        ingest.semantic(),
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

/// Stages 7-8 with a prior: humanify-core's `prior::match_prior_version`
/// (the TS matchPriorVersion's orchestration — both graphs, the function
/// cascade with propagation, the function/binding alternation, the tail
/// tiers, the close tier, then the TS same-program assertion,
/// prior-version.ts:621 — a prior sharing (nearly) no structural hashes is
/// a wrong file and fails the run). ONE owner: the WP2.x/WP3.2 gate dumps
/// call the same function.
pub fn match_prior(
    fresh: &str,
    prior: &str,
    bundler: Option<&str>,
    minifier: Option<&str>,
) -> Result<MatchSummary, String> {
    let input = humanify_core::prior::PriorMatchInput {
        fresh,
        prior,
        bundler,
        minifier,
        visit_optional_calls: false,
    };
    humanify_core::prior::match_prior_version(input, |stage| {
        let result = stage.function_result;
        Ok(MatchSummary {
            prior_functions: stage.prior.graph.functions.len(),
            matched: result.matches.len(),
            ambiguous: result.ambiguous.len(),
            unmatched: result.unmatched.len(),
            binding_matched: stage.binding_result.map(|b| b.matches.len()),
        })
    })
}
