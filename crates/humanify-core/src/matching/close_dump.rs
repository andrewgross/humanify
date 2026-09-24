//! The WP2.2 gate's dump: rebuild the TS matches-close.json's rows — the
//! close-match tier's candidate sets, assignment outcomes and corroboration
//! verdicts. The harness runs the tier with the SAME inputs the TS's
//! `buildCloseMatchContext` (prior-version.ts :865-983) gets: the function
//! cascade's final `matches` (post alternation + ordinal + interchangeable
//! pools), the two graphs, and the two fingerprint indexes.
//!
//! Rows anchor: prior spans on `"prior"`, fresh spans on `"fresh"` — oxc
//! spans are already UTF-8 byte offsets, the shape the TS writer converts
//! to at write time. The per-pair corroboration reuses the ported
//! machinery ([`super::close::corroborate`],
//! [`super::statement_align::compute_body_local_transfers`]) — this module
//! owns only the ROW assembly and the two derivations that need no hook
//! inside the tier:
//!
//! - [`derive_close_assignment_events`] — the TS
//!   `deriveCloseAssignmentEvents` (close-match.ts) mirrored: per-candidate
//!   outcomes replayed from the candidate list + the won set. The
//!   derivation is exact because the assignment marks an endpoint used ONLY
//!   on a win; a free-endpoint candidate outside the won set is exactly a
//!   tie abstention. The dump ASSERTS the replay's won set equals
//!   [`super::close::find_close_matches`]'s pairs — a drift between the two
//!   implementations of the derivation is a gate failure, not a silent
//!   mismatch.
//! - the candidate-vector reconstruction — [`super::close`]`::build_vector_map`
//!   is private (hands-off file), so the same loop is re-derived here from
//!   the public index API with a KEEP-IN-SYNC note; `score_pairs` itself is
//!   public and IS the scoring both sides share.
//!
//! The TS writes the file only when its artifact dump is armed, and skips
//! it entirely when one side has no unmatched functions (the early return
//! precedes the record); the parity dump writes it whenever the tier runs —
//! absent-on-both is agreement in the engine.

use std::collections::HashMap;

use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use serde_json::Value;

use humanify_model::dump::{
    CloseCandidateRow, CloseHintRow, CloseNamePair, ClosePairRow, CloseStatsRow, MatchesCloseFile,
    SpanKey,
};

use crate::hash::serialize::SymbolTables;

use super::close::{
    self, CloseCandidate, Corroboration, DEFAULT_CLOSE_MATCH_THRESHOLD, compute_feature_vector,
    score_pairs,
};
use super::statement_align::{AlignSide, compute_body_local_transfers, row_json_in};
use super::{IndexNode, row_node_ids};

/// The tier's inputs, all borrowed from [`super::matches_dump`]`s flow.
pub struct CloseDumpSides<'a> {
    pub prior_graph: &'a crate::graph::UnifiedGraph,
    pub fresh_graph: &'a crate::graph::UnifiedGraph,
    pub prior_semantic: &'a Semantic<'a>,
    pub fresh_semantic: &'a Semantic<'a>,
    pub prior_tables: &'a SymbolTables,
    pub fresh_tables: &'a SymbolTables,
    pub prior_index: &'a super::FingerprintIndex<'a>,
    pub fresh_index: &'a super::FingerprintIndex<'a>,
    /// The function cascade's final matches (prior session id → fresh).
    pub fn_matches: &'a HashMap<String, String>,
}

