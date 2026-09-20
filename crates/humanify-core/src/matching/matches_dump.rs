//! The WP2.1 gate's dump: rebuild the TS matches.json's rows from a TS
//! dump's two texts. The matching machinery is FULLY cold — no LLM anywhere
//! (the function cascade, propagation, the function↔binding alternation,
//! ordinal, interchangeable pools). Migration scaffolding — deleted at
//! phase 6 with the TS core (02 §9).
//!
//! The flow mirrors the TS matchAndApplyFunctions (prior-version.ts
//! :524-596): the initial function cascade with propagation, the
//! alternation (matching::alternation) with the prepared binding setup,
//! then the tail tiers on the FUNCTION result only — and both cascades'
//! FINAL rows are captured (:584-592, the binding result as-is, without
//! the tail tiers).
//!
//! Rows anchor: prior spans on `"prior"`, fresh spans on `"fresh"` (the TS
//! captureMatchDump's spanOf through the two indexes' node maps). The dump
//! rows carry the FUNCTION cascade first (`cascade: "function"`), then the
//! BINDING cascade's (`cascade: "binding"`) — the capture order the TS
//! writes in, though the differ joins by (prior, fresh, cascade) keys.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use oxc_allocator::Allocator;
use serde_json::{Value, json};

use crate::graph::{Eligibility, build_unified_graph_with_eligibility};
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;

use super::alternation::{self, GraphSide, prepare_binding_matching};
use super::build_fingerprint_index;
use super::cascade::{
    MatchOptions, Side, assign_interchangeable_pools, match_functions, resolve_ambiguous_by_ordinal,
};
use super::statement_context::StatementContexts;

/// The spanKey for one side's anchored span.
fn span_key(label: &str, span: oxc_span::Span) -> Value {
    json!({"text": label, "start": span.start, "end": span.end})
}

/// One cascade result's pair + rejection rows, as dump JSON (the TS
/// captureMatchDump's two loops — capture.ts:180-205).
fn cascade_rows(
    result: &super::cascade::MatchResult,
    prior_spans: &HashMap<String, oxc_span::Span>,
    fresh_spans: &HashMap<String, oxc_span::Span>,
    cascade_label: &str,
) -> (Vec<Value>, Vec<Value>) {
    let span_of = |spans: &HashMap<String, oxc_span::Span>, id: &str| -> Option<oxc_span::Span> {
        spans.get(id).copied()
    };
    let mut pairs: Vec<Value> = result
        .pair_resolutions
        .iter()
        .map(|p| {
            json!({
                "cascade": cascade_label,
                "prior": span_of(prior_spans, &p.prior)
                    .map(|s| span_key("prior", s))
                    .unwrap_or(json!({"text": "prior", "start": -1, "end": -1})),
                "fresh": span_of(fresh_spans, &p.fresh)
                    .map(|s| span_key("fresh", s))
                    .unwrap_or(json!({"text": "fresh", "start": -1, "end": -1})),
                "tier": p.tier,
            })
        })
        .collect();
    pairs.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["prior"]["start"].as_i64().unwrap_or(0),
                v["prior"]["end"].as_i64().unwrap_or(0),
            )
        };
        key(a).cmp(&key(b))
    });
    let rejections: Vec<Value> = result
        .pair_rejections
        .iter()
        .map(|r| {
            json!({
                "cascade": cascade_label,
                "prior": span_of(prior_spans, &r.prior)
                    .map(|s| span_key("prior", s))
                    .unwrap_or(json!({"text": "prior", "start": -1, "end": -1})),
                "kind": r.kind.as_str(),
                "candidates": r.candidates.as_ref().map(|cands| {
                    let mut spans: Vec<(i64, i64)> = cands
                        .iter()
                        .filter_map(|c| span_of(fresh_spans, c))
                        .map(|s| (s.start as i64, s.end as i64))
                        .collect();
                    spans.sort();
                    spans.iter()
                        .map(|(s, e)| json!({"text": "fresh", "start": s, "end": e}))
                        .collect::<Vec<_>>()
                }),
            })
        })
        .collect();
    (pairs, rejections)
}

/// One parsed side: the graph, its statement contexts, and the session-id →
/// span maps — shared by [`dump_matches`] and [`dump_stmt_contexts`].
pub struct BuiltSide {
    pub graph: crate::graph::UnifiedGraph,
    pub ctx: StatementContexts,
    pub spans: HashMap<String, oxc_span::Span>,
    /// The arena. The graph/semantic borrow it; leaking at fixture/CLI
    /// scale is the established pattern (the borrows are 'static then).
    _allocator: &'static Allocator,
}

