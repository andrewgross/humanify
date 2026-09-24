//! `close_dump`'s tests: the row assembly, the two derivations and the
//! hint fold, over real graphs. The end-to-end fixtures are the TS suite's
//! (`src/analysis/close-match.test.ts`); the fold and partial-transfer
//! cases mirror the TS recorder's semantics (prior-version.ts
//! `buildPriorNameHints` / `computePartialTransfer`).

use std::collections::HashMap;

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::Span;
use serde_json::Value;

use crate::graph::UnifiedGraph;
use crate::graph::build_unified_graph;
use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::matching::build_fingerprint_index;
use crate::matching::close::CloseCandidate;
use crate::matching::row_node_ids;
use crate::matching::statement_align::{
    AlignSide, NameHint, build_side_index, compute_body_local_transfers, parse_json_unbounded,
};

use super::super::close_dump::close_dump;
use super::{
    CloseDumpSides, derive_close_assignment_events, f64_bits_hex, fold_hints, partial_transfer,
};

/// One fixture pair through [`close_dump`] with an EMPTY fn_matches (every
/// function unmatched — the tier runs on both sides' whole function set).
fn with_close_pair<T>(
    prior_code: &str,
    fresh_code: &str,
    run: impl FnOnce(CloseDumpSides<'_>, Value, Value, HashMap<String, String>) -> T,
) -> T {
    let prior_alloc = Allocator::default();
    let fresh_alloc = Allocator::default();
    let prior_ingest = Ingest::parse(&prior_alloc, prior_code, "prior.js");
    assert!(
        prior_ingest.errors.is_empty(),
        "prior must parse: {:?}",
        prior_ingest.errors
    );
    let fresh_ingest = Ingest::parse(&fresh_alloc, fresh_code, "input.js");
    assert!(
        fresh_ingest.errors.is_empty(),
        "fresh must parse: {:?}",
        fresh_ingest.errors
    );
    let prior_tables = SymbolTables::build(prior_ingest.semantic());
    let fresh_tables = SymbolTables::build(fresh_ingest.semantic());
    let prior_graph = build_unified_graph(
        prior_ingest.semantic(),
        prior_ingest.program,
        "prior.js",
        &[],
        None,
        None,
    );
    let fresh_graph = build_unified_graph(
        fresh_ingest.semantic(),
        fresh_ingest.program,
        "input.js",
        &[],
        None,
        None,
    );
    let prior_index = build_fingerprint_index(&prior_graph, prior_ingest.semantic(), &prior_tables);
    let fresh_index = build_fingerprint_index(&fresh_graph, fresh_ingest.semantic(), &fresh_tables);
    let prior_program_json =
        parse_json_unbounded(&prior_ingest.program.to_estree_json(false, true));
    let fresh_program_json =
        parse_json_unbounded(&fresh_ingest.program.to_estree_json(false, true));
    let sides = CloseDumpSides {
        prior_graph: &prior_graph,
        fresh_graph: &fresh_graph,
        prior_semantic: prior_ingest.semantic(),
        fresh_semantic: fresh_ingest.semantic(),
        prior_tables: &prior_tables,
        fresh_tables: &fresh_tables,
        prior_index: &prior_index,
        fresh_index: &fresh_index,
        fn_matches: &HashMap::new(),
    };
    run(
        sides,
        prior_program_json,
        fresh_program_json,
        HashMap::new(),
    )
}

/// The TS suite's "differ by one statement" pair — the close tier's win.
const PRIOR_ONE_STMT: &str = r#"
    function process(x) {
      if (!x) return null;
      for (var i = 0; i < x.length; i++) {
        console.log(x[i]);
      }
      return x;
    }
"#;
const FRESH_ONE_STMT: &str = r#"
    function process(x) {
      console.log("debug");
      if (!x) return null;
      for (var i = 0; i < x.length; i++) {
        console.log(x[i]);
      }
      return x;
    }
"#;

#[test]
fn end_to_end_candidate_row_carries_the_won_outcome() {
    with_close_pair(PRIOR_ONE_STMT, FRESH_ONE_STMT, |sides, pj, fj, matches| {
        let file = close_dump(&sides, &pj, &fj)
            .unwrap()
            .expect("the tier runs on two non-empty unmatched sets");
        assert_eq!(file.schema_version, 1);
        assert!(
            file.candidates
                .iter()
                .any(|c| c.outcome == "won" && c.rank == 1),
            "the winning candidate carries rank 1 + won: {:?}",
            file.candidates
        );
        assert!(
            file.candidates
                .iter()
                .all(|c| c.score_bits.starts_with("0x")),
            "every candidate carries the bits identity: {:?}",
            file.candidates
        );
        // The won pair's row: one pair, some corroboration verdict, and
        // the counters add up to the pair count.
        assert_eq!(file.pairs.len(), 1);
        let pair = &file.pairs[0];
        assert!(
            ["alignment", "shingles", "uncorroborated"].contains(&pair.verdict.as_str()),
            "a verdict from the three buckets: {}",
            pair.verdict
        );
        let stats_total = file.stats.corroborated_by_alignment
            + file.stats.corroborated_by_shingles
            + file.stats.uncorroborated;
        assert_eq!(stats_total, file.pairs.len() as u64);
        let _ = matches;
    });
}

#[test]
fn empty_unmatched_side_produces_no_file() {
    with_close_pair("", FRESH_ONE_STMT, |sides, pj, fj, _matches| {
        // The prior side has NO functions — the TS returns before the tier
        // and never records.
        assert!(close_dump(&sides, &pj, &fj).unwrap().is_none());
    });
}

#[test]
fn tie_candidates_abstain_and_no_pairs_form() {
    // The TS tie fixture: two old and two new functions with IDENTICAL
    // feature vectors (different free callees so no hash matches) tie at
    // cosine 1.0 in all four pairings — every candidate abstains.
    let prior = r#"
      function loadAlpha(x) { if (x) { return alphaSvc(x); } return 0; }
      function loadBeta(x) { if (x) { return betaSvc(x); } return 0; }
    "#;
    let fresh = r#"
      function n1(x) { if (x) { return gammaSvc(x); } return 0; }
      function n2(x) { if (x) { return deltaSvc(x); } return 0; }
    "#;
    with_close_pair(prior, fresh, |sides, pj, fj, _matches| {
        let file = close_dump(&sides, &pj, &fj).unwrap().unwrap();
        assert!(
            file.pairs.is_empty(),
            "tied candidates must abstain: {:?}",
            file.pairs
        );
        assert!(
            file.candidates.len() >= 2,
            "the tie was scored, not skipped: {:?}",
            file.candidates
        );
        assert!(
            file.candidates.iter().all(|c| c.outcome == "abstained:tie"),
            "every tied candidate labels abstained:tie: {:?}",
            file.candidates
        );
        assert_eq!(file.stats.uncorroborated, 0, "no pairs, no verdicts");
    });
}

// ---------------------------------------------------------------------------
// the derivation, directly
// ---------------------------------------------------------------------------

/// The TS derivation test's consistent fixture: same-score pairs sharing
/// an endpoint abstain MUTUALLY (o5/o6), an endpoint-disjoint equal-score
/// pair still wins (o7), taken endpoints label abstained:taken (o3/o4).
#[test]
fn derive_events_labels_won_taken_and_tie() {
    let candidates = vec![
        CloseCandidate {
            old_id: "o1".into(),
            new_id: "n1".into(),
            score: 0.9,
        },
        CloseCandidate {
            old_id: "o2".into(),
            new_id: "n2".into(),
            score: 0.8,
        },
        CloseCandidate {
            old_id: "o3".into(),
            new_id: "n2".into(),
            score: 0.7,
        },
        CloseCandidate {
            old_id: "o4".into(),
            new_id: "n1".into(),
            score: 0.6,
        },
        CloseCandidate {
            old_id: "o5".into(),
            new_id: "n3".into(),
            score: 0.5,
        },
        CloseCandidate {
            old_id: "o6".into(),
            new_id: "n3".into(),
            score: 0.5,
        },
        CloseCandidate {
            old_id: "o7".into(),
            new_id: "n4".into(),
            score: 0.5,
        },
    ];
    let won = vec![("o1", "n1"), ("o2", "n2"), ("o7", "n4")]
        .into_iter()
        .map(|(p, f)| super::super::close::CloseMatchPair {
            prior_id: p.into(),
            fresh_id: f.into(),
            score: 1.0,
        })
        .collect::<Vec<_>>();
    let events = derive_close_assignment_events(&candidates, &won);
    let outcome_of = |o: &str, n: &str| {
        events
            .iter()
            .find(|e| e.candidate.old_id == o && e.candidate.new_id == n)
            .map(|e| (e.rank, e.outcome))
            .unwrap_or_else(|| panic!("no event for {o}->{n}"))
    };
    assert_eq!(outcome_of("o1", "n1"), (1, "won"));
    assert_eq!(outcome_of("o2", "n2"), (2, "won"));
    assert_eq!(outcome_of("o3", "n2"), (3, "abstained:taken"));
    assert_eq!(outcome_of("o4", "n1"), (4, "abstained:taken"));
    assert_eq!(outcome_of("o5", "n3"), (5, "abstained:tie"));
    assert_eq!(outcome_of("o6", "n3"), (6, "abstained:tie"));
    assert_eq!(outcome_of("o7", "n4"), (7, "won"));
}

// ---------------------------------------------------------------------------
// the hint fold + partial transfer
// ---------------------------------------------------------------------------

#[test]
fn fold_hints_drops_shadowing_siblings_and_ands_snap() {
    let hints = vec![
        NameHint {
            new_name: "a".into(),
            prior_name: "alpha".into(),
            snap_eligible: true,
        },
        NameHint {
            new_name: "a".into(),
            prior_name: "alpha".into(),
            snap_eligible: false,
        },
        NameHint {
            new_name: "b".into(),
            prior_name: "beta".into(),
            snap_eligible: true,
        },
        // A shadowing sibling with a DIFFERENT prior name — the name
        // becomes ambiguous and is dropped entirely.
        NameHint {
            new_name: "c".into(),
            prior_name: "gamma".into(),
            snap_eligible: true,
        },
        NameHint {
            new_name: "c".into(),
            prior_name: "delta".into(),
            snap_eligible: true,
        },
    ];
    let transfers = vec![super::CloseNamePair {
        old_name: "b".into(),
        new_name: "beta".into(),
    }];
    let (hints, snaps) = fold_hints(&hints, &transfers);
    // 'b' is transferred — excluded; 'c' is ambiguous — dropped; 'a' ANDs
    // its two occurrences' snap eligibility (true && false = false).
    assert_eq!(hints.len(), 1, "only 'a' survives: {hints:?}");
    assert_eq!(hints[0].new_name, "a");
    assert_eq!(hints[0].prior_name, "alpha");
    assert!(!hints[0].snap_eligible, "snap ANDs across occurrences");
    assert!(snaps.is_empty(), "the snap subset shrank with it");
}

#[test]
fn fold_hints_keeps_a_unanimous_snap() {
    let hints = vec![
        NameHint {
            new_name: "a".into(),
            prior_name: "alpha".into(),
            snap_eligible: true,
        },
        NameHint {
            new_name: "a".into(),
            prior_name: "alpha".into(),
            snap_eligible: true,
        },
    ];
    let (hints, snaps) = fold_hints(&hints, &[]);
    assert_eq!(hints.len(), 1);
    assert!(hints[0].snap_eligible);
    assert_eq!(snaps.len(), 1);
    assert!(snaps[0].snap_eligible);
}

#[test]
fn partial_transfer_pairs_name_and_params_positionally() {
    let prior = parse_json_unbounded(
        r#"{"type":"FunctionDeclaration","start":0,"end":40,
            "id":{"type":"Identifier","name":"oldName","start":9,"end":16},
            "params":[
              {"type":"Identifier","name":"a","start":17,"end":18},
              {"type":"AssignmentPattern","start":20,"end":26,
               "left":{"type":"Identifier","name":"b","start":20,"end":21},
               "right":{"type":"Literal","value":1}},
              {"type":"RestElement","start":28,"end":34,
               "argument":{"type":"Identifier","name":"c","start":31,"end":32}}
            ]}"#,
    );
    let fresh = parse_json_unbounded(
        r#"{"type":"FunctionDeclaration","start":0,"end":40,
            "id":{"type":"Identifier","name":"n1","start":9,"end":11},
            "params":[
              {"type":"Identifier","name":"x","start":12,"end":13},
              {"type":"AssignmentPattern","start":14,"end":20,
               "left":{"type":"Identifier","name":"y","start":14,"end":15},
               "right":{"type":"Literal","value":1}},
              {"type":"RestElement","start":21,"end":27,
               "argument":{"type":"Identifier","name":"z","start":24,"end":25}}
            ]}"#,
    );
    let transfers = partial_transfer(&prior, &fresh);
    assert_eq!(
        transfers,
        vec![
            ("n1".to_string(), "oldName".to_string()),
            ("x".to_string(), "a".to_string()),
            ("y".to_string(), "b".to_string()),
            ("z".to_string(), "c".to_string()),
        ]
    );
}

