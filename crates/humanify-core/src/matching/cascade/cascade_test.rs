//! The cascade's tests (WP2.1 part 2), ported from
//! `src/analysis/fingerprint-index.test.ts`. Two layers:
//!
//! 1. BEHAVIOR: the TS suite's meaningful cases re-run against the Rust
//!    cascade — the same fixtures, the same assertions (counts and stats).
//! 2. PARITY against the frozen TS probe (`test/parity/wp21-cascade-probe.mjs`,
//!    frozen at `test/parity/wp21-cascade-synthetic.json`): the cascade's
//!    DECISION OUTPUTS — matches, tiers, rejections, the whole stats bag —
//!    asserted exactly (these are decisions, not hash bytes).
//!
//! The TS suite's two propagation-dependent tests are ADAPTED here because
//! `enable_propagation` is a STUB in this scope (the hook point is wired,
//! the pass is not): "propagation re-resolves demoted claims injectively"
//! and "resolves ambiguous functions that cascade alone cannot" pin the
//! stub's semantics (unchanged result, `propagationResolved: 0`) and note
//! the TS expectations that come back once the pass is ported.

use std::collections::{HashMap, HashSet};
use std::fs;

use oxc_allocator::Allocator;

use crate::graph::UnifiedGraph;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::cascade::{
    self, MatchOptions, MatchResult, certify_interchangeable_pools, find_new_functions,
    get_match_stats, resolve_ambiguous_by_ordinal,
};
use crate::matching::statement_context::StatementContexts;
use crate::matching::{
    CalleeShape, CfgType, FingerprintIndex, build_binding_fingerprint_index,
    build_fingerprint_index, callee_shapes_equal,
};
use serde_json::json;

/// One parsed side of a cascade test: its graph, both indexes (the cascade
/// runs over function indexes AND module-binding indexes) and the statement
/// contexts (one build serves both — the contexts carry function AND binding
/// rows).
struct SideRefs<'a> {
    graph: &'a UnifiedGraph,
    fn_index: &'a FingerprintIndex<'a>,
    binding_index: &'a FingerprintIndex<'a>,
    ctx: &'a StatementContexts,
}

impl SideRefs<'_> {
    /// The session id of the function row NAMED `name` (the TS helpers find
    /// ids through the AST's function id).
    fn id_of(&self, name: &str) -> String {
        self.graph
            .functions
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.session_id.clone())
            .unwrap_or_else(|| panic!("no function named {name}"))
    }

    /// The inverse of `id_of` — the source name behind a session id.
    fn name_of(&self, session_id: &str) -> String {
        self.graph
            .functions
            .iter()
            .find(|f| f.session_id == session_id)
            .map(|f| f.name.clone())
            .unwrap_or_else(|| session_id.to_string())
    }

    /// TS `matchFunctions(oldIndex, newIndex)` over the function indexes.
    fn match_fn(&self, new: &SideRefs<'_>, options: MatchOptions<'_>) -> MatchResult {
        cascade::match_functions(self.fn_index, new.fn_index, self.ctx, new.ctx, options)
    }

    /// The same cascade over the BINDING indexes (TS `buildBindingIndexAsMap`
    /// + `matchFunctions`).
    fn match_bindings(&self, new: &SideRefs<'_>, options: MatchOptions<'_>) -> MatchResult {
        cascade::match_functions(
            self.binding_index,
            new.binding_index,
            self.ctx,
            new.ctx,
            options,
        )
    }

    /// The `Side` handle the tail tiers take.
    fn fn_side(&self) -> cascade::Side<'_, '_> {
        cascade::Side::new(self.fn_index, self.ctx)
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

    let old_tables = SymbolTables::build(&old_ingest.semantic);
    let new_tables = SymbolTables::build(&new_ingest.semantic);
    let old_graph = crate::graph::build_unified_graph(
        &old_ingest.semantic,
        old_ingest.program,
        "test.js",
        &[],
        None,
        None,
    );
    let new_graph = crate::graph::build_unified_graph(
        &new_ingest.semantic,
        new_ingest.program,
        "test.js",
        &[],
        None,
        None,
    );
    let old_fn_index = build_fingerprint_index(&old_graph, &old_ingest.semantic, &old_tables);
    let new_fn_index = build_fingerprint_index(&new_graph, &new_ingest.semantic, &new_tables);
    let old_binding_index =
        build_binding_fingerprint_index(&old_graph, &old_ingest.semantic, &old_tables);
    let new_binding_index =
        build_binding_fingerprint_index(&new_graph, &new_ingest.semantic, &new_tables);
    let old_ctx = StatementContexts::build(
        &old_graph,
        &old_ingest.semantic,
        &old_tables,
        old_ingest.program,
        old_code,
    );
    let new_ctx = StatementContexts::build(
        &new_graph,
        &new_ingest.semantic,
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

/// Every fresh id in `matches` is claimed at most once.
fn assert_injective(matches: &HashMap<String, String>) {
    let claimed: HashSet<&String> = matches.values().collect();
    assert_eq!(
        claimed.len(),
        matches.len(),
        "a fresh id was claimed twice: {matches:?}"
    );
}

/// A relational array is sorted (the `arraysEqual` precondition; the TS
/// suite's `isSorted`).
fn is_sorted(a: &[String]) -> bool {
    a.windows(2).all(|w| w[0] <= w[1])
}

// ---------------------------------------------------------------------------
// buildFingerprintIndex
// ---------------------------------------------------------------------------

/// TS "indexes all functions by structuralHash".
#[test]
fn indexes_all_functions_by_structural_hash() {
    with_sides(
        "\n      function a() { return \"hello\"; }\n      function b(x) { return x + 1; }\n      function c(x, y) { if (x) return y; return null; }\n    ",
        "\n      function a() { return \"hello\"; }\n      function b(x) { return x + 1; }\n      function c(x, y) { if (x) return y; return null; }\n    ",
        |old, _new| {
            assert_eq!(old.fn_index.entries.len(), 3, "3 fingerprints");
            assert_eq!(old.fn_index.by_structural_hash.len(), 3, "3 unique hashes");
        },
    );
}

/// TS "groups duplicate structures under same structuralHash".
#[test]
fn groups_duplicate_structures_under_same_hash() {
    with_sides(INJECT_V1, INJECT_V1, |old, _new| {
        assert_eq!(old.fn_index.by_structural_hash.len(), 1, "1 unique hash");
        // The single bucket holds both entries.
        let total: usize = old.fn_index.by_structural_hash.values().map(Vec::len).sum();
        assert_eq!(total, 2, "hash maps to 2 entries");
    });
}

// ---------------------------------------------------------------------------
// matchFunctions — the core tiers
// ---------------------------------------------------------------------------

/// TS "matches identical functions across versions".
#[test]
fn matches_identical_functions_across_versions() {
    let code = "\n      function add(a, b) { return a + b; }\n      function sub(a, b) { return a - b; }\n    ";
    with_sides(code, code, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(result.matches.len(), 2, "both functions match");
        assert!(result.unmatched.is_empty(), "no unmatched");
        assert!(result.ambiguous.is_empty(), "no ambiguous");
    });
}

/// TS "handles renamed identifiers (minification)".
#[test]
fn handles_renamed_identifiers() {
    with_sides(
        "\n      function add(a, b) { return a + b; }\n      function multiply(x, y) { return x * y; }\n    ",
        "\n      function n(o, p) { return o + p; }\n      function q(r, s) { return r * s; }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(result.matches.len(), 2, "both match despite rename");
            assert!(result.unmatched.is_empty(), "no unmatched");
        },
    );
}