/// Parse + classify + graph + contexts for one side's text. `file_id` is the
/// session-id anchor ("input.js" fresh / "prior.js" prior); `eligibility`
/// selects the pipeline's fresh-side skip-set or the driver's prior-side all.
pub fn build_side(
    text: &str,
    file_id: &str,
    eligibility: Eligibility,
) -> Result<BuiltSide, String> {
    // The borrows inside (semantic/graph) must outlive the BuiltSide;
    // leaking the arena makes them 'static (the CLI process is
    // short-lived; the tests drop the leak at exit).
    let allocator: &'static Allocator = Box::leak(Box::new(Allocator::default()));
    let ingest = Ingest::parse(allocator, text, file_id);
    if !ingest.errors.is_empty() {
        return Err(format!(
            "oxc on {file_id}: {} diagnostic(s)",
            ingest.errors.len()
        ));
    }
    let wrapper = crate::modules::wrapper::find_wrapper_function(ingest.program, &ingest.semantic);
    let tables = SymbolTables::build(&ingest.semantic);
    let classification = crate::modules::classify_bun_modules(
        text,
        ingest.program,
        &ingest.semantic,
        wrapper.as_ref().map(|w| w.body_span),
        &tables,
    );
    let factories = classification.map(|c| c.factories).unwrap_or_default();
    let graph = build_unified_graph_with_eligibility(
        &ingest.semantic,
        ingest.program,
        file_id,
        &factories,
        eligibility,
    );
    let ctx = StatementContexts::build(&graph, &ingest.semantic, &tables, ingest.program, text);
    let mut spans: HashMap<String, oxc_span::Span> = HashMap::new();
    for f in &graph.functions {
        spans.insert(f.session_id.clone(), f.span);
    }
    for mb in &graph.module_bindings {
        spans.insert(mb.session_id.clone(), mb.span);
    }
    Ok(BuiltSide {
        graph,
        ctx,
        spans,
        _allocator: allocator,
    })
}

/// The parity probe's evidence dump: both sides' statement contexts as JSONL
/// (one row per function and binding row), for diffing against the TS probe.
pub fn dump_stmt_contexts(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
    let meta_text =
        fs::read_to_string(ts_dump_dir.join("meta.json")).map_err(|e| format!("meta.json: {e}"))?;
    let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;
    let fresh = fs::read_to_string(ts_dump_dir.join("text").join("fresh.js"))
        .map_err(|e| format!("fresh: {e}"))?;
    let prior = fs::read_to_string(ts_dump_dir.join("text").join("prior.js"))
        .map_err(|e| format!("prior: {e}"))?;
    let meta_flags = &meta["flags"];
    let bundler = meta_flags["bundler"].as_str();
    let minifier = meta_flags["minifier"].as_str();

    let fresh_side = build_side(
        &fresh,
        "input.js",
        Eligibility::SkipSet { bundler, minifier },
    )?;
    let prior_side = build_side(&prior, "prior.js", Eligibility::All)?;

    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
    let mut count = 0usize;
    for (name, side) in [("prior", &prior_side), ("fresh", &fresh_side)] {
        let mut rows: Vec<Value> = Vec::new();
        for (f, ctx) in side.graph.functions.iter().zip(side.ctx.function_rows()) {
            rows.push(json!({
                "kind": "function",
                "start": f.span.start,
                "end": f.span.end,
                "sessionId": f.session_id,
                "stmt": ctx.stmt_span.map(span_json),
                "isOwn": ctx.is_own_statement,
                "hash": ctx.hash,
            }));
        }
        for (b, ctx) in side
            .graph
            .module_bindings
            .iter()
            .zip(side.ctx.binding_rows())
        {
            rows.push(json!({
                "kind": "binding",
                "start": b.span.start,
                "end": b.span.end,
                "sessionId": b.session_id,
                "stmt": ctx.stmt_span.map(span_json),
                "prev": ctx.prev_sibling.map(span_json),
                "next": ctx.next_sibling.map(span_json),
                "hash": ctx.hash,
            }));
        }
        fs::write(
            out_dir.join(format!("stmtctx-{name}.jsonl")),
            rows.iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("serialize: {e}"))?
                .join("\n"),
        )
        .map_err(|e| format!("write {name}: {e}"))?;
        count += rows.len();
    }
    Ok(count)
}

fn span_json(s: oxc_span::Span) -> Value {
    json!([s.start, s.end])
}

