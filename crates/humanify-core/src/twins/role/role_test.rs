//! Tests for the binding-role port ([`super`]) — TS original:
//! `src/prior-version/binding-role.test.ts` (its 15 cases), plus the
//! MIRROR WALK's proof test: the shingle token stream joined must equal
//! the canonical walk's parts byte-for-byte, or the two walks have drifted.

use std::collections::{BTreeSet, HashMap};

use oxc_allocator::Allocator;

use super::{
    BindingRole, RoleSide, SINGLE_VOTE_CONTENT_FLOOR, binding_roles_agree, compute_binding_role,
    jaccard, verbatim_tokens,
};
use crate::graph::UnifiedGraph;
use crate::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize};
use crate::ingest::Ingest;

/// Parse + graph + tables, handed to the assertions inside the scope that
/// owns the arena (alternation_test.rs's `with_harness` pattern).
fn with_side<T>(code: &str, run: impl FnOnce(&Ingest<'_>, &UnifiedGraph, &SymbolTables) -> T) -> T {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let graph = crate::graph::build_unified_graph(
        &ingest.semantic,
        ingest.program,
        "input.js",
        &[],
        None,
        None,
    );
    run(&ingest, &graph, &tables)
}

fn binding_named<'g>(graph: &'g UnifiedGraph, name: &str) -> &'g crate::graph::ModuleBindingNode {
    graph
        .module_bindings
        .iter()
        .find(|b| b.name == name)
        .unwrap_or_else(|| panic!("no module binding named {name}"))
}

fn role_of(
    ingest: &Ingest<'_>,
    graph: &UnifiedGraph,
    tables: &SymbolTables,
    name: &str,
) -> BindingRole {
    let join = crate::matching::alternation::session_join(graph);
    let side = RoleSide {
        semantic: &ingest.semantic,
        tables,
        // No wrapper in these fixtures — the container is the program.
        container_span: ingest.program.span,
        session_join: &join,
    };
    compute_binding_role(binding_named(graph, name), &side)
}

// ---------------------------------------------------------------------------
// computeBindingRole (TS describe #1)
// ---------------------------------------------------------------------------

#[test]
fn is_rename_invariant() {
    with_side(
        "var cfg = { port: 8080, host: \"local\" }; console.log(cfg);",
        |ingest, graph, tables| {
            let a = role_of(ingest, graph, tables, "cfg");
            assert!(a.content_shingles.as_ref().is_some_and(|s| !s.is_empty()));
            with_side(
                "var serverConfig = { port: 8080, host: \"local\" }; console.log(serverConfig);",
                |ingest, graph, tables| {
                    let b = role_of(ingest, graph, tables, "serverConfig");
                    assert_eq!(a.structural_hash, b.structural_hash);
                    assert_eq!(a.content_shingles, b.content_shingles);
                },
            );
        },
    );
}

#[test]
fn preserves_literals() {
    with_side(
        "var greeting = \"alpha\"; console.log(greeting);",
        |ingest, graph, tables| {
            let a = role_of(ingest, graph, tables, "greeting");
            with_side(
                "var greeting = \"omega\"; console.log(greeting);",
                |ingest, graph, tables| {
                    let b = role_of(ingest, graph, tables, "greeting");
                    assert_ne!(a.structural_hash, b.structural_hash);
                    let (sa, sb) = (a.content_shingles.unwrap(), b.content_shingles.unwrap());
                    assert!(
                        jaccard(&sa, &sb) < 1.0,
                        "different string literals must not produce identical shingles"
                    );
                },
            );
        },
    );
}

#[test]
fn reads_content_from_the_first_assignment() {
    with_side(
        "var slot; slot = { retries: 3, mode: \"fast\" }; console.log(slot);",
        |ingest, graph, tables| {
            let a = role_of(ingest, graph, tables, "slot");
            assert!(a.content_shingles.as_ref().is_some_and(|s| !s.is_empty()));
            with_side(
                "var other = { retries: 3, mode: \"fast\" }; console.log(other);",
                |ingest, graph, tables| {
                    let b = role_of(ingest, graph, tables, "other");
                    assert_eq!(a.content_shingles, b.content_shingles);
                },
            );
        },
    );
}

#[test]
fn has_null_content_for_a_bare_binding() {
    with_side("var bare; console.log(bare);", |ingest, graph, tables| {
        let a = role_of(ingest, graph, tables, "bare");
        assert!(a.content_shingles.is_none());
        assert!(a.structural_hash.is_none());
    });
}