/// TS "marks changed functions as unmatched".
#[test]
fn marks_changed_functions_unmatched() {
    with_sides(
        "\n      function calc(x) { return x + 1; }\n    ",
        "\n      function calc(x) { return x * 2; }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(result.unmatched.len(), 1, "changed function unmatched");
            assert_eq!(result.matches.len(), 0, "no matches");
        },
    );
}

/// TS "uses callee shapes to disambiguate duplicates".
#[test]
fn uses_callee_shapes_to_disambiguate_duplicates() {
    with_sides(SHAPES_V1, SHAPES_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(result.matches.len(), 4, "all 4 functions match");
        assert!(result.ambiguous.is_empty(), "no ambiguous matches");
    });
}

/// TS "reports ambiguous when cannot disambiguate".
#[test]
fn reports_ambiguous_when_cannot_disambiguate() {
    with_sides(
        "\n      function wrapper1() { return helper1(); }\n      function wrapper2() { return helper2(); }\n      function helper1() { return 1; }\n      function helper2() { return 1; }\n    ",
        "\n      function a() { return c(); }\n      function b() { return d(); }\n      function c() { return 1; }\n      function d() { return 1; }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert!(
                !result.ambiguous.is_empty() || result.matches.len() == 4,
                "either ambiguous or all matched"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// resolutionStats tracking
// ---------------------------------------------------------------------------

/// TS "counts structuralHashUnique when each function has a unique hash".
#[test]
fn counts_structural_hash_unique() {
    let code =
        "\n      function a() { return \"hello\"; }\n      function b(x) { return x + 1; }\n    ";
    with_sides(code, code, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(result.resolution_stats.structural_hash_unique, 2);
        assert_eq!(result.resolution_stats.unmatched, 0);
        assert_eq!(result.resolution_stats.still_ambiguous, 0);
    });
}

/// TS "counts unmatched when hash not found".
#[test]
fn counts_unmatched_when_hash_not_found() {
    with_sides(
        "function a(x) { return x + 1; }",
        "function b(x) { return x * 2; }",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(result.resolution_stats.unmatched, 1);
            assert_eq!(result.resolution_stats.structural_hash_unique, 0);
        },
    );
}

/// TS "respects maxCascadeDepth option".
#[test]
fn respects_max_cascade_depth() {
    with_sides(SHAPES_V1, SHAPES_V2, |old, new| {
        let hash_only = old.match_fn(
            new,
            MatchOptions {
                max_cascade_depth: 0,
                ..MatchOptions::default()
            },
        );
        assert!(
            hash_only.resolution_stats.still_ambiguous > 0,
            "ambiguous at hash-only matching"
        );

        let full = old.match_fn(
            new,
            MatchOptions {
                max_cascade_depth: 2,
                ..MatchOptions::default()
            },
        );
        assert_eq!(full.resolution_stats.still_ambiguous, 0);
    });
}

/// TS "records WHY the enclosing-statement rung abstained".
#[test]
fn records_why_the_enclosing_rung_abstained() {
    with_sides(ABSTAIN_V1, ABSTAIN_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        let abstain = &result.resolution_stats.enclosing_stmt_abstain;
        let total = abstain.no_hash_is_statement
            + abstain.no_hash_too_long
            + abstain.no_hash_other
            + abstain.no_new_holders
            + abstain.count_mismatch
            + abstain.partner_filtered;
        assert!(
            total > 0,
            "the rung abstained but recorded no reason: {abstain:?}"
        );
        let bucketed: usize = abstain.reached_span_buckets.iter().sum();
        assert_eq!(
            bucketed, abstain.reached,
            "every function reaching the rung lands in exactly one span bucket"
        );
    });
}

/// TS "attributes a cap exclusion to the cap, not to the catch-all".
#[test]
fn attributes_cap_exclusion_to_the_cap() {
    let (v1, v2) = cap_fixtures();
    with_sides(&v1, &v2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        let abstain = &result.resolution_stats.enclosing_stmt_abstain;
        assert!(
            abstain.no_hash_too_long > 0,
            "expected the cap to be named as the reason, got {abstain:?}"
        );
        assert_eq!(
            abstain.no_hash_is_statement, 0,
            "an arrow inside a call is not its own statement"
        );
    });
}

// ---------------------------------------------------------------------------
// cross-version matching integration
// ---------------------------------------------------------------------------

/// TS "handles realistic minification scenario".
#[test]
fn handles_realistic_minification() {
    with_sides(MINIFY_V1, MINIFY_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        let stats = get_match_stats(&result);
        assert_eq!(stats.matched, 3, "all 3 functions match");
        assert_eq!(stats.unmatched, 0, "no unmatched");
    });
}

/// TS "detects when function internals change".
#[test]
fn detects_when_function_internals_change() {
    with_sides(
        "\n      function calculate(x) {\n        return x + 1;\n      }\n    ",
        "\n      function calculate(x) {\n        // Bug fix: multiply instead of add\n        return x * 2;\n      }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(result.matches.len(), 0, "changed function does not match");
            assert_eq!(result.unmatched.len(), 1, "unmatched");
        },
    );
}

/// TS "matches functions when unrelated code is added".
#[test]
fn matches_functions_when_unrelated_code_is_added() {
    with_sides(
        "\n      function existing() { return 42; }\n    ",
        "\n      function newFeature() {\n        for (let i = 0; i < 10; i++) {\n          if (i > 5) console.log(i);\n        }\n      }\n      function existing() { return 42; }\n      function anotherNew() { return \"hello\"; }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(result.matches.len(), 1, "existing function matches");
            assert_eq!(
                find_new_functions(old.fn_index, new.fn_index, &result).len(),
                2,
                "2 new functions identified"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// callerShapes disambiguation
// ---------------------------------------------------------------------------

/// TS "resolves when callerShapes differ".
#[test]
fn resolves_when_caller_shapes_differ() {
    with_sides(
        "\n      function complexCaller() {\n        for (let i = 0; i < 10; i++) {\n          if (i > 5) leaf1();\n        }\n      }\n      function simpleCaller() { leaf2(); }\n      function leaf1() { return 1; }\n      function leaf2() { return 1; }\n    ",
        "\n      function a() {\n        for (let i = 0; i < 10; i++) {\n          if (i > 5) c();\n        }\n      }\n      function b() { d(); }\n      function c() { return 1; }\n      function d() { return 1; }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert!(
                result.resolution_stats.caller_shapes_resolved > 0,
                "callerShapes resolutions"
            );
            assert_eq!(result.resolution_stats.still_ambiguous, 0, "no ambiguous");
        },
    );
}

/// TS "falls through when callerShapes also identical".
#[test]
fn falls_through_when_caller_shapes_identical() {
    with_sides(
        "\n      function caller1() { return leaf1(); }\n      function caller2() { return leaf2(); }\n      function leaf1() { return 1; }\n      function leaf2() { return 1; }\n    ",
        "\n      function a() { return c(); }\n      function b() { return d(); }\n      function c() { return 1; }\n      function d() { return 1; }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(
                result.resolution_stats.caller_shapes_resolved, 0,
                "callerShapes cannot help when the caller shapes are identical"
            );
        },
    );
}

