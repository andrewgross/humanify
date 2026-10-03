//! Statement-twin tests (WP2.3): UNIT tests on synthetic bundles — the
//! wrapper-body selection, the count-1 join's collision behavior, and the
//! graph-row → enclosing-statement assignment. (The frozen wp23 probe —
//! the TS's recorded inventories over the `<fixture>/ts/text` corpora —
//! was retired 2026-09-28 with the other TS-capture replays; the inventory
//! shape it pinned is exercised live by these tests and, at scale, by the
//! eval.)

use crate::graph::build_unified_graph;
use crate::ingest::Ingest;
use crate::twins::{
    FRESH_ANCHOR, PRIOR_ANCHOR, SideInventory, StatementRecord, statement_inventory,
    unique_twin_proposals,
};

/// A wrapper bundle: 55 filler `var` statements trip the wrapper gate
/// (WRAPPER_IIFE_BINDING_THRESHOLD = 50), then the given statements inside
/// the wrapper body. The fillers all read the SAME literal — identifiers
/// are masked, so they collide into ONE 55-member hash bucket (the
/// collision behavior the unique tier must exclude).
fn wrapper_bundle(statements: &[&str], filler_prefix: &str) -> String {
    let filler: Vec<String> = (0..55)
        .map(|i| format!("var {filler_prefix}{i} = 7;"))
        .collect();
    let mut code = String::from("(function(){\n");
    code.push_str(&filler.join("\n"));
    code.push('\n');
    for statement in statements {
        code.push_str(statement);
        code.push('\n');
    }
    code.push_str("})();\n");
    code
}

/// The PRIOR side of the synthetic pair: descriptive names, statements in
/// bundle order priceBase / computeTotal / labels / tag.
fn prior_bundle() -> String {
    wrapper_bundle(
        &[
            "var priceBase = 42;",
            "function computeTotal(qty) { return priceBase * qty; }",
            "var labelA = \"alpha\", labelB = labelA;",
            "var tag = \"beta\";",
        ],
        "z",
    )
}

/// The FRESH side: the SAME code renamed (the statement hash masks every
/// identifier name) and REORDERED — the hash is order-free by design.
fn fresh_bundle() -> String {
    wrapper_bundle(
        &[
            "function b3(q) { return a7 * q; }",
            "var a7 = 42;",
            "var c1 = \"alpha\", c2 = c1;",
            "var tag2 = \"gamma\";",
        ],
        "q",
    )
}

/// The statements' indices past the 55 fillers. The two sides lay the four
/// real statements out in DIFFERENT orders (the reorder the hash must
/// survive): the fresh side leads with the fn, the prior side with the
/// price literal.
const FRESH_FN_STMT: usize = 55;
const FRESH_PRICE_STMT: usize = 56;
const FRESH_LABEL_STMT: usize = 57;
const FRESH_TAG_STMT: usize = 58;
const PRIOR_PRICE_STMT: usize = 55;
const PRIOR_FN_STMT: usize = 56;
const PRIOR_LABEL_STMT: usize = 57;
const PRIOR_TAG_STMT: usize = 58;
/// The first statement after the fillers, either side.
const FIRST_STMT: usize = 55;

fn inventory_of(
    text: &str,
    anchor: &'static str,
    graph: Option<&crate::graph::UnifiedGraph>,
) -> SideInventory {
    statement_inventory(text, anchor, graph).expect("clean ingest")
}

// ── the unique-tier join on renamed + reordered code ─────────────────────

