//! The close-match tier's tests, ported from `src/analysis/close-match.test.ts`
//! (fixture-for-fixture; see each test's doc comment).
//!
//! ADAPTATION of the TS suite's `featurelessIndex` helper: the TS builds a
//! synthetic FingerprintIndex whose fingerprints carry no `features` (what
//! `buildBindingFullFingerprint` produces for every module binding). The
//! Rust type cannot lie that way — a [`FingerprintIndex`] is built from real
//! rows — so the live-path equivalent is used: the BINDING fingerprint index
//! (`build_binding_fingerprint_index`), whose entries are
//! `Function::Binding` with `features() == None`. That is the exact
//! production condition the TS test documents (a caller passing binding ids),
//! exercised through the real builder.

use std::collections::HashMap;

use oxc_allocator::Allocator;

use crate::graph::UnifiedGraph;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::cascade::{self, MatchOptions};
use crate::matching::statement_context::StatementContexts;
use crate::matching::{FingerprintIndex, build_binding_fingerprint_index, build_fingerprint_index};

use super::{
    CLOSE_MATCH_TOP_K, Corroboration, FeatureVector, corroborate, find_close_matches, score_pairs,
};

/// One parsed side of a close-match test: its graph, both indexes and the
/// statement contexts (the cascade reads them; the close tier does not).
struct SideRefs<'a> {
    graph: &'a UnifiedGraph,
    fn_index: &'a FingerprintIndex<'a>,
    binding_index: &'a FingerprintIndex<'a>,
    ctx: &'a StatementContexts,
}

impl SideRefs<'_> {
    /// The session id of the function row NAMED `name`.
    fn id_of(&self, name: &str) -> String {
        self.graph
            .functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.session_id.clone())
            .unwrap_or_else(|| panic!("no function named {name}"))
    }

    fn all_fn_ids(&self) -> Vec<String> {
        self.graph
            .functions
            .iter()
            .map(|f| f.session_id.clone())
            .collect()
    }

    /// The module-binding session ids (`module:<name>`) — the ids whose
    /// fingerprints carry no `features`.
    fn all_binding_ids(&self) -> Vec<String> {
        self.graph
            .module_bindings
            .iter()
            .map(|b| b.session_id.clone())
            .collect()
    }

    /// TS `matchFunctions(oldIndex, newIndex)` over the function indexes.
    fn match_fn(&self, new: &SideRefs<'_>) -> cascade::MatchResult {
        cascade::match_functions(
            self.fn_index,
            new.fn_index,
            self.ctx,
            new.ctx,
            MatchOptions::default(),
        )
    }
}

/// Build both sides and hand them to the assertions inside the scope that
/// owns the arenas (the Semantic borrows the allocator, and the indexes
/// borrow the graphs — nothing can leave).
fn with_sides<T>(
    old_code: &str,
    new_code: &str,
    run: impl FnOnce(&SideRefs<'_>, &SideRefs<'_>) -> T,
) -> T {
    let old_allocator = Allocator::default();
    let new_allocator = Allocator::default();
    let old_ingest = Ingest::parse(&old_allocator, old_code, "test.js");
    assert!(
        old_ingest.errors.is_empty(),
        "old must parse: {:?}",
        old_ingest.errors
    );
    let new_ingest = Ingest::parse(&new_allocator, new_code, "test.js");
    assert!(
        new_ingest.errors.is_empty(),
        "new must parse: {:?}",
        new_ingest.errors
    );

    let old_tables = SymbolTables::build(old_ingest.semantic());
    let new_tables = SymbolTables::build(new_ingest.semantic());
    let old_graph = crate::graph::build_unified_graph(
        old_ingest.semantic(),
        old_ingest.program,
        "test.js",
        &[],
        None,
        None,
    );
    let new_graph = crate::graph::build_unified_graph(
        new_ingest.semantic(),
        new_ingest.program,
        "test.js",
        &[],
        None,
        None,
    );
    let old_fn_index = build_fingerprint_index(&old_graph, old_ingest.semantic(), &old_tables);
    let new_fn_index = build_fingerprint_index(&new_graph, new_ingest.semantic(), &new_tables);
    let old_binding_index =
        build_binding_fingerprint_index(&old_graph, old_ingest.semantic(), &old_tables);
    let new_binding_index =
        build_binding_fingerprint_index(&new_graph, new_ingest.semantic(), &new_tables);
    let old_ctx = StatementContexts::build(
        &old_graph,
        old_ingest.semantic(),
        &old_tables,
        old_ingest.program,
        old_code,
    );
    let new_ctx = StatementContexts::build(
        &new_graph,
        new_ingest.semantic(),
        &new_tables,
        new_ingest.program,
        new_code,
    );

    let old = SideRefs {
        graph: &old_graph,
        fn_index: &old_fn_index,
        binding_index: &old_binding_index,
        ctx: &old_ctx,
    };
    let new = SideRefs {
        graph: &new_graph,
        fn_index: &new_fn_index,
        binding_index: &new_binding_index,
        ctx: &new_ctx,
    };
    run(&old, &new)
}