/// TS "falls through when callerShapes empty".
#[test]
fn falls_through_when_caller_shapes_empty() {
    with_sides(
        "\n      function entry1() { return 1; }\n      function entry2() { return 1; }\n    ",
        INJECT_V1,
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(
                result.resolution_stats.caller_shapes_resolved, 0,
                "no callers, no callerShapes resolutions"
            );
            assert!(
                result.resolution_stats.still_ambiguous > 0,
                "remains ambiguous"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// memberKey disambiguation
// ---------------------------------------------------------------------------

/// TS "resolves two identical-hash functions by different object keys".
#[test]
fn resolves_two_identical_hash_functions_by_object_keys() {
    with_sides(MEMBERKEY_V1, MEMBERKEY_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(
            result.resolution_stats.member_key_resolved, 2,
            "both resolve via memberKey"
        );
        assert_eq!(result.resolution_stats.still_ambiguous, 0);
        assert_eq!(result.matches.len(), 2);
    });
}

/// TS "memberKey runs before callerShapes in the cascade".
#[test]
fn member_key_runs_before_caller_shapes() {
    with_sides(
        "\n      function complexCaller() {\n        for (let i = 0; i < 10; i++) { if (i > 5) obj.getCount(); }\n      }\n      function simpleCaller() { obj.getLabel(); }\n      var obj = {\n        getCount: function() { return 1; },\n        getLabel: function() { return 1; }\n      };\n    ",
        "\n      function a() {\n        for (let i = 0; i < 10; i++) { if (i > 5) o.getCount(); }\n      }\n      function b() { o.getLabel(); }\n      var o = {\n        getCount: function() { return 1; },\n        getLabel: function() { return 1; }\n      };\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(
                result.resolution_stats.member_key_resolved, 2,
                "memberKey fires, not callerShapes"
            );
            assert_eq!(
                result.resolution_stats.caller_shapes_resolved, 0,
                "callerShapes must not fire when memberKey already resolved"
            );
        },
    );
}

/// TS "falls through when functions have no memberKey".
#[test]
fn falls_through_when_functions_have_no_member_key() {
    with_sides(
        "\n      function leaf1() { return 1; }\n      function leaf2() { return 1; }\n    ",
        INJECT_V1,
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(
                result.resolution_stats.member_key_resolved, 0,
                "standalone functions carry no memberKey"
            );
        },
    );
}

/// TS "falls through when memberKey filter yields 0 matches".
#[test]
fn falls_through_when_member_key_filter_yields_zero() {
    with_sides(
        "\n      var obj = {\n        alpha: function() { return 1; },\n        beta: function() { return 1; }\n      };\n    ",
        INJECT_V1,
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(
                result.resolution_stats.member_key_resolved, 0,
                "new side has no matching keys"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// stop-on-empty cascade
// ---------------------------------------------------------------------------

/// TS "a candidate rejected by memberKey cannot win at a weaker stage".
#[test]
fn candidate_rejected_by_member_key_cannot_win_at_a_weaker_stage() {
    with_sides(
        "\n      var api = { run: function () { return work(); } };\n      function work(x) {\n        for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }\n        return 1;\n      }\n    ",
        "\n      var a1 = { run: function () { return lin1(); } };\n      var a2 = { run: function () { return lin2(); } };\n      var a3 = { walk: function () { return loopHelper(); } };\n      function lin1() { return 1; }\n      function lin2() { return 2; }\n      function loopHelper(x) {\n        for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }\n        return 1;\n      }\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(
                result.resolution_stats.callee_hashes_resolved, 0,
                "calleeHashes must not resolve using candidates memberKey rejected"
            );
            // F stays ambiguous; only the work→loopHelper pair may match.
            assert_eq!(result.matches.len(), 1);
            assert_eq!(result.ambiguous.len(), 1);
        },
    );
}

/// TS "propagation does not match a candidate that contradicts a matched
/// callee" — this one needs no adaptation: the expected matches (C→Cn,
/// H→Hn) close at the unique-hash tier, before propagation runs, and the
/// stub correctly leaves P ambiguous.
#[test]
fn propagation_does_not_match_a_candidate_contradicting_a_matched_callee() {
    with_sides(
        "\n      function C(x) {\n        for (let i = 0; i < 3; i++) { if (x) P(); }\n        return 42;\n      }\n      function P() { return H(); }\n      function H() { return true; }\n    ",
        "\n      function Cn(x) {\n        for (let i = 0; i < 3; i++) { if (x) Q1(); }\n        return 42;\n      }\n      function C2n(y) {\n        for (let k = 0; k < 9; k++) { if (y) Q2(); }\n        return \"s\";\n      }\n      function Q1() { return L1(); }\n      function Q2() { return L2(); }\n      function L1() { return 1; }\n      function L2() { return \"z\"; }\n      function Hn() { return true; }\n    ",
        |old, new| {
            let result = old.match_fn(
                new,
                MatchOptions {
                    enable_propagation: true,
                    ..MatchOptions::default()
                },
            );
            assert_eq!(result.matches.len(), 2, "C→Cn and H→Hn match");
            assert!(
                !result.ambiguous.is_empty(),
                "P stays ambiguous under contradiction"
            );
        },
    );
}

// ---------------------------------------------------------------------------
// singleton-bucket corroboration gate
// ---------------------------------------------------------------------------

/// TS "rejects a singleton match whose memberKeys contradict".
#[test]
fn rejects_singleton_match_with_contradicting_member_keys() {
    with_sides(SINGLETON_V1, SINGLETON_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(
            result.matches.len(),
            0,
            "memberKey contradiction must reject the singleton match"
        );
        assert_eq!(result.unmatched.len(), 1);
        assert_eq!(result.resolution_stats.singleton_rejected, 1);
    });
}

/// TS "accepts a singleton when the signal is one-sided (no contradiction)".
#[test]
fn accepts_singleton_when_signal_is_one_sided() {
    with_sides(
        SINGLETON_V1,
        "\n      var runner = function (y) {\n        for (let j = 0; j < 10; j++) { if (y > j) console.log(j); }\n        return 2;\n      };\n    ",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(result.matches.len(), 1);
            assert_eq!(result.resolution_stats.singleton_rejected, 0);
        },
    );
}

/// TS "reports singleton accepts the guard could not examine, per cascade":
/// function fingerprints carry features so every singleton accept IS
/// examined; binding fingerprints carry neither features nor a memberKey so
/// EVERY singleton accept is unexamined.
#[test]
fn reports_singleton_accepts_the_guard_could_not_examine() {
    with_sides(
        "var runner = function (y) { return y + 1; };",
        "var walker = function (z) { return z + 1; };",
        |old, new| {
            let result = old.match_fn(new, MatchOptions::default());
            assert_eq!(
                result.resolution_stats.singleton_unguarded, 0,
                "function fingerprints carry features, so every singleton accept IS examined"
            );
        },
    );

    let binding_code = |a: &str, b: &str| {
        format!(
            "const {a} = {{ path: \"/x\", timeout: 30 }};\nfunction use() {{ return {a}.path; }}\nmodule.exports = {{ use, {b}: {a} }};"
        )
    };
    let v1 = binding_code("alpha", "one");
    let v2 = binding_code("beta", "two");
    with_sides(&v1, &v2, |old, new| {
        let result = old.match_bindings(new, MatchOptions::default());
        assert!(
            result.resolution_stats.structural_hash_unique > 0,
            "fixture must produce singleton binding accepts, or the next assertion is vacuous"
        );
        assert_eq!(
            result.resolution_stats.singleton_unguarded,
            result.resolution_stats.structural_hash_unique,
            "binding fingerprints carry no features and no memberKey, so EVERY singleton accept is unexamined"
        );
    });
}