#[test]
fn partial_transfer_reads_method_params_under_value_and_skips_the_key() {
    // A method row: oxc nests the function under `value`; the TS's
    // `getFunctionNameId` answers null for methods (no name transfer), and
    // the params are the method's.
    let prior = parse_json_unbounded(
        r#"{"type":"MethodDefinition","start":0,"end":40,
            "key":{"type":"Identifier","name":"m1"},
            "value":{"type":"FunctionExpression","start":3,"end":40,
              "params":[{"type":"Identifier","name":"a","start":6,"end":7}]}}"#,
    );
    let fresh = parse_json_unbounded(
        r#"{"type":"MethodDefinition","start":0,"end":40,
            "key":{"type":"Identifier","name":"m2"},
            "value":{"type":"FunctionExpression","start":3,"end":40,
              "params":[{"type":"Identifier","name":"x","start":6,"end":7}]}}"#,
    );
    let transfers = partial_transfer(&prior, &fresh);
    assert_eq!(
        transfers,
        vec![("x".to_string(), "a".to_string())],
        "the method key is NOT a transfer (no fn name), the params are"
    );
}

#[test]
fn partial_transfer_keeps_first_position_on_a_duplicate_key() {
    // The TS Record semantics: a param named like the fn name overwrites
    // the VALUE in place, keeping the first-insert position.
    let prior = parse_json_unbounded(
        r#"{"type":"FunctionDeclaration",
            "id":{"type":"Identifier","name":"priorName"},
            "params":[{"type":"Identifier","name":"priorName"}]}"#,
    );
    let fresh = parse_json_unbounded(
        r#"{"type":"FunctionDeclaration",
            "id":{"type":"Identifier","name":"minName"},
            "params":[{"type":"Identifier","name":"minName"}]}"#,
    );
    let transfers = partial_transfer(&prior, &fresh);
    assert_eq!(
        transfers,
        vec![("minName".to_string(), "priorName".to_string())],
        "one key, the param's overwrite wins on the value"
    );
}