#[test]
fn is_insertion_robust() {
    with_side(
        "var mk = () => { let acc = 0; for (let i = 0; i < 9; i++) { acc += lookup(i, \"seed\"); } return acc * 2; };",
        |ingest, graph, tables| {
            let a = role_of(ingest, graph, tables, "mk");
            with_side(
                "var mk = () => { let extra = \"pre\"; let acc = 0; for (let i = 0; i < 9; i++) { acc += lookup(i, \"seed\"); } return acc * 2; };",
                |ingest, graph, tables| {
                    let b = role_of(ingest, graph, tables, "mk");
                    let (sa, sb) = (a.content_shingles.unwrap(), b.content_shingles.unwrap());
                    let similarity = jaccard(&sa, &sb);
                    assert!(
                        similarity >= SINGLE_VOTE_CONTENT_FLOOR,
                        "one inserted statement should keep shingle overlap high, got {similarity}"
                    );
                },
            );
        },
    );
}

#[test]
fn scores_unrelated_contents_low() {
    with_side(
        "var mk = { retries: 3, timeoutMs: 500, mode: \"fast\", region: \"us\" };",
        |ingest, graph, tables| {
            let a = role_of(ingest, graph, tables, "mk");
            with_side(
                "var mk = loadRemoteSettings(process.env.CONFIG_URL, [1, 2, 3]);",
                |ingest, graph, tables| {
                    let b = role_of(ingest, graph, tables, "mk");
                    let (sa, sb) = (a.content_shingles.unwrap(), b.content_shingles.unwrap());
                    let similarity = jaccard(&sa, &sb);
                    assert!(
                        similarity < SINGLE_VOTE_CONTENT_FLOOR,
                        "unrelated contents must score below the floor, got {similarity}"
                    );
                },
            );
        },
    );
}

// ---------------------------------------------------------------------------
// bindingRolesAgree (TS describe #2) — hand-built roles, no AST needed
// ---------------------------------------------------------------------------

fn role_with(hash: Option<&str>, shingles: &[&str], callee_ids: &[&str]) -> BindingRole {
    BindingRole {
        structural_hash: hash.map(str::to_string),
        content_shingles: Some(
            shingles
                .iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<_>>(),
        ),
        fn_callee_ids: callee_ids.iter().map(|s| s.to_string()).collect(),
        has_binding_callees: false,
    }
}

fn content_free() -> BindingRole {
    BindingRole::default()
}