// ---------------------------------------------------------------------------
// injectivity
// ---------------------------------------------------------------------------

/// TS "never matches two old functions to the same new function".
#[test]
fn never_matches_two_old_functions_to_the_same_new_function() {
    with_sides(INJECT_V1, INJECT_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_injective(&result.matches);
        assert_eq!(
            result.matches.len(),
            0,
            "neither old function can safely claim the single new function"
        );
        assert_eq!(result.ambiguous.len(), 2, "both demoted to ambiguous");
        assert_eq!(result.resolution_stats.injectivity_demoted, 2);
        assert_eq!(
            result.resolution_stats.structural_hash_unique, 0,
            "demoted matches must not be counted as resolved"
        );
        assert_eq!(result.resolution_stats.still_ambiguous, 2);
    });
}

/// The ambiguous map's ORDER: parks append in walk order and a demote
/// re-park APPENDS at the map's end — the TS `Map.set` on a key that has no
/// entry, because a demoted prior was MATCHED and so never parked. The
/// port's previous reconstruction (old-index entry order) re-positioned the
/// re-parks at their index positions, mid-map, which changed which entries
/// saw whose claims during propagation — the WP2.1 residual (a
/// stillAmbiguous pool of 10 candidates in TS vs 7 in Rust on 2.1.85→86,
/// pinned by the traced dump against the TS replay probe).
///
/// Fixture: keyA1/keyA2 share a memberKey and both claim newKeyA (the
/// cascade does not enforce injectivity) — both demoted, re-parked with the
/// full bucket; plain parks during the walk (its 2-candidate bucket has no
/// distinguishing feature). Walk order keyA1, keyA2, plain — so the TS's
/// final map order is [plain, keyA1, keyA2], NOT the index order
/// [keyA1, keyA2, plain].
#[test]
fn demote_reparks_append_at_the_ambiguous_maps_end() {
    let v1 = "\n      var o1 = { keyA: function() { return 1; } };\n      var o2 = { keyA: function() { return 2; } };\n      function plain() { return 1; }\n    ";
    let v2 = "\n      var n1 = { keyA: function() { return 5; } };\n      var n2 = { keyB: function() { return 5; } };\n      var n3 = { keyC: function() { return 5; } };\n      function plainX() { return 1; }\n      function plainY() { return 1; }\n    ";
    with_sides(v1, v2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(
            result.resolution_stats.injectivity_demoted, 2,
            "keyA1 and keyA2 must over-claim newKeyA, or the order assertion is vacuous"
        );
        assert_eq!(result.ambiguous.len(), 3, "both demoted + plain parked");
        // The graph's function rows are in build order: [keyA1, keyA2, plain]
        // (the two object members share the property name "keyA", so the
        // session ids are the only unambiguous handle).
        let ids: Vec<String> = old
            .graph
            .functions
            .iter()
            .map(|f| f.session_id.clone())
            .collect();
        assert_eq!(ids.len(), 3, "fixture must have exactly three functions");
        let order: Vec<&String> = result.ambiguous.iter().map(|(id, _)| id).collect();
        assert_eq!(
            order,
            vec![&ids[2], &ids[0], &ids[1]],
            "the walk-parked prior comes first; the demote re-parks APPEND — they must not sit at their old-index positions"
        );
    });
}

/// TS "propagation re-resolves demoted claims injectively" — ADAPTED for
/// the stub: the TS expects matches == 2 / ambiguous == 1 once propagation
/// gives the single new leaf to leaf1 via the caller constraint. The stub
/// leaves the demotion in place, so this pins the DEMOTION instead: the
/// claims stay injective, bigCaller keeps its unique match, both leaf
/// priors sit in ambiguous. When the propagation pass is ported, this
/// assertion should be tightened to the TS's (2 matches, 1 ambiguous).
#[test]
fn propagation_re_resolves_demoted_claims_injectively_stub_semantics() {
    with_sides(
        "\n      function bigCaller(x) {\n        for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }\n        return leaf1();\n      }\n      function leaf1() { return 1; }\n      function leaf2() { return 1; }\n    ",
        "\n      function bc(y) {\n        for (let j = 0; j < 10; j++) { if (y > j) console.log(j); }\n        return L();\n      }\n      function L() { return 1; }\n    ",
        |old, new| {
            let result = old.match_fn(
                new,
                MatchOptions {
                    enable_propagation: true,
                    ..MatchOptions::default()
                },
            );
            assert_injective(&result.matches);
            // The propagation pass is WIRED: the TS expects 2 matches /
            // 1 ambiguous — propagation re-resolves ONE leaf via the
            // caller constraint (leaf1 into L; leaf2's pool shrinks to
            // nothing... the TS's own expectation).
            assert_eq!(
                result.matches.len(),
                2,
                "matches: {matches:#?}",
                matches = result.matches
            );
            assert_eq!(result.ambiguous.len(), 1);
            assert_eq!(result.resolution_stats.injectivity_demoted, 2);
            assert_eq!(result.resolution_stats.propagation_resolved, 1);
        },
    );
}

// ---------------------------------------------------------------------------
// enablePropagation integration
// ---------------------------------------------------------------------------

/// TS "resolves ambiguous functions that cascade alone cannot" — ADAPTED
/// for the stub: the TS expects the propagation post-pass to clear the
/// ambiguity (ambiguous == 0, `propagationResolved > 0`). The stub leaves
/// the cascade's result untouched, which is exactly what this pins. When
/// the pass is ported, tighten to the TS assertions.
#[test]
fn enable_propagation_resolves_ambiguous_stub_semantics() {
    with_sides(
        "\n      function wrapper1() { return uniqueA(); }\n      function wrapper2() { return uniqueB(); }\n      function uniqueA() { return \"hello\"; }\n      function uniqueB(x) { return x + 1; }\n    ",
        "\n      function w1() { return uA(); }\n      function w2() { return uB(); }\n      function uA() { return \"hello\"; }\n      function uB(x) { return x + 1; }\n    ",
        |old, new| {
            let without = old.match_fn(
                new,
                MatchOptions {
                    max_cascade_depth: 0,
                    ..MatchOptions::default()
                },
            );
            assert!(
                !without.ambiguous.is_empty(),
                "ambiguous without propagation"
            );

            // The propagation pass is WIRED (crate::propagation): the TS
            // expects propagation to resolve every ambiguous entry here
            // (the wrappers call the unique functions — the matchedCallee
            // rung closes both).
            let with = old.match_fn(
                new,
                MatchOptions {
                    max_cascade_depth: 0,
                    enable_propagation: true,
                    ..MatchOptions::default()
                },
            );
            assert_eq!(
                with.ambiguous.len(),
                0,
                "propagation resolves the wrappers; TS expects ambiguous == 0"
            );
            assert_eq!(with.resolution_stats.propagation_resolved, 2);
        },
    );
}

/// TS "propagationResolved is 0 when propagation not enabled".
#[test]
fn propagation_resolved_is_zero_when_not_enabled() {
    let code = "function a() { return 1; }";
    with_sides(code, code, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(result.resolution_stats.propagation_resolved, 0);
    });
}

// ---------------------------------------------------------------------------
// shingle tier on the binding cascade
// ---------------------------------------------------------------------------