/// Fresh ids not claimed by the cascade's matches (the TS
/// `[...newIndex.fingerprints.keys()].filter(id => !claimed.has(id))`).
fn unclaimed_new(new: &SideRefs<'_>, matches: &HashMap<String, String>) -> Vec<String> {
    new.all_fn_ids()
        .into_iter()
        .filter(|id| !matches.values().any(|v| v == id))
        .collect()
}

// ---------------------------------------------------------------------------
// scorePairs candidate bound
// ---------------------------------------------------------------------------

/// TS "keeps at most top-K candidates per old function": 50x50 identical
/// vectors used to materialize 2,500 pairs; on the real bundle that is
/// ~8Kx8K unmatched functions — a memory cliff.
#[test]
fn keeps_at_most_top_k_candidates_per_old_function() {
    let vector = FeatureVector {
        arity: 1.0,
        complexity: 2.0,
        return_count: 1.0,
        loop_count: 0.0,
        branch_count: 1.0,
        try_count: 0.0,
        callee_count: 1.0,
        external_call_count: 1.0,
        string_literal_count: 1.0,
        property_access_count: 2.0,
        numeric_literal_count: 0.0,
        has_rest_param: 0.0,
    };
    let olds: Vec<(String, FeatureVector)> = (0..50).map(|i| (format!("old{i}"), vector)).collect();
    let news: Vec<(String, FeatureVector)> = (0..50).map(|i| (format!("new{i}"), vector)).collect();

    let candidates = score_pairs(&olds, &news, 0.8);

    assert!(
        candidates.len() <= 50 * CLOSE_MATCH_TOP_K,
        "expected <= {} candidates, got {}",
        50 * CLOSE_MATCH_TOP_K,
        candidates.len()
    );
    let olds_covered: std::collections::HashSet<&str> =
        candidates.iter().map(|c| c.old_id.as_str()).collect();
    assert_eq!(olds_covered.len(), 50, "every old keeps its best K");
}

// ---------------------------------------------------------------------------
// findCloseMatches
// ---------------------------------------------------------------------------

