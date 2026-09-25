//! The matching cascade (WP2.1 part 2) — TS original:
//! `src/analysis/fingerprint-index.ts` from `filterByMemberKey` (:143)
//! through `findNewFunctions` (:1507), ported line-for-line IN MUTATION
//! ORDER. The TS's mutation order is load-bearing: `demoteNonInjectiveMatches`
//! and `revokeCrossedContainers` deliberately delete from `matches` but NOT
//! from `resolutions` (the stats-vs-rows discrepancies in the oracle
//! matches.json are CORRECT), and the stats attribution loop runs BEFORE the
//! propagation post-pass. A clean-room "fix" of that staleness would diverge
//! by single digits — so the staleness is preserved and documented here.
//!
//! IDENTITY AND ORDER. The TS addresses everything by session-id string and
//! iterates Maps in insertion order; the Rust addresses entries by the same
//! session-id strings (the index's `entries` Vec IS the build order, so
//! every "iterate the fingerprints Map" loop iterates `entries` in Vec
//! order) and keeps `matches` in a hash map — safe because every consumer
//! of it is either a keyed lookup, an order-independent count, or
//! explicitly re-sorted before it can reach output bytes (07 §2; the house
//! rule: hash-map iteration order must never reach a decision). `ambiguous`
//! is NOT one of those maps: its INSERTION ORDER is a decision input —
//! propagation resolves entries in map order, so an entry's position decides
//! which other entries' claims it sees within an iteration — so it lives in
//! `AmbiguousMatches` (the TS `Map` semantics: parks append in walk order,
//! and a demote/revoke re-park of a MATCHED prior — which never parked —
//! appends at the map's END, exactly the TS's `Map.set` after no prior
//! entry). The one other place TS iteration order feeds a DECISION is the
//! candidate pool order — the strict `>` in `tryShingleResolve` keeps the
//! EARLIER candidate on a tie — and pool order comes from
//! `byStructuralHash` bucket lists, which are insertion-ordered `Vec`s in
//! both implementations.
//!
//! THE PROPAGATION HOOK: the TS calls `propagate` (:828-841) after the
//! stats attribution; the port lives in `crate::propagation` and runs
//! against the SAME ordered ambiguous map the cascade parked into — see
//! `match_functions` for the exact wiring.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde_json::json;

use super::match_map::MatchMap;
use super::statement_context::STMT_SPAN_BUCKETS;
use super::{
    CalleeShape, FingerprintIndex, IndexKind, IndexNode, SHINGLE_SIMILARITY_FLOOR,
    callee_shapes_equal, jaccard_similarity,
};
use crate::matching::statement_context::{StatementContexts, span_bucket};
use crate::propagation::{AmbiguousMatches, PropagationRungCounts};

// ---------------------------------------------------------------------------
// Resolutions (fingerprint-index.ts :215, :1266)
// ---------------------------------------------------------------------------

/// TS `Resolution` (:215) — which cascade stage produced a verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Identity,
    MemberKey,
    EnclosingStatement,
    CalleeShapes,
    CallerShapes,
    CalleeHashes,
    TwoHopShapes,
    ShingleSimilarity,
    Ambiguous,
}

impl Resolution {
    /// The TS union member's spelling (the artifact dumps carry it).
    pub fn as_str(self) -> &'static str {
        match self {
            Resolution::Identity => "identity",
            Resolution::MemberKey => "memberKey",
            Resolution::EnclosingStatement => "enclosingStatement",
            Resolution::CalleeShapes => "calleeShapes",
            Resolution::CallerShapes => "callerShapes",
            Resolution::CalleeHashes => "calleeHashes",
            Resolution::TwoHopShapes => "twoHopShapes",
            Resolution::ShingleSimilarity => "shingleSimilarity",
            Resolution::Ambiguous => "ambiguous",
        }
    }

    /// The matched form of this resolution, or None for "ambiguous" (the TS
    /// `if (resolution !== "ambiguous") state.resolutions.set(...)`).
    fn matched(self) -> Option<MatchedResolution> {
        match self {
            Resolution::Identity => Some(MatchedResolution::Identity),
            Resolution::MemberKey => Some(MatchedResolution::MemberKey),
            Resolution::EnclosingStatement => Some(MatchedResolution::EnclosingStatement),
            Resolution::CalleeShapes => Some(MatchedResolution::CalleeShapes),
            Resolution::CallerShapes => Some(MatchedResolution::CallerShapes),
            Resolution::CalleeHashes => Some(MatchedResolution::CalleeHashes),
            Resolution::TwoHopShapes => Some(MatchedResolution::TwoHopShapes),
            Resolution::ShingleSimilarity => Some(MatchedResolution::ShingleSimilarity),
            Resolution::Ambiguous => None,
        }
    }
}

/// TS `MatchedResolution` (:1266) — the stages that produce a match
/// (everything but "ambiguous"), plus the singleton tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedResolution {
    StructuralHashUnique,
    Identity,
    MemberKey,
    EnclosingStatement,
    CalleeShapes,
    CallerShapes,
    CalleeHashes,
    TwoHopShapes,
    ShingleSimilarity,
}

impl MatchedResolution {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchedResolution::StructuralHashUnique => "structuralHashUnique",
            MatchedResolution::Identity => "identity",
            MatchedResolution::MemberKey => "memberKey",
            MatchedResolution::EnclosingStatement => "enclosingStatement",
            MatchedResolution::CalleeShapes => "calleeShapes",
            MatchedResolution::CallerShapes => "callerShapes",
            MatchedResolution::CalleeHashes => "calleeHashes",
            MatchedResolution::TwoHopShapes => "twoHopShapes",
            MatchedResolution::ShingleSimilarity => "shingleSimilarity",
        }
    }

    /// TS `RESOLUTION_STAT_KEY` (:1291) — the stats field each stage bumps.
    fn bump(self, stats: &mut ResolutionStats) {
        match self {
            MatchedResolution::StructuralHashUnique => stats.structural_hash_unique += 1,
            MatchedResolution::Identity => stats.identity_resolved += 1,
            MatchedResolution::MemberKey => stats.member_key_resolved += 1,
            MatchedResolution::EnclosingStatement => stats.enclosing_statement_resolved += 1,
            MatchedResolution::CalleeShapes => stats.callee_shapes_resolved += 1,
            MatchedResolution::CallerShapes => stats.caller_shapes_resolved += 1,
            MatchedResolution::CalleeHashes => stats.callee_hashes_resolved += 1,
            MatchedResolution::TwoHopShapes => stats.two_hop_shapes_resolved += 1,
            MatchedResolution::ShingleSimilarity => stats.shingle_similarity_resolved += 1,
        }
    }
}

// ---------------------------------------------------------------------------
// The stats bag (fingerprint-index.ts :43 emptyResolutionStats; types.ts)
// ---------------------------------------------------------------------------

/// TS `EnclosingStmtAbstainCounts` (types.ts) — why the enclosing-statement
/// rung did not resolve, plus where its arrivals sat (`reachedSpanBuckets`,
/// indexed by [`STMT_SPAN_BUCKETS`]; the "unknown" slot is last).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnclosingStmtAbstainCounts {
    pub no_hash_is_statement: usize,
    pub no_hash_too_long: usize,
    pub no_hash_other: usize,
    pub no_new_holders: usize,
    pub count_mismatch: usize,
    pub partner_filtered: usize,
    pub reached: usize,
    pub resolved_local: usize,
    pub resolved_spanning: usize,
    pub count_mismatch_local: usize,
    pub count_mismatch_spanning: usize,
    pub spanning_parent_agrees: usize,
    pub spanning_parent_disagrees: usize,
    pub spanning_parent_unknown: usize,
    pub reached_span_buckets: [usize; STMT_SPAN_BUCKETS.len()],
}

impl EnclosingStmtAbstainCounts {
    /// The TS bag, byte-for-value (the gate compares the whole bag).
    pub fn to_ts_value(&self) -> serde_json::Value {
        let buckets: serde_json::Map<String, serde_json::Value> = STMT_SPAN_BUCKETS
            .iter()
            .zip(self.reached_span_buckets)
            .map(|(name, count)| (name.to_string(), json!(count)))
            .collect();
        json!({
            "noHashIsStatement": self.no_hash_is_statement,
            "noHashTooLong": self.no_hash_too_long,
            "noHashOther": self.no_hash_other,
            "noNewHolders": self.no_new_holders,
            "countMismatch": self.count_mismatch,
            "partnerFiltered": self.partner_filtered,
            "reached": self.reached,
            "resolvedLocal": self.resolved_local,
            "resolvedSpanning": self.resolved_spanning,
            "countMismatchLocal": self.count_mismatch_local,
            "countMismatchSpanning": self.count_mismatch_spanning,
            "spanningParentAgrees": self.spanning_parent_agrees,
            "spanningParentDisagrees": self.spanning_parent_disagrees,
            "spanningParentUnknown": self.spanning_parent_unknown,
            "reachedSpanBuckets": buckets,
        })
    }
}