/// TS "counts consultations the tier cannot examine (no functions map)".
#[test]
fn shingle_unconsultable_is_counted_on_the_binding_cascade() {
    with_sides(BINDING_V1, BINDING_V2, |old, new| {
        let result = old.match_bindings(new, MatchOptions::default());
        assert!(
            result.resolution_stats.shingle_unconsultable > 0,
            "the unconsultable skip must be counted, stats: {:?}",
            result.resolution_stats
        );
        assert_eq!(result.resolution_stats.shingle_similarity_resolved, 0);
    });
}

// ---------------------------------------------------------------------------
// relational fingerprint arrays are canonical at construction
// ---------------------------------------------------------------------------

/// The TS suite's shared fixture: enough call structure that every
/// relational array is non-empty on BOTH builders.
const RELATIONAL_CODE: &str = "\n    function zetaLeaf(v) { return v + 313; }\n    function alphaLeaf(v) { return v * 727; }\n    function midOne(a) { return zetaLeaf(a) + alphaLeaf(a); }\n    function midTwo(b) { return alphaLeaf(b) - zetaLeaf(b); }\n    function top(c) { return midOne(c) + midTwo(c); }\n    const configHolder = { limit: 5, mode: \"fast\" };\n    function readsHolder() { return configHolder.limit; }\n    const comboList = [zetaLeaf, alphaLeaf, midOne];\n    const namedPair = { a: alphaLeaf, z: zetaLeaf, m: midTwo };\n    module.exports = { top, readsHolder, configHolder, comboList, namedPair };\n  ";

/// TS `checkSorted`: every multi-element array the positional comparator
/// touches is sorted; returns how many it actually looked at (0 would make
/// the check vacuous).
fn check_sorted(index: &FingerprintIndex<'_>, label: &str) -> usize {
    let mut checked = 0;
    for entry in &index.entries {
        let fp = &entry.fingerprint;
        for (field, arr) in [
            ("calleeHashes", fp.callee_hashes()),
            ("twoHopShapes", fp.two_hop_shapes()),
        ] {
            if arr.len() < 2 {
                continue;
            }
            checked += 1;
            assert!(
                is_sorted(arr),
                "{label} {}.{field} is not sorted: {arr:?} — arraysEqual compares it positionally",
                entry.session_id
            );
        }
        // The singleton guard compares these two with arraysEqual as well.
        if let Some(features) = fp.features() {
            for (field, arr) in [
                ("features.externalCalls", features.external_calls.as_slice()),
                (
                    "features.propertyAccesses",
                    features.property_accesses.as_slice(),
                ),
            ] {
                if arr.len() < 2 {
                    continue;
                }
                checked += 1;
                assert!(
                    is_sorted(arr),
                    "{label} {}.{field} is not sorted: {arr:?}",
                    entry.session_id
                );
            }
        }
    }
    assert!(
        checked > 0,
        "{label}: fixture produced no multi-element relational array, so this check cannot detect an unsorted one"
    );
    checked
}

/// TS "sorts every array arraysEqual compares — function fingerprints".
#[test]
fn sorts_every_array_arrays_equal_compares_function_fingerprints() {
    with_sides(RELATIONAL_CODE, RELATIONAL_CODE, |old, _new| {
        check_sorted(old.fn_index, "fn");
    });
}

/// TS "sorts every array arraysEqual compares — binding fingerprints".
#[test]
fn sorts_every_array_arrays_equal_compares_binding_fingerprints() {
    with_sides(RELATIONAL_CODE, RELATIONAL_CODE, |old, _new| {
        check_sorted(old.binding_index, "binding");
    });
}

/// TS "the sort is what makes arraysEqual agree with calleeShapesEqual":
/// `callee_shapes_equal` re-sorts its inputs (order-insensitive); the
/// positional comparator does NOT — hence the construction-time sort.
#[test]
fn the_sort_makes_arrays_equal_agree_with_callee_shapes_equal() {
    let shapes = vec![
        CalleeShape {
            arity: 2,
            complexity: 3,
            cfg_type: CfgType::Linear,
            has_external_calls: false,
        },
        CalleeShape {
            arity: 1,
            complexity: 1,
            cfg_type: CfgType::Looping,
            has_external_calls: true,
        },
    ];
    let reversed: Vec<CalleeShape> = shapes.iter().rev().cloned().collect();
    assert!(
        callee_shapes_equal(&shapes, &reversed),
        "callee_shapes_equal is order-insensitive"
    );

    let serialized: Vec<String> = shapes.iter().map(|s| s.serialized()).collect();
    let positional = |a: &[String], b: &[String]| {
        a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
    };
    assert!(
        !positional(
            &serialized,
            &serialized.iter().rev().cloned().collect::<Vec<_>>()
        ),
        "the positional comparator is NOT — hence the construction-time sort"
    );
}

// ---------------------------------------------------------------------------
// certifyInterchangeablePools (exp036 task B)
// ---------------------------------------------------------------------------

const CERTIFY_V1: &str = "\n    function helperAlpha(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }\n    function helperBeta(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }\n    function wrapAlphaCall(a) { return helperAlpha(a); }\n    function wrapBetaCall(b) { return helperBeta(b); }\n    function firstSameWrap(c) { return helperAlpha(c); }\n    function secondSameWrap(d) { return helperAlpha(d); }\n  ";
const CERTIFY_V2: &str = "\n    function hA(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }\n    function hB(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }\n    function w1(a) { return hA(a); }\n    function w2(b) { return hB(b); }\n    function s1(c) { return hA(c); }\n    function s2(d) { return hA(d); }\n  ";

/// TS `certified()` — cascade, then ordinal, then the certificate. The
/// pool shape is returned as NAMES so it survives the scope (and the
/// session ids inside the certificate are compared through them).
fn certified_shape(v1: &str, v2: &str) -> Vec<(Vec<String>, Vec<String>, String)> {
    with_sides(v1, v2, |old, new| {
        let mut result = old.match_fn(new, MatchOptions::default());
        let old_side = old.fn_side();
        let new_side = new.fn_side();
        resolve_ambiguous_by_ordinal(&mut result, &old_side, &new_side);
        let pools = certify_interchangeable_pools(&result, &old_side, &new_side);
        pools
            .iter()
            .map(|pool| {
                (
                    pool.priors
                        .iter()
                        .map(|id| old.name_of(id).clone())
                        .collect(),
                    pool.candidates
                        .iter()
                        .map(|id| new.name_of(id).clone())
                        .collect(),
                    pool.evidence_key.clone(),
                )
            })
            .collect()
    })
}

/// TS "certifies the reciprocal indistinguishable pool without assigning".
#[test]
fn certifies_the_reciprocal_pool_without_assigning() {
    with_sides(CERTIFY_V1, CERTIFY_V2, |old, new| {
        let mut result = old.match_fn(new, MatchOptions::default());
        let old_side = old.fn_side();
        let new_side = new.fn_side();
        resolve_ambiguous_by_ordinal(&mut result, &old_side, &new_side);
        let pools = certify_interchangeable_pools(&result, &old_side, &new_side);
        assert_eq!(pools.len(), 1, "one certified pool");
        assert_eq!(pools[0].priors.len(), 3);
        assert_eq!(pools[0].candidates.len(), 3);
        assert!(!pools[0].evidence_key.is_empty());
        // The certificate is read-only: nothing entered matches.
        for prior in &pools[0].priors {
            assert!(
                !result.matches.contains_key(prior),
                "certificate must not assign"
            );
            assert!(result.ambiguous.contains(prior), "members stay ambiguous");
        }
    });
}