pub fn dump_matches(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
    let meta_text =
        fs::read_to_string(ts_dump_dir.join("meta.json")).map_err(|e| format!("meta.json: {e}"))?;
    let meta: Value = serde_json::from_str(&meta_text).map_err(|e| format!("meta: {e}"))?;
    let fresh = fs::read_to_string(ts_dump_dir.join("text").join("fresh.js"))
        .map_err(|e| format!("fresh: {e}"))?;
    let prior = fs::read_to_string(ts_dump_dir.join("text").join("prior.js"))
        .map_err(|e| format!("prior: {e}"))?;
    let meta_flags = &meta["flags"];
    let bundler = meta_flags["bundler"].as_str();
    let minifier = meta_flags["minifier"].as_str();

    // ── the fresh side (the pipeline's own eligibility) ─────────────────
    let fresh_allocator = Allocator::default();
    let fresh_ingest = Ingest::parse(&fresh_allocator, &fresh, "fresh.js");
    if !fresh_ingest.errors.is_empty() {
        return Err(format!(
            "oxc on fresh: {} diagnostic(s)",
            fresh_ingest.errors.len()
        ));
    }
    let fresh_wrapper = crate::modules::wrapper::find_wrapper_function(
        fresh_ingest.program,
        &fresh_ingest.semantic,
    );
    let fresh_tables = SymbolTables::build(&fresh_ingest.semantic);
    let fresh_classification = crate::modules::classify_bun_modules(
        &fresh,
        fresh_ingest.program,
        &fresh_ingest.semantic,
        fresh_wrapper.as_ref().map(|w| w.body_span),
        &fresh_tables,
    );
    let fresh_factories = fresh_classification
        .map(|c| c.factories)
        .unwrap_or_default();
    let fresh_graph = build_unified_graph_with_eligibility(
        &fresh_ingest.semantic,
        fresh_ingest.program,
        "input.js",
        &fresh_factories,
        Eligibility::SkipSet { bundler, minifier },
    );
    let fresh_ctx = super::statement_context::StatementContexts::build(
        &fresh_graph,
        &fresh_ingest.semantic,
        &fresh_tables,
        fresh_ingest.program,
        &fresh,
    );
    let mut fresh_spans: HashMap<String, oxc_span::Span> = HashMap::new();
    for f in &fresh_graph.functions {
        fresh_spans.insert(f.session_id.clone(), f.span);
    }
    for mb in &fresh_graph.module_bindings {
        fresh_spans.insert(mb.session_id.clone(), mb.span);
    }

    // ── the prior side (ALL bindings eligible — prior-version.ts:284-288) ─
    let prior_allocator = Allocator::default();
    let prior_ingest = Ingest::parse(&prior_allocator, &prior, "prior.js");
    if !prior_ingest.errors.is_empty() {
        return Err(format!(
            "oxc on prior: {} diagnostic(s)",
            prior_ingest.errors.len()
        ));
    }
    let prior_wrapper = crate::modules::wrapper::find_wrapper_function(
        prior_ingest.program,
        &prior_ingest.semantic,
    );
    let prior_tables = SymbolTables::build(&prior_ingest.semantic);
    let prior_classification = crate::modules::classify_bun_modules(
        &prior,
        prior_ingest.program,
        &prior_ingest.semantic,
        prior_wrapper.as_ref().map(|w| w.body_span),
        &prior_tables,
    );
    let prior_factories = prior_classification
        .map(|c| c.factories)
        .unwrap_or_default();
    let prior_graph = build_unified_graph_with_eligibility(
        &prior_ingest.semantic,
        prior_ingest.program,
        "prior.js",
        &prior_factories,
        Eligibility::All,
    );
    let prior_ctx = super::statement_context::StatementContexts::build(
        &prior_graph,
        &prior_ingest.semantic,
        &prior_tables,
        prior_ingest.program,
        &prior,
    );
    let mut prior_spans: HashMap<String, oxc_span::Span> = HashMap::new();
    for f in &prior_graph.functions {
        prior_spans.insert(f.session_id.clone(), f.span);
    }
    for mb in &prior_graph.module_bindings {
        prior_spans.insert(mb.session_id.clone(), mb.span);
    }

    // ── matchAndApplyFunctions (prior-version.ts:524-596) ────────────────
    // The initial function cascade (propagation on), the alternation with
    // the prepared binding setup, then the tail tiers on the FUNCTION
    // result; both cascades' final rows are captured (:584-592).
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
    let mut function_result = outcome.function_result;
    // The last tiers (prior-version.ts:566-573): ordinal pairing, then the
    // certified interchangeable pools — after every evidence source.
    let old_side = Side::new(&prior_index, &prior_ctx);
    let new_side = Side::new(&fresh_index, &fresh_ctx);
    resolve_ambiguous_by_ordinal(&mut function_result, &old_side, &new_side);
    assign_interchangeable_pools(&mut function_result, &old_side, &new_side);

    let (mut pairs, mut rejections) =
        cascade_rows(&function_result, &prior_spans, &fresh_spans, "function");
    let binding_stats = match &outcome.binding_result {
        // The binding cascade joins through ITS OWN (matchable-filtered)
        // indexes — prior-version.ts:585-592; no tail tiers on it.
        Some(binding_result) => {
            let (b_pairs, b_rejections) =
                cascade_rows(binding_result, &prior_spans, &fresh_spans, "binding");
            pairs.extend(b_pairs);
            rejections.extend(b_rejections);
            binding_result.resolution_stats.to_ts_value()
        }
        None => Value::Null,
    };

    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
    fs::write(
        out_dir.join("meta.json"),
        serde_json::to_string(&meta).unwrap(),
    )
    .map_err(|e| format!("write meta: {e}"))?;
    fs::write(
        out_dir.join("matches.json"),
        serde_json::to_string(&json!({
            "schemaVersion": 1,
            "resolutionStats": function_result.resolution_stats.to_ts_value(),
            "bindingResolutionStats": binding_stats,
            "pairs": pairs,
            "rejections": rejections,
        }))
        .unwrap(),
    )
    .map_err(|e| format!("write matches: {e}"))?;
    Ok(pairs.len())
}