/// Same code modulo renaming and reordering: the three unchanged statements
/// join 1:1 across the sides (in fresh order, across the reorder); the
/// changed statement (different literal) does not; the 55 fillers are one
/// 55-member bucket on each side and never join.
#[test]
fn renamed_reordered_statements_join_one_to_one() {
    let prior = inventory_of(&prior_bundle(), PRIOR_ANCHOR, None);
    let fresh = inventory_of(&fresh_bundle(), FRESH_ANCHOR, None);

    assert_eq!(prior.statements.len(), 59);
    assert_eq!(fresh.statements.len(), 59);

    // The fillers collide into one bucket (identifiers are masked).
    let filler_hash = prior.statements[0].hash.clone();
    assert_eq!(prior.hash_counts.get(&filler_hash), Some(&55));
    assert_eq!(fresh.hash_counts.get(&filler_hash), Some(&55));
    // ...and a collided hash is on neither side's unique index.
    assert!(!prior.unique_index.contains_key(&filler_hash));
    assert!(!fresh.unique_index.contains_key(&filler_hash));

    let proposals = unique_twin_proposals(&prior, &fresh);
    // fresh order: fn / price / labels join — the fn pair crosses the
    // reorder; the changed tag does not join.
    assert_eq!(
        proposals.pairs,
        vec![
            (FRESH_FN_STMT, PRIOR_FN_STMT),
            (FRESH_PRICE_STMT, PRIOR_PRICE_STMT),
            (FRESH_LABEL_STMT, PRIOR_LABEL_STMT)
        ]
    );
    assert_eq!(proposals.unique_twins, 3);
    // The changed statement (different literal) is absent on both sides.
    assert!(
        !proposals
            .pairs
            .iter()
            .any(|&(fresh_idx, prior_idx)| fresh_idx == FRESH_TAG_STMT
                || prior_idx == PRIOR_TAG_STMT)
    );
}

/// A statement whose hash is unique on the prior side but occurs TWICE on
/// the fresh side is not a proposal (the join is 1:1 by count on BOTH
/// sides — TS :1137 `hashCounts.get(hash) !== 1` continue).
#[test]
fn a_duplicated_fresh_statement_stays_unjoined() {
    let mut fresh = wrapper_bundle(&["var a7 = 42;", "var c1 = \"alpha\", c2 = c1;"], "q");
    // A second copy of the price statement, before the closing "})();" (the
    // tail is exactly 6 bytes: `})();\n`).
    fresh.insert_str(fresh.len() - 6, "var a9 = 42;\n");
    let prior = inventory_of(&prior_bundle(), PRIOR_ANCHOR, None);
    let fresh = inventory_of(&fresh, FRESH_ANCHOR, None);

    // The price statement's hash is count-2 on fresh...
    let price_hash = prior.statements[PRIOR_PRICE_STMT].hash.clone();
    assert_eq!(fresh.hash_counts.get(&price_hash), Some(&2));
    let proposals = unique_twin_proposals(&prior, &fresh);
    // ...so it never appears in the proposals (the labels statement does).
    assert!(
        !proposals
            .pairs
            .iter()
            .any(|&(fresh_idx, _)| fresh.statements[fresh_idx].hash == price_hash)
    );
    assert!(
        proposals
            .pairs
            .iter()
            .any(|&(fresh_idx, _)| fresh.statements[fresh_idx].hash
                == prior.statements[PRIOR_LABEL_STMT].hash)
    );
}

/// An empty hash CANNOT alias the hole-free spelling: an array hole
/// ([1, , 2]) is an explicit marker (statement-hash.ts :77-86).
#[test]
fn array_holes_do_not_alias_hash_free_spelling() {
    let prior = inventory_of(&prior_bundle(), PRIOR_ANCHOR, None);
    let holed = wrapper_bundle(&["var xs = [1, , 2];", "var ys = [1, 2];"], "q");
    let fresh = inventory_of(&holed, FRESH_ANCHOR, None);
    let hole_hash = fresh.statements[FIRST_STMT].hash.clone();
    let dense_hash = fresh.statements[FIRST_STMT + 1].hash.clone();
    assert_ne!(hole_hash, dense_hash);
    // Neither joins anything on the prior side (different literals).
    assert!(unique_twin_proposals(&prior, &fresh).pairs.is_empty());
}

// ── statement selection (TS topLevelStatements :151-164) ─────────────────

/// A sub-threshold IIFE has NO wrapper (the 50-binding gate) — the TS falls
/// back to the Program body, so the unit is the WHOLE IIFE expression
/// statement, and two renamed copies still join as that one statement.
#[test]
fn a_sub_threshold_iife_falls_back_to_the_program_body() {
    let prior = "(function(){\nvar priceBase = 42;\nfunction computeTotal(q) { return priceBase * q; }\n})();";
    let fresh = "(function(){\nvar a7 = 42;\nfunction b3(q) { return a7 * q; }\n})();";
    let prior = inventory_of(prior, PRIOR_ANCHOR, None);
    let fresh = inventory_of(fresh, FRESH_ANCHOR, None);
    assert_eq!(prior.statements.len(), 1);
    assert_eq!(fresh.statements.len(), 1);
    let proposals = unique_twin_proposals(&prior, &fresh);
    assert_eq!(proposals.pairs, vec![(0, 0)]);
}