/// TS "refuses unequal counts (membership churn)".
#[test]
fn refuses_unequal_counts() {
    let v2_short = CERTIFY_V2.replace("function s2(d) { return hA(d); }", "");
    assert_eq!(
        certified_shape(CERTIFY_V1, &v2_short).len(),
        0,
        "3:2 must not certify"
    );
}

/// TS "is stable across a re-parse of the same sources".
#[test]
fn certificate_is_stable_across_a_reparse() {
    let a = certified_shape(CERTIFY_V1, CERTIFY_V2);
    let b = certified_shape(CERTIFY_V1, CERTIFY_V2);
    assert_eq!(a, b, "certificate must be reparse-stable");
}

// ---------------------------------------------------------------------------
// assignInterchangeablePools (exp036 task C)
// ---------------------------------------------------------------------------

const ASSIGN_V1: &str = "\n    function helperAlpha(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }\n    function helperBeta(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }\n    function wrapBeta(b) { return helperBeta(b); }\n    function uniqueLeft(x) { let u = x + 13; for (let i = 0; i < 4; i++) { u ^= i; } return u; }\n    function firstWrap(c) { return helperAlpha(c); }\n    function uniqueRight(y) { let w = y * 31; do { w -= 5; } while (w > 50); return w; }\n    function secondWrap(d) { return helperAlpha(d); }\n  ";
const ASSIGN_V2_SWAPPED: &str = "\n    function hA(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }\n    function hB(v) { let t = v * 222; if (t > 99) { t = t % 7; } return t; }\n    function wB(b) { return hB(b); }\n    function uR(y) { let w = y * 31; do { w -= 5; } while (w > 50); return w; }\n    function s2(d) { return hA(d); }\n    function uL(x) { let u = x + 13; for (let i = 0; i < 4; i++) { u ^= i; } return u; }\n    function s1(c) { return hA(c); }\n  ";

/// TS `assigned()`: cascade + ordinal + assign, returning the resolved
/// count, the matches as (old-name, new-name) pairs and the tier counter.
/// The matches are collected SORTED so two runs compare equal.
fn assigned_shape(v1: &str, v2: &str) -> (usize, Vec<(String, String)>, usize) {
    with_sides(v1, v2, |old, new| {
        let mut result = old.match_fn(new, MatchOptions::default());
        let old_side = old.fn_side();
        let new_side = new.fn_side();
        resolve_ambiguous_by_ordinal(&mut result, &old_side, &new_side);
        let resolved = cascade::assign_interchangeable_pools(&mut result, &old_side, &new_side);
        let mut by_name: Vec<(String, String)> = result
            .matches
            .iter()
            .map(|(o, n)| (old.name_of(o).clone(), new.name_of(n).clone()))
            .collect();
        by_name.sort();
        (
            resolved,
            by_name,
            result.resolution_stats.interchangeable_resolved,
        )
    })
}

/// TS "follows matched anchors when bundle positions swapped".
#[test]
fn follows_matched_anchors_when_bundle_positions_swapped() {
    let (resolved, by_name, interchangeable) = assigned_shape(ASSIGN_V1, ASSIGN_V2_SWAPPED);
    assert_eq!(resolved, 2, "the 2:2 pool assigns");
    // Anchor-following: firstWrap travels with uniqueLeft, secondWrap with
    // uniqueRight — source order would say the opposite.
    let get = |from: &str| {
        by_name
            .iter()
            .find(|(o, _)| o == from)
            .map(|(_, n)| n.as_str())
            .unwrap_or_else(|| panic!("{from} not matched"))
    };
    assert_eq!(get("firstWrap"), "s1");
    assert_eq!(get("secondWrap"), "s2");
    assert_eq!(interchangeable, 2);
}

/// TS "is the identity on a self-hop (same sources both sides)".
#[test]
fn assignment_is_identity_on_a_self_hop() {
    let (_, by_name, _) = assigned_shape(ASSIGN_V1, ASSIGN_V1);
    let get = |from: &str| {
        by_name
            .iter()
            .find(|(o, _)| o == from)
            .map(|(_, n)| n.as_str())
            .unwrap_or_else(|| panic!("{from} not matched"))
    };
    assert_eq!(get("firstWrap"), "firstWrap");
    assert_eq!(get("secondWrap"), "secondWrap");
}

/// TS "is deterministic across repeated runs".
#[test]
fn assignment_is_deterministic_across_repeated_runs() {
    let a = assigned_shape(ASSIGN_V1, ASSIGN_V2_SWAPPED);
    let b = assigned_shape(ASSIGN_V1, ASSIGN_V2_SWAPPED);
    assert_eq!(a, b, "assignment must be deterministic");
}

/// TS "ordinal pairing refuses a bucket holding a demoted prior".
#[test]
fn ordinal_pairing_refuses_a_bucket_holding_a_demoted_prior() {
    let code = "\n      function helperOne(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }\n      function wrapA(a) { return helperOne(a); }\n      function wrapB(b) { return helperOne(b); }\n    ";
    with_sides(code, code, |old, new| {
        let old_a = old.id_of("wrapA");
        let old_b = old.id_of("wrapB");
        let new_a = new.id_of("wrapA");
        // A broken identity resolver claims the SAME fresh wrapper for both
        // priors — demotion must fire and park both.
        let resolver = {
            let (old_a, old_b, new_a) = (old_a.clone(), old_b.clone(), new_a.clone());
            move |old_id: &str, _candidates: &[String]| -> Option<String> {
                if old_id == old_a || old_id == old_b {
                    Some(new_a.clone())
                } else {
                    None
                }
            }
        };
        let options = MatchOptions {
            resolve_ambiguous_candidate: Some(&resolver),
            ..MatchOptions::default()
        };
        let mut result = old.match_fn(new, options);
        assert!(
            result.resolution_stats.injectivity_demoted >= 2,
            "both claimants demote, got stats: {:?}",
            result.resolution_stats
        );
        let resolved = resolve_ambiguous_by_ordinal(&mut result, &old.fn_side(), &new.fn_side());
        assert_eq!(
            resolved, 0,
            "a contested bucket must not be pair-by-position resolved"
        );
        assert!(!result.matches.contains_key(&old_a) && !result.matches.contains_key(&old_b));
    });
}