/// Build the matches-close.json content, or `None` when the tier would not
/// run at all (one side has no unmatched functions — the TS returns before
/// the tier and never records; absent-on-both is agreement).
///
/// `program_json` is each side's program JSON
/// ([`crate::ingest::program_estree_json`]) — the rows' JSON and the snap
/// gate's content lookups.
pub fn close_dump(
    sides: &CloseDumpSides<'_>,
    prior_program_json: &Value,
    fresh_program_json: &Value,
) -> Result<Option<MatchesCloseFile>, String> {
    // The content-lookup indexes: built ONCE per side (720 pairs share
    // them; a per-pair build cost 2.5s × 2 × 720 — the close dump's
    // runtime).
    let (prior_json_index, fresh_json_index) = super::statement_align::build_side_indexes(
        (sides.prior_semantic, prior_program_json),
        (sides.fresh_semantic, fresh_program_json),
    );
    let matched_prior: std::collections::HashSet<&String> = sides.fn_matches.keys().collect();
    let matched_fresh: std::collections::HashSet<&String> = sides.fn_matches.values().collect();
    // The TS's unmatched lists: graph-row order (the Map key order the
    // candidate scoring iterates in — load-bearing for tie order).
    let unmatched_prior: Vec<String> = sides
        .prior_graph
        .functions
        .iter()
        .filter(|f| !matched_prior.contains(&f.session_id))
        .map(|f| f.session_id.clone())
        .collect();
    let unmatched_fresh: Vec<String> = sides
        .fresh_graph
        .functions
        .iter()
        .filter(|f| !matched_fresh.contains(&f.session_id))
        .map(|f| f.session_id.clone())
        .collect();
    if unmatched_prior.is_empty() || unmatched_fresh.is_empty() {
        return Ok(None);
    }

    // The vector reconstruction (KEEP-IN-SYNC with close::build_vector_map —
    // private there; this re-derives it from the public index API so
    // `score_pairs` can be called for the candidate rows).
    let (prior_vectors, _) = rebuild_vectors(&unmatched_prior, sides.prior_index);
    let (fresh_vectors, _) = rebuild_vectors(&unmatched_fresh, sides.fresh_index);
    let candidates: Vec<CloseCandidate> = score_pairs(
        &prior_vectors,
        &fresh_vectors,
        DEFAULT_CLOSE_MATCH_THRESHOLD,
    );

    // The real assignment + the skip counters (the tier itself).
    let real = close::find_close_matches(
        &unmatched_prior,
        &unmatched_fresh,
        sides.prior_index,
        sides.fresh_index,
        None,
    );

    let events = derive_close_assignment_events(&candidates, &real.pairs);
    // Drift detector: the replay's won set must BE the assignment. A
    // mismatch means the two derivation implementations disagree — a gate
    // that compares rows built on a broken derivation is worse than no
    // gate.
    let won: std::collections::HashSet<(&str, &str)> = events
        .iter()
        .filter(|e| e.outcome == "won")
        .map(|e| (e.candidate.old_id.as_str(), e.candidate.new_id.as_str()))
        .collect();
    let assigned: std::collections::HashSet<(&str, &str)> = real
        .pairs
        .iter()
        .map(|p| (p.prior_id.as_str(), p.fresh_id.as_str()))
        .collect();
    if won != assigned {
        return Err(format!(
            "close-dump drift: the outcome replay's won set ({}) does not equal the assignment ({})",
            won.len(),
            assigned.len()
        ));
    }

    // Per-pair rows (assignment order — the TS's context insertion order;
    // the writer's sort makes the file order canonical either way).
    let prior_row_by_session: HashMap<&str, usize> = sides
        .prior_graph
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.session_id.as_str(), i))
        .collect();
    let fresh_row_by_session: HashMap<&str, usize> = sides
        .fresh_graph
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.session_id.as_str(), i))
        .collect();
    let prior_row_ids = row_node_ids(&sides.prior_graph.functions, sides.prior_semantic.nodes());
    let fresh_row_ids = row_node_ids(&sides.fresh_graph.functions, sides.fresh_semantic.nodes());

    let mut stats = CloseStatsRow::default();
    let mut pair_rows: Vec<ClosePairRow> = Vec::new();
    for pair in &real.pairs {
        let (Some(&prior_row), Some(&fresh_row)) = (
            prior_row_by_session.get(pair.prior_id.as_str()),
            fresh_row_by_session.get(pair.fresh_id.as_str()),
        ) else {
            // The TS skips a pair whose ids are missing from the function
            // maps — no row, no stats counter.
            continue;
        };
        let row = pair_row(
            sides,
            pair,
            PairRowCtx {
                prior_row,
                fresh_row,
                prior_row_ids: &prior_row_ids,
                fresh_row_ids: &fresh_row_ids,
                prior_json_index: &prior_json_index,
                fresh_json_index: &fresh_json_index,
            },
        )?;
        stats_row_bump(&mut stats, &row.verdict);
        pair_rows.push(row);
    }

    let mut candidate_rows: Vec<CloseCandidateRow> = events
        .iter()
        .map(|e| CloseCandidateRow {
            prior: session_span(sides.prior_graph, &e.candidate.old_id, "prior"),
            fresh: session_span(sides.fresh_graph, &e.candidate.new_id, "fresh"),
            score: e.candidate.score,
            score_bits: f64_bits_hex(e.candidate.score),
            rank: e.rank as u64,
            outcome: e.outcome.to_string(),
        })
        .collect();
    sort_rows(&mut candidate_rows, &mut pair_rows);

    Ok(Some(MatchesCloseFile {
        schema_version: 1,
        candidates: candidate_rows,
        pairs: pair_rows,
        stats,
        skipped_old: real.skipped_old as u64,
        skipped_new: real.skipped_new as u64,
    }))
}