/// TS `ResolutionStats` (types.ts) — the whole matching pass's counters.
/// Not used to steer the pipeline; consumed by the parity gate (whole-bag
/// equality against the TS) and the experiment harnesses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolutionStats {
    pub structural_hash_unique: usize,
    pub identity_resolved: usize,
    pub member_key_resolved: usize,
    pub enclosing_statement_resolved: usize,
    pub callee_shapes_resolved: usize,
    pub caller_shapes_resolved: usize,
    pub callee_hashes_resolved: usize,
    pub two_hop_shapes_resolved: usize,
    pub shingle_similarity_resolved: usize,
    pub shingle_unconsultable: usize,
    pub ordinal_resolved: usize,
    pub interchangeable_resolved: usize,
    pub injectivity_demoted: usize,
    pub singleton_rejected: usize,
    pub singleton_unguarded: usize,
    pub still_ambiguous: usize,
    pub unmatched: usize,
    pub propagation_resolved: usize,
    pub propagation_by_rung: PropagationRungCounts,
    pub crossed_container_revoked: usize,
    pub enclosing_stmt_abstain: EnclosingStmtAbstainCounts,
}

impl ResolutionStats {
    /// The TS bag, byte-for-value (`emptyResolutionStats` :43's shape).
    pub fn to_ts_value(&self) -> serde_json::Value {
        let r = &self.propagation_by_rung;
        json!({
            "structuralHashUnique": self.structural_hash_unique,
            "identityResolved": self.identity_resolved,
            "memberKeyResolved": self.member_key_resolved,
            "enclosingStatementResolved": self.enclosing_statement_resolved,
            "calleeShapesResolved": self.callee_shapes_resolved,
            "callerShapesResolved": self.caller_shapes_resolved,
            "calleeHashesResolved": self.callee_hashes_resolved,
            "twoHopShapesResolved": self.two_hop_shapes_resolved,
            "shingleSimilarityResolved": self.shingle_similarity_resolved,
            "shingleUnconsultable": self.shingle_unconsultable,
            "ordinalResolved": self.ordinal_resolved,
            "interchangeableResolved": self.interchangeable_resolved,
            "injectivityDemoted": self.injectivity_demoted,
            "singletonRejected": self.singleton_rejected,
            "singletonUnguarded": self.singleton_unguarded,
            "stillAmbiguous": self.still_ambiguous,
            "unmatched": self.unmatched,
            "propagationResolved": self.propagation_resolved,
            "propagationByRung": {
                "matchedCallee": r.matched_callee,
                "matchedCaller": r.matched_caller,
                "scopeParent": r.scope_parent,
                "externalRefs": r.external_refs,
                "scopeOrdinal": r.scope_ordinal,
            },
            "crossedContainerRevoked": self.crossed_container_revoked,
            "enclosingStmtAbstain": self.enclosing_stmt_abstain.to_ts_value(),
        })
    }
}

// ---------------------------------------------------------------------------
// Options + result shapes (fingerprint-index.ts :746, types.ts MatchResult)
// ---------------------------------------------------------------------------

/// TS `resolveAmbiguousCandidate` — the caller-supplied disambiguator: given
/// an ambiguous old-side id and its candidates, return the single matching
/// candidate (or None). The TS options field holds a bare function; the type
/// exists here because clippy reads the bare form as a very complex type.
pub type AmbiguityResolver<'o> = &'o dyn Fn(&str, &[String]) -> Option<String>;

/// TS `MatchOptions` (:746). The DEFAULT is the TS's optionless call:
/// depth 2, no exclusions, no resolver, propagation off.
pub struct MatchOptions<'o> {
    /// Maximum cascade depth (0 = uniqueHash + memberKey only, 1 = also
    /// calleeShapes + callerShapes, 2 = also calleeHashes + twoHopShapes +
    /// shingle). Default 2.
    pub max_cascade_depth: u8,
    /// SessionIds to exclude from matching (e.g., Bun CJS wrapper functions
    /// that always change between versions).
    pub exclude_session_ids: Option<&'o HashSet<String>>,
    /// Enable call-graph propagation to resolve ambiguous functions using
    /// confirmed matches as constraints. Default false. STUB — see the
    /// module doc and `match_functions`.
    pub enable_propagation: bool,
    /// Caller-supplied disambiguator, tried before all fingerprint stages:
    /// given an ambiguous old-side id and its candidates, return the single
    /// matching candidate (or None).
    pub resolve_ambiguous_candidate: Option<AmbiguityResolver<'o>>,
    /// Matched-binding reference evidence for propagation (see
    /// `propagation::ExternalRefEvidence`). Only consulted when
    /// `enable_propagation` is set; the stub never reads it.
    pub external_ref_evidence: Option<crate::propagation::ExternalRefEvidence>,
}

impl Default for MatchOptions<'_> {
    fn default() -> Self {
        MatchOptions {
            max_cascade_depth: 2,
            exclude_session_ids: None,
            enable_propagation: false,
            resolve_ambiguous_candidate: None,
            external_ref_evidence: None,
        }
    }
}

impl MatchOptions<'_> {
    /// TS `options?.maxCascadeDepth ?? 2` — the pipeline only ever passes
    /// 0/1/2 (the TS type is the literal union), so anything above clamps.
    fn depth(&self) -> u8 {
        self.max_cascade_depth.min(2)
    }
}

/// TS `MatchResult` (types.ts). `matches` is keyed by the PRIOR session id
/// ("input.js:L:C" / "module:<name>"), valued by the FRESH session id — the
/// TS convention the artifact dumps and propagation both read.
#[derive(Debug, Clone, Default)]
pub struct MatchResult {
    pub matches: MatchMap,
    /// TS `MatchResult.ambiguous: Map<string, string[]>` — the ORDERED map
    /// (`AmbiguousMatches`), because the map's insertion order is a
    /// propagation decision input. Consumers that only size or look up use
    /// `len`/`contains`/`get`; consumers that iterate sort first unless
    /// order is the point.
    pub ambiguous: AmbiguousMatches,
    pub unmatched: Vec<String>,
    pub demoted_priors: BTreeSet<String>,
    pub resolution_stats: ResolutionStats,
    pub pair_resolutions: Vec<PairResolution>,
    pub pair_rejections: Vec<PairRejection>,
}

/// One observation row: prior → fresh with the tier that closed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairResolution {
    pub prior: String,
    pub fresh: String,
    /// TS `tier` — a `MatchedResolution` spelling, or "propagation" for a
    /// pair the propagation post-pass closed, or "ordinal"/"interchangeable"
    /// for the tail tiers.
    pub tier: String,
}

/// One rejection row with its class (fingerprint-index.ts :853).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairRejection {
    pub prior: String,
    pub kind: RejectionKind,
    /// The candidate pool, for stillAmbiguous rows.
    pub candidates: Option<Vec<String>>,
}

/// TS `pairRejections`' kind union (:853-856).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectionKind {
    Unmatched,
    StillAmbiguous,
    Demoted,
    Revoked,
}

impl RejectionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RejectionKind::Unmatched => "unmatched",
            RejectionKind::StillAmbiguous => "stillAmbiguous",
            RejectionKind::Demoted => "demoted",
            RejectionKind::Revoked => "revoked",
        }
    }
}

/// One side of the cascade: the fingerprint index over a graph plus that
/// side's statement contexts (the enclosing-statement rung's evidence).
#[derive(Clone, Copy)]
pub struct Side<'a, 'g> {
    pub index: &'a FingerprintIndex<'g>,
    pub ctx: &'a StatementContexts,
}

impl<'a, 'g> Side<'a, 'g> {
    pub fn new(index: &'a FingerprintIndex<'g>, ctx: &'a StatementContexts) -> Self {
        Side { index, ctx }
    }

    /// The entry fingerprint for a session id.
    fn fingerprint(&self, session_id: &str) -> Option<&super::FunctionFingerprint> {
        self.index
            .entry_of_session(session_id)
            .map(|i| &self.index.entries[i].fingerprint)
    }

    /// The graph row index when this side holds FUNCTIONS (TS
    /// `index.functions?.get(id)` — undefined on a binding index).
    fn fn_row(&self, session_id: &str) -> Option<usize> {
        if self.index.kind != IndexKind::Functions {
            return None;
        }
        self.index
            .entry_of_session(session_id)
            .and_then(|i| match self.index.entries[i].node {
                IndexNode::Function(row) => Some(row),
                IndexNode::Binding(_) => None,
            })
    }

    /// TS `getEnclosingStmtHash` (:349): the function's enclosing-statement
    /// hash or the binding's neighbor-context hash, from the precomputed
    /// contexts.
    fn context_hash(&self, session_id: &str) -> Option<&str> {
        let i = self.index.entry_of_session(session_id)?;
        self.ctx.context_hash(self.index.entries[i].node)
    }

    /// TS `fn.scopeParent?.sessionId` — undefined on a binding side or when
    /// the row has no scope parent.
    fn scope_parent_session(&self, session_id: &str) -> Option<&str> {
        let row = self.fn_row(session_id)?;
        let parent = self.index.graph.functions[row].scope_parent?;
        self.index.function_session_of_span(parent)
    }
}