/// TS "never lets two pools claim the same fresh candidate (injectivity)":
/// two certified pools CAN overlap (evidence keys omit twoHop and
/// call-graph evidence; propagation narrows ambiguous pools in place), so
/// the assign tier must abstain on any pool whose candidate is already
/// taken.
#[test]
fn never_lets_two_pools_claim_the_same_fresh_candidate() {
    with_sides(
        "\n      function helperAlpha(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }\n      function wrapOne(a) { return helperAlpha(a); }\n      function wrapTwo(b) { return helperAlpha(b); }\n      function wrapThree(c) { return helperAlpha(c); }\n      function wrapFour(d) { return helperAlpha(d); }\n    ",
        "\n      function hA(v) { let s = v + 111; while (s > 9) { s -= 3; } return s; }\n      function x(a) { return hA(a); }\n      function y(b) { return hA(b); }\n      function z(c) { return hA(c); }\n    ",
        |old, new| {
            let mut result = old.match_fn(new, MatchOptions::default());
            let p1 = old.id_of("wrapOne");
            let p2 = old.id_of("wrapTwo");
            let p3 = old.id_of("wrapThree");
            let p4 = old.id_of("wrapFour");
            let cx = new.id_of("x");
            let cy = new.id_of("y");
            let cz = new.id_of("z");
            // The 4:3 wrapper bucket is ambiguous; narrow the pools in place
            // the way propagation does, to two overlapping candidate sets.
            assert!(
                result.ambiguous.contains(&p1),
                "wrappers must start ambiguous"
            );
            result
                .ambiguous
                .insert(p1.clone(), vec![cx.clone(), cy.clone()]);
            result
                .ambiguous
                .insert(p2.clone(), vec![cx.clone(), cy.clone()]);
            result
                .ambiguous
                .insert(p3.clone(), vec![cy.clone(), cz.clone()]);
            result
                .ambiguous
                .insert(p4.clone(), vec![cy.clone(), cz.clone()]);

            cascade::assign_interchangeable_pools(&mut result, &old.fn_side(), &new.fn_side());

            let mut claimed: HashMap<&String, Vec<&String>> = HashMap::new();
            let claimed_pairs: Vec<(&String, &String)> = result.matches.iter().collect();
            for (old_id, new_id) in claimed_pairs {
                claimed.entry(new_id).or_default().push(old_id);
            }
            let mut claimant_rows: Vec<(&String, Vec<&String>)> = claimed.into_iter().collect();
            claimant_rows.sort();
            for (new_id, claimants) in claimant_rows {
                assert!(
                    claimants.len() <= 1,
                    "fresh {new_id} claimed by {} priors: {claimants:?}",
                    claimants.len()
                );
            }
        },
    );
}

const MINIFY_V1: &str = "\n      function fetchUserData(userId) {\n        if (!userId) {\n          throw new Error(\"userId required\");\n        }\n        return fetch(\"/api/users/\" + userId);\n      }\n\n      function processResponse(data) {\n        if (!data) return [];\n        for (var i = 0; i < data.length; i++) {\n          console.log(data[i]);\n        }\n        return data;\n      }\n\n      function main() {\n        var result = fetchUserData(123);\n        return processResponse(result);\n      }\n    ";
const MINIFY_V2: &str = "\n      function a(b) {\n        if (!b) {\n          throw new Error(\"userId required\");\n        }\n        return fetch(\"/api/users/\" + b);\n      }\n\n      function c(d) {\n        if (!d) return [];\n        for (var e = 0; e < d.length; e++) {\n          console.log(d[e]);\n        }\n        return d;\n      }\n\n      function f() {\n        var g = a(123);\n        return c(g);\n      }\n    ";
const SHAPES_V1: &str = "\n      function wrapper1() { return simple(); }\n      function wrapper2() { return complex(); }\n      function simple() { return 1; }\n      function complex(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }\n    ";
const SHAPES_V2: &str = "\n      function a() { return b(); }\n      function c() { return d(); }\n      function b() { return 1; }\n      function d(x) { for(let i=0;i<10;i++) { if(x) return i; } return 0; }\n    ";
const MEMBERKEY_V1: &str = "\n      var store = {\n        getCount: function() { return 1; },\n        getLabel: function() { return 1; }\n      };\n    ";
const MEMBERKEY_V2: &str = "\n      var s = {\n        getCount: function() { return 1; },\n        getLabel: function() { return 1; }\n      };\n    ";
const SINGLETON_V1: &str = "\n      var api = {\n        run: function (x) {\n          for (let i = 0; i < 10; i++) { if (x > i) console.log(i); }\n          return 1;\n        }\n      };\n    ";
const SINGLETON_V2: &str = "\n      var api = {\n        walk: function (y) {\n          for (let j = 0; j < 10; j++) { if (y > j) console.log(j); }\n          return 2;\n        }\n      };\n    ";
const INJECT_V1: &str =
    "\n      function a() { return 1; }\n      function b() { return 1; }\n    ";
const INJECT_V2: &str = "\n      function x() { return 1; }\n    ";
const BINDING_V1: &str = "\n      var loadAlpha = wrap(() => { seed = seedImpl; });\n      var loadBeta = wrap(() => { seed = seedImpl; });\n      console.log(loadAlpha, loadBeta);\n    ";
const BINDING_V2: &str = "\n      var a1 = wrap(() => { seed = seedImpl; });\n      var a2 = wrap(() => { seed = seedImpl; });\n      var a3 = wrap(() => { seed = seedImpl; });\n      console.log(a1, a2, a3);\n    ";
const ABSTAIN_V1: &str = "register(() => x, () => x);";
const ABSTAIN_V2: &str = "register(() => x, () => x, () => x);";

// ---------------------------------------------------------------------------
// PARITY: the frozen TS probe (test/parity/wp21-cascade-probe.mjs)
// ---------------------------------------------------------------------------

fn parity_json() -> serde_json::Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/parity/wp21-cascade-synthetic.json"
    );
    serde_json::from_str(&fs::read_to_string(path).expect("probe JSON exists"))
        .expect("probe JSON parses")
}

/// The probe's id→name mapping: a row's OWN id name, else the session id
/// (TS `path.node.id?.name ?? sessionId` — anonymous rows display as their
/// id; Rust `GraphFunction.name` is "" for exactly those rows). Module
/// bindings always carry a name (`module:<name>`).
fn display_name(side: &SideRefs<'_>, session_id: &str) -> String {
    let row_name = side
        .graph
        .functions
        .iter()
        .find(|f| f.session_id == session_id)
        .map(|f| f.name.clone())
        .or_else(|| {
            side.graph
                .module_bindings
                .iter()
                .find(|b| b.session_id == session_id)
                .map(|b| b.name.clone())
        });
    match row_name {
        Some(name) if !name.is_empty() => name,
        _ => session_id.to_string(),
    }
}

/// The probe's `freeze()`: the cascade's decision outputs, keyed by source
/// name. `unmatched` keeps PUSH order; matches/ambiguous/demoted are sorted
/// (the TS freeze sorts them); pairResolutions/pairRejections keep their
/// row order.
fn freeze(result: &MatchResult, old: &SideRefs<'_>, new: &SideRefs<'_>) -> serde_json::Value {
    let matches: serde_json::Map<String, serde_json::Value> = result
        .matches
        .iter()
        .map(|(o, n)| (display_name(old, o), json!(display_name(new, n))))
        .collect();
    let mut ambiguous: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let mut ambiguous_rows: Vec<(&String, &Vec<String>)> = result.ambiguous.iter().collect();
    ambiguous_rows.sort_by(|a, b| a.0.cmp(b.0));
    for (o, candidates) in ambiguous_rows {
        let mut names: Vec<String> = candidates.iter().map(|c| display_name(new, c)).collect();
        names.sort();
        ambiguous.insert(display_name(old, o), json!(names));
    }
    let unmatched: Vec<serde_json::Value> = result
        .unmatched
        .iter()
        .map(|o| json!(display_name(old, o)))
        .collect();
    let mut demoted: Vec<String> = result
        .demoted_priors
        .iter()
        .map(|o| display_name(old, o))
        .collect();
    demoted.sort();
    let pair_resolutions: Vec<serde_json::Value> = result
        .pair_resolutions
        .iter()
        .map(|r| {
            json!({
                "prior": display_name(old, &r.prior),
                "fresh": display_name(new, &r.fresh),
                "tier": r.tier,
            })
        })
        .collect();
    let pair_rejections: Vec<serde_json::Value> = result
        .pair_rejections
        .iter()
        .map(|r| {
            let mut row = serde_json::Map::new();
            row.insert("prior".to_string(), json!(display_name(old, &r.prior)));
            row.insert("kind".to_string(), json!(r.kind.as_str()));
            if let Some(candidates) = &r.candidates {
                let mut names: Vec<String> =
                    candidates.iter().map(|c| display_name(new, c)).collect();
                names.sort();
                row.insert("candidates".to_string(), json!(names));
            }
            serde_json::Value::Object(row)
        })
        .collect();
    json!({
        "matches": matches,
        "ambiguous": ambiguous,
        "unmatched": unmatched,
        "demotedPriors": demoted,
        "resolutionStats": result.resolution_stats.to_ts_value(),
        "pairResolutions": pair_resolutions,
        "pairRejections": pair_rejections,
    })
}

