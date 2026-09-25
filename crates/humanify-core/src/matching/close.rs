//! The CLOSE-match tier (WP2.2 part 1) — TS original: `src/analysis/close-match.ts`
//! (258 LOC), ported whole: `computeFeatureVector` (:64), `cosineSimilarity`
//! (:87), `findCloseMatches` (:111), `buildVectorMap` (:145),
//! `CLOSE_MATCH_TOP_K` (:171), `scorePairs` (:177), `insertTopK` (:198),
//! `assignGreedy` (:217), `tiedRival` (:238).
//!
//! Where it sits (prior-version.ts :865-983 `buildCloseMatchContext`): after
//! the cascade + alternation + tail tiers, the UNMATCHED prior/fresh ids go
//! through this tier; every surviving pair then runs the statement alignment
//! (`matching::statement_align`, WP2.2 part 2) whose aligned-statement count
//! is the pair's first corroboration signal (prior-version.ts :835-852 —
//! `recordCorroboration`, ported here as [`corroborate`]; the shingle
//! fallback reuses the index shingle sets this module's neighbor
//! `matching` already carries).
//!
//! FLOAT DETERMINISM (the one thing this module must get exactly right).
//! Every decision here is float-driven:
//!
//! - the threshold skip (`score < threshold`),
//! - the top-k insertion (`list[i-1].score < candidate.score`),
//! - the candidate sort (`b.score - a.score`),
//! - the tie abstention (`candidates[j].score !== c.score` — EXACT equality).
//!
//! so the port must produce bit-identical f64s, not merely close ones. It
//! can: every input is an integer count (u32, < 2^53), so `dot`, `normA`
//! and `normB` are EXACT f64 integers accumulated in the TS's
//! `FEATURE_KEYS` order (12 sequential `+=` of one multiplication each —
//! no reassociation, no FMA: Rust's default is strict IEEE, never
//! fast-math); the only inexact steps are the two `sqrt`s, their product
//! and the final division — each correctly rounded in BOTH languages, in
//! the same order. Same formula, same order, same bits. `Math.sqrt` and
//! `f64::sqrt` are both correctly rounded per IEEE-754.
//!
//! The probe (`test/parity/wp22-probe.mjs`, frozen at
//! `test/parity/wp22-synthetic.json`) pins the TS's scores as JSON numbers
//! (shortest-roundtrip representation — parsing one back yields the
//! identical bits) and the pinned tie cases. What "identical" vectors
//! score is norm-dependent — `sqrt(n)*sqrt(n)` is correctly rounded and
//! lands on exactly `n` for some norms and one ulp off for others
//! (`1.0000000000000002` and `0.9999999999999998` both appear in the
//! frozen JSON) — so the doc makes no general claim: both implementations
//! compute the same bits for the same vectors, and the tie abstention
//! keys on that exact value.
//!
//! Iteration ORDER is load-bearing and ported verbatim: `scorePairs`
//! iterates old vectors in `unmatchedOld` order and fresh vectors in
//! `unmatchedNew` order (the TS's Map insertion order = the
//! `newFunctions`/`priorFnMap` key order = the graph's row order); ties
//! inside `insertTopK` keep first-seen first, and the candidate sort is
//! STABLE (ES2019 `Array.sort`; `Vec::sort_by` is a stable merge sort).
//! So equal-score candidates arrive at `assignGreedy` in the same order
//! both sides — which matters exactly because a tie ABSTAINS (the order
//! only decides which pair enters `tiedRival` first, and the verdict is
//! mutual).

use std::collections::HashSet;

use super::{FingerprintIndex, SHINGLE_SIMILARITY_FLOOR, StructuralFeatures, jaccard_similarity};

/// TS `CLOSE_MATCH_TOP_K` (:171): max candidates kept per old function —
/// bounds the pair matrix to O(old x K).
pub const CLOSE_MATCH_TOP_K: usize = 3;

/// TS `options?.threshold ?? 0.8` (:118).
pub const DEFAULT_CLOSE_MATCH_THRESHOLD: f64 = 0.8;

/// TS `FeatureVector` (:30-43): fixed-length numeric feature vector,
/// DECKARD-inspired — function shape without identifier sensitivity.
/// Counts as f64 IN KEY ORDER ([`FEATURE_KEY_COUNT`]) so the cosine loop
/// iterates the TS's `FEATURE_KEYS` order by index; the named accessors
/// exist for construction only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureVector {
    pub arity: f64,
    pub complexity: f64,
    pub return_count: f64,
    pub loop_count: f64,
    pub branch_count: f64,
    pub try_count: f64,
    pub callee_count: f64,
    pub external_call_count: f64,
    pub string_literal_count: f64,
    pub property_access_count: f64,
    pub numeric_literal_count: f64,
    pub has_rest_param: f64,
}