fn matches(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn agrees_on_equal_non_null_hashes() {
    let prior = role_with(Some("H1"), &["a", "b"], &[]);
    let next = role_with(Some("H1"), &["c", "d"], &[]);
    let verdict = binding_roles_agree(&prior, &next, &HashMap::new(), false);
    assert!(verdict.agrees);
}

#[test]
fn agrees_on_shingle_overlap_at_or_above_the_floor() {
    let prior = role_with(Some("H1"), &["a", "b", "c", "d"], &[]);
    let next = role_with(Some("H2"), &["a", "b", "c", "e"], &[]);
    let verdict = binding_roles_agree(&prior, &next, &HashMap::new(), false);
    assert!(verdict.agrees, "reason: {}", verdict.reason);
}

#[test]
fn content_free_elimination_is_opt_in() {
    // exp066: symmetric content absence agrees by elimination, but ONLY for
    // callers whose own gates prove exclusivity — the twin tier's pairwise
    // comparison must NOT get it (the license is the CALLER's).
    let with_flag = binding_roles_agree(&content_free(), &content_free(), &HashMap::new(), true);
    assert!(with_flag.agrees);
    assert_eq!(with_flag.reason, "content-free-elimination");
    let without_flag =
        binding_roles_agree(&content_free(), &content_free(), &HashMap::new(), false);
    assert!(!without_flag.agrees);
    assert_eq!(without_flag.reason, "no-content-evidence");
}

#[test]
fn still_refuses_asymmetric_content_absence() {
    let with_content = role_with(Some("H1"), &["a b c", "b c d"], &[]);
    let verdict = binding_roles_agree(&with_content, &content_free(), &HashMap::new(), false);
    assert!(!verdict.agrees);
    assert_eq!(verdict.reason, "no-content-evidence");
}

#[test]
fn refuses_on_shingle_overlap_below_the_floor() {
    let prior = role_with(Some("H1"), &["a", "b", "c", "d"], &[]);
    let next = role_with(Some("H2"), &["x", "y", "z", "a"], &[]);
    let verdict = binding_roles_agree(&prior, &next, &HashMap::new(), false);
    assert!(!verdict.agrees);
}

#[test]
fn vetoes_hash_equal_content_when_mapped_callees_disagree() {
    // Two structurally identical wrappers reference different functions —
    // the callee identity mapped through the function matches must agree,
    // or the pin is refused (the twin cross-pin guard).
    let prior = role_with(Some("H1"), &["a"], &["prior:fnA"]);
    let next = role_with(Some("H1"), &["a"], &["new:fnB"]);
    let verdict = binding_roles_agree(&prior, &next, &matches(&[("prior:fnA", "new:fnA")]), false);
    assert!(!verdict.agrees);
    assert_eq!(verdict.reason, "callee-mismatch");
}

#[test]
fn passes_when_mapped_callees_agree() {
    let prior = role_with(Some("H1"), &["a"], &["prior:fnA"]);
    let next = role_with(Some("H1"), &["a"], &["new:fnA"]);
    let verdict = binding_roles_agree(&prior, &next, &matches(&[("prior:fnA", "new:fnA")]), false);
    assert!(verdict.agrees);
}

#[test]
fn unmatched_prior_callee_is_inconclusive_not_a_veto() {
    let prior = role_with(Some("H1"), &["a"], &["prior:fnGone"]);
    let next = role_with(Some("H1"), &["a"], &["new:fnB"]);
    let verdict = binding_roles_agree(&prior, &next, &HashMap::new(), false);
    assert!(verdict.agrees);
}

#[test]
fn skips_the_callee_check_when_a_side_references_module_bindings() {
    let mut prior = role_with(Some("H1"), &["a"], &["prior:fnA"]);
    prior.has_binding_callees = true;
    let next = role_with(Some("H1"), &["a"], &["new:fnB"]);
    let verdict = binding_roles_agree(&prior, &next, &HashMap::new(), false);
    assert!(verdict.agrees);
}

// ---------------------------------------------------------------------------
// The mirror walk's proof: the shingle token stream IS the canonical stream
// ---------------------------------------------------------------------------

#[test]
fn tokens_join_to_the_canonical_parts() {
    // DUPLICATION NOTICE follow-through (role.rs's verbatim_tokens): for
    // any subtree, the tokens joined must equal
    // canonical_serialize(.., Verbatim).parts byte-for-byte. A masking
    // decision that drifts between the two walks fails here. The fixture
    // covers every branch the walk owns: slots, verbatim property keys,
    // computed keys, labels, per-class privates, strings / numbers /
    // bigint / regex / templates, bare-statement block unwrapping, and
    // free identifiers.
    let code = r#"
var keep = function compute(alpha, beta) {
    const obj = { [alpha]: 1, literal: 2, 0: "zero" };
    obj.literal = obj[alpha] + beta.length;
    outer: for (const item of obj.entries) {
        if (item > 0) continue outer;
        else { break outer; }
    }
    try { throw new Error("boom/" + alpha); } catch (err) { log(`${err.message}:${keep}`); }
    return /ab+c/gi.test(String(alpha)) === (10n ** 2n > 0n);
};
class Widget extends Base {
    #state = 0;
    static #make(x) { return new Widget(); }
    get value() { return this.#state; }
    render(input) { this.#state = input ?? 1; return #state in this; }
}
var label = `a${keep}b${widget_ref}`;
if (keep) { var fallback = label; }
"#;
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(&ingest.semantic);
    let (_inv, values) =
        crate::twins::statement_inventory_with_values(code, "fresh", None).expect("inventory");
    assert!(!values.is_empty());
    for (i, stmt) in values.iter().enumerate() {
        let tokens = verbatim_tokens(stmt, &tables).join("");
        let canonical = canonical_serialize(stmt, &tables, LiteralPolicy::Verbatim).parts;
        assert_eq!(
            tokens, canonical,
            "statement {i}'s mirror walk drifted from the canonical serialization"
        );
    }
}

/// The shingle computation's contract on a real subtree: k-grams of the
/// blinded token stream, capped.
#[test]
fn content_shingles_blind_slot_ordinals() {
    // Two contents identical except for WHICH slot ordinal a shared
    // binding gets (an earlier declaration shifting the counter) must
    // shingle identically — the blind is the point.
    let a = "var mk = () => { let x = alpha; return x + alpha; };";
    let b = "var mk = () => { let pre = 0, x = alpha; return x + alpha; };";
    with_side(a, |ingest, graph, tables| {
        let row = binding_named(graph, "mk");
        let join = crate::matching::alternation::session_join(graph);
        let side = RoleSide {
            semantic: &ingest.semantic,
            tables,
            container_span: ingest.program.span,
            session_join: &join,
        };
        let role_a = compute_binding_role(row, &side);
        with_side(b, |ingest, graph, tables| {
            let row = binding_named(graph, "mk");
            let join = crate::matching::alternation::session_join(graph);
            let side = RoleSide {
                semantic: &ingest.semantic,
                tables,
                container_span: ingest.program.span,
                session_join: &join,
            };
            let role_b = compute_binding_role(row, &side);
            let (sa, sb) = (
                role_a.content_shingles.unwrap(),
                role_b.content_shingles.unwrap(),
            );
            assert!(
                jaccard(&sa, &sb) >= SINGLE_VOTE_CONTENT_FLOOR,
                "slot ordinals are blinded; the shifted counter must not split the shingles"
            );
        });
    });
}
