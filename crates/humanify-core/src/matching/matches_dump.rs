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
use oxc_span::GetSpan;
use serde_json::{Value, json};

use crate::graph::{
    Eligibility, build_unified_graph_with_eligibility, build_unified_graph_with_eligibility_opts,
};
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
    let wrapper = crate::modules::wrapper::find_wrapper_function(ingest.program, ingest.semantic());
    let tables = SymbolTables::build(ingest.semantic());
    let classification = crate::modules::classify_bun_modules(
        text,
        ingest.program,
        ingest.semantic(),
        wrapper.as_ref().map(|w| w.body_span),
        &tables,
    );
    let factories = classification.map(|c| c.factories).unwrap_or_default();
    let graph = build_unified_graph_with_eligibility(
        ingest.semantic(),
        ingest.program,
        file_id,
        &factories,
        eligibility,
    );
    let ctx = StatementContexts::build(&graph, ingest.semantic(), &tables, ingest.program, text);
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
        // The graph rows with their reference edges as SESSION IDS — the
        // binding identity resolver's inputs (calleeNeighborIds /
        // callerFnIds), for replicating the identity evidence offline.
        let join = alternation::session_join(&side.graph);
        let mut graph_rows: Vec<Value> = Vec::new();
        for f in &side.graph.functions {
            graph_rows.push(json!({
                "kind": "function",
                "sessionId": f.session_id,
                "structuralHash": f.structural_hash,
                "internalCallees": alternation::neighbor_ids(&f.internal_callees, &join),
            }));
        }
        for b in &side.graph.module_bindings {
            graph_rows.push(json!({
                "kind": "module-binding",
                "sessionId": b.session_id,
                "structuralHash": b.fingerprint_hash,
                "internalCallees": alternation::neighbor_ids(&b.internal_callees, &join),
                "callers": alternation::neighbor_ids(&b.callers, &join),
            }));
        }
        fs::write(
            out_dir.join(format!("graphrows-{name}.jsonl")),
            graph_rows
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("serialize: {e}"))?
                .join("\n"),
        )
        .map_err(|e| format!("write graphrows-{name}: {e}"))?;
        count += graph_rows.len();

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

/// The WP2.1 hash probe: canonical hash + token stream for requested
/// (side, span) graph-entry nodes, one JSONL row each — the cross-side
/// hash-equality divergences are bisected on the STREAM, not the digest.
/// `spans_path` is a JSON array of `{"side": "prior"|"fresh", "start", "end"}`.
pub fn dump_hash_probe(
    ts_dump_dir: &Path,
    spans_path: &Path,
    out_path: &Path,
) -> Result<usize, String> {
    let spans_text = fs::read_to_string(spans_path).map_err(|e| format!("spans: {e}"))?;
    let spans: Vec<Value> =
        serde_json::from_str(&spans_text).map_err(|e| format!("spans json: {e}"))?;
    let mut by_side: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
    for s in &spans {
        let side = s["side"].as_str().unwrap_or_default().to_string();
        let start = s["start"].as_u64().unwrap_or_default() as u32;
        let end = s["end"].as_u64().unwrap_or_default() as u32;
        by_side.entry(side).or_default().push((start, end));
    }

    let mut rows: Vec<Value> = Vec::new();
    for side in ["prior", "fresh"] {
        let Some(want) = by_side.get(side) else {
            continue;
        };
        let text = fs::read_to_string(ts_dump_dir.join("text").join(format!("{side}.js")))
            .map_err(|e| format!("{side}: {e}"))?;
        let allocator = Box::leak(Box::new(Allocator::default()));
        let ingest = Ingest::parse(allocator, &text, side);
        if !ingest.errors.is_empty() {
            return Err(format!(
                "oxc on {side}: {} diagnostic(s)",
                ingest.errors.len()
            ));
        }
        let tables = SymbolTables::build(ingest.semantic());
        let nodes = ingest.semantic().nodes();
        let mut want_set: std::collections::HashSet<(u32, u32)> = want.iter().copied().collect();
        for node in nodes.iter() {
            let span = node.span();
            if !want_set.remove(&(span.start, span.end)) {
                continue;
            }
            // Graph-entry kinds only (the hash covers the entry's own
            // subtree — graph.rs's pass-1 match).
            if !matches!(
                node.kind(),
                oxc_ast::AstKind::Function(_)
                    | oxc_ast::AstKind::ArrowFunctionExpression(_)
                    | oxc_ast::AstKind::MethodDefinition(_)
                    | oxc_ast::AstKind::ObjectProperty(_)
            ) {
                continue;
            }
            let out = crate::graph::hash_entry_subtree(nodes, node.id(), &tables);
            // Per-identifier table presence inside the subtree: which
            // lookups hit decl_by_start / ref_by_start — the missing-slot
            // bisection surface.
            let mut ids: Vec<Value> = Vec::new();
            for inner in nodes.iter() {
                let ispan = inner.span();
                if ispan.start < span.start || ispan.end > span.end {
                    continue;
                }
                if let oxc_ast::AstKind::IdentifierName(_)
                | oxc_ast::AstKind::BindingIdentifier(_) = inner.kind()
                {
                    let name = match inner.kind() {
                        oxc_ast::AstKind::IdentifierName(i) => i.name.to_string(),
                        oxc_ast::AstKind::BindingIdentifier(i) => i.name.to_string(),
                        _ => unreachable!(),
                    };
                    ids.push(json!({
                        "start": ispan.start,
                        "name": name,
                        "decl": tables.decl_by_start.contains_key(&ispan.start),
                        "ref": tables.ref_by_start.contains_key(&ispan.start),
                    }));
                }
            }
            rows.push(json!({
                "side": side,
                "start": span.start,
                "end": span.end,
                "hash": out.hash,
                "parts": out.parts,
                "ids": ids,
            }));
        }
    }
    fs::write(
        out_path,
        rows.iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("serialize: {e}"))?
            .join("\n"),
    )
    .map_err(|e| format!("write: {e}"))?;
    Ok(rows.len())
}