/// TS `FEATURE_KEYS` (:45-58) length — the cosine accumulation order.
pub const FEATURE_KEY_COUNT: usize = 12;

impl FeatureVector {
    /// The values in TS `FEATURE_KEYS` order — the cosine loop's input.
    fn keys(&self) -> [f64; FEATURE_KEY_COUNT] {
        [
            self.arity,
            self.complexity,
            self.return_count,
            self.loop_count,
            self.branch_count,
            self.try_count,
            self.callee_count,
            self.external_call_count,
            self.string_literal_count,
            self.property_access_count,
            self.numeric_literal_count,
            self.has_rest_param,
        ]
    }
}

/// TS `computeFeatureVector` (:64) from a fingerprint's structural features
/// and the callee count.
pub fn compute_feature_vector(features: &StructuralFeatures, callee_count: usize) -> FeatureVector {
    FeatureVector {
        arity: f64::from(features.arity),
        complexity: f64::from(features.complexity),
        return_count: f64::from(features.return_count),
        loop_count: f64::from(features.loop_count),
        branch_count: f64::from(features.branch_count),
        try_count: f64::from(features.try_count),
        callee_count: callee_count as f64,
        external_call_count: features.external_calls.len() as f64,
        string_literal_count: features.string_literals.len() as f64,
        property_access_count: features.property_accesses.len() as f64,
        numeric_literal_count: features.numeric_literals.len() as f64,
        has_rest_param: if features.has_rest_param { 1.0 } else { 0.0 },
    }
}

/// TS `cosineSimilarity` (:87) — 0-1 similarity; 0 when either norm is 0.
/// See the module doc: bit-exact to the TS by construction.
fn cosine_similarity(a: &FeatureVector, b: &FeatureVector) -> f64 {
    let ka = a.keys();
    let kb = b.keys();
    let mut dot_product = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for i in 0..FEATURE_KEY_COUNT {
        let va = ka[i];
        let vb = kb[i];
        dot_product += va * vb;
        norm_a += va * va;
        norm_b += vb * vb;
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot_product / (norm_a.sqrt() * norm_b.sqrt())
}

/// One scored (old, fresh) candidate — TS `scorePairs`'s row shape.
#[derive(Debug, Clone, PartialEq)]
pub struct CloseCandidate {
    pub old_id: String,
    pub new_id: String,
    pub score: f64,
}

/// One assigned close match, in ASSIGNMENT order (the TS's `closeMatches`
/// Map insertion order, which the context builder iterates).
#[derive(Debug, Clone, PartialEq)]
pub struct CloseMatchPair {
    /// TS `closeMatches` key — the PRIOR session id.
    pub prior_id: String,
    /// TS `closeMatches` value — the FRESH session id.
    pub fresh_id: String,
    /// TS `scores.get(oldId)` — the assigned pair's cosine score.
    pub score: f64,
}

/// TS `CloseMatchResult` (:4-23). `pairs` replaces the two Maps
/// (`closeMatches` + `scores` — one entry per assignment, so a Vec carries
/// both without a second keying); the skip counters keep their TS
/// meaning: ids that could not be scored AT ALL because their fingerprint
/// carries no `features` — NOT ids scored and found dissimilar.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CloseMatchResult {
    pub pairs: Vec<CloseMatchPair>,
    pub skipped_old: usize,
    pub skipped_new: usize,
}

/// TS `findCloseMatches` (:111): close matches between unmatched old and
/// unmatched new functions by cosine similarity on structural feature
/// vectors. Each old matches at most one fresh (best above threshold);
/// each fresh at most one old (greedy best-first, tie-abstaining).
/// `threshold` is `options?.threshold ?? 0.8` — pass [`None`] for the
/// default.
pub fn find_close_matches(
    unmatched_old: &[String],
    unmatched_new: &[String],
    old_index: &FingerprintIndex<'_>,
    new_index: &FingerprintIndex<'_>,
    threshold: Option<f64>,
) -> CloseMatchResult {
    let threshold = threshold.unwrap_or(DEFAULT_CLOSE_MATCH_THRESHOLD);
    if unmatched_old.is_empty() || unmatched_new.is_empty() {
        // The TS's early return also zeroes the skip counters (:122-124).
        return CloseMatchResult::default();
    }

    let (old_vectors, skipped_old) = build_vector_map(unmatched_old, old_index);
    let (fresh_vectors, skipped_new) = build_vector_map(unmatched_new, new_index);

    let candidates = score_pairs(&old_vectors, &fresh_vectors, threshold);
    let pairs = assign_greedy(candidates);

    CloseMatchResult {
        pairs,
        skipped_old,
        skipped_new,
    }
}