/// Every scenario of the frozen probe, asserted exactly. These are
/// DECISIONS (matches, tiers, rejections, the whole stats bag), not hash
/// bytes — the structuralHash bytes are part-1/WP1.4 parity's job.
#[test]
fn parity_frozen_ts_cascade() {
    let probe = parity_json();

    // A: the rich call-structure fixture, self-hop — function AND binding.
    with_sides(RELATIONAL_CODE, RELATIONAL_CODE, |old, new| {
        let fn_result = old.match_fn(new, MatchOptions::default());
        assert_eq!(
            freeze(&fn_result, old, new),
            probe["A"]["fn"],
            "A: function cascade"
        );
        let binding_result = old.match_bindings(new, MatchOptions::default());
        assert_eq!(
            freeze(&binding_result, old, new),
            probe["A"]["binding"],
            "A: binding cascade"
        );
    });

    // B: realistic minification.
    with_sides(MINIFY_V1, MINIFY_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["B"], "B: minification");
    });

    // C: twins split by callee shapes.
    with_sides(SHAPES_V1, SHAPES_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["C"], "C: callee shapes");
    });

    // D: memberKey disambiguation.
    with_sides(MEMBERKEY_V1, MEMBERKEY_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["D"], "D: memberKey");
    });

    // E: the singleton corroboration gate.
    with_sides(SINGLETON_V1, SINGLETON_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["E"], "E: singleton gate");
    });

    // F: injectivity demotion.
    with_sides(INJECT_V1, INJECT_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["F"], "F: injectivity");
    });

    // G: crossed containers revoked (propagation is the stub, so the frozen
    // ground truth is the pure cascade + revocation behavior — see the
    // probe's scenario-G comment).
    with_sides(&cross_v1(), &cross_v2(), |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["G"], "G: revocation");
    });

    // H: the binding cascade reaching the shingle tier it cannot consult.
    with_sides(BINDING_V1, BINDING_V2, |old, new| {
        let result = old.match_bindings(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["H"], "H: binding shingle");
    });

    // I: the enclosing-statement rung's abstain counters and the cap.
    with_sides(ABSTAIN_V1, ABSTAIN_V2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(
            freeze(&result, old, new),
            probe["I"]["abstain"],
            "I: abstain"
        );
    });
    let (cap_v1, cap_v2) = cap_fixtures();
    with_sides(&cap_v1, &cap_v2, |old, new| {
        let result = old.match_fn(new, MatchOptions::default());
        assert_eq!(freeze(&result, old, new), probe["I"]["cap"], "I: cap");
    });

    // J: the exp036 tiers, step by step.
    with_sides(ASSIGN_V1, ASSIGN_V2_SWAPPED, |old, new| {
        let mut result = old.match_fn(new, MatchOptions::default());
        assert_eq!(
            freeze(&result, old, new),
            probe["J"]["afterMatch"],
            "J: after match"
        );
        let old_side = old.fn_side();
        let new_side = new.fn_side();
        resolve_ambiguous_by_ordinal(&mut result, &old_side, &new_side);
        let pools = certify_interchangeable_pools(&result, &old_side, &new_side);
        let frozen_pools: Vec<serde_json::Value> = pools
            .iter()
            .map(|pool| {
                json!({
                    "priors": pool.priors.iter().map(|id| display_name(old, id)).collect::<Vec<_>>(),
                    "candidates": pool.candidates.iter().map(|id| display_name(new, id)).collect::<Vec<_>>(),
                })
            })
            .collect();
        assert_eq!(
            json!(frozen_pools),
            probe["J"]["afterOrdinal"]["pools"],
            "J: certified pools"
        );
        assert_eq!(
            freeze(&result, old, new),
            probe["J"]["afterOrdinal"]["result"],
            "J: after ordinal"
        );
        let resolved = cascade::assign_interchangeable_pools(&mut result, &old_side, &new_side);
        assert_eq!(
            resolved,
            probe["J"]["afterAssign"]["resolved"]
                .as_u64()
                .expect("resolved") as usize,
            "J: assign count"
        );
        assert_eq!(
            freeze(&result, old, new),
            probe["J"]["afterAssign"]["result"],
            "J: after assign"
        );
    });
}

// The crossed-container and cap fixtures as strings (scenario G and I reuse
// them; the behavior tests build their own).
fn cross_v1() -> String {
    let wrapper =
        |name: &str, ret: &str| format!("function {name}(q) {{ run(() => q); return {ret}; }}");
    format!("{}\n{}", wrapper("alpha", "1"), wrapper("beta", "1000"))
}

fn cross_v2() -> String {
    let wrapper =
        |name: &str, ret: &str| format!("function {name}(q) {{ run(() => q); return {ret}; }}");
    format!("{}\n{}", wrapper("beta", "1000"), wrapper("alpha", "1"))
}

fn cap_fixtures() -> (String, String) {
    let filler: String = (0..60)
        .map(|i| format!("  {i},"))
        .collect::<Vec<_>>()
        .join("\n");
    let code = |extra: &str| format!("register(\n{filler}\n{extra});");
    (
        code("  () => x,\n  () => x\n"),
        code("  () => x,\n  () => x,\n  () => x\n"),
    )
}

// ---------------------------------------------------------------------------
// crossed-container revocation
// ---------------------------------------------------------------------------

/// TS "revokes a cross-container pair and lets propagation re-resolve it" —
/// the wrappers are REORDERED, so pooling by position hands alpha's arrow
/// to beta; the post-pass revokes the provable crossings. Propagation is
/// the stub here, so the assertions are exactly the ones the TS test makes:
/// parent-consistency over the REMAINING matches and a nonzero revoke
/// count.
#[test]
fn revokes_a_cross_container_pair() {
    let (v1, v2) = (cross_v1(), cross_v2());
    with_sides(&v1, &v2, |old, new| {
        let result = old.match_fn(
            new,
            MatchOptions {
                enable_propagation: true,
                ..MatchOptions::default()
            },
        );
        let old_side = old.fn_side();
        let new_side = new.fn_side();
        // Every arrow must land inside the wrapper its own wrapper matched to.
        let matched_pairs: Vec<(&String, &String)> = result.matches.iter().collect();
        for (old_id, new_id) in matched_pairs {
            let old_parent = old_side.scope_parent_session(old_id);
            let new_parent = new_side.scope_parent_session(new_id);
            let (Some(old_parent), Some(new_parent)) = (old_parent, new_parent) else {
                continue;
            };
            let Some(parent_went) = result.matches.get(old_parent) else {
                continue;
            };
            assert_eq!(
                new_parent,
                parent_went.as_str(),
                "a child was paired into a container its parent did not match to ({old_id} -> {new_id})"
            );
        }
        assert!(
            result.resolution_stats.crossed_container_revoked > 0,
            "expected the post-pass to revoke the crossing, got {}",
            result.resolution_stats.crossed_container_revoked
        );
    });
}