pub fn dump_matches(ts_dump_dir: &Path, out_dir: &Path) -> Result<usize, String> {
    dump_matches_opts(ts_dump_dir, out_dir, false)
}

/// The sizing-probe variant: `visit_optional_calls` = the FIX for babel's
/// optional-call blind spot (the graphs see the optional calls' edges).
pub fn dump_matches_opts(
    ts_dump_dir: &Path,
    out_dir: &Path,
    visit_optional_calls: bool,
) -> Result<usize, String> {
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
        fresh_ingest.semantic(),
    );
    let fresh_tables = SymbolTables::build(fresh_ingest.semantic());
    let fresh_classification = crate::modules::classify_bun_modules(
        &fresh,
        fresh_ingest.program,
        fresh_ingest.semantic(),
        fresh_wrapper.as_ref().map(|w| w.body_span),
        &fresh_tables,
    );
    let fresh_factories = fresh_classification
        .map(|c| c.factories)
        .unwrap_or_default();
    let fresh_graph = build_unified_graph_with_eligibility_opts(
        fresh_ingest.semantic(),
        fresh_ingest.program,
        "input.js",
        &fresh_factories,
        Eligibility::SkipSet { bundler, minifier },
        visit_optional_calls,
    );
    let fresh_ctx = super::statement_context::StatementContexts::build(
        &fresh_graph,
        fresh_ingest.semantic(),
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
        prior_ingest.semantic(),
    );
    let prior_tables = SymbolTables::build(prior_ingest.semantic());
    let prior_classification = crate::modules::classify_bun_modules(
        &prior,
        prior_ingest.program,
        prior_ingest.semantic(),
        prior_wrapper.as_ref().map(|w| w.body_span),
        &prior_tables,
    );
    let prior_factories = prior_classification
        .map(|c| c.factories)
        .unwrap_or_default();
    let prior_graph = build_unified_graph_with_eligibility_opts(
        prior_ingest.semantic(),
        prior_ingest.program,
        "prior.js",
        &prior_factories,
        Eligibility::All,
        visit_optional_calls,
    );
    let prior_ctx = super::statement_context::StatementContexts::build(
        &prior_graph,
        prior_ingest.semantic(),
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
    let prior_index = build_fingerprint_index(&prior_graph, prior_ingest.semantic(), &prior_tables);
    let fresh_index = build_fingerprint_index(&fresh_graph, fresh_ingest.semantic(), &fresh_tables);
    let prior_side = GraphSide::build(&prior_graph, prior_ingest.semantic());
    let fresh_side = GraphSide::build(&fresh_graph, fresh_ingest.semantic());
    let setup = prepare_binding_matching(
        &prior_graph,
        prior_ingest.semantic(),
        &prior_tables,
        &fresh_graph,
        fresh_ingest.semantic(),
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
    // The WP2.1 probe's tracing surface: the last reference-identity
    // evidence the alternation built, plus the final ambiguous map —
    // which propagation rung failed for WHICH pair is bisectable offline.
    let ev_dump = outcome
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
    // The WP2.1 probe's tracing surface: the last reference-identity
    // evidence the alternation built, plus the final ambiguous map —
    // which propagation rung failed for WHICH pair is bisectable offline.
    fs::create_dir_all(out_dir).ok();
    if !ev_dump.is_null() {
        let amb: std::collections::BTreeMap<&String, &Vec<String>> =
            function_result.ambiguous.iter().collect();
        fs::write(
            out_dir.join("evidence.json"),
            serde_json::to_string(&json!({
                "evidence": ev_dump,
                "ambiguous": amb,
            }))
            .unwrap(),
        )
        .map_err(|e| format!("write evidence: {e}"))?;
    }
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
    let fn_matches_for_close = function_result.matches.clone();
    let close_file = super::close_dump::close_dump(
        &super::close_dump::CloseDumpSides {
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
        super::statement_align::parse_json_unbounded(
            &prior_ingest.program.to_estree_json(false, true),
        ),
        super::statement_align::parse_json_unbounded(
            &fresh_ingest.program.to_estree_json(false, true),
        ),
    )?;
    // The TS's same-program sanity check (:624) — a prior sharing nearly no
    // structural hashes with the new version is a wrong file, not an
    // aggressive refactor. Fails the dump loudly instead of transferring
    // nothing (the pipeline throws; the dump cannot proceed past it).
    crate::prior::assert_prior_looks_like_same_program(
        prior_graph.functions.len(),
        function_result.unmatched.len(),
    )?;

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

    // ── the twins (WP2.3): inventories + unique tier + the gates ────────
    // The runtime computes them inside matchPriorVersion (the TS's
    // recorders fire there) — the same flow, here.
    let (prior_inventory, prior_values) =
        crate::twins::statement_inventory_with_values(&prior, "prior", Some(&prior_graph))?;
    let (fresh_inventory, fresh_values) =
        crate::twins::statement_inventory_with_values(&fresh, "fresh", Some(&fresh_graph))?;
    let prior_wrapper = crate::modules::wrapper::find_wrapper_function(
        prior_ingest.program,
        prior_ingest.semantic(),
    );
    let fresh_wrapper = crate::modules::wrapper::find_wrapper_function(
        fresh_ingest.program,
        fresh_ingest.semantic(),
    );
    let prior_gate_side = crate::twins::gates::GateSide::build(
        &prior_graph,
        prior_ingest.semantic(),
        &prior_tables,
        &prior_inventory,
        &prior_values,
        &prior_side,
        prior_wrapper.as_ref().map(|w| w.span),
    );
    let fresh_gate_side = crate::twins::gates::GateSide::build(
        &fresh_graph,
        fresh_ingest.semantic(),
        &fresh_tables,
        &fresh_inventory,
        &fresh_values,
        &fresh_side,
        fresh_wrapper.as_ref().map(|w| w.span),
    );
    // The cascade's results, as the twins read them (the WP2.4 derivation —
    // the gates test's exact shape).
    let fn_matches: HashMap<String, String> = function_result.matches.clone();
    // The cascades' matches are SESSION-ID keyed ("module:<name>",
    // "input.js:L:C"); the gate tests binding NAMES — convert through the
    // graphs' session-id registries (the raw ids here were the original
    // parity bug — every claimed-test read false and every bucket ref-key
    // lookup missed).
    let (claimed, identity_pairs) = outcome
        .binding_result
        .as_ref()
        .map(|r| {
            crate::twins::gates::binding_cascade_name_inputs(
                &prior_gate_side,
                &fresh_gate_side,
                &r.matches,
                &fn_matches,
            )
        })
        .unwrap_or_default();
    let fn_states: HashMap<String, crate::twins::gates::RowState> = fresh_graph
        .functions
        .iter()
        .map(|f| {
            let state = if fn_matches.values().any(|v| v == &f.session_id) {
                crate::twins::gates::RowState::ExactMatched
            } else {
                crate::twins::gates::RowState::Pending
            };
            (f.session_id.clone(), state)
        })
        .collect();
    let binding_states: HashMap<String, crate::twins::gates::RowState> = fresh_graph
        .module_bindings
        .iter()
        .map(|b| (b.session_id.clone(), crate::twins::gates::RowState::Pending))
        .collect();
    let twin_inputs = crate::twins::gates::TwinInputs {
        fn_matches: &fn_matches,
        claimed_old_names: &claimed,
        binding_identity_pairs: &identity_pairs,
        fn_states: &fn_states,
        binding_states: &binding_states,
    };
    let twin_output = crate::twins::gates::compute_gated_statement_twins(
        &prior_gate_side,
        &fresh_gate_side,
        &twin_inputs,
    )?;

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
    // matches-close.json (WP2.2's gate): the close tier's decision record.
    // Absent when the tier would not run (one side has no unmatched
    // functions) — the TS never records then; absent-on-both is agreement.
    if let Some(close_file) = close_file {
        fs::write(
            out_dir.join("matches-close.json"),
            serde_json::to_string(&close_file).unwrap(),
        )
        .map_err(|e| format!("write matches-close: {e}"))?;
    }
    // twins.json: the inventories + the unique-tier pair set (spans only —
    // the digest bytes are serializer artifacts); twin-gates.json: the
    // gates' per-proposal outcomes + the stats bag + conflicts.
    fs::write(
        out_dir.join("twins.json"),
        serde_json::to_string(&json!({
            "schemaVersion": 1,
            "inventories": {
                "prior": twins_inventory_json(&prior_inventory),
                "fresh": twins_inventory_json(&fresh_inventory)
            },
            "uniqueTier": unique_tier_json(&fresh_inventory, &prior_inventory)
        }))
        .unwrap(),
    )
    .map_err(|e| format!("write twins: {e}"))?;
    let mut gates =
        crate::twins::gates::gate_dump(&twin_output, &prior_gate_side, &fresh_gate_side);
    if let Some(obj) = gates.as_object_mut() {
        obj.insert("schemaVersion".into(), json!(1));
    }
    fs::write(
        out_dir.join("twin-gates.json"),
        serde_json::to_string(&gates).unwrap(),
    )
    .map_err(|e| format!("write twin-gates: {e}"))?;
    Ok(pairs.len())
}

/// One inventory, as the dump's scalars (the TS twinInventorySnapshot).
fn twins_inventory_json(inv: &crate::twins::SideInventory) -> Value {
    let mut histogram: std::collections::BTreeMap<u32, u32> = Default::default();
    let (mut distinct, mut unique, mut max_bucket) = (0u32, 0u32, 0u32);
    #[allow(clippy::iter_over_hash_type)]
    for count in inv.hash_counts.values() {
        distinct += 1;
        max_bucket = max_bucket.max(*count);
        if *count == 1 {
            unique += 1;
        }
        *histogram.entry(*count).or_default() += 1;
    }
    json!({
        "statements": inv.statements.len(),
        "distinctHashes": distinct,
        "uniqueHashes": unique,
        "maxBucket": max_bucket,
        "bucketHistogram": histogram.iter()
            .map(|(k, v)| (k.to_string(), *v))
            .collect::<std::collections::BTreeMap<String, u32>>()
    })
}

/// The unique-tier pair set, spans only, sorted by fresh span.
fn unique_tier_json(
    fresh: &crate::twins::SideInventory,
    prior: &crate::twins::SideInventory,
) -> Value {
    let proposals = crate::twins::unique_twin_proposals(prior, fresh);
    let mut pairs: Vec<Value> = proposals
        .pairs
        .iter()
        .map(|(fresh_idx, prior_idx)| {
            let fresh_stmt = &fresh.statements[*fresh_idx];
            let prior_stmt = &prior.statements[*prior_idx];
            json!({
                "prior": {"text": "prior", "start": prior_stmt.span.start, "end": prior_stmt.span.end},
                "fresh": {"text": "fresh", "start": fresh_stmt.span.start, "end": fresh_stmt.span.end},
                "hash": fresh_stmt.hash,
            })
        })
        .collect();
    pairs.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["fresh"]["start"].as_u64().unwrap_or(u64::MAX),
                v["fresh"]["end"].as_u64().unwrap_or(u64::MAX),
            )
        };
        key(a).cmp(&key(b))
    });
    json!({"uniqueTwins": proposals.unique_twins, "pairs": pairs})
}
/// The WP2.1 ref probe: for requested (side, span) function nodes, the
/// raw resolved-reference rows inside the fn's span joined against the
/// matchable-binding and function-holder identity maps — the
/// empty-oldRefs bisection surface (which reference missed the identity
/// map, and whether it is even in the refs table). `spans_path` is a JSON
/// array of `{"side": "prior"|"fresh", "start", "end"}`.
#[allow(clippy::too_many_lines)]
pub fn dump_ref_probe(
    ts_dump_dir: &Path,
    spans_path: &Path,
    out_path: &Path,
) -> Result<usize, String> {
    let spans_text = fs::read_to_string(spans_path).map_err(|e| format!("spans: {e}"))?;
    let spans: Vec<Value> =
        serde_json::from_str(&spans_text).map_err(|e| format!("spans json: {e}"))?;
    let mut by_side: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
    for s in &spans {
        let side = s["side"].as_str().unwrap_or_default().to_string();
        by_side.entry(side).or_default().push((
            s["start"].as_u64().unwrap_or_default() as u32,
            s["end"].as_u64().unwrap_or_default() as u32,
        ));
    }
    let meta: Value = serde_json::from_str(
        &fs::read_to_string(ts_dump_dir.join("meta.json")).map_err(|e| format!("meta: {e}"))?,
    )
    .map_err(|e| format!("meta json: {e}"))?;
    let bundler = meta["flags"]["bundler"].as_str();
    let minifier = meta["flags"]["minifier"].as_str();

    // One parsed side: the graph, the symbol tables, the evidence inputs,
    // the semantic (which the GraphSide borrows), and the source text for
    // the occurrence excerpts. The arena + text are leaked so every borrow
    // is \'static (the established probe pattern).
    struct ProbeSide {
        text: &'static str,
        graph: &'static crate::graph::UnifiedGraph,
        tables: SymbolTables,
        semantic: &'static oxc_semantic::Semantic<'static>,
        gside: alternation::GraphSide<'static>,
    }
    let build =
        |label: &str, file_id: &str, eligibility: Eligibility| -> Result<ProbeSide, String> {
            let text: &'static str = Box::leak(
                fs::read_to_string(ts_dump_dir.join("text").join(format!("{label}.js")))
                    .map_err(|e| format!("{label}: {e}"))?
                    .into_boxed_str(),
            );
            let allocator: &'static Allocator = Box::leak(Box::new(Allocator::default()));
            let ingest: &'static Ingest<'static> =
                Box::leak(Box::new(Ingest::parse(allocator, text, file_id)));
            if !ingest.errors.is_empty() {
                return Err(format!(
                    "oxc on {label}: {} diagnostic(s)",
                    ingest.errors.len()
                ));
            }
            let wrapper =
                crate::modules::wrapper::find_wrapper_function(ingest.program, ingest.semantic());
            let tables = SymbolTables::build(ingest.semantic());
            let classification = crate::modules::classify_bun_modules(
                text,
                ingest.program,
                ingest.semantic(),
                wrapper.as_ref().map(|w| w.body_span),
                &tables,
            );
            let factories = classification.map(|c| c.factories).unwrap_or_default();
            let graph: &'static crate::graph::UnifiedGraph =
                Box::leak(Box::new(build_unified_graph_with_eligibility(
                    ingest.semantic(),
                    ingest.program,
                    file_id,
                    &factories,
                    eligibility,
                )));
            let gside = alternation::GraphSide::build(graph, ingest.semantic());
            Ok(ProbeSide {
                text,
                graph,
                tables,
                semantic: ingest.semantic(),
                gside,
            })
        };
    let mut sides: HashMap<String, ProbeSide> = HashMap::new();
    if by_side.contains_key("prior") {
        sides.insert(
            "prior".into(),
            build("prior", "prior.js", Eligibility::All)?,
        );
    }
    if by_side.contains_key("fresh") {
        sides.insert(
            "fresh".into(),
            build(
                "fresh",
                "input.js",
                Eligibility::SkipSet { bundler, minifier },
            )?,
        );
    }
    let prior = sides.remove("prior").ok_or("no prior spans requested")?;
    let fresh = sides.remove("fresh").ok_or("no fresh spans requested")?;
    let setup = alternation::prepare_binding_matching(
        prior.graph,
        prior.semantic,
        &prior.tables,
        fresh.graph,
        fresh.semantic,
        &fresh.tables,
    );
    let Some(setup) = setup else {
        return Err("no matchable bindings — the identity maps are empty".to_string());
    };
    let prior_ids = alternation::reference_ids_by_binding(Some(&setup.prior_by_id), &prior.gside);
    let new_ids = alternation::reference_ids_by_binding(Some(&setup.new_by_id), &fresh.gside);
    let matchable_syms =
        |by_id: &std::collections::BTreeMap<String, &crate::graph::ModuleBindingNode>| {
            by_id
                .values()
                .map(|b| b.symbol)
                .collect::<std::collections::HashSet<_>>()
        };
    let prior_matchable = matchable_syms(&setup.prior_by_id);
    let new_matchable = matchable_syms(&setup.new_by_id);

    let mut rows: Vec<Value> = Vec::new();
    for (label, side, want, ids, matchable) in [
        (
            "prior",
            &prior,
            by_side["prior"].clone(),
            &prior_ids,
            &prior_matchable,
        ),
        (
            "fresh",
            &fresh,
            by_side["fresh"].clone(),
            &new_ids,
            &new_matchable,
        ),
    ] {
        let row_by_span: HashMap<(u32, u32), usize> = side
            .graph
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| ((f.span.start, f.span.end), i))
            .collect();
        for (start, end) in want {
            let Some(&row) = row_by_span.get(&(start, end)) else {
                rows.push(json!({"side": label, "start": start, "end": end, "found": false}));
                continue;
            };
            let span = side.graph.functions[row].span;
            let occs: Vec<Value> = side
                .gside
                .raw_refs(span)
                .into_iter()
                .map(|(s, e, symbol)| {
                    // Why the identity map missed: the symbol's declaration
                    // node + its parent (the holder arms read both), and
                    // whether the occurrence's span IS the symbol span.
                    let scoping = side.semantic.scoping();
                    let nodes = side.semantic.nodes();
                    let decl_id = scoping.symbol_declaration(symbol);
                    let decl_kind = match nodes.get_node(decl_id).kind() {
                        oxc_ast::AstKind::Function(_) => "Function",
                        oxc_ast::AstKind::VariableDeclarator(_) => "VariableDeclarator",
                        oxc_ast::AstKind::BindingIdentifier(_) => "BindingIdentifier",
                        other => Box::leak(format!("{other:?}").into_boxed_str()),
                    };
                    let decl_parent_kind = match nodes.get_node(nodes.parent_id(decl_id)).kind() {
                        oxc_ast::AstKind::Program(_) => "Program",
                        oxc_ast::AstKind::FunctionBody(_) => "FunctionBody",
                        oxc_ast::AstKind::BlockStatement(_) => "BlockStatement",
                        oxc_ast::AstKind::VariableDeclarator(_) => "VariableDeclarator",
                        other => Box::leak(format!("{other:?}").into_boxed_str()),
                    };
                    let sym_span = scoping.symbol_span(symbol);
                    json!({
                        "start": s,
                        "end": e,
                        "text": side.text.get(s as usize..e as usize).unwrap_or("?"),
                        "refId": ids.get(&symbol),
                        "matchable": matchable.contains(&symbol),
                        "holder": side.gside.holders().contains_key(&symbol),
                        "declKind": decl_kind,
                        "declParent": decl_parent_kind,
                        "symSpanMatches": sym_span == oxc_span::Span::new(s, e),
                    })
                })
                .collect();
            rows.push(json!({
                "side": label,
                "start": start,
                "end": end,
                "sessionId": side.graph.functions[row].session_id,
                "refs": side.gside.collect_referenced_binding_ids(row, ids),
                "occ": occs,
            }));
        }
    }
    rows.sort_by(|a, b| {
        let key = |v: &Value| {
            (
                v["side"].as_str().unwrap_or_default().to_string(),
                v["start"].as_u64().unwrap_or(0),
            )
        };
        key(a).cmp(&key(b))
    });
    fs::write(
        out_path,
        rows.iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("serialize: {e}"))?
            .join("\n"),
    )
    .map_err(|e| format!("write: {e}"))?;
    Ok(rows.len())
}