/// TS `buildVectorMap` (:145): vectors for the ids that can be scored, in
/// id order (the TS Map's insertion order), plus the count of ids that
/// cannot — the two are not the same fact (see `CloseMatchResult`).
fn build_vector_map(
    ids: &[String],
    index: &FingerprintIndex<'_>,
) -> (Vec<(String, FeatureVector)>, usize) {
    let mut vectors = Vec::with_capacity(ids.len());
    let mut skipped = 0usize;
    for id in ids {
        let entry = index.entry_of_session(id).map(|i| &index.entries[i]);
        let features = entry.and_then(|e| e.fingerprint.features());
        let (entry, features) = match (entry, features) {
            (Some(e), Some(f)) => (e, f),
            // TS `if (!fp || !features)` (:154).
            _ => {
                skipped += 1;
                continue;
            }
        };
        // TS `fp.calleeHashes?.length ?? fp.calleeShapes?.length ?? 0`
        // (:158): the `??` arms fire only on undefined/null, which the
        // Rust fingerprint type cannot represent (both arrays always
        // present, possibly empty) — the first arm is the whole rule.
        let callee_count = entry.fingerprint.callee_hashes().len();
        vectors.push((id.clone(), compute_feature_vector(features, callee_count)));
    }
    (vectors, skipped)
}

/// TS `scorePairs` (:177): cosine similarity for (old, new) pairs above
/// threshold, keeping the top-K per old function. Iteration order (old
/// vectors then fresh vectors, in list order) is load-bearing — see the
/// module doc.
pub fn score_pairs(
    old_vectors: &[(String, FeatureVector)],
    fresh_vectors: &[(String, FeatureVector)],
    threshold: f64,
) -> Vec<CloseCandidate> {
    let mut candidates: Vec<CloseCandidate> = Vec::new();
    for (old_id, old_vec) in old_vectors {
        let mut top: Vec<CloseCandidate> = Vec::new();
        for (new_id, new_vec) in fresh_vectors {
            let score = cosine_similarity(old_vec, new_vec);
            if score < threshold {
                continue;
            }
            insert_top_k(
                &mut top,
                CloseCandidate {
                    old_id: old_id.clone(),
                    new_id: new_id.clone(),
                    score,
                },
            );
        }
        candidates.extend(top);
    }
    candidates
}

/// TS `insertTopK` (:198): insert into a descending-sorted list capped at
/// `CLOSE_MATCH_TOP_K`. Ties keep first-seen first (`<` is strict, so an
/// equal score stops the walk and the candidate lands AFTER its equal).
fn insert_top_k(list: &mut Vec<CloseCandidate>, candidate: CloseCandidate) {
    let mut i = list.len();
    while i > 0 && list[i - 1].score < candidate.score {
        i -= 1;
    }
    list.insert(i, candidate);
    if list.len() > CLOSE_MATCH_TOP_K {
        list.pop();
    }
}

/// TS `assignGreedy` (:217): greedy best-first assignment, each side at
/// most once, a pair that TIES with another still-available pair sharing
/// either endpoint ABSTAINS — every cascade tier requires best >
/// second-best, and resolving an exact tie by Map insertion order would
/// cross-pair same-shaped siblings and present the coin flip as a match.
/// Equal-score pairs with disjoint endpoints are not in contention and
/// still match. Returns the assignments in decision order.
fn assign_greedy(mut candidates: Vec<CloseCandidate>) -> Vec<CloseMatchPair> {
    // TS `candidates.sort((a, b) => b.score - a.score)` — STABLE
    // (ES2019): equal scores keep candidate order. NaN cannot occur (the
    // cosine's norms are positive by the early return), so the
    // partial_cmp fallback never fires.
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<CloseMatchPair> = Vec::new();
    let mut used_new: HashSet<String> = HashSet::new();
    for i in 0..candidates.len() {
        let candidate = &candidates[i];
        let old_taken = out.iter().any(|m| m.prior_id == candidate.old_id);
        if old_taken || used_new.contains(&candidate.new_id) {
            continue;
        }
        if tied_rival(&candidates, i, &out, &used_new) {
            continue;
        }
        used_new.insert(candidate.new_id.clone());
        out.push(CloseMatchPair {
            prior_id: candidate.old_id.clone(),
            fresh_id: candidate.new_id.clone(),
            score: candidate.score,
        });
    }
    out
}