/// A byte sort of session-id strings — equal to the TS's default string
/// compare (`a.prior < b.prior ? -1 : ...`) for the ASCII ids the pipeline
/// produces (`input.js:L:C`, `module:<name>`).
fn ts_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.as_bytes().cmp(b.as_bytes())
}

/// TS `sessionPosition` (:392): `(Number(parts[1]) || 0, Number(parts[2]) || 0)`
/// from a `file:line:col` id — "module:x" lands on (0, 0) since `Number`
/// gives NaN there.
fn session_position(session_id: &str) -> (i64, i64) {
    let num_or_zero =
        |part: Option<&str>| -> i64 { part.and_then(|p| p.parse().ok()).unwrap_or(0) };
    let mut parts = session_id.split(':');
    let _file = parts.next();
    let line = num_or_zero(parts.next());
    let col = num_or_zero(parts.next());
    (line, col)
}

/// TS `bySessionPosition` (:397): `al - bl || ac - bc`.
fn by_session_position(a: &str, b: &str) -> std::cmp::Ordering {
    let (al, ac) = session_position(a);
    let (bl, bc) = session_position(b);
    al.cmp(&bl).then(ac.cmp(&bc))
}

/// TS `arraysEqual` (:1471) — elementwise equality over the sorted arrays.
fn arrays_equal(a: &[String], b: &[String]) -> bool {
    a == b
}

// ---------------------------------------------------------------------------
// The filters (:143-:212)
// ---------------------------------------------------------------------------

/// TS `filterByMemberKey` (:143). May return [] — every candidate
/// contradicting the old memberKey is a contradiction the caller must STOP
/// on, not fall through.
///
/// DELIBERATE asymmetry with `singleton_verdict`, which treats an absent key
/// as missing evidence rather than disagreement: here there are RIVALS, so a
/// candidate that cannot carry the key loses to ones that do — and when all
/// of them lack it, the empty pool parks the prior as ambiguous (a missed
/// match), never a wrong one. With rivals, absence loses to presence;
/// without rivals, absence is not evidence.
fn filter_by_member_key(
    candidates: &[String],
    old_key: Option<&str>,
    new_side: &Side<'_, '_>,
) -> Vec<String> {
    let Some(old_key) = old_key else {
        return candidates.to_vec();
    };
    candidates
        .iter()
        .filter(|id| new_side.fingerprint(id).and_then(|fp| fp.member_key()) == Some(old_key))
        .cloned()
        .collect()
}

/// TS `filterByCalleeShapes` (:166) — blurred callee structural shapes.
fn filter_by_callee_shapes(
    candidates: &[String],
    old_shapes: &[CalleeShape],
    new_side: &Side<'_, '_>,
) -> Vec<String> {
    candidates
        .iter()
        .filter(|id| {
            new_side
                .fingerprint(id)
                .is_some_and(|fp| callee_shapes_equal(old_shapes, fp.callee_shapes()))
        })
        .cloned()
        .collect()
}

/// TS `filterByCallerShapes` (:178) — blurred caller structural shapes.
fn filter_by_caller_shapes(
    candidates: &[String],
    old_shapes: &[CalleeShape],
    new_side: &Side<'_, '_>,
) -> Vec<String> {
    candidates
        .iter()
        .filter(|id| {
            new_side
                .fingerprint(id)
                .is_some_and(|fp| callee_shapes_equal(old_shapes, fp.caller_shapes()))
        })
        .cloned()
        .collect()
}

/// TS `filterByCalleeHashes` (:190) — exact callee hashes.
fn filter_by_callee_hashes(
    candidates: &[String],
    old_hashes: &[String],
    new_side: &Side<'_, '_>,
) -> Vec<String> {
    candidates
        .iter()
        .filter(|id| {
            new_side
                .fingerprint(id)
                .is_some_and(|fp| arrays_equal(old_hashes, fp.callee_hashes()))
        })
        .cloned()
        .collect()
}