/// TS "matches functions that differ by one statement": a function with a
/// minor modification (added console.log). Standard matching must not match
/// (different structure); close matching finds the similarity.
#[test]
fn matches_functions_that_differ_by_one_statement() {
    with_sides(
        r#"
      function process(x) {
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    "#,
        r#"
      function process(x) {
        console.log("debug");
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    "#,
        |old, new| {
            // Standard matching should not match (different structure).
            let result = old.match_fn(new);
            assert_eq!(result.matches.len(), 0, "Should not exact-match");

            // Close matching should find the similarity.
            let close = find_close_matches(
                &result.unmatched,
                &unclaimed_new(new, &result.matches),
                old.fn_index,
                new.fn_index,
                None,
            );
            assert_eq!(close.pairs.len(), 1, "Should find 1 close match");
            for pair in &close.pairs {
                assert!(
                    pair.score > 0.5,
                    "Similarity should be high, got {}",
                    pair.score
                );
            }
        },
    );
}

/// TS "does not match unrelated functions".
#[test]
fn does_not_match_unrelated_functions() {
    with_sides(
        r#"
      function fetchData(url) {
        return fetch(url).then(function(r) { return r.json(); });
      }
    "#,
        r#"
      function calculate(x, y) {
        for (var i = 0; i < x; i++) {
          if (i > y) return i;
        }
        return 0;
      }
    "#,
        |old, new| {
            let close = find_close_matches(
                &old.all_fn_ids(),
                &new.all_fn_ids(),
                old.fn_index,
                new.fn_index,
                Some(0.7),
            );
            assert_eq!(
                close.pairs.len(),
                0,
                "Unrelated functions should not close-match"
            );
        },
    );
}

/// TS "abstains when two candidates tie exactly — an arbitrary pick is not
/// a match": every cascade tier abstains on a tie; the close tier resolved
/// one by Map insertion order, which cross-version can cross-pair
/// same-shaped siblings and present the coin flip as a match. Two old and
/// two new functions with IDENTICAL feature vectors (same counts, different
/// free callees so no hash matches) tie in all four pairings — nothing may
/// match.
#[test]
fn abstains_when_two_candidates_tie_exactly() {
    with_sides(
        r#"
      function loadAlpha(x) { if (x) { return alphaSvc(x); } return 0; }
      function loadBeta(x) { if (x) { return betaSvc(x); } return 0; }
    "#,
        r#"
      function n1(x) { if (x) { return gammaSvc(x); } return 0; }
      function n2(x) { if (x) { return deltaSvc(x); } return 0; }
    "#,
        |old, new| {
            let result = old.match_fn(new);
            assert_eq!(result.unmatched.len(), 2, "both must reach close-match");

            let close = find_close_matches(
                &result.unmatched,
                &new.all_fn_ids(),
                old.fn_index,
                new.fn_index,
                Some(0.7),
            );
            assert!(
                close.pairs.is_empty(),
                "tied candidates must abstain, got: {:?}",
                close.pairs
            );
        },
    );
}

/// TS "returns empty when no unmatched functions".
#[test]
fn returns_empty_when_no_unmatched_functions() {
    with_sides("function a() {}", "function b() {}", |old, new| {
        let close = find_close_matches(&[], &[], old.fn_index, new.fn_index, None);
        assert!(close.pairs.is_empty());
        assert_eq!(close.skipped_old, 0);
        assert_eq!(close.skipped_new, 0);
    });
}

/// TS "respects threshold parameter": functions with meaningfully different
/// structure (different feature vectors). Very high threshold rejects; a
/// lower one accepts.
#[test]
fn respects_threshold_parameter() {
    with_sides(
        r#"
      function calc(x) {
        return x + 1;
      }
    "#,
        r#"
      function calc(x) {
        for (var i = 0; i < x; i++) {
          if (i > 5) return i;
        }
        return x + 1;
      }
    "#,
        |old, new| {
            let strict = find_close_matches(
                &old.all_fn_ids(),
                &new.all_fn_ids(),
                old.fn_index,
                new.fn_index,
                Some(0.99),
            );
            assert!(strict.pairs.is_empty(), "Strict threshold should reject");

            let relaxed = find_close_matches(
                &old.all_fn_ids(),
                &new.all_fn_ids(),
                old.fn_index,
                new.fn_index,
                Some(0.3),
            );
            assert_eq!(relaxed.pairs.len(), 1, "Relaxed threshold should accept");
        },
    );
}

/// TS "picks the best match when multiple candidates exist": one old
/// function, two new candidates with different similarity.
#[test]
fn picks_the_best_match_when_multiple_candidates_exist() {
    with_sides(
        r#"
      function process(x) {
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
    "#,
        r#"
      function processV2(x) {
        console.log("start");
        if (!x) return null;
        for (var i = 0; i < x.length; i++) {
          console.log(x[i]);
        }
        return x;
      }
      function totallyDifferent(a, b, c) {
        try { return a + b + c; } catch(e) { return 0; }
      }
    "#,
        |old, new| {
            let close = find_close_matches(
                &old.all_fn_ids(),
                &new.all_fn_ids(),
                old.fn_index,
                new.fn_index,
                Some(0.3),
            );
            assert_eq!(close.pairs.len(), 1);
            // Should match to the similar function, not the totally
            // different one (the TS asserts only the count; the id pin is
            // free additional precision here).
            assert_eq!(close.pairs[0].fresh_id, new.id_of("processV2"));
        },
    );
}

// ---------------------------------------------------------------------------
// Skip counters
// ---------------------------------------------------------------------------

/// TS "counts ids it skipped for lack of features, per side" — through the
/// binding indexes (see the module doc for the featurelessIndex adaptation).
#[test]
fn counts_ids_it_skipped_for_lack_of_features_per_side() {
    with_sides(
        r#"var helperA = require("./a");
var helperB = require("./b");"#,
        r#"var helperA = require("./a");
var helperB = require("./b");"#,
        |old, new| {
            let old_ids = old.all_binding_ids();
            let new_ids = new.all_binding_ids();
            assert!(!old_ids.is_empty(), "fixture must produce module bindings");
            assert_eq!(old_ids.len(), 2);
            assert_eq!(new_ids.len(), 2);

            let result = find_close_matches(
                &old_ids,
                &new_ids,
                old.binding_index,
                new.binding_index,
                None,
            );
            assert!(result.pairs.is_empty(), "nothing can be scored");
            assert_eq!(
                result.skipped_old, 2,
                "both old ids were dropped for lack of features and it must be visible"
            );
            assert_eq!(result.skipped_new, 2);
        },
    );
}

/// TS "reports zero skips when every fingerprint carries features" — the
/// live path. If this ever reports a skip, some producer stopped populating
/// `features` and the close-match tier is quietly shrinking.
#[test]
fn reports_zero_skips_when_every_fingerprint_carries_features() {
    with_sides(
        "function a(x) { return x + 1; }",
        "function b(x) { return x + 2; }",
        |old, new| {
            let result = find_close_matches(
                &old.all_fn_ids(),
                &new.all_fn_ids(),
                old.fn_index,
                new.fn_index,
                None,
            );
            assert_eq!(result.skipped_old, 0);
            assert_eq!(result.skipped_new, 0);
        },
    );
}

/// TS "distinguishes 'nothing eligible' from 'nothing close'" — the two
/// cases the old signature collapsed into one empty Map.
#[test]
fn distinguishes_nothing_eligible_from_nothing_close() {
    with_sides(
        r#"var helperA = require("./a");"#,
        r#"var helperB = require("./b");"#,
        |old, new| {
            let nothing_eligible = find_close_matches(
                &old.all_binding_ids(),
                &new.all_binding_ids(),
                old.binding_index,
                new.binding_index,
                None,
            );
            let nothing_close = find_close_matches(
                &old.all_fn_ids(),
                &new.all_fn_ids(),
                old.fn_index,
                new.fn_index,
                Some(0.99),
            );

            assert!(nothing_eligible.pairs.is_empty());
            assert!(nothing_close.pairs.is_empty());
            assert!(
                nothing_eligible.skipped_old > 0 && nothing_close.skipped_old == 0,
                "the skip counters are what tells these two apart"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// Corroboration (prior-version.ts recordCorroboration — no TS unit test;
// pinned here for the WP2.2 gate's verdict column)
// ---------------------------------------------------------------------------

/// The short-circuit: at least one aligned statement corroborates WITHOUT
/// consulting the shingle sets (even indexes whose shingle sets are empty).
#[test]
fn corroboration_alignment_short_circuits_the_shingles() {
    with_sides("function a() {}", "function b() {}", |old, new| {
        let prior_fn = old
            .fn_index
            .entry_of_session(&old.id_of("a"))
            .expect("row exists");
        let fresh_fn = new
            .fn_index
            .entry_of_session(&new.id_of("b"))
            .expect("row exists");
        assert_eq!(
            corroborate(1, prior_fn, fresh_fn, old.fn_index, new.fn_index),
            Corroboration::Alignment
        );
    });
}

/// The shingle arm: identical rename-invariant content under different
/// names clears the floor without any aligned statement. The edge n-grams
/// are HASH-PREFIXED (the function's own structural hash), so a close pair
/// — which by definition did not pair by hash — can never intersect there;
/// the fallback signal is the ext:/prop:/str: FEATURE TOKENS. This fixture
/// carries one property access, nothing else: no callees, no literals, so
/// the sets are exactly `{prop:value}` on both sides (jaccard 1).
#[test]
fn corroboration_shingles_fallback_and_empty_refusal() {
    with_sides(
        "function priorFn(x) { return x.value; }",
        "function freshFn(x) { return x.value; }",
        |old, new| {
            let prior_fn = old
                .fn_index
                .entry_of_session(&old.id_of("priorFn"))
                .expect("row exists");
            let fresh_fn = new
                .fn_index
                .entry_of_session(&new.id_of("freshFn"))
                .expect("row exists");
            assert_eq!(
                corroborate(0, prior_fn, fresh_fn, old.fn_index, new.fn_index),
                Corroboration::Shingles
            );
        },
    );
    // Empty shingle sets are missing evidence, not agreement — tiny
    // featureless functions must not pass on vacuous similarity.
    with_sides("function e1() {}", "function e2() {}", |old, new| {
        let prior_fn = old
            .fn_index
            .entry_of_session(&old.id_of("e1"))
            .expect("row exists");
        let fresh_fn = new
            .fn_index
            .entry_of_session(&new.id_of("e2"))
            .expect("row exists");
        let prior_set = old.fn_index.compute_shingle_set(prior_fn);
        let fresh_set = new.fn_index.compute_shingle_set(fresh_fn);
        assert!(
            prior_set.is_empty() && fresh_set.is_empty(),
            "fixture must produce empty shingle sets, got {prior_set:?} / {fresh_set:?}"
        );
        assert_eq!(
            corroborate(0, prior_fn, fresh_fn, old.fn_index, new.fn_index),
            Corroboration::Uncorroborated
        );
    });
}

// ---------------------------------------------------------------------------
// PARITY against the frozen TS probe (`test/parity/wp22-probe.mjs`, frozen
// at `test/parity/wp22-synthetic.json`): the cosine scores, the ordered
// candidate matrix and the real-fixture assignments are asserted EXACTLY —
// JSON numbers are shortest-roundtrip, so the frozen value IS the bit
// pattern the Rust side must produce.
// ---------------------------------------------------------------------------

fn frozen() -> serde_json::Value {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/parity/wp22-synthetic.json"
    ))
    .expect("the WP2.2 probe output must be frozen at test/parity/wp22-synthetic.json");
    serde_json::from_str(&raw).expect("probe JSON parses")
}

/// The frozen f64 BIT pattern (the probe's `scoreBits` hex). The frozen
/// DECIMAL is shortest-roundtrip and parses back to the identical bits
/// under a correctly rounding parser, but serde_json's default float
/// parser is up to 1 ulp off on full-precision literals — so the parity
/// asserts read the hex, never the decimal.
fn frozen_bits(row: &serde_json::Value) -> u64 {
    u64::from_str_radix(row["scoreBits"].as_str().expect("scoreBits hex"), 16)
        .expect("scoreBits parses")
}

/// The probe's synthetic vectors (VEC in wp22-probe.mjs), in TS field order.
fn probe_vector(name: &str) -> FeatureVector {
    let (
        arity,
        complexity,
        return_count,
        loop_count,
        branch_count,
        try_count,
        callee_count,
        external_call_count,
        string_literal_count,
        property_access_count,
        numeric_literal_count,
        has_rest_param,
    ) = match name {
        "suiteVector" => (1.0, 2.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0, 2.0, 0.0, 0.0),
        "rich" => (3.0, 7.0, 2.0, 2.0, 4.0, 1.0, 5.0, 3.0, 6.0, 9.0, 4.0, 1.0),
        "v2" => (2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        "v4" => (0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        "zeros" => (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        "disjoint" => (0.0, 0.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
        _ => panic!("unknown probe vector {name}"),
    };
    FeatureVector {
        arity,
        complexity,
        return_count,
        loop_count,
        branch_count,
        try_count,
        callee_count,
        external_call_count,
        string_literal_count,
        property_access_count,
        numeric_literal_count,
        has_rest_param,
    }
}

/// Every frozen cosine score is reproduced BIT-EXACTLY (one (old, new)
/// pair per scorePairs call, threshold 0 — no top-K capping hides a score).
#[test]
fn parity_cosine_scores_are_bit_exact() {
    let frozen = frozen();
    assert_eq!(
        frozen["closeMatchTopK"].as_u64(),
        Some(CLOSE_MATCH_TOP_K as u64),
        "the TS top-K constant the port must mirror"
    );
    for (label, case) in frozen["cosine"].as_object().expect("cosine object") {
        let a = probe_vector(case["a"].as_str().expect("vector name"));
        let b = probe_vector(case["b"].as_str().expect("vector name"));
        let candidates = score_pairs(&[("a".to_string(), a)], &[("b".to_string(), b)], 0.0);
        assert_eq!(candidates.len(), 1, "{label}: one candidate");
        let frozen_score = frozen_bits(case);
        assert!(
            candidates[0].score.to_bits() == frozen_score,
            "{label}: Rust bits {:x} vs frozen TS bits {:x}",
            candidates[0].score.to_bits(),
            frozen_score
        );
    }
}

/// The frozen candidate matrix (4x4 synthetic, ties and distinct scores at
/// threshold 0.5) is reproduced in ORDER with the top-K cap — insertion
/// order is load-bearing, so the list equality includes it.
#[test]
fn parity_candidate_matrix_matches_in_order() {
    let frozen = frozen();
    let olds: Vec<(String, FeatureVector)> = [
        ("o1", "v2"),
        ("o2", "v4"),
        ("o3", "suiteVector"),
        ("o4", "rich"),
    ]
    .iter()
    .map(|(id, v)| (id.to_string(), probe_vector(v)))
    .collect();
    let news: Vec<(String, FeatureVector)> = [
        ("n1", "v2"),
        ("n2", "v4"),
        ("n3", "suiteVector"),
        ("n4", "disjoint"),
    ]
    .iter()
    .map(|(id, v)| (id.to_string(), probe_vector(v)))
    .collect();

    let candidates = score_pairs(&olds, &news, 0.5);

    let frozen_matrix = frozen["matrix"].as_array().expect("matrix array");
    assert_eq!(
        candidates.len(),
        frozen_matrix.len(),
        "candidate count (the top-K cap included)"
    );
    for (candidate, row) in candidates.iter().zip(frozen_matrix) {
        assert_eq!(candidate.old_id, row["oldId"].as_str().expect("oldId"));
        assert_eq!(candidate.new_id, row["newId"].as_str().expect("newId"));
        assert!(
            candidate.score.to_bits() == frozen_bits(row),
            "matrix {}->{}: Rust bits {:x} vs frozen bits {:x}",
            candidate.old_id,
            candidate.new_id,
            candidate.score.to_bits(),
            frozen_bits(row)
        );
    }
}

/// The frozen real-fixture assignments (tie abstention, disjoint matches,
/// threshold behavior, binding-id skips) are reproduced exactly — pair ids
/// (the session-id BYTES), scores (bit-exact) and the skip counters.
#[test]
fn parity_assignments_match_the_frozen_fixtures() {
    let frozen = frozen()["assign"]
        .as_object()
        .expect("assign object")
        .clone();
    // The Rust side re-runs the same fixtures; the ids come out of the real
    // graph, so this test walks the frozen cases by name.
    for label in frozen.keys() {
        match label.as_str() {
            "contested_tie_abstains" => with_sides(
                "function only(x) { if (x) { return svc(x); } return 0; }",
                "function n1(x) { if (x) { return svc(x); } return 0; }\nfunction n2(x) { if (x) { return svc(x); } return 0; }",
                |old, new| {
                    let close = find_close_matches(
                        &old.all_fn_ids(),
                        &new.all_fn_ids(),
                        old.fn_index,
                        new.fn_index,
                        Some(0.8),
                    );
                    let case = &frozen[label];
                    assert_eq!(close.pairs.len(), case["pairs"].as_array().unwrap().len());
                    assert_eq!(
                        close.skipped_old,
                        case["skippedOld"].as_u64().unwrap() as usize
                    );
                    assert_eq!(
                        close.skipped_new,
                        case["skippedNew"].as_u64().unwrap() as usize
                    );
                },
            ),
            "disjoint_pairs_both_match" => with_sides(
                "function aa(x, y) { }\nfunction bb(o) { o.a; o.b; }",
                "function m(x, y) { }\nfunction n(o) { o.x; o.y; }",
                |old, new| {
                    let close = find_close_matches(
                        &old.all_fn_ids(),
                        &new.all_fn_ids(),
                        old.fn_index,
                        new.fn_index,
                        Some(0.8),
                    );
                    let case = &frozen[label];
                    let frozen_pairs = case["pairs"].as_array().unwrap();
                    assert_eq!(
                        close.pairs.len(),
                        frozen_pairs.len(),
                        "both disjoint pairs match"
                    );
                    for (pair, frozen_pair) in close.pairs.iter().zip(frozen_pairs) {
                        assert_eq!(
                            pair.prior_id,
                            frozen_pair["oldId"].as_str().unwrap(),
                            "session-id bytes"
                        );
                        assert_eq!(
                            pair.fresh_id,
                            frozen_pair["newId"].as_str().unwrap(),
                            "session-id bytes"
                        );
                        assert!(
                            pair.score.to_bits() == frozen_bits(frozen_pair),
                            "{label}: Rust bits {:x} vs frozen bits {:x}",
                            pair.score.to_bits(),
                            frozen_bits(frozen_pair)
                        );
                    }
                },
            ),
            "one_statement_diff"
            | "threshold_strict_rejects"
            | "threshold_relaxed_accepts"
            | "best_pick" => {
                let (old_code, new_code, threshold) = match label.as_str() {
                    "one_statement_diff" | "best_pick" => (
                        concat!(
                            "\n      function process(x) {\n",
                            "        if (!x) return null;\n",
                            "        for (var i = 0; i < x.length; i++) {\n",
                            "          console.log(x[i]);\n",
                            "        }\n",
                            "        return x;\n",
                            "      }\n    "
                        ),
                        concat!(
                            "\n      function process(x) {\n",
                            "        console.log(\"debug\");\n",
                            "        if (!x) return null;\n",
                            "        for (var i = 0; i < x.length; i++) {\n",
                            "          console.log(x[i]);\n",
                            "        }\n",
                            "        return x;\n",
                            "      }\n    "
                        ),
                        0.8,
                    ),
                    _ => (
                        "\n      function calc(x) {\n        return x + 1;\n      }\n    ",
                        concat!(
                            "\n      function calc(x) {\n",
                            "        for (var i = 0; i < x; i++) {\n",
                            "          if (i > 5) return i;\n",
                            "        }\n",
                            "        return x + 1;\n",
                            "      }\n    "
                        ),
                        if label == "threshold_strict_rejects" {
                            0.99
                        } else {
                            0.3
                        },
                    ),
                };
                with_sides(old_code, new_code, |old, new| {
                    let close = find_close_matches(
                        &old.all_fn_ids(),
                        &new.all_fn_ids(),
                        old.fn_index,
                        new.fn_index,
                        Some(threshold),
                    );
                    let case = &frozen[label];
                    let frozen_pairs = case["pairs"].as_array().unwrap();
                    assert_eq!(close.pairs.len(), frozen_pairs.len(), "{label}");
                    for (pair, frozen_pair) in close.pairs.iter().zip(frozen_pairs) {
                        assert_eq!(
                            pair.prior_id,
                            frozen_pair["oldId"].as_str().unwrap(),
                            "session-id bytes"
                        );
                        assert_eq!(
                            pair.fresh_id,
                            frozen_pair["newId"].as_str().unwrap(),
                            "session-id bytes"
                        );
                        assert!(
                            pair.score.to_bits() == frozen_bits(frozen_pair),
                            "{label}: Rust bits {:x} vs frozen bits {:x}",
                            pair.score.to_bits(),
                            frozen_bits(frozen_pair)
                        );
                    }
                });
            }
            "binding_ids_skipped" => with_sides(
                "var helperA = require(\"./a\");\nvar helperB = require(\"./b\");",
                "var helperA = require(\"./a\");\nvar helperB = require(\"./b\");",
                |old, new| {
                    let close = find_close_matches(
                        &old.all_binding_ids(),
                        &new.all_binding_ids(),
                        old.binding_index,
                        new.binding_index,
                        None,
                    );
                    let case = &frozen[label];
                    assert!(close.pairs.is_empty());
                    assert_eq!(
                        close.skipped_old,
                        case["skippedOld"].as_u64().unwrap() as usize
                    );
                    assert_eq!(
                        close.skipped_new,
                        case["skippedNew"].as_u64().unwrap() as usize
                    );
                },
            ),
            other => panic!("frozen assignment case {other} has no Rust runner"),
        }
    }
}
