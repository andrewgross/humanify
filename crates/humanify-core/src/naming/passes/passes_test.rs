//! The WP4.4/4.5 TS-probed fixtures: every top-level call the passes' OWN
//! TS unit tests make (diff-reconcile, reconcile-step, minted-census,
//! coverage-sweep, sweep-step, family-permute(-step), class-id-floor,
//! decoration-retry, plugin-cross-version), recorded by running those
//! suites in an instrumented scratch tree of the oracle commit
//! (test/parity/wp445-harvest-hooks.py → wp445-harvest-collect.py →
//! test/parity/wp445-fixtures.json). Each row is replayed here against the
//! Rust port: inputs → outputs, rendered text, the strategy-trail rows the
//! pass wrote, and (sweeps) the exact LLM requests.
//!
//! These rows are the regimes the four oracle pairs never exercise: the
//! mixed-hunk tier and skipImportDeclarations (the post-split pass's
//! options), dry-run, decoration-retry APPLIES, the pre-generate sweep,
//! carried-name exemptions, family-permute reverts, eval taint.

use std::collections::HashSet;

use humanify_model::js::JsValue;
use humanify_model::llm::{
    BatchRenameResponse, LlmCall, LlmError, LlmErrorKind, NameProvider, Renames,
};
use oxc_allocator::Allocator;
use serde_json::{Value, json};

use super::census::{
    MintedBinding, MintedFamily, collect_free_references, collect_minted_bindings, summarize_census,
};
use super::family_permute::{BucketMember, assign_bucket, run_family_permute};
use super::floor_passes::{derive_expression_inner_names, retry_decorated_names};
use super::sweep::{
    collect_sweep_targets, is_sweep_target, run_deferred_sweep, sweep_minted_names,
};
use crate::ingest::Ingest;
use crate::modules::soundness::collect_eval_with_taint;
use crate::naming::reconcile::hunks::{compute_normal_diff, parse_normal_diff};
use crate::naming::reconcile::lexer::{TokenKind, tokenize_line, units};
use crate::naming::reconcile::step::run_prior_diff_reconciliation;
use crate::naming::reconcile::{ReconcileOptions, collect_word_tokens, reconcile_diff_noise};
use crate::naming::waves::render::render_program;
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::{
    is_below_floor_name, is_bun_token, is_decorated_descriptive, is_half_mint_head,
    is_wordless_mint_shape,
};
use crate::rename::validated::RenameState;
use crate::trail::{Anchor, StrategyTrail, TrailEntry};

fn fixtures() -> Vec<Value> {
    let path = format!(
        "{}/../../test/parity/wp445-fixtures.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let doc: Value = serde_json::from_str(&text).unwrap();
    doc["rows"].as_array().unwrap().clone()
}

fn rows_of(kind: &str) -> Vec<Value> {
    let rows: Vec<Value> = fixtures()
        .into_iter()
        .filter(|r| r["kind"] == kind)
        .collect();
    assert!(!rows.is_empty(), "no fixture rows of kind {kind}");
    rows
}

/// The Eligibility whose ineligible words match the recorded set (the TS
/// tests use `createIsEligible("bun", "bun")` or the default — both the
/// universal skip set).
fn eligibility_for(row: &Value, texts: &[&str]) -> Eligibility {
    let recorded: HashSet<String> = row["ineligible"]
        .as_array()
        .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
        .unwrap_or_default();
    let eligible = Eligibility::new(Some("bun"), Some("bun"));
    let mut words = HashSet::new();
    for t in texts {
        words.extend(collect_word_tokens(t));
    }
    let ours: HashSet<String> = words
        .into_iter()
        .filter(|w| !eligible.is_eligible(w))
        .collect();
    assert_eq!(ours, recorded, "eligibility differs from the TS predicate");
    eligible
}

fn attempt_json(a: &crate::trail::Attempt) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("strategy".into(), json!(a.tier.as_str()));
    o.insert("outcome".into(), json!(a.outcome.as_str()));
    if let Some(r) = &a.reason {
        o.insert("reason".into(), json!(r));
    }
    if let Some(n) = &a.proposed_name {
        o.insert("newName".into(), json!(n));
    }
    Value::Object(o)
}