/// The span key for a graph row's session id — the sentinel (-1/-1) when
/// the id is absent (the TS `spanOf` fallback).
fn session_span(graph: &crate::graph::UnifiedGraph, session_id: &str, label: &str) -> SpanKey {
    let span = graph
        .functions
        .iter()
        .find(|f| f.session_id == session_id)
        .map(|f| f.span);
    match span {
        Some(s) => SpanKey {
            text: label.to_string(),
            start: i64::from(s.start),
            end: i64::from(s.end),
        },
        None => SpanKey {
            text: label.to_string(),
            start: -1,
            end: -1,
        },
    }
}

/// The per-pair lookup structures, bundled to keep [`pair_row`]'s argument
/// list short.
struct PairRowCtx<'a> {
    prior_row: usize,
    fresh_row: usize,
    prior_row_ids: &'a HashMap<(u32, u32), (oxc_semantic::NodeId, AstKind<'a>)>,
    fresh_row_ids: &'a HashMap<(u32, u32), (oxc_semantic::NodeId, AstKind<'a>)>,
    prior_json_index: &'a super::statement_align::SideIndex<'a>,
    fresh_json_index: &'a super::statement_align::SideIndex<'a>,
}

/// One won pair's assembled row (its corroboration verdict included).
#[allow(clippy::too_many_lines)]
fn pair_row(
    sides: &CloseDumpSides<'_>,
    pair: &close::CloseMatchPair,
    ctx: PairRowCtx<'_>,
) -> Result<ClosePairRow, String> {
    let prior_span = sides.prior_graph.functions[ctx.prior_row].span;
    let fresh_span = sides.fresh_graph.functions[ctx.fresh_row].span;
    // The function INDEX row (the shingle sets' addressing) — the graph row
    // for a function entry.
    let prior_fn_idx = match sides
        .prior_index
        .entry_of_session(&pair.prior_id)
        .map(|i| &sides.prior_index.entries[i].node)
    {
        Some(IndexNode::Function(idx)) => *idx,
        _ => {
            return Err(format!(
                "close-dump: prior {} is not a function index entry",
                pair.prior_id
            ));
        }
    };
    let fresh_fn_idx = match sides
        .fresh_index
        .entry_of_session(&pair.fresh_id)
        .map(|i| &sides.fresh_index.entries[i].node)
    {
        Some(IndexNode::Function(idx)) => *idx,
        _ => {
            return Err(format!(
                "close-dump: fresh {} is not a function index entry",
                pair.fresh_id
            ));
        }
    };

    // The rows' JSON: each row's node in its side's program JSON.
    let prior_json = ctx
        .prior_row_ids
        .get(&(prior_span.start, prior_span.end))
        .and_then(|(_, kind)| row_json_in(ctx.prior_json_index, prior_span, kind));
    let fresh_json = ctx
        .fresh_row_ids
        .get(&(fresh_span.start, fresh_span.end))
        .and_then(|(_, kind)| row_json_in(ctx.fresh_json_index, fresh_span, kind));
    let (Some(prior_json), Some(fresh_json)) = (prior_json, fresh_json) else {
        return Err(format!(
            "close-dump: row JSON unavailable for {} / {}",
            pair.prior_id, pair.fresh_id
        ));
    };

    let prior_align = AlignSide::build(
        sides.prior_semantic,
        sides.prior_tables,
        ctx.prior_json_index,
        prior_json,
        prior_span,
    );
    let fresh_align = AlignSide::build(
        sides.fresh_semantic,
        sides.fresh_tables,
        ctx.fresh_json_index,
        fresh_json,
        fresh_span,
    );
    let alignment = compute_body_local_transfers(&prior_align, &fresh_align);
    let verdict = close::corroborate(
        alignment.aligned_statements,
        prior_fn_idx,
        fresh_fn_idx,
        sides.prior_index,
        sides.fresh_index,
    );

    // TS `nameTransfers` (:940-947): signature-position pairs FIRST (they
    // win on collision downstream), then the body-local ones — empty when
    // uncorroborated.
    let signature = if verdict == Corroboration::Uncorroborated {
        Vec::new()
    } else {
        partial_transfer(prior_json, fresh_json)
    };
    let transfers: Vec<CloseNamePair> = if verdict == Corroboration::Uncorroborated {
        Vec::new()
    } else {
        signature
            .iter()
            .map(|(new_name, prior_name)| CloseNamePair {
                old_name: new_name.clone(),
                new_name: prior_name.clone(),
            })
            .chain(alignment.transfers.iter().map(|t| CloseNamePair {
                old_name: t.old_name.clone(),
                new_name: t.new_name.clone(),
            }))
            .collect()
    };

    // TS `buildPriorNameHints` (:1237): the folded per-identifier hints —
    // transferred names excluded, shadowing siblings that disagree dropped,
    // snap eligibility AND-folded across occurrences.
    let (hints, snaps) = fold_hints(&alignment.hints, &transfers);

    Ok(ClosePairRow {
        prior: SpanKey {
            text: "prior".to_string(),
            start: i64::from(prior_span.start),
            end: i64::from(prior_span.end),
        },
        fresh: SpanKey {
            text: "fresh".to_string(),
            start: i64::from(fresh_span.start),
            end: i64::from(fresh_span.end),
        },
        verdict: verdict_label(verdict).to_string(),
        aligned_statements: alignment.aligned_statements as u64,
        total_new_statements: alignment.total_new_statements as u64,
        transfers,
        hints,
        snaps,
    })
}