/// TS `tiedRival` (:238): an UNSPENT equal-score pair sharing an endpoint
/// with `candidates[i]`. Scanned both directions — an earlier rival that
/// abstained because of THIS pair must make this pair abstain too
/// (mutual). Float EXACT equality (`!==`), safe because both sides
/// compute identical bits (module doc).
fn tied_rival(
    candidates: &[CloseCandidate],
    i: usize,
    assigned: &[CloseMatchPair],
    used_new: &HashSet<String>,
) -> bool {
    let c = &candidates[i];
    let contends = |r: &CloseCandidate| {
        let old_free = !assigned.iter().any(|m| m.prior_id == r.old_id);
        old_free && !used_new.contains(&r.new_id) && (r.old_id == c.old_id || r.new_id == c.new_id)
    };
    for r in &candidates[i + 1..] {
        if r.score != c.score {
            break;
        }
        if contends(r) {
            return true;
        }
    }
    for r in candidates[..i].iter().rev() {
        if r.score != c.score {
            break;
        }
        if contends(r) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Corroboration (prior-version.ts :835-852 recordCorroboration + :1090
// shinglesCorroborate) — the verdict the WP2.2 gate compares.
// ---------------------------------------------------------------------------

/// TS `CloseMatchStats`'s three buckets (:115-125), exhaustive and
/// mutually exclusive per pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corroboration {
    /// `corroboratedByAlignment`: an aligned statement (identical
    /// normalized content) corroborated the pair.
    Alignment,
    /// `corroboratedByShingles`: no aligned statement, but
    /// rename-invariant shingle overlap cleared the floor.
    Shingles,
    /// `uncorroborated`: neither. Transfers gated; the pair still serves
    /// as LLM context.
    Uncorroborated,
}

/// TS `recordCorroboration` (:835) for one close pair. The short-circuit
/// is preserved exactly — the shingle sets are consulted only when no
/// statement aligned — so the counters record which signal actually
/// fired. `prior_fn` / `fresh_fn` are row indices into the two FUNCTION
/// indexes (the shingle sets are the same ones the cascade's tiebreaker
/// reads).
pub fn corroborate(
    aligned_statements: usize,
    prior_fn: usize,
    fresh_fn: usize,
    prior_index: &FingerprintIndex<'_>,
    fresh_index: &FingerprintIndex<'_>,
) -> Corroboration {
    if aligned_statements >= 1 {
        return Corroboration::Alignment;
    }
    // TS `shinglesCorroborate` (:1090): empty shingle sets are missing
    // evidence, not agreement — tiny featureless functions must not pass
    // on vacuous similarity.
    let prior_shingles = prior_index.compute_shingle_set(prior_fn);
    let fresh_shingles = fresh_index.compute_shingle_set(fresh_fn);
    if prior_shingles.is_empty() || fresh_shingles.is_empty() {
        return Corroboration::Uncorroborated;
    }
    if jaccard_similarity(&prior_shingles, &fresh_shingles) >= SHINGLE_SIMILARITY_FLOOR {
        Corroboration::Shingles
    } else {
        Corroboration::Uncorroborated
    }
}

/// `--probe shingle-probe`'s line for one close pair (prior-version.ts
/// `probeShingles`): the score as computed, the score with each edge
/// n-gram's own hash prefix dropped (`edge:<callee>`), the edge and token
/// counts, and both verdicts against the floor. It changes nothing — the
/// corroboration verdict is [`corroborate`]'s either way.
pub fn shingle_probe_line(
    fresh_id: &str,
    prior: &std::collections::BTreeSet<String>,
    fresh: &std::collections::BTreeSet<String>,
    aligned: usize,
) -> String {
    use humanify_model::js::to_fixed;
    if prior.is_empty() || fresh.is_empty() {
        return format!(
            "shingle-probe {fresh_id}: empty set (prior {}, fresh {}), aligned={aligned}",
            prior.len(),
            fresh.len()
        );
    }
    let unprefixed = |s: &std::collections::BTreeSet<String>| {
        s.iter()
            .map(|tok| match tok.split_once('→') {
                Some((_, callee)) => format!("edge:{callee}"),
                None => tok.clone(),
            })
            .collect::<std::collections::BTreeSet<String>>()
    };
    let as_is = jaccard_similarity(prior, fresh);
    let no_prefix = jaccard_similarity(&unprefixed(prior), &unprefixed(fresh));
    let edges =
        |s: &std::collections::BTreeSet<String>| s.iter().filter(|t| t.contains('→')).count();
    let verdict = |x: f64| {
        if x >= SHINGLE_SIMILARITY_FLOOR {
            "pass"
        } else {
            "fail"
        }
    };
    format!(
        "shingle-probe {fresh_id}: asis={} noprefix={} edges={}/{} tokens={}/{} aligned={aligned} verdict={}/{}",
        to_fixed(as_is, 4),
        to_fixed(no_prefix, 4),
        edges(prior),
        edges(fresh),
        prior.len(),
        fresh.len(),
        verdict(as_is),
        verdict(no_prefix)
    )
}

#[cfg(test)]
mod close_test;