/// TS `filterByTwoHopShapes` (:202).
fn filter_by_two_hop_shapes(
    candidates: &[String],
    old_shapes: &[String],
    new_side: &Side<'_, '_>,
) -> Vec<String> {
    candidates
        .iter()
        .filter(|id| {
            new_side
                .fingerprint(id)
                .is_some_and(|fp| arrays_equal(old_shapes, fp.two_hop_shapes()))
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// The shingle tiebreaker (:234)
// ---------------------------------------------------------------------------

/// TS `tryShingleResolve` (:234): Jaccard similarity between the old
/// function's shingle set and each candidate's; accept the best iff it
/// clears the floor and beats the runner-up. Strict `>` comparisons — the
/// EARLIER candidate wins a tie (pool order is decision input).
fn try_shingle_resolve(
    old_id: &str,
    candidates: &[String],
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
) -> Option<String> {
    // TS: `oldIndex.functions?.get(oldId)` then `!newIndex.functions` — both
    // sides must be function sides.
    let old_row = old_side.fn_row(old_id)?;
    if new_side.index.kind != IndexKind::Functions {
        return None;
    }

    let old_shingles = old_side.index.compute_shingle_set(old_row);
    if old_shingles.is_empty() {
        return None;
    }

    let mut best_id: Option<String> = None;
    let mut best_sim = -1.0f64;
    let mut second_best_sim = -1.0f64;
    for cand_id in candidates {
        let Some(new_row) = new_side.fn_row(cand_id) else {
            continue;
        };
        let sim = jaccard_similarity(&old_shingles, &new_side.index.compute_shingle_set(new_row));
        if sim > best_sim {
            second_best_sim = best_sim;
            best_sim = sim;
            best_id = Some(cand_id.clone());
        } else if sim > second_best_sim {
            second_best_sim = sim;
        }
    }

    // Accept if above threshold and clearly better than runner-up.
    match best_id {
        Some(id) if best_sim >= SHINGLE_SIMILARITY_FLOOR && best_sim > second_best_sim => Some(id),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The calleeHash cascade (:290)
// ---------------------------------------------------------------------------

/// TS `CalleeHashOutcome` (:283).
enum CalleeHashOutcome {
    Match {
        id: String,
        resolution: MatchedResolution,
    },
    /// A stage filtered every candidate out — stop, don't widen.
    Contradiction,
    /// Narrowed (or unchanged) but still >1 candidates.
    Ambiguous { pool: Vec<String> },
}

/// TS `tryCalleeHashCascade` (:290): exact callee hashes, then two-hop
/// shapes.
fn try_callee_hash_cascade(
    candidates: &[String],
    old_fp: &super::FunctionFingerprint,
    new_side: &Side<'_, '_>,
) -> CalleeHashOutcome {
    let callee_hash_candidates =
        filter_by_callee_hashes(candidates, old_fp.callee_hashes(), new_side);
    if callee_hash_candidates.is_empty() {
        return CalleeHashOutcome::Contradiction;
    }
    if callee_hash_candidates.len() == 1 {
        return CalleeHashOutcome::Match {
            id: callee_hash_candidates[0].clone(),
            resolution: MatchedResolution::CalleeHashes,
        };
    }

    let two_hop_candidates =
        filter_by_two_hop_shapes(&callee_hash_candidates, old_fp.two_hop_shapes(), new_side);
    if two_hop_candidates.is_empty() {
        return CalleeHashOutcome::Contradiction;
    }
    if two_hop_candidates.len() == 1 {
        return CalleeHashOutcome::Match {
            id: two_hop_candidates[0].clone(),
            resolution: MatchedResolution::TwoHopShapes,
        };
    }

    CalleeHashOutcome::Ambiguous {
        pool: two_hop_candidates,
    }
}

// ---------------------------------------------------------------------------
// The identity resolver (:326)
// ---------------------------------------------------------------------------

/// TS `tryIdentityResolve` (:326): run the caller-supplied identity
/// resolver; record the match when unique.
fn try_identity_resolve(
    old_id: &str,
    candidates: &[String],
    resolver: Option<AmbiguityResolver<'_>>,
    matches: &mut MatchMap,
) -> bool {
    let Some(resolver) = resolver else {
        return false;
    };
    if let Some(resolved) = resolver(old_id, candidates)
        && candidates.contains(&resolved)
    {
        matches.insert(old_id.to_string(), resolved);
        return true;
    }
    false
}

// ---------------------------------------------------------------------------
// The enclosing-statement rung (:417) and its bookkeeping
// ---------------------------------------------------------------------------

/// TS `recordArrival` (:484): record one arrival at the rung and return its
/// enclosing-statement hash, or None with the reason counted. Classifies
/// BEFORE asking for the hash — the hash is cached and returns a bare None,
/// so why it was None is only knowable here, and it asks the same owner the
/// cap itself uses (`statement_usability`), so the counter cannot disagree
/// with the rule that did the excluding.
fn record_arrival(
    old_id: &str,
    old_side: &Side<'_, '_>,
    abstain: &mut EnclosingStmtAbstainCounts,
) -> Option<String> {
    abstain.reached += 1;
    // TS: `fnNode?.path.getStatementParent()` — absent on a binding side
    // (noNode, "unknown" bucket).
    let fn_ctx = old_side
        .fn_row(old_id)
        .and_then(|row| old_side.ctx.fn_context(row));
    let (usability_lines, is_own_statement) = match fn_ctx {
        Some(ctx) => (
            match ctx.usability {
                super::statement_context::StmtUsability::Ok { lines }
                | super::statement_context::StmtUsability::TooLong { lines } => Some(lines),
                super::statement_context::StmtUsability::NoNode => None,
            },
            ctx.is_own_statement,
        ),
        None => (None, false),
    };
    // TS: `isOwnStatement ? "unknown" : spanBucket(usability.span)` —
    // "unknown" is the LAST bucket slot.
    let bucket = if is_own_statement {
        STMT_SPAN_BUCKETS.len() - 1
    } else {
        span_bucket(usability_lines)
    };
    abstain.reached_span_buckets[bucket] += 1;

    let hash = old_side.context_hash(old_id).map(str::to_string);
    if hash.is_some() {
        return hash;
    }
    if is_own_statement {
        abstain.no_hash_is_statement += 1;
    } else if matches!(
        fn_ctx.map(|c| c.usability),
        Some(super::statement_context::StmtUsability::TooLong { .. })
    ) {
        abstain.no_hash_too_long += 1;
    } else {
        abstain.no_hash_other += 1;
    }
    None
}

/// TS `distinctStatements` (:507): how many distinct enclosing-statement
/// NODES the holders sit in (binding holders contribute nothing — the TS's
/// `index.functions?.get(id)` is undefined there).
fn distinct_statements(holders: &[String], side: &Side<'_, '_>) -> usize {
    let mut nodes: HashSet<oxc_semantic::NodeId> = HashSet::new();
    for id in holders {
        if let Some(row) = side.fn_row(id)
            && let Some(ctx) = side.ctx.fn_context(row)
            && let Some(node) = ctx.stmt_node_id
        {
            nodes.insert(node);
        }
    }
    nodes.len()
}

/// TS `recordParentAgreement` (:524): does a spanning pair's enclosing
/// function agree with a match already made? Undercounts agreement — the
/// matches map is still filling — so read `disagrees` as a floor on the
/// error rate, not a rate.
fn record_parent_agreement(
    old_id: &str,
    new_id: &str,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
    matches: &HashMap<String, String>,
    abstain: &mut EnclosingStmtAbstainCounts,
) {
    let old_parent = old_side.scope_parent_session(old_id);
    let new_parent = new_side.scope_parent_session(new_id);
    let (Some(old_parent), Some(new_parent)) = (old_parent, new_parent) else {
        abstain.spanning_parent_unknown += 1;
        return;
    };
    match matches.get(old_parent) {
        None => abstain.spanning_parent_unknown += 1,
        Some(claimed) if claimed == new_parent => abstain.spanning_parent_agrees += 1,
        Some(_) => abstain.spanning_parent_disagrees += 1,
    }
}

/// TS `tryEnclosingStatementResolve` (:417): resolve a bucket member by its
/// enclosing statement's rename-invariant hash. RESOLVER semantics, not a
/// filter — an enclosing statement legitimately drifts between versions, so
/// a no-holder falls through to the next stage.
///
/// Unique on both sides → the 1:1 claim. EQUAL counts above one → the
/// members are semantically interchangeable, so pair by source ordinal (the
/// bucket-level frame, not candidate-level, so every old member computes the
/// SAME bijection). Unequal counts stay ambiguous.
#[allow(clippy::too_many_arguments)]
fn try_enclosing_statement_resolve(
    old_id: &str,
    candidates: &[String],
    old_fp: &super::FunctionFingerprint,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
    matches: &mut MatchMap,
    abstain: &mut EnclosingStmtAbstainCounts,
) -> Option<String> {
    let hash = record_arrival(old_id, old_side, abstain)?;

    let old_bucket = bucket_session_ids(old_side.index, old_fp.structural_hash());
    let new_bucket = bucket_session_ids(new_side.index, old_fp.structural_hash());
    // The ordinal frame is bucket-level (not candidate-level) so every old
    // member of the group computes the SAME bijection regardless of its own
    // upstream candidate filtering.
    let old_holders: Vec<String> = old_bucket
        .iter()
        .filter(|id| old_side.context_hash(id) == Some(hash.as_str()))
        .cloned()
        .collect();
    let new_holders: Vec<String> = new_bucket
        .iter()
        .filter(|id| new_side.context_hash(id) == Some(hash.as_str()))
        .cloned()
        .collect();
    if new_holders.is_empty() {
        abstain.no_new_holders += 1;
        return None;
    }
    // A group is LOCAL only when every holder on BOTH sides sits in one
    // statement. Anything else pools unrelated statements that merely hash
    // the same, and pairing those by source position is bundle-scale
    // positional assignment.
    let is_local = distinct_statements(&old_holders, old_side) <= 1
        && distinct_statements(&new_holders, new_side) <= 1;
    if old_holders.len() != new_holders.len() {
        abstain.count_mismatch += 1;
        if is_local {
            abstain.count_mismatch_local += 1;
        } else {
            abstain.count_mismatch_spanning += 1;
        }
        return None;
    }

    let mut old_ordered = old_holders;
    let mut new_ordered = new_holders;
    old_ordered.sort_by(|a, b| by_session_position(a, b));
    new_ordered.sort_by(|a, b| by_session_position(a, b));
    let position = old_ordered.iter().position(|id| id == old_id);
    let matched = position.and_then(|i| new_ordered.get(i)).cloned();
    // A member filtered from THIS old's candidates was rejected by stronger
    // evidence upstream (memberKey contradiction) — never claim across it.
    if let Some(matched) = matched
        && candidates.contains(&matched)
    {
        if is_local {
            abstain.resolved_local += 1;
        } else {
            abstain.resolved_spanning += 1;
            record_parent_agreement(old_id, &matched, old_side, new_side, matches, abstain);
        }
        return Some(matched);
    }
    abstain.partner_filtered += 1;
    None
}

/// The session ids of one structural-hash bucket, in bucket-insertion order
/// (the TS `byStructuralHash.get(hash) ?? []`).
fn bucket_session_ids<'g>(index: &FingerprintIndex<'g>, hash: &str) -> Vec<String> {
    index
        .bucket(hash)
        .unwrap_or(&[])
        .iter()
        .map(|&i| index.entries[i].session_id.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// resolveMatch (:597) + resolveDeepStages (:703)
// ---------------------------------------------------------------------------

/// TS `resolveMatch` (:597): the disambiguation cascade in order — identity,
/// memberKey, enclosingStatement, depth gate, calleeShapes, callerShapes,
/// then the deep stages. THE ORDER IS LOAD-BEARING; each ambiguous park uses
/// the pool the FAILING stage narrowed to.
#[allow(clippy::too_many_arguments)]
fn resolve_match(
    old_id: &str,
    candidates: &[String],
    old_fp: &super::FunctionFingerprint,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
    matches: &mut MatchMap,
    ambiguous: &mut AmbiguousMatches,
    max_cascade_depth: u8,
    stats: &mut ResolutionStats,
    resolver: Option<AmbiguityResolver<'_>>,
) -> Resolution {
    // Caller-supplied identity resolution runs first — it carries evidence
    // the fingerprint fields cannot (e.g. correspondence under an existing
    // match result), which stays discriminating even when candidates wrap
    // structurally identical code.
    if try_identity_resolve(old_id, candidates, resolver, matches) {
        return Resolution::Identity;
    }

    // memberKey disambiguation: runs before callee shapes.
    let mk_candidates = filter_by_member_key(candidates, old_fp.member_key(), new_side);
    if mk_candidates.is_empty() {
        // Contradiction: every candidate carries a different memberKey than
        // the old function. Stop — a candidate rejected by strong evidence
        // must not win at a weaker stage.
        ambiguous.insert(old_id.to_string(), candidates.to_vec());
        return Resolution::Ambiguous;
    }
    if mk_candidates.len() == 1 {
        matches.insert(old_id.to_string(), mk_candidates[0].clone());
        return Resolution::MemberKey;
    }

    let stmt_match = try_enclosing_statement_resolve(
        old_id,
        &mk_candidates,
        old_fp,
        old_side,
        new_side,
        matches,
        &mut stats.enclosing_stmt_abstain,
    );
    if let Some(stmt_match) = stmt_match {
        matches.insert(old_id.to_string(), stmt_match);
        return Resolution::EnclosingStatement;
    }

    if max_cascade_depth < 1 {
        ambiguous.insert(old_id.to_string(), mk_candidates);
        return Resolution::Ambiguous;
    }

    // calleeShapes: blurred callee structural shapes.
    let callee_shape_candidates =
        filter_by_callee_shapes(&mk_candidates, old_fp.callee_shapes(), new_side);
    if callee_shape_candidates.is_empty() {
        ambiguous.insert(old_id.to_string(), mk_candidates);
        return Resolution::Ambiguous;
    }
    if callee_shape_candidates.len() == 1 {
        matches.insert(old_id.to_string(), callee_shape_candidates[0].clone());
        return Resolution::CalleeShapes;
    }

    // callerShapes: blurred caller structural shapes (upstream context).
    let caller_shape_candidates =
        filter_by_caller_shapes(&callee_shape_candidates, old_fp.caller_shapes(), new_side);
    if caller_shape_candidates.is_empty() {
        ambiguous.insert(old_id.to_string(), callee_shape_candidates);
        return Resolution::Ambiguous;
    }
    if caller_shape_candidates.len() == 1 {
        matches.insert(old_id.to_string(), caller_shape_candidates[0].clone());
        return Resolution::CallerShapes;
    }

    resolve_deep_stages(
        old_id,
        &caller_shape_candidates,
        old_fp,
        old_side,
        new_side,
        matches,
        ambiguous,
        max_cascade_depth,
        stats,
    )
}

/// TS `resolveDeepStages` (:703): calleeHashes + twoHopShapes, then the
/// shingle tiebreak.
#[allow(clippy::too_many_arguments)]
fn resolve_deep_stages(
    old_id: &str,
    candidates: &[String],
    old_fp: &super::FunctionFingerprint,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
    matches: &mut MatchMap,
    ambiguous: &mut AmbiguousMatches,
    max_cascade_depth: u8,
    stats: &mut ResolutionStats,
) -> Resolution {
    let mut pool = candidates.to_vec();
    if max_cascade_depth >= 2 {
        let outcome = try_callee_hash_cascade(&pool, old_fp, new_side);
        match outcome {
            CalleeHashOutcome::Match { id, resolution } => {
                matches.insert(old_id.to_string(), id);
                return match resolution {
                    MatchedResolution::CalleeHashes => Resolution::CalleeHashes,
                    _ => Resolution::TwoHopShapes,
                };
            }
            CalleeHashOutcome::Contradiction => {
                ambiguous.insert(old_id.to_string(), pool);
                return Resolution::Ambiguous;
            }
            CalleeHashOutcome::Ambiguous { pool: narrowed } => {
                pool = narrowed;
            }
        }
    }

    // shingleSimilarity: Jaccard similarity tiebreaker. A binding index has
    // no `functions` map, so on the binding cascade the tier cannot run —
    // COUNT that, so its zero is never read as "consulted, found nothing".
    if new_side.index.kind != IndexKind::Functions {
        stats.shingle_unconsultable += 1;
    }
    if let Some(shingle_match) = try_shingle_resolve(old_id, &pool, old_side, new_side) {
        matches.insert(old_id.to_string(), shingle_match);
        return Resolution::ShingleSimilarity;
    }

    // Still ambiguous.
    ambiguous.insert(old_id.to_string(), pool);
    Resolution::Ambiguous
}

// ---------------------------------------------------------------------------
// The singleton guard (:1338)
// ---------------------------------------------------------------------------

/// TS `SingletonVerdict` (:1312). `unguarded` is deliberately distinct from
/// `accept`: both let the match through, but they are not the same claim.
/// `accept` means the guard examined version-stable evidence and found no
/// contradiction; `unguarded` means there was nothing to examine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SingletonVerdict {
    Reject,
    Accept,
    Unguarded,
}

/// TS `singletonVerdict` (:1338): contradiction check for zero-corroboration
/// singleton accepts, using only version-stable signals — memberKey,
/// propertyAccesses, externalCalls. A signal absent on either side is
/// missing evidence, not an opposing signal; only explicit disagreement
/// rejects. **It reports when it had no evidence at all, and that is not a
/// detail** — `buildBindingFullFingerprint` sets neither `memberKey` nor
/// `features`, so on the module-binding cascade this can only ever return
/// `unguarded` (11,094 accepts examined 0 times on 2.1.215→216; exp058).
fn singleton_verdict(
    old_fp: &super::FunctionFingerprint,
    new_fp: Option<&super::FunctionFingerprint>,
) -> SingletonVerdict {
    let Some(new_fp) = new_fp else {
        return SingletonVerdict::Unguarded;
    };
    let both_member_keys = old_fp.member_key().is_some() && new_fp.member_key().is_some();
    let old_features = old_fp.features();
    let new_features = new_fp.features();
    let both_features = old_features.is_some() && new_features.is_some();
    if !both_member_keys && !both_features {
        return SingletonVerdict::Unguarded;
    }
    if both_member_keys && old_fp.member_key() != new_fp.member_key() {
        return SingletonVerdict::Reject;
    }
    if let (Some(old_features), Some(new_features)) = (old_features, new_features)
        && (!arrays_equal(
            &old_features.property_accesses,
            &new_features.property_accesses,
        ) || !arrays_equal(&old_features.external_calls, &new_features.external_calls))
    {
        return SingletonVerdict::Reject;
    }
    SingletonVerdict::Accept
}

// ---------------------------------------------------------------------------
// The matching state and the pass loop (:1270, :1367, :1377)
// ---------------------------------------------------------------------------

/// TS `MatchingState` (:1270). `resolutions` is deliberately NOT kept in
/// sync with `matches` by the post-passes — the staleness is load-bearing
/// (module doc).
struct MatchingState<'s, 'a, 'g> {
    old: Side<'a, 'g>,
    new: Side<'a, 'g>,
    max_cascade_depth: u8,
    exclude_ids: Option<&'s HashSet<String>>,
    resolver: Option<AmbiguityResolver<'s>>,
    matches: MatchMap,
    /// TS `ambiguous: Map<string, string[]>` — ORDERED (see module doc):
    /// the propagation pass resolves entries in map order, so the insertion
    /// order is a decision input, not a representation detail.
    ambiguous: AmbiguousMatches,
    unmatched: Vec<String>,
    demoted_priors: BTreeSet<String>,
    stats: ResolutionStats,
    resolutions: HashMap<String, MatchedResolution>,
    /// Pairs revoked by crossed containers, recorded before deletion (the
    /// artifact dump's rejection rows need the pair, which is gone after).
    revoked: Vec<(String, String)>,
}

/// TS `candidatesForHash` (:1367): new-side hash-bucket candidates for an
/// old fingerprint, minus exclusions.
fn candidates_for_hash(
    structural_hash: &str,
    new_side: &Side<'_, '_>,
    exclude_ids: Option<&HashSet<String>>,
) -> Vec<String> {
    let raw = bucket_session_ids(new_side.index, structural_hash);
    match exclude_ids {
        None => raw,
        Some(exclude) => raw.into_iter().filter(|id| !exclude.contains(id)).collect(),
    }
}

/// TS `runMatchingPass` (:1377): the per-old-id matching loop — uniqueHash
/// accept or the disambiguation cascade, in old-index entry order.
fn run_matching_pass(state: &mut MatchingState<'_, '_, '_>) {
    for entry_idx in 0..state.old.index.entries.len() {
        let old_id = state.old.index.entries[entry_idx].session_id.clone();
        let old_fp = &state.old.index.entries[entry_idx].fingerprint;
        // Skip excluded functions (e.g., Bun CJS wrapper).
        if state.exclude_ids.is_some_and(|set| set.contains(&old_id)) {
            continue;
        }

        let candidates =
            candidates_for_hash(old_fp.structural_hash(), &state.new, state.exclude_ids);

        if candidates.is_empty() {
            state.unmatched.push(old_id);
            state.stats.unmatched += 1;
            continue;
        }

        if candidates.len() == 1 {
            // A singleton bucket matches with zero cascade corroboration —
            // exactly where a deleted helper and an unrelated added helper
            // auto-match. Reject when a version-stable signal contradicts,
            // and COUNT the accepts where there was no signal to consult,
            // so the guard's silence is never mistaken for its approval.
            let verdict = singleton_verdict(old_fp, state.new.fingerprint(&candidates[0]));
            match verdict {
                SingletonVerdict::Reject => {
                    state.unmatched.push(old_id);
                    state.stats.unmatched += 1;
                    state.stats.singleton_rejected += 1;
                }
                SingletonVerdict::Unguarded => {
                    state.stats.singleton_unguarded += 1;
                    state.matches.insert(old_id.clone(), candidates[0].clone());
                    state
                        .resolutions
                        .insert(old_id, MatchedResolution::StructuralHashUnique);
                }
                SingletonVerdict::Accept => {
                    state.matches.insert(old_id.clone(), candidates[0].clone());
                    state
                        .resolutions
                        .insert(old_id, MatchedResolution::StructuralHashUnique);
                }
            }
            continue;
        }

        // Multiple candidates — use the disambiguation cascade.
        let resolution = resolve_match(
            &old_id,
            &candidates,
            old_fp,
            &state.old,
            &state.new,
            &mut state.matches,
            &mut state.ambiguous,
            state.max_cascade_depth,
            &mut state.stats,
            state.resolver,
        );
        if let Some(matched) = resolution.matched() {
            state.resolutions.insert(old_id, matched);
        }
    }
}

/// TS `demoteNonInjectiveMatches` (:1441): enforce injectivity — a new-side
/// function claimed by more than one old-side function is a contradiction.
/// Demote every claimant back to ambiguous with its full hash-bucket
/// candidate list. NOTE: `resolutions` is deliberately NOT deleted here (the
/// staleness is load-bearing).
fn demote_non_injective_matches(state: &mut MatchingState<'_, '_, '_>) {
    // claimants keyed by new id; the groups are visited in FIRST-CLAIM
    // order (= the TS's Map iteration order) — the outcome is
    // order-independent (every over-claimed group is demoted whole), but the
    // house rule keeps the walk deterministic anyway.
    let mut claimant_order: Vec<String> = Vec::new();
    let mut claimants: HashMap<String, Vec<String>> = HashMap::new();
    for entry_idx in 0..state.old.index.entries.len() {
        let old_id = &state.old.index.entries[entry_idx].session_id;
        if let Some(new_id) = state.matches.get(old_id) {
            let list = claimants.entry(new_id.clone()).or_insert_with(|| {
                claimant_order.push(new_id.clone());
                Vec::new()
            });
            list.push(old_id.clone());
        }
    }

    for new_id in claimant_order {
        let old_ids = &claimants[&new_id];
        if old_ids.len() <= 1 {
            continue;
        }
        for old_id in old_ids {
            state.demoted_priors.insert(old_id.clone());
            state.matches.remove(old_id);
            crate::propagation::trace::entry_line(
                old_id,
                format_args!("DEMOTE {old_id} (contested {new_id}) — re-parked at the map's end"),
            );
            let candidates = state
                .old
                .fingerprint(old_id)
                .map(|fp| fp.structural_hash().to_string())
                .map(|hash| candidates_for_hash(&hash, &state.new, state.exclude_ids))
                .unwrap_or_default();
            state.ambiguous.insert(old_id.clone(), candidates);
            state.stats.injectivity_demoted += 1;
        }
    }
}

/// TS `crossedContainerIds` (:564): the enclosing-statement-resolved pairs
/// whose two containers matched to DIFFERENT things — the pairing crossed
/// containers. Iterated in old-entry order (the TS's matches insertion
/// order); the consumer sorts.
fn crossed_container_ids(state: &MatchingState<'_, '_, '_>) -> Vec<String> {
    let mut crossed: Vec<String> = Vec::new();
    for entry_idx in 0..state.old.index.entries.len() {
        let old_id = &state.old.index.entries[entry_idx].session_id;
        let Some(new_id) = state.matches.get(old_id) else {
            continue;
        };
        if state.resolutions.get(old_id).copied() != Some(MatchedResolution::EnclosingStatement) {
            continue;
        }
        let (Some(old_parent), Some(new_parent)) = (
            state.old.scope_parent_session(old_id),
            state.new.scope_parent_session(new_id),
        ) else {
            continue;
        };
        match state.matches.get(old_parent) {
            Some(parent_went) if parent_went != new_parent => crossed.push(old_id.clone()),
            _ => {}
        }
    }
    crossed
}

/// TS `revokeCrossedContainers` (:579): revoke matches whose pair sits in
/// containers that matched to DIFFERENT things, and return them to the
/// ambiguous pool for propagation. Runs BEFORE the stats attribution, or
/// `enclosingStatementResolved` counts matches this pass then took away.
/// The pool is the RAW bucket — NOT filtered by excludeIds: propagation
/// excludes candidates already claimed by another old id, so handing it the
/// whole bucket is correct rather than generous.
fn revoke_crossed_containers(state: &mut MatchingState<'_, '_, '_>) {
    let crossed = crossed_container_ids(state);
    for old_id in &crossed {
        let fresh_id = state.matches.get(old_id).cloned();
        if let Some(fresh_id) = fresh_id {
            state.revoked.push((old_id.clone(), fresh_id));
        }
        crate::propagation::trace::entry_line(
            old_id,
            format_args!(
                "REVOKE {} (was matched {:?}) — re-parked at the map's end",
                old_id,
                state.matches.get(old_id)
            ),
        );
        state.matches.remove(old_id);
        state.resolutions.remove(old_id);
        let pool = state
            .old
            .fingerprint(old_id)
            .map(|fp| bucket_session_ids(state.new.index, fp.structural_hash()))
            .unwrap_or_default();
        if !pool.is_empty() {
            state.ambiguous.insert(old_id.clone(), pool);
        }
    }
    state.stats.crossed_container_revoked = crossed.len();
}

// ---------------------------------------------------------------------------
// matchFunctions (:784) — THE ORCHESTRATOR
// ---------------------------------------------------------------------------

/// TS `matchFunctions` (:784): matches functions from an old version to a
/// new version using the disambiguation cascade. Mutation order is
/// load-bearing — pass, demote, REVOKE, stats attribution, stillAmbiguous,
/// (propagation stub), observation rows.
pub fn match_functions(
    old_index: &FingerprintIndex<'_>,
    new_index: &FingerprintIndex<'_>,
    old_ctx: &StatementContexts,
    new_ctx: &StatementContexts,
    options: MatchOptions<'_>,
) -> MatchResult {
    let max_cascade_depth = options.depth();
    let exclude_ids = options.exclude_session_ids;

    // Resolution stage per matched old id. Stats are accumulated only after
    // injectivity enforcement so demoted matches never count as resolved.
    let mut state = MatchingState {
        old: Side::new(old_index, old_ctx),
        new: Side::new(new_index, new_ctx),
        max_cascade_depth,
        exclude_ids,
        resolver: options.resolve_ambiguous_candidate,
        matches: MatchMap::new(),
        ambiguous: AmbiguousMatches::new(),
        unmatched: Vec::new(),
        demoted_priors: BTreeSet::new(),
        stats: ResolutionStats::default(),
        resolutions: HashMap::new(),
        revoked: Vec::new(),
    };
    run_matching_pass(&mut state);
    demote_non_injective_matches(&mut state);

    // Revoke BEFORE attributing, or `enclosingStatementResolved` counts
    // matches this pass then took away — 12,997 reported against 93 revoked,
    // with nothing saying the first number included the second.
    revoke_crossed_containers(&mut state);
    for entry_idx in 0..old_index.entries.len() {
        let old_id = &old_index.entries[entry_idx].session_id;
        if state.matches.contains_key(old_id)
            && let Some(resolution) = state.resolutions.get(old_id).copied()
        {
            resolution.bump(&mut state.stats);
        }
    }
    state.stats.still_ambiguous = state.ambiguous.len();

    // Post-pass: call-graph propagation to resolve remaining ambiguity.
    // The ambiguous map's INSERTION ORDER is decision input (the TS's Map:
    // entries entered ambiguity during the run pass in walk order, and a
    // demote/revoke re-park APPENDS at the map's end) — `state.ambiguous`
    // IS that map, so propagation consumes it directly. Reconstructing the
    // order from the old index's entry order instead (the previous port)
    // re-positioned every re-parked prior at its index position, mid-map,
    // which changed which entries saw whose claims within an iteration.
    if options.enable_propagation && !state.ambiguous.is_empty() {
        let outcome = crate::propagation::propagate(
            &mut state.matches,
            &mut state.ambiguous,
            old_index,
            new_index,
            crate::propagation::PropagationOptions {
                max_iterations: None,
                external_ref_evidence: options.external_ref_evidence.clone(),
            },
        );
        state.stats.propagation_resolved = outcome.resolved;
        state.stats.propagation_by_rung = outcome.by_rung;
        state.stats.still_ambiguous -= outcome.resolved;
    }

    let mut result = MatchResult {
        matches: state.matches,
        ambiguous: state.ambiguous,
        unmatched: state.unmatched,
        demoted_priors: state.demoted_priors,
        resolution_stats: state.stats,
        pair_resolutions: Vec::new(),
        pair_rejections: Vec::new(),
    };

    // Observation rows for the artifact dump: pairs with tiers, rejections
    // by class. Sorted by prior id — no Map iteration order may reach dump
    // bytes (07 §2 ordering rules).
    result.pair_resolutions = result
        .matches
        .iter()
        .map(|(prior, fresh)| PairResolution {
            prior: prior.clone(),
            fresh: fresh.clone(),
            tier: state
                .resolutions
                .get(prior)
                .map(|r| r.as_str())
                .unwrap_or("propagation")
                .to_string(),
        })
        .collect();
    result
        .pair_resolutions
        .sort_by(|a, b| ts_cmp(&a.prior, &b.prior));

    let mut ambiguous_rows: Vec<PairRejection> = result
        .ambiguous
        .iter()
        .map(|(prior, candidates)| PairRejection {
            prior: prior.clone(),
            kind: RejectionKind::StillAmbiguous,
            candidates: {
                let mut sorted = candidates.clone();
                sorted.sort_by(|a, b| ts_cmp(a, b));
                Some(sorted)
            },
        })
        .collect();
    ambiguous_rows.sort_by(|a, b| ts_cmp(&a.prior, &b.prior));

    let mut demoted_rows: Vec<PairRejection> = result
        .demoted_priors
        .iter()
        .map(|prior| PairRejection {
            prior: prior.clone(),
            kind: RejectionKind::Demoted,
            candidates: None,
        })
        .collect();
    demoted_rows.sort_by(|a, b| ts_cmp(&a.prior, &b.prior));

    let mut revoked_rows: Vec<PairRejection> = state
        .revoked
        .iter()
        .map(|(prior, _)| PairRejection {
            prior: prior.clone(),
            kind: RejectionKind::Revoked,
            candidates: None,
        })
        .collect();
    revoked_rows.sort_by(|a, b| ts_cmp(&a.prior, &b.prior));

    result.pair_rejections = result
        .unmatched
        .iter()
        .map(|prior| PairRejection {
            prior: prior.clone(),
            kind: RejectionKind::Unmatched,
            candidates: None,
        })
        .chain(ambiguous_rows)
        .chain(demoted_rows)
        .chain(revoked_rows)
        .collect();

    result
}

// ---------------------------------------------------------------------------
// The tail tiers: ordinal (:899) and interchangeable pools (:944-:1204)
// ---------------------------------------------------------------------------

/// TS `resolveAmbiguousByOrdinal` (:899): final tie-break for buckets no
/// evidence can crack — equal unmatched counts, every member carrying
/// identical distinguishing features → pair by source order. Runs AFTER
/// binding alternation and propagation so no genuine evidence is pre-empted.
pub fn resolve_ambiguous_by_ordinal(
    match_result: &mut MatchResult,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
) -> usize {
    let matched_new: HashSet<String> = match_result.matches.values().cloned().collect();

    // The hashes of the still-ambiguous priors (a Set — order-free). Sort
    // the keys anyway so no map order reaches the pass.
    let mut old_ids: Vec<&String> = match_result.ambiguous.iter().map(|(k, _)| k).collect();
    old_ids.sort_by(|a, b| ts_cmp(a, b));
    let mut hashes: BTreeSet<String> = BTreeSet::new();
    for old_id in old_ids {
        if let Some(hash) = old_side
            .fingerprint(old_id)
            .map(|fp| fp.structural_hash().to_string())
        {
            hashes.insert(hash);
        }
    }

    let mut resolved = 0;
    for hash in hashes {
        resolved += ordinal_pair_bucket(&hash, match_result, &matched_new, old_side, new_side);
    }
    match_result
        .pair_resolutions
        .sort_by(|a, b| ts_cmp(&a.prior, &b.prior));
    match_result.resolution_stats.ordinal_resolved += resolved;
    match_result.resolution_stats.still_ambiguous = match_result.ambiguous.len();
    resolved
}

/// TS `ordinalPairBucket` (:1224): pair one bucket by source order when all
/// the ordinal gates hold.
fn ordinal_pair_bucket(
    hash: &str,
    match_result: &mut MatchResult,
    matched_new: &HashSet<String>,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
) -> usize {
    let old_bucket = bucket_session_ids(old_side.index, hash);
    let new_bucket = bucket_session_ids(new_side.index, hash);
    if old_bucket.is_empty() || old_bucket.len() != new_bucket.len() {
        return 0;
    }
    // Every member must still be undecided on both sides — a partially
    // matched bucket means evidence existed for someone, and ordinal pairing
    // of the remainder would shift against it.
    if old_bucket
        .iter()
        .any(|id| match_result.matches.contains_key(id) || !match_result.ambiguous.contains(id))
    {
        return 0;
    }
    if new_bucket.iter().any(|id| matched_new.contains(id)) {
        return 0;
    }
    // A contested (demoted) prior must not be pair-by-position resolved.
    if old_bucket
        .iter()
        .any(|id| match_result.demoted_priors.contains(id))
    {
        return 0;
    }

    let mut keys: BTreeSet<Option<String>> = BTreeSet::new();
    for id in &old_bucket {
        keys.insert(evidence_key(old_side, id));
    }
    for id in &new_bucket {
        keys.insert(evidence_key(new_side, id));
    }
    if keys.len() != 1 || keys.contains(&None) {
        return 0;
    }

    let mut old_ordered = old_bucket;
    let mut new_ordered = new_bucket;
    old_ordered.sort_by(|a, b| by_session_position(a, b));
    new_ordered.sort_by(|a, b| by_session_position(a, b));
    for (old_id, new_id) in old_ordered.iter().zip(new_ordered.iter()) {
        match_result.matches.insert(old_id.clone(), new_id.clone());
        match_result.ambiguous.remove(old_id);
        match_result.pair_resolutions.push(PairResolution {
            prior: old_id.clone(),
            fresh: new_id.clone(),
            tier: "ordinal".to_string(),
        });
    }
    old_ordered.len()
}

/// TS `InterchangeablePool` (:944): a certified interchangeable pool —
/// ambiguous priors sharing one exact candidate set, reciprocal (equal
/// counts, every candidate unmatched), every member on BOTH sides carrying
/// the same non-null evidence key. CERTIFIES who may enter a
/// stable-assignment tier; assigns nothing itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterchangeablePool {
    /// Prior-side ids, session-position order (stable per artifact).
    pub priors: Vec<String>,
    /// Fresh-side candidate ids, session-position order.
    pub candidates: Vec<String>,
    /// The single evidence key every member shares.
    pub evidence_key: String,
}

/// TS `certifyInterchangeablePools` (:953). Contested (demoted) priors are
/// propagation's to re-resolve, never a pool's.
pub fn certify_interchangeable_pools(
    match_result: &MatchResult,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
) -> Vec<InterchangeablePool> {
    let matched_new: HashSet<String> = match_result.matches.values().cloned().collect();
    let by_candidates = group_priors_by_candidate_set(&match_result.ambiguous);
    let mut pools: Vec<InterchangeablePool> = Vec::new();
    // TS: `[...byCandidates.entries()].sort()` — sorted by the candidate-set
    // KEY (byte sort for these ASCII ids).
    for (key, priors) in &by_candidates {
        if priors
            .iter()
            .any(|id| match_result.demoted_priors.contains(id))
        {
            continue;
        }
        let candidates: Vec<String> = key.split(',').map(str::to_string).collect();
        if let Some(pool) = certify_one_pool(&candidates, priors, &matched_new, old_side, new_side)
        {
            pools.push(pool);
        }
    }
    pools
}

/// TS `groupPriorsByCandidateSet` (:978): group ambiguous priors by their
/// sorted candidate-set key. The BTreeMap gives the sorted-key iteration the
/// TS's `.sort()` produces.
fn group_priors_by_candidate_set(ambiguous: &AmbiguousMatches) -> BTreeMap<String, Vec<String>> {
    let mut by_candidates: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Sort the keys so no map order reaches the grouping (the TS's Map
    // insertion order becomes the byte-sorted order here; every consumer
    // position-sorts its output).
    let mut rows: Vec<(&String, &Vec<String>)> = ambiguous.iter().collect();
    rows.sort_by(|a, b| ts_cmp(a.0, b.0));
    for (old_id, candidates) in rows {
        let mut sorted = candidates.clone();
        sorted.sort_by(|a, b| ts_cmp(a, b));
        let key = sorted.join(",");
        by_candidates.entry(key).or_default().push(old_id.clone());
    }
    by_candidates
}

/// TS `certifyOnePool` (:995): the certificate gates for one pool; None when
/// any gate fails.
fn certify_one_pool(
    candidates: &[String],
    priors: &[String],
    matched_new: &HashSet<String>,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
) -> Option<InterchangeablePool> {
    if priors.len() != candidates.len() {
        return None;
    }
    if candidates.iter().any(|id| matched_new.contains(id)) {
        return None;
    }
    let mut keys: BTreeSet<Option<String>> = BTreeSet::new();
    for id in priors {
        keys.insert(evidence_key(old_side, id));
    }
    for id in candidates {
        keys.insert(evidence_key(new_side, id));
    }
    if keys.len() != 1 {
        return None;
    }
    let only = keys.into_iter().next().flatten()?;
    let mut priors_sorted = priors.to_vec();
    let mut candidates_sorted = candidates.to_vec();
    priors_sorted.sort_by(|a, b| by_session_position(a, b));
    candidates_sorted.sort_by(|a, b| by_session_position(a, b));
    Some(InterchangeablePool {
        priors: priors_sorted,
        candidates: candidates_sorted,
        evidence_key: only,
    })
}

/// TS `assignInterchangeablePools` (:1029): prior-anchored assignment over
/// certified interchangeable pools. Inside a certified pool any pairing is
/// semantically valid, so the pairing maximizes agreement with
/// ALREADY-MATCHED surroundings. Certification snapshots matched candidates
/// ONCE, but pools can overlap — a candidate claimed by an earlier pool must
/// void every later pool that contains it (a pool losing a candidate has
/// lost its reciprocity certificate — abstain, never assign the remainder).
pub fn assign_interchangeable_pools(
    match_result: &mut MatchResult,
    old_side: &Side<'_, '_>,
    new_side: &Side<'_, '_>,
) -> usize {
    let pools = certify_interchangeable_pools(match_result, old_side, new_side);
    if pools.is_empty() {
        return 0;
    }
    let old_nav = build_anchor_nav(old_side);
    let new_nav = build_anchor_nav(new_side);
    let mut claimed: HashSet<String> = match_result.matches.values().cloned().collect();
    let mut resolved = 0;
    for pool in &pools {
        if pool.candidates.iter().any(|id| claimed.contains(id)) {
            continue;
        }
        resolved += assign_one_pool(pool, match_result, &old_nav, &new_nav);
        claimed.extend(pool.candidates.iter().cloned());
    }
    match_result
        .pair_resolutions
        .sort_by(|a, b| ts_cmp(&a.prior, &b.prior));
    match_result.resolution_stats.interchangeable_resolved += resolved;
    match_result.resolution_stats.still_ambiguous = match_result.ambiguous.len();
    resolved
}

/// TS `AnchorNav` (:1061): neighbor/caller/callee lookups for anchor-affinity
/// scoring.
#[derive(Debug, Default)]
struct AnchorNav {
    prev: HashMap<String, String>,
    next: HashMap<String, String>,
    callees: HashMap<String, Vec<String>>,
    callers: HashMap<String, Vec<String>>,
    callee_set: HashMap<String, HashSet<String>>,
    caller_set: HashMap<String, HashSet<String>>,
}

/// TS `buildAnchorNav` (:1070): ids sorted by session position (stable —
/// ties keep build order), then the internal-callee edges off the function
/// rows (a binding index has no `functions` — the TS's `index.functions
/// ?? []`).
fn build_anchor_nav(side: &Side<'_, '_>) -> AnchorNav {
    let mut nav = AnchorNav::default();
    let mut ids: Vec<String> = side
        .index
        .entries
        .iter()
        .map(|e| e.session_id.clone())
        .collect();
    ids.sort_by(|a, b| by_session_position(a, b));
    for i in 0..ids.len() {
        if i > 0 {
            nav.prev.insert(ids[i].clone(), ids[i - 1].clone());
        }
        if i < ids.len() - 1 {
            nav.next.insert(ids[i].clone(), ids[i + 1].clone());
        }
    }
    if side.index.kind == IndexKind::Functions {
        for row in 0..side.index.graph.functions.len() {
            let id = side.index.graph.functions[row].session_id.clone();
            let mut seen: HashSet<String> = HashSet::new();
            for span in &side.index.graph.functions[row].internal_callees {
                let Some(callee_id) = side.index.function_session_of_span(*span) else {
                    continue;
                };
                if !seen.insert(callee_id.to_string()) {
                    continue;
                }
                let callee_id = callee_id.to_string();
                nav.callees
                    .entry(id.clone())
                    .or_default()
                    .push(callee_id.clone());
                nav.callers
                    .entry(callee_id.clone())
                    .or_default()
                    .push(id.clone());
                nav.callee_set
                    .entry(id.clone())
                    .or_default()
                    .insert(callee_id.clone());
                nav.caller_set
                    .entry(callee_id.clone())
                    .or_default()
                    .insert(id.clone());
            }
        }
    }
    nav
}

/// TS `anchorAffinity` (:1111): agreement between a prior member and a fresh
/// candidate through already-matched surroundings — neighbors weight 1,
/// callers/callees weight 2.
fn anchor_affinity(
    prior_id: &str,
    fresh_id: &str,
    matches: &HashMap<String, String>,
    old_nav: &AnchorNav,
    new_nav: &AnchorNav,
) -> i64 {
    let mut score = 0i64;
    let prev_match: Option<&String> = old_nav.prev.get(prior_id).and_then(|id| matches.get(id));
    if prev_match.is_some() && prev_match == new_nav.prev.get(fresh_id) {
        score += 1;
    }
    let next_match: Option<&String> = old_nav.next.get(prior_id).and_then(|id| matches.get(id));
    if next_match.is_some() && next_match == new_nav.next.get(fresh_id) {
        score += 1;
    }
    score += 2 * anchor_hits(
        old_nav.callers.get(prior_id),
        matches,
        new_nav.caller_set.get(fresh_id),
    ) as i64;
    score += 2 * anchor_hits(
        old_nav.callees.get(prior_id),
        matches,
        new_nav.callee_set.get(fresh_id),
    ) as i64;
    score
}

/// TS `anchorHits` (:1147).
fn anchor_hits(
    prior_side: Option<&Vec<String>>,
    matches: &HashMap<String, String>,
    fresh_side: Option<&HashSet<String>>,
) -> usize {
    let (Some(prior_side), Some(fresh_side)) = (prior_side, fresh_side) else {
        return 0;
    };
    prior_side
        .iter()
        .filter(|id| matches.get(*id).is_some_and(|m| fresh_side.contains(m)))
        .count()
}

/// TS `assignOnePool` (:1163): greedy max-affinity assignment; deterministic
/// tie-break by paired session order (the certificate returns both sides
/// position-sorted).
fn assign_one_pool(
    pool: &InterchangeablePool,
    match_result: &mut MatchResult,
    old_nav: &AnchorNav,
    new_nav: &AnchorNav,
) -> usize {
    let mut pairs: Vec<(usize, usize, i64)> = Vec::new();
    for pi in 0..pool.priors.len() {
        for fi in 0..pool.candidates.len() {
            pairs.push((
                pi,
                fi,
                anchor_affinity(
                    &pool.priors[pi],
                    &pool.candidates[fi],
                    &match_result.matches,
                    old_nav,
                    new_nav,
                ),
            ));
        }
    }
    // TS: `b.score - a.score || a.pi - b.pi || a.fi - b.fi`.
    pairs.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
    let mut used_p: HashSet<usize> = HashSet::new();
    let mut used_f: HashSet<usize> = HashSet::new();
    let mut resolved = 0;
    for (pi, fi, _) in pairs {
        if used_p.contains(&pi) || used_f.contains(&fi) {
            continue;
        }
        used_p.insert(pi);
        used_f.insert(fi);
        let prior = pool.priors[pi].clone();
        let fresh = pool.candidates[fi].clone();
        match_result.matches.insert(prior.clone(), fresh.clone());
        match_result.pair_resolutions.push(PairResolution {
            prior,
            fresh,
            tier: "interchangeable".to_string(),
        });
        match_result.ambiguous.remove(&pool.priors[pi]);
        resolved += 1;
    }
    resolved
}

/// TS `evidenceKey` (:1207): the distinguishing-feature vector of one
/// fingerprint, or None when absent. Every feature the cascade itself
/// distinguishes on, twoHop included — omitting it let two pools narrowed by
/// DIFFERENT two-hop evidence certify as "identical evidence" and overlap
/// (the injectivity hole's enabler). The JSON spelling differs from the TS's
/// `JSON.stringify` only in object key order (serde sorts map keys) — the
/// key is compared for EQUALITY within a run, never frozen as bytes.
fn evidence_key(side: &Side<'_, '_>, id: &str) -> Option<String> {
    let fp = side.fingerprint(id)?;
    let shapes = |shapes: &[CalleeShape]| -> serde_json::Value {
        serde_json::Value::Array(
            shapes
                .iter()
                .map(|s| {
                    json!({
                        "arity": s.arity,
                        "cfgType": s.cfg_type.as_str(),
                        "complexity": s.complexity,
                        "hasExternalCalls": s.has_external_calls,
                    })
                })
                .collect(),
        )
    };
    Some(
        json!([
            fp.member_key(),
            shapes(fp.callee_shapes()),
            shapes(fp.caller_shapes()),
            fp.callee_hashes(),
            fp.two_hop_shapes(),
        ])
        .to_string(),
    )
}

// ---------------------------------------------------------------------------
// The observation helpers (:1483, :1507)
// ---------------------------------------------------------------------------

/// TS `getMatchStats` (:1483) — consumed by the harnesses.
pub fn get_match_stats(result: &MatchResult) -> MatchStats {
    let matched = result.matches.len();
    let ambiguous = result.ambiguous.len();
    let unmatched = result.unmatched.len();
    let total = matched + ambiguous + unmatched;
    let match_rate = if total > 0 {
        matched as f64 / total as f64
    } else {
        0.0
    };
    MatchStats {
        matched,
        ambiguous,
        unmatched,
        total,
        match_rate,
    }
}

/// TS `getMatchStats`' return shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchStats {
    pub matched: usize,
    pub ambiguous: usize,
    pub unmatched: usize,
    pub total: usize,
    pub match_rate: f64,
}

/// TS `findNewFunctions` (:1507): functions in the new index with no match
/// in the old index — likely added in this version. New-index entry order.
pub fn find_new_functions(
    _old_index: &FingerprintIndex<'_>,
    new_index: &FingerprintIndex<'_>,
    match_result: &MatchResult,
) -> Vec<String> {
    let mut matched_new_ids: HashSet<String> = match_result.matches.values().cloned().collect();
    let ambiguous_candidates: Vec<String> = match_result
        .ambiguous
        .iter()
        .flat_map(|(_, candidates)| candidates.iter().cloned())
        .collect();
    for c in ambiguous_candidates {
        matched_new_ids.insert(c);
    }
    new_index
        .entries
        .iter()
        .filter(|entry| !matched_new_ids.contains(&entry.session_id))
        .map(|entry| entry.session_id.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod cascade_test;