/// No IIFE at all: the program body's statements, joined across renaming.
#[test]
fn an_unwrapped_bundle_uses_the_program_body() {
    let prior = "var priceBase = 42;\nfunction computeTotal(q) { return priceBase * q; }\n";
    let fresh = "function b3(q) { return a7 * q; }\nvar a7 = 42;\n";
    let prior = inventory_of(prior, PRIOR_ANCHOR, None);
    let fresh = inventory_of(fresh, FRESH_ANCHOR, None);
    assert_eq!(prior.statements.len(), 2);
    assert_eq!(fresh.statements.len(), 2);
    assert_eq!(
        unique_twin_proposals(&prior, &fresh).pairs,
        vec![(0, 1), (1, 0)]
    );
}

// ── the graph rows' enclosing-statement assignment ───────────────────────

/// The row assignment (TS `assignToStatements` :209): every row lands on
/// the top-level statement containing its span; a row that is no
/// statement's child (the wrapper itself) counts unassigned. The expected
/// unassigned set is computed by an independent brute-force containment
/// sweep, not by the port's own binary search.
#[test]
fn graph_rows_assign_to_their_enclosing_statement() {
    let allocator = oxc_allocator::Allocator::default();
    let fresh_text = fresh_bundle();
    let ingest = Ingest::parse(&allocator, &fresh_text, FRESH_ANCHOR);
    let graph = build_unified_graph(
        ingest.semantic(),
        ingest.program,
        "input.js",
        &[],
        crate::rename::eligibility::NeverRename::UNIVERSAL,
    );
    let fresh = inventory_of(&fresh_bundle(), FRESH_ANCHOR, Some(&graph));

    // Consistency: assigned + unassigned accounts for every row.
    let assigned_functions: usize = fresh.fns_by_statement.values().map(Vec::len).sum();
    assert_eq!(
        assigned_functions + fresh.unassigned_functions,
        graph.functions.len()
    );
    let assigned_bindings: usize = fresh.bindings_by_statement.values().map(Vec::len).sum();
    assert_eq!(
        assigned_bindings + fresh.unassigned_bindings,
        graph.module_bindings.len()
    );

    // The named rows land on the statements that declare them.
    let b3 = graph
        .functions
        .iter()
        .position(|f| f.name == "b3")
        .expect("b3 is a function row");
    assert_eq!(
        fresh
            .fns_by_statement
            .iter()
            .find(|(_, rows)| rows.contains(&b3))
            .map(|(stmt, _)| *stmt),
        Some(FRESH_FN_STMT)
    );
    let a7 = graph
        .module_bindings
        .iter()
        .position(|b| b.name == "a7")
        .expect("a7 is a binding row");
    assert_eq!(
        fresh
            .bindings_by_statement
            .iter()
            .find(|(_, rows)| rows.contains(&a7))
            .map(|(stmt, _)| *stmt),
        Some(FRESH_PRICE_STMT)
    );
    // A filler binding lands on its own filler statement.
    let q0 = graph
        .module_bindings
        .iter()
        .position(|b| b.name == "q0")
        .expect("q0 is a binding row");
    assert_eq!(
        fresh
            .bindings_by_statement
            .iter()
            .find(|(_, rows)| rows.contains(&q0))
            .map(|(stmt, _)| *stmt),
        Some(0)
    );

    // The brute-force oracle: unassigned = rows no statement contains.
    let brute = |rows: &[oxc_span::Span]| -> usize {
        rows.iter()
            .filter(|row| {
                !fresh
                    .statements
                    .iter()
                    .any(|s: &StatementRecord| s.span.start <= row.start && row.end <= s.span.end)
            })
            .count()
    };
    let fn_spans: Vec<oxc_span::Span> = graph.functions.iter().map(|f| f.span).collect();
    assert_eq!(brute(&fn_spans), fresh.unassigned_functions);
    let binding_spans: Vec<oxc_span::Span> = graph.module_bindings.iter().map(|b| b.span).collect();
    assert_eq!(brute(&binding_spans), fresh.unassigned_bindings);
}