/// The trail rows as (oldName, attempts) per binding, in first-record
/// order — the TS sink grouped by its identifier span, the Rust entries.
fn ts_trail(row: &Value) -> Vec<(String, Vec<Value>)> {
    let mut out: Vec<((i64, i64), String, Vec<Value>)> = Vec::new();
    for r in row["trail"].as_array().unwrap() {
        let key = (r["start"].as_i64().unwrap(), r["end"].as_i64().unwrap());
        let mut attempt = r["attempt"].clone();
        if let Some(o) = attempt.as_object_mut() {
            o.retain(|_, v| !v.is_null());
        }
        match out.iter_mut().find(|(k, _, _)| *k == key) {
            Some((_, _, list)) => list.push(attempt),
            None => out.push((
                key,
                r["oldName"].as_str().unwrap().to_string(),
                vec![attempt],
            )),
        }
    }
    out.into_iter().map(|(_, n, a)| (n, a)).collect()
}

fn rust_trail(entries: &[TrailEntry]) -> Vec<(String, Vec<Value>)> {
    entries
        .iter()
        .map(|e| {
            (
                e.old_name.clone(),
                e.attempts.iter().map(attempt_json).collect(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// the pure pieces
// ---------------------------------------------------------------------------

#[test]
fn predicates_match_the_ts() {
    let mut n = 0;
    for row in fixtures() {
        let Some(kind) = row["kind"].as_str().and_then(|k| k.strip_prefix("pred:")) else {
            continue;
        };
        let name = row["name"].as_str().unwrap();
        let ours = match kind {
            "isBunToken" => is_bun_token(name),
            "isDecoratedDescriptive" => is_decorated_descriptive(name),
            "isWordlessMintShape" => is_wordless_mint_shape(name),
            "isBelowFloorName" => is_below_floor_name(name),
            "isHalfMintHead" => is_half_mint_head(name),
            "isSweepTarget" => is_sweep_target(name),
            other => panic!("unknown predicate {other}"),
        };
        assert_eq!(ours, row["out"].as_bool().unwrap(), "{kind}({name:?})");
        n += 1;
    }
    assert!(n > 150, "predicate rows: {n}");
}

#[test]
fn tokenize_line_matches_the_ts() {
    for row in rows_of("tokenize") {
        let line = row["line"].as_str().unwrap();
        let u = units(line);
        let ours = tokenize_line(&u).map(|tokens| {
            tokens
                .iter()
                .map(|t| {
                    json!({
                        "kind": if t.kind == TokenKind::Ident { "ident" } else { "text" },
                        "text": String::from_utf16_lossy(t.text(&u)),
                        "col": t.col(),
                    })
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(json!(ours), row["out"], "tokenizeLine({line:?})");
    }
}

#[test]
fn diff_parse_and_word_tokens_match_the_ts() {
    for row in rows_of("diff") {
        let out = compute_normal_diff(
            row["priorText"].as_str().unwrap(),
            row["newText"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(json!(out), row["out"]);
    }
    for row in rows_of("parse-diff") {
        let hunks = parse_normal_diff(row["diffText"].as_str().unwrap());
        let ours: Vec<Value> = hunks
            .iter()
            .map(|h| {
                json!({
                    "op": (h.op as char).to_string(),
                    "priorStart": h.prior_start,
                    "newStart": h.new_start,
                    "priorLines": h.prior_lines,
                    "newLines": h.new_lines,
                })
            })
            .collect();
        assert_eq!(json!(ours), row["out"]);
    }
    for row in rows_of("word-tokens") {
        let mut ours: Vec<String> = collect_word_tokens(row["text"].as_str().unwrap())
            .into_iter()
            .collect();
        ours.sort();
        assert_eq!(json!(ours), row["out"]);
    }
}

#[test]
fn assign_bucket_matches_the_ts() {
    let members = |v: &Value| -> Vec<BucketMember> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|m| BucketMember {
                name: m["name"].as_str().unwrap().to_string(),
                contexts: m["contexts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap().to_string())
                    .collect(),
            })
            .collect()
    };
    for row in rows_of("assign-bucket") {
        let ineligible: HashSet<&str> = row["ineligible"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let out = assign_bucket(&members(&row["fresh"]), &members(&row["prior"]), &|n| {
            !ineligible.contains(n)
        });
        let ours: Vec<Value> = out
            .iter()
            .map(|a| json!({"fromName": a.from_name, "toName": a.to_name, "support": a.support}))
            .collect();
        assert_eq!(json!(ours), row["out"]);
    }
}

fn family_of(s: &str) -> MintedFamily {
    match s {
        "classExprId" => MintedFamily::ClassExprId,
        "fnExprId" => MintedFamily::FnExprId,
        "param" => MintedFamily::Param,
        "fnDecl" => MintedFamily::FnDecl,
        _ => MintedFamily::VarOther,
    }
}

#[test]
fn summarize_census_matches_the_ts() {
    for row in rows_of("summarize") {
        let bindings: Vec<MintedBinding> = row["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| MintedBinding {
                name: b["name"].as_str().unwrap().to_string(),
                family: family_of(b["family"].as_str().unwrap()),
                derived_from: b["derivedFrom"].as_str().map(str::to_string),
                ref_count: b["refCount"].as_u64().unwrap() as usize,
                binding: crate::rename::validated::scopes::BindingId(0),
            })
            .collect();
        let free: Vec<String> = row["freeReferences"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
            .unwrap_or_default();
        let total = row["totalBindings"].as_u64().unwrap_or(0) as usize;
        let c = summarize_census(&bindings, total, free);
        let out = &row["out"];
        assert_eq!(out["total"], json!(c.total));
        assert_eq!(out["decorated"], json!(c.decorated));
        assert_eq!(out["names"], json!(c.names));
        assert_eq!(out["decoratedNames"], json!(c.decorated_names));
        assert_eq!(out["derivableExprIds"], json!(c.derivable_expr_ids));
        assert_eq!(out["zeroRefExprIds"], json!(c.zero_ref_expr_ids));
        let fam = &out["byFamily"];
        for (i, k) in ["classExprId", "fnExprId", "param", "fnDecl", "varOther"]
            .iter()
            .enumerate()
        {
            assert_eq!(fam[k], json!(c.by_family[i]), "byFamily.{k}");
        }
    }
}

// ---------------------------------------------------------------------------
// the passes over parsed text
// ---------------------------------------------------------------------------

#[test]
fn census_walk_and_free_references_match_the_ts() {
    for row in rows_of("census") {
        let text = row["text"].as_str().unwrap();
        let eligible = eligibility_for(&row, &[text]);
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, text);
        let state = RenameState::new(ingest.semantic(), Anchor::Fresh);
        let walk = collect_minted_bindings(ingest.semantic(), &state, &eligible);
        let entries: Vec<Value> = walk
            .entries
            .iter()
            .map(|e| {
                json!({
                    "name": e.name,
                    "family": e.family.as_str(),
                    "derivedFrom": e.derived_from,
                    "refCount": e.ref_count,
                })
            })
            .collect();
        assert_eq!(json!(entries), row["out"]["entries"], "census of {text}");
        assert_eq!(
            json!(walk.total_bindings),
            row["out"]["totalBindings"],
            "{text}"
        );
    }
    for row in rows_of("free-refs") {
        let text = row["text"].as_str().unwrap();
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, text);
        let state = RenameState::new(ingest.semantic(), Anchor::Fresh);
        assert_eq!(json!(collect_free_references(&state)), row["out"], "{text}");
    }
}

fn reconcile_options(o: &Value) -> ReconcileOptions {
    let flag = |k: &str| o[k].as_bool().unwrap_or(false);
    ReconcileOptions {
        apply: flag("apply"),
        descriptive_tier: flag("descriptiveTier"),
        max_hunk_lines: o["maxHunkLines"].as_u64().map_or(10, |v| v as usize),
        mixed_hunk_tier: flag("mixedHunkTier"),
        prior_line_count: o["priorLineCount"].as_u64().map(|v| v as usize),
        consumer_tier: flag("consumerTier"),
        prior_names: o["priorNames"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect()),
        last_resort_tier: flag("lastResortTier"),
        skip_import_declarations: flag("skipImportDeclarations"),
        skeleton_vote_tier: flag("skeletonVoteTier"),
        plant: None,
    }
}

#[test]
fn reconcile_matches_the_ts_on_every_unit_test_call() {
    for row in rows_of("reconcile") {
        let text = row["text"].as_str().unwrap();
        let diff = row["diffText"].as_str().unwrap();
        let eligible = eligibility_for(&row, &[text, diff]);
        let opts = reconcile_options(&row["options"]);
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, text);
        let semantic = ingest.semantic();
        let mut state = RenameState::new(semantic, Anchor::Generated);
        let r = reconcile_diff_noise(semantic, &mut state, diff, &eligible, &opts);
        let mut ours = json!({
            "renames": r.renames.iter().map(|x| json!({
                "fromName": x.from_name, "toName": x.to_name, "votes": x.votes,
                "kind": x.kind.as_str(), "declLine": x.decl_line, "applied": x.applied,
            })).collect::<Vec<_>>(),
            "skipped": r.skipped.iter().map(|x| json!({
                "fromName": x.from_name, "toName": x.to_name, "reason": x.reason, "votes": x.votes,
            })).collect::<Vec<_>>(),
            "hunks": {
                "changed": r.hunks.changed, "noise": r.hunks.noise, "genuine": r.hunks.genuine,
                "oversized": r.hunks.oversized, "tainted": r.hunks.tainted, "mixed": r.hunks.mixed,
            },
        });
        if r.prior_too_dissimilar {
            ours["priorTooDissimilar"] = json!(true);
        }
        assert_eq!(
            ours, row["out"],
            "reconcile of\n{text}\n--- diff ---\n{diff}"
        );
        assert_eq!(
            render_program(semantic, &state),
            row["output"].as_str().unwrap(),
            "rendered output of\n{text}"
        );
        assert_eq!(
            rust_trail(state.trail().entries()),
            ts_trail(&row),
            "trail of\n{text}"
        );
    }
}

#[test]
fn reconcile_step_matches_the_ts() {
    for row in rows_of("reconcile-step") {
        let text = row["text"].as_str().unwrap();
        let prior = row["prior"].as_str().unwrap();
        let eligible = eligibility_for(&row, &[text, prior]);
        let out = run_prior_diff_reconciliation(
            text,
            prior,
            &eligible,
            StrategyTrail::enabled(),
            None,
            false,
        );
        let ours = match &out {
            Err(_) => Value::Null,
            Ok(o) => {
                let mut by_reason = serde_json::Map::new();
                for s in &o.result.skipped {
                    let n = by_reason
                        .get(&s.reason)
                        .and_then(Value::as_u64)
                        .unwrap_or(0);
                    by_reason.insert(s.reason.clone(), json!(n + 1));
                }
                json!({
                    "stats": {
                        "renames": o.result.renames.len(),
                        "skipped": o.result.skipped.len(),
                        "skippedByReason": by_reason,
                    },
                    "renames": o.result.renames.iter()
                        .map(|r| json!({"fromName": r.from_name, "toName": r.to_name}))
                        .collect::<Vec<_>>(),
                    "code": o.code,
                })
            }
        };
        assert_eq!(ours, row["out"], "reconcile-step of\n{text}");
        if let Ok(o) = out {
            assert_eq!(rust_trail(o.trail.entries()), ts_trail(&row));
        }
    }
}

#[test]
fn floor_passes_match_the_ts() {
    for kind in ["class-id-floor", "decoration-retry"] {
        for row in rows_of(kind) {
            let text = row["text"].as_str().unwrap();
            let eligible = eligibility_for(&row, &[text]);
            let allocator = Allocator::default();
            let ingest = Ingest::parse_unambiguous(&allocator, text);
            let semantic = ingest.semantic();
            let taint = collect_eval_with_taint(semantic);
            let mut state = RenameState::new(semantic, Anchor::Fresh);
            let ours = if kind == "class-id-floor" {
                let r = derive_expression_inner_names(semantic, &mut state, &eligible, &taint);
                json!({
                    "derived": r.derived,
                    "skipped": r.skipped.iter().map(|s| json!({
                        "name": s.name, "toName": s.to_name, "reason": s.reason,
                    })).collect::<Vec<_>>(),
                })
            } else {
                let r = retry_decorated_names(semantic, &mut state, &eligible, &taint);
                json!({"undecorated": r.undecorated, "skipped": r.skipped})
            };
            assert_eq!(ours, row["out"], "{kind} of\n{text}");
            assert_eq!(
                render_program(semantic, &state),
                row["output"].as_str().unwrap()
            );
            assert_eq!(
                rust_trail(state.trail().entries()),
                ts_trail(&row),
                "{kind} trail"
            );
        }
    }
}

/// A provider answering from the TS's recorded calls, in order, and
/// checking every request is the TS's request.
struct RecordedProvider {
    calls: Vec<Value>,
    next: std::cell::Cell<usize>,
}

impl NameProvider for RecordedProvider {
    fn run_wave(&self, calls: Vec<LlmCall>) -> Vec<Result<BatchRenameResponse, LlmError>> {
        calls
            .into_iter()
            .map(|call| {
                let i = self.next.get();
                self.next.set(i + 1);
                let rec = &self.calls[i];
                let req = &rec["request"];
                assert_eq!(json!(call.request.code), req["code"], "request code #{i}");
                assert_eq!(json!(call.request.identifiers), req["identifiers"]);
                assert_eq!(
                    json!(call.request.used_names),
                    req["usedNames"],
                    "usedNames #{i}"
                );
                if rec.get("error").is_some() {
                    return Err(LlmError::new(LlmErrorKind::Other, "recorded error"));
                }
                let JsValue::Object(obj) = JsValue::parse(&rec["response"].to_string()).unwrap()
                else {
                    panic!("response is not an object");
                };
                Ok(BatchRenameResponse {
                    renames: Renames::from_js(&obj).unwrap(),
                    finish_reason: None,
                    usage: None,
                })
            })
            .collect()
    }
}

fn recorded(row: &Value) -> RecordedProvider {
    RecordedProvider {
        calls: row["calls"].as_array().unwrap().clone(),
        next: std::cell::Cell::new(0),
    }
}

fn anchor_of(s: &str) -> Anchor {
    match s {
        "generated" => Anchor::Generated,
        "reconciled" => Anchor::Reconciled,
        "shipped" => Anchor::Shipped,
        _ => Anchor::Fresh,
    }
}

fn mark_carried(
    semantic: &oxc_semantic::Semantic<'_>,
    state: &mut RenameState,
    eligible: &Eligibility,
    row: &Value,
) {
    let carried: HashSet<&str> = row["carried"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let walk = collect_minted_bindings(semantic, state, eligible);
    for e in walk.entries {
        if carried.contains(e.name.as_str()) {
            state.record_carried(e.binding);
        }
    }
}

#[test]
fn sweeps_match_the_ts() {
    let params = humanify_model::llm::CacheKeyParams::default();
    for row in rows_of("sweep") {
        let text = row["text"].as_str().unwrap();
        let eligible = eligibility_for(&row, &[text]);
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, text);
        let semantic = ingest.semantic();
        let taint = collect_eval_with_taint(semantic);
        let mut state = RenameState::new(semantic, anchor_of(row["spanAnchor"].as_str().unwrap()));
        mark_carried(semantic, &mut state, &eligible, &row);
        let provider = recorded(&row);
        let r = sweep_minted_names(semantic, &mut state, &eligible, &taint, &provider, &params);
        assert_eq!(provider.next.get(), row["calls"].as_array().unwrap().len());
        assert_eq!(
            json!({"named": r.named, "skipped": r.skipped, "groups": r.groups}),
            row["out"],
            "sweep of\n{text}"
        );
        assert_eq!(
            render_program(semantic, &state),
            row["output"].as_str().unwrap()
        );
        assert_eq!(
            rust_trail(state.trail().entries()),
            ts_trail(&row),
            "sweep trail"
        );
    }
    for row in rows_of("sweep-targets") {
        let text = row["text"].as_str().unwrap();
        let eligible = eligibility_for(&row, &[text]);
        let allocator = Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, text);
        let semantic = ingest.semantic();
        let taint = collect_eval_with_taint(semantic);
        let mut state = RenameState::new(semantic, Anchor::Fresh);
        mark_carried(semantic, &mut state, &eligible, &row);
        let names: Vec<String> = collect_sweep_targets(semantic, &state, &eligible, &taint)
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(json!(names), row["out"], "targets of\n{text}");
    }
    for row in rows_of("deferred-sweep") {
        let text = row["text"].as_str().unwrap();
        let eligible = eligibility_for(&row, &[text]);
        let provider = recorded(&row);
        let anchor = anchor_of(row["spanAnchor"].as_str().unwrap());
        let out = run_deferred_sweep(
            text,
            anchor,
            &eligible,
            &provider,
            &params,
            StrategyTrail::enabled(),
            false,
        );
        let ours = match &out {
            Err(_) => Value::Null,
            Ok(o) => json!({"named": o.sweep.named, "skipped": o.sweep.skipped, "code": o.code}),
        };
        assert_eq!(ours, row["out"], "deferred sweep of\n{text}");
        if let Ok(o) = out {
            assert_eq!(rust_trail(o.trail.entries()), ts_trail(&row));
        }
    }
}

#[test]
fn family_permute_matches_the_ts() {
    for row in rows_of("permute") {
        let text = row["text"].as_str().unwrap();
        let prior = row["prior"].as_str().unwrap();
        let eligible = eligibility_for(&row, &[text, prior]);
        let ours = match run_family_permute(text, prior, &eligible, None) {
            Err(_) => Value::Null,
            Ok(p) => json!({
                "applied": p.applied,
                "buckets": p.buckets,
                "skipped": p.skipped,
                "moves": p.moves.iter().map(|m| json!({
                    "from": m.from, "to": m.to, "support": m.support,
                })).collect::<Vec<_>>(),
                "code": p.code,
            }),
        };
        assert_eq!(
            ours, row["out"],
            "permute of\n{text}\n--- prior ---\n{prior}"
        );
    }
}