fn verdict_label(verdict: Corroboration) -> &'static str {
    match verdict {
        Corroboration::Alignment => "alignment",
        Corroboration::Shingles => "shingles",
        Corroboration::Uncorroborated => "uncorroborated",
    }
}

fn stats_row_bump(stats: &mut CloseStatsRow, verdict: &str) {
    match verdict {
        "alignment" => stats.corroborated_by_alignment += 1,
        "shingles" => stats.corroborated_by_shingles += 1,
        _ => stats.uncorroborated += 1,
    }
}

/// A row JSON node's type name (empty when absent).
fn row_type(row: &Value) -> &str {
    row.get("type").and_then(Value::as_str).unwrap_or("")
}

/// TS `computePartialTransfer` (:1300) over the two rows' ESTree JSON: the
/// fn name (declarations/expressions only — methods carry none, matching
/// the TS's `getFunctionNameId`) and parameter i to parameter i, keyed by
/// the NEW name (an occupied key is OVERWRITTEN in place, insertion order
/// kept — the TS Record semantics). Only differing names transfer.
fn partial_transfer(prior_row: &Value, fresh_row: &Value) -> Vec<(String, String)> {
    let row_kind = row_type;
    let mut transfers: Vec<(String, String)> = Vec::new();
    let mut index_of: HashMap<String, usize> = HashMap::new();
    let mut record = |new_name: String, prior_name: String| match index_of.get(&new_name) {
        Some(&i) => transfers[i].1 = prior_name,
        None => {
            index_of.insert(new_name.clone(), transfers.len());
            transfers.push((new_name, prior_name));
        }
    };

    let params_of = |row: &Value| -> Vec<Value> {
        // Method rows carry the function under `value` (oxc nests; babel
        // has one node — the TS reads `node.params` directly, which is the
        // method's params).
        row.get("value")
            .and_then(|v| v.get("params"))
            .or_else(|| row.get("params"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let name_of = |row: &Value| -> Option<String> {
        if !matches!(row_kind(row), "FunctionDeclaration" | "FunctionExpression") {
            return None;
        }
        row.get("id")
            .and_then(|id| id.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };

    // The fn name (TS :1308-1312).
    if let (Some(prior_name), Some(new_name)) = (name_of(prior_row), name_of(fresh_row))
        && prior_name != new_name
    {
        record(new_name, prior_name);
    }

    // Parameter i to parameter i (TS :1314-1324), unwrapping the three
    // simple binding forms (`getParamIdentifier` :1272).
    let param_identifier = |param: &Value| -> Option<String> {
        match row_kind(param) {
            "Identifier" => param
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string),
            "AssignmentPattern" => param
                .get("left")
                .filter(|l| row_kind(l) == "Identifier")
                .and_then(|l| l.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string),
            "RestElement" => param
                .get("argument")
                .filter(|a| row_kind(a) == "Identifier")
                .and_then(|a| a.get("name"))
                .and_then(Value::as_str)
                .map(str::to_string),
            _ => None,
        }
    };
    let prior_params = params_of(prior_row);
    let fresh_params = params_of(fresh_row);
    for i in 0..prior_params.len().min(fresh_params.len()) {
        if let (Some(prior_name), Some(new_name)) = (
            param_identifier(&prior_params[i]),
            param_identifier(&fresh_params[i]),
        ) && prior_name != new_name
        {
            record(new_name, prior_name);
        }
    }
    transfers
}

/// TS `buildPriorNameHints` (:1237) + `accumulateHint` (:1209): per
/// minified name, a first sighting seeds it; a shadowing sibling with a
/// DIFFERENT prior name marks it ambiguous (dropped); a repeat ANDs the
/// snap eligibility. Transferred names are excluded (they appear as
/// already-renamed context). Returns (hints, snaps) in first-sighting
/// order — the writer's sort makes the file order canonical.
fn fold_hints(
    hints: &[super::statement_align::NameHint],
    transfers: &[CloseNamePair],
) -> (Vec<CloseHintRow>, Vec<CloseHintRow>) {
    let transferred: std::collections::HashSet<&str> =
        transfers.iter().map(|t| t.old_name.as_str()).collect();
    struct HintAccum {
        prior_name: Option<String>,
        snap: bool,
    }
    let mut by_name: Vec<(String, HintAccum)> = Vec::new();
    let mut index_of: HashMap<String, usize> = HashMap::new();
    for hint in hints {
        if transferred.contains(hint.new_name.as_str()) {
            continue;
        }
        match index_of.get(&hint.new_name) {
            Some(&i) => {
                if by_name[i].1.prior_name.as_deref() != Some(hint.prior_name.as_str()) {
                    by_name[i].1 = HintAccum {
                        prior_name: None,
                        snap: false,
                    };
                } else {
                    by_name[i].1.snap = by_name[i].1.snap && hint.snap_eligible;
                }
            }
            None => {
                index_of.insert(hint.new_name.clone(), by_name.len());
                by_name.push((
                    hint.new_name.clone(),
                    HintAccum {
                        prior_name: Some(hint.prior_name.clone()),
                        snap: hint.snap_eligible,
                    },
                ));
            }
        }
    }
    let mut hint_rows = Vec::new();
    let mut snap_rows = Vec::new();
    for (new_name, acc) in &by_name {
        let Some(prior_name) = &acc.prior_name else {
            continue;
        };
        hint_rows.push(CloseHintRow {
            new_name: new_name.clone(),
            prior_name: prior_name.clone(),
            snap_eligible: acc.snap,
        });
        if acc.snap {
            snap_rows.push(CloseHintRow {
                new_name: new_name.clone(),
                prior_name: prior_name.clone(),
                snap_eligible: true,
            });
        }
    }
    (hint_rows, snap_rows)
}

/// The vector reconstruction — KEEP-IN-SYNC with `close::build_vector_map`
/// (private there; the hands-off file): graph-row-order ids, featureless
/// ids skipped and counted, callee count from the fingerprint's callee
/// hashes.
fn rebuild_vectors(
    ids: &[String],
    index: &super::FingerprintIndex<'_>,
) -> (Vec<(String, close::FeatureVector)>, usize) {
    let mut vectors = Vec::with_capacity(ids.len());
    let mut skipped = 0usize;
    for id in ids {
        let entry = index.entry_of_session(id).map(|i| &index.entries[i]);
        let features = entry.and_then(|e| e.fingerprint.features());
        let (entry, features) = match (entry, features) {
            (Some(e), Some(f)) => (e, f),
            _ => {
                skipped += 1;
                continue;
            }
        };
        let callee_count = entry.fingerprint.callee_hashes().len();
        vectors.push((id.clone(), compute_feature_vector(features, callee_count)));
    }
    (vectors, skipped)
}

/// One candidate's fate in the greedy assignment — the TS
/// `CloseAssignmentEvent` (close-match.ts) mirrored. KEEP-IN-SYNC with the
/// TS derivation: the two must label the same candidate lists the same
/// way, or the drift assertion above fires.
struct CloseAssignmentEvent<'c> {
    candidate: &'c CloseCandidate,
    rank: usize,
    outcome: &'static str,
}

/// TS `deriveCloseAssignmentEvents`: replay the decision order (stable
/// descending sort) against the won set. Endpoints are marked used only
/// when a pair WINS, so a skip's cause is observable: a taken endpoint, or
/// — endpoints free and not in the won set — exactly the tie rule's
/// abstention.
fn derive_close_assignment_events<'c>(
    candidates: &'c [CloseCandidate],
    pairs: &[close::CloseMatchPair],
) -> Vec<CloseAssignmentEvent<'c>> {
    // The assignment's own sort: descending score, STABLE (`sort_by` is a
    // stable merge sort; NaN cannot occur — the cosine's norms are
    // positive by its early return).
    let mut order: Vec<&CloseCandidate> = candidates.iter().collect();
    order.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let won: std::collections::HashSet<(&str, &str)> = pairs
        .iter()
        .map(|p| (p.prior_id.as_str(), p.fresh_id.as_str()))
        .collect();
    let mut used_old: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut used_new: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut events = Vec::with_capacity(order.len());
    for (i, candidate) in order.iter().enumerate() {
        let key = (candidate.old_id.as_str(), candidate.new_id.as_str());
        if won.contains(&key) {
            used_old.insert(candidate.old_id.as_str());
            used_new.insert(candidate.new_id.as_str());
            events.push(CloseAssignmentEvent {
                candidate,
                rank: i + 1,
                outcome: "won",
            });
        } else if used_old.contains(candidate.old_id.as_str())
            || used_new.contains(candidate.new_id.as_str())
        {
            events.push(CloseAssignmentEvent {
                candidate,
                rank: i + 1,
                outcome: "abstained:taken",
            });
        } else {
            events.push(CloseAssignmentEvent {
                candidate,
                rank: i + 1,
                outcome: "abstained:tie",
            });
        }
    }
    events
}

/// The f64's IEEE bits, hex — the TS `f64BitsHex` (close-match.ts) exactly:
/// lowercase hex, no padding (`0x0` for zero).
fn f64_bits_hex(score: f64) -> String {
    format!("0x{:x}", score.to_bits())
}

/// The file's canonical row order (the TS writer's): candidates and pairs
/// by (prior, fresh) span key — `SpanKey`'s derived Ord is (text, start,
/// end), the TS `spanKeyOrder`.
fn sort_rows(candidates: &mut [CloseCandidateRow], pairs: &mut [ClosePairRow]) {
    candidates.sort_by(|a, b| (&a.prior, &a.fresh).cmp(&(&b.prior, &b.fresh)));
    pairs.sort_by(|a, b| (&a.prior, &a.fresh).cmp(&(&b.prior, &b.fresh)));
    for row in pairs.iter_mut() {
        row.transfers
            .sort_by(|a, b| (&a.old_name, &a.new_name).cmp(&(&b.old_name, &b.new_name)));
        row.hints.sort_by(|a, b| a.new_name.cmp(&b.new_name));
        row.snaps.sort_by(|a, b| a.new_name.cmp(&b.new_name));
    }
}

#[cfg(test)]
mod close_dump_test;