#[test]
fn bits_hex_matches_the_ts_format() {
    assert_eq!(f64_bits_hex(1.0), "0x3ff0000000000000");
    assert_eq!(f64_bits_hex(0.0), "0x0");
    assert_eq!(f64_bits_hex(-0.0), "0x8000000000000000");
}

/// The onSetModel pair from the real gate (oracle-b53b3a8 pair
/// 2.1.118-2.1.119, prior[20236397..20236686) fresh[14339282..14339592)):
/// OBJECT-METHOD rows. The TS gives aligned 3/4, four body transfers
/// (including the DECLARED local mH→resolvedModelString) and two hints
/// (mH snap-eligible, the param jH not). Outer bindings are declared at
/// module scope so they resolve like the real bundle.
#[test]
fn object_method_rows_transfer_their_declared_locals() {
    let prior = r#"
      let getDefaultModelString; let userSpecifiedModel; let setMainLoopModelOverride;
      const bridge = {
        onSetModel(modelName) {
          let resolvedModelString = modelName === "default" ? getDefaultModelString() : modelName;
          userSpecifiedModel = resolvedModelString;
          setMainLoopModelOverride(resolvedModelString);
        }
      };
    "#;
    let fresh = r#"
      let d2; let r; let JR; let z;
      const bridge = {
        onSetModel(jH) {
          let mH = jH === "default" ? d2() : jH;
          r = mH;
          JR(mH);
          z(uH => ({
            ...uH,
            mainLoopModelForSession: mH ?? null
          }));
        }
      };
    "#;
    with_close_pair(prior, fresh, |sides, pj, fj, _matches| {
        let (prior_row, prior_span) =
            method_row("onSetModel", sides.prior_graph, sides.prior_semantic);
        let (fresh_row, fresh_span) =
            method_row("onSetModel", sides.fresh_graph, sides.fresh_semantic);
        let prior_json_index = build_side_index(sides.prior_semantic, &pj);
        let fresh_json_index = build_side_index(sides.fresh_semantic, &fj);
        let prior_side = AlignSide::build(
            sides.prior_semantic,
            sides.prior_tables,
            &prior_json_index,
            &prior_row,
            prior_span,
        );
        let fresh_side = AlignSide::build(
            sides.fresh_semantic,
            sides.fresh_tables,
            &fresh_json_index,
            &fresh_row,
            fresh_span,
        );
        let a = compute_body_local_transfers(&prior_side, &fresh_side);
        assert_eq!(
            (a.aligned_statements, a.total_new_statements),
            (3, 4),
            "the three shared statements align: {:?}",
            a
        );
        let transfers: Vec<(String, String)> = a
            .transfers
            .iter()
            .map(|t| (t.old_name.clone(), t.new_name.clone()))
            .collect();
        assert!(
            transfers.contains(&("mH".into(), "resolvedModelString".into())),
            "the declared local mH must transfer its prior name; got {transfers:?}"
        );
        assert_eq!(
            transfers.len(),
            4,
            "mH, d2, r, JR — the whole TS transfer set: {transfers:?}"
        );
        let hints: Vec<(String, String, bool)> = a
            .hints
            .iter()
            .map(|h| (h.new_name.clone(), h.prior_name.clone(), h.snap_eligible))
            .collect();
        assert!(
            hints.contains(&("mH".into(), "resolvedModelString".into(), true)),
            "mH is a snap-eligible hint (the TS dump's snap flag): {hints:?}"
        );
        assert!(
            hints.contains(&("jH".into(), "modelName".into(), false)),
            "the param jH is a non-snap hint: {hints:?}"
        );
    });
}

/// The object-method row whose key is `name`, as row JSON + row span.
/// oxc models an object method as an ObjectProperty(method: true) whose
/// value is the FunctionExpression, and its ESTree JSON type is
/// "Property" — the row's parent is the ObjectExpression.
fn method_row(name: &str, graph: &UnifiedGraph, semantic: &Semantic<'_>) -> (Value, Span) {
    let rows = row_node_ids(&graph.functions, semantic.nodes());
    let nodes = semantic.nodes();
    for f in &graph.functions {
        let Some(&(node_id, _)) = rows.get(&(f.span.start, f.span.end)) else {
            continue;
        };
        let parent = nodes.parent_id(node_id);
        if matches!(nodes.get_node(parent).kind(), AstKind::ObjectExpression(_))
            && let json = crate::graph::entry_subtree_json(nodes, node_id)
        {
            let value = parse_json_unbounded(&json);
            if value.get("type").and_then(Value::as_str) == Some("Property")
                && value
                    .get("key")
                    .and_then(|k| k.get("name"))
                    .and_then(Value::as_str)
                    == Some(name)
            {
                return (value, f.span);
            }
        }
    }
    panic!("no {name} method row in fixture");
}
