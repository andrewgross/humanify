//! The match stage's rows in the `--dump-artifacts` catalog (matches.json,
//! twins.json). The matching machinery is FULLY cold — no LLM anywhere
//! (the function cascade, propagation, the function↔binding alternation,
//! ordinal, interchangeable pools).
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

use serde_json::{Value, json};

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

/// The match stage's dump sections, as the TS artifact dump records them
/// (`captureMatchDump`, `recordCloseMatches`, `recordTwinProposals`,
/// `recordTwinGates`) — the one builder the WP2.x verb and the pipeline's
/// `--dump-artifacts` share.
pub struct MatchSections {
    /// matches.json: both cascades' pairs + rejections, the stats bags.
    pub matches: Value,
    /// matches-close.json — None when the close tier did not run.
    pub matches_close: Option<Value>,
    /// twins.json: the inventories + the unique-tier pair set.
    pub twins: Value,
    /// twin-gates.json: the gates' per-proposal outcomes + stats.
    pub twin_gates: Value,
    pub pair_count: usize,
    /// The two cascades' stats bags (their TS key order serializes).
    pub resolution_stats: crate::matching::cascade::ResolutionStats,
    pub binding_resolution_stats: Option<crate::matching::cascade::ResolutionStats>,
}

/// Build [`MatchSections`] from the stage and the transfer stage's twin
/// gating (the SAME output the run's transfers read).
pub fn match_sections(
    stage: &crate::prior::MatchStage<'_, '_>,
    twin_output: &crate::twins::gates::TwinGateOutput,
) -> MatchSections {
    let function_result = stage.function_result;
    let (mut pairs, mut rejections) = cascade_rows(
        function_result,
        stage.prior.spans,
        stage.fresh.spans,
        "function",
    );
    let binding_stats = match stage.binding_result {
        // The binding cascade joins through ITS OWN (matchable-filtered)
        // indexes — prior-version.ts:585-592; no tail tiers on it.
        Some(binding_result) => {
            let (b_pairs, b_rejections) = cascade_rows(
                binding_result,
                stage.prior.spans,
                stage.fresh.spans,
                "binding",
            );
            pairs.extend(b_pairs);
            rejections.extend(b_rejections);
            binding_result.resolution_stats.to_ts_value()
        }
        None => Value::Null,
    };
    let pair_count = pairs.len();
    let matches = json!({
        "schemaVersion": 1,
        "resolutionStats": function_result.resolution_stats.to_ts_value(),
        "bindingResolutionStats": binding_stats,
        "pairs": pairs,
        "rejections": rejections,
    });
    let matches_close = stage
        .close_file
        .map(|f| serde_json::to_value(f).expect("the close file serializes"));
    let twins = json!({
        "schemaVersion": 1,
        "inventories": {
            "prior": twins_inventory_json(stage.prior.inventory),
            "fresh": twins_inventory_json(stage.fresh.inventory)
        },
        "uniqueTier": unique_tier_json(stage.fresh.inventory, stage.prior.inventory)
    });
    let mut twin_gates = crate::twins::gates::gate_dump(
        twin_output,
        &stage.prior.gate_side(),
        &stage.fresh.gate_side(),
    );
    if let Some(obj) = twin_gates.as_object_mut() {
        obj.insert("schemaVersion".into(), json!(1));
    }
    MatchSections {
        matches,
        matches_close,
        twins,
        twin_gates,
        pair_count,
        resolution_stats: function_result.resolution_stats.clone(),
        binding_resolution_stats: stage.binding_result.map(|r| r.resolution_stats.clone()),
    }
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
