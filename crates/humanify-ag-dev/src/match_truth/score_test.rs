//! score.test.ts's scorer vectors, ported one for one, plus the fixture's
//! whole scorecard held to the TS scorer's bytes.

use serde_json::{Value, json};

use super::{score_match_dump, scorecard_json};

fn row(name: &str, slice: &str, n: u64) -> Value {
    json!({
        "id": format!("input.js:1:{n}"),
        "start": n * 100,
        "end": n * 100 + 10,
        "name": name,
        "slice": slice,
    })
}

/// Six prior functions, six fresh ones, five reported pairs.
fn hand_dump() -> Value {
    let body = |mul: &str, add: &str| {
        format!(
            "function x(n) {{\n  var t = 0;\n  for (var i = 0; i < 9; i++) {{\n    t += i;\n  }}\n  t = t * {mul};\n  t = t + n;\n  t -= {add};\n  return t;\n}}"
        )
    };
    let far_body = "function y(n) {\n  var s = 0;\n  while (n > 0) {\n    s += n % 10;\n    n = Math.floor(n / 10);\n  }\n  s = s * s;\n  s += 1000;\n  if (s > 5) {\n    s = 0;\n  }\n  return s;\n}";
    let prior = vec![
        row("alpha", "function alpha(x, y) {\n  return x + y;\n}", 1),
        row("renamed", "function renamed(p, q) {\n  return p * q;\n}", 2),
        row("near", &body("2", "4"), 3),
        row("far", far_body, 4),
        row("dupe", "function dupe() {\n  return 1;\n}", 5),
        row("dupe", "function dupe() {\n  return 1;\n}", 6),
    ];
    let fresh = vec![
        row("alpha", "function alpha(x, y) {\n  return x + y;\n}", 1),
        row(
            "renamed2",
            "function renamed2(a, b) {\n  return a * b;\n}",
            2,
        ),
        row("near2", &body("3", "4"), 3),
        row("other", "function other(z) {\n  return z - z;\n}", 4),
        row("dupe", "function dupe() {\n  return 1;\n}", 5),
        row("dupe", "function dupe() {\n  return 1;\n}", 6),
    ];
    let pair = |p: usize, f: usize, tier: &str| {
        json!({
            "prior": p,
            "fresh": f,
            "priorId": prior[p]["id"],
            "freshId": fresh[f]["id"],
            "tier": tier,
        })
    };
    let pairs = vec![
        pair(0, 0, "identity"),
        pair(1, 1, "identity"),
        pair(2, 2, "ordinal"),
        pair(3, 4, "interchangeable"),
        pair(4, 5, "interchangeable"),
    ];
    let stmt = |slice: &str, n: u64| json!({"start": n, "end": n + 1, "hash": format!("h{n}"), "slice": slice});
    json!({
        "schemaVersion": 1,
        "tool": "humanify match",
        "meta": {},
        "files": [{
            "path": "index.js",
            "freshText": "",
            "functions": {
                "prior": prior, "fresh": fresh, "pairs": pairs,
                "unmatched": [], "ambiguous": [], "rejections": [], "stats": {}
            },
            "close": null,
            "twins": {
                "prior": [stmt("var a = 1;", 1), stmt("var b = 2;", 2)],
                "fresh": [stmt("var a = 1;", 11), stmt("var b = 9;", 12)],
                "gates": {"rows": [
                    {"tier": "unique", "prior": {"start": 1, "end": 2}, "fresh": {"start": 11, "end": 12}, "outcome": "bridged"},
                    {"tier": "unique", "prior": {"start": 2, "end": 3}, "fresh": {"start": 12, "end": 13}, "outcome": "vetoed:callee"}
                ]}
            }
        }]
    })
}

#[test]
fn scorer_arithmetic_recall_classes_duplicates_statements() {
    let card = score_match_dump(&hand_dump()).unwrap();
    function_arithmetic(&card.functions);
    let s = &card.statements;
    assert_eq!(s.prior_count, 2);
    assert_eq!(s.must_match, 1);
    assert_eq!(s.proposed_of_must, 1);
    assert_eq!(s.bridged_of_must, 1);
    assert_eq!(s.recall, Some(1.0));
}

/// Must-set: alpha and renamed (near differs in a literal; dupe is the
/// duplicate class). Tier 2: near<->near2, reported.
fn function_arithmetic(f: &super::FunctionScore) {
    assert_eq!(f.prior_count, 6);
    assert_eq!(f.fresh_count, 6);
    assert_eq!(f.must_match, 2);
    assert_eq!(f.matched_of_must, 2);
    assert_eq!(f.recall, Some(1.0));
    assert_eq!(f.should_match, 1);
    assert_eq!(f.matched_of_should, 1);
    assert_eq!(f.should_recall, Some(1.0));
    assert_eq!(f.duplicate_class, 4);
    reported_arithmetic(f);
}

/// Reported pairs: alpha, renamed, dupe identical; near near; far far.
fn reported_arithmetic(f: &super::FunctionScore) {
    assert_eq!(f.reported, 5);
    assert_eq!(f.reported_classes.identical, 3);
    assert_eq!(f.reported_classes.near, 1);
    assert_eq!(f.reported_classes.far, 1);
    assert_eq!(f.far_pairs.len(), 1);
    assert_eq!(f.far_pairs[0].prior_name, "far");
    assert_eq!(f.tier_counts, vec![("identity".to_string(), 2)]);
}

#[test]
fn a_must_pair_the_matcher_missed_is_reported_not_averaged_away() {
    let mut dump = hand_dump();
    let pairs = dump["files"][0]["functions"]["pairs"]
        .as_array_mut()
        .unwrap();
    pairs.retain(|p| p["prior"] != 0);
    let card = score_match_dump(&dump).unwrap();
    assert_eq!(card.functions.must_match, 2);
    assert_eq!(card.functions.matched_of_must, 1);
    assert_eq!(card.functions.recall, Some(0.5));
    assert_eq!(card.functions.missed_must.len(), 1);
    assert_eq!(card.functions.missed_must[0].prior_name, "alpha");
}

#[test]
fn the_dump_schema_version_is_checked_loudly() {
    let mut dump = hand_dump();
    dump["schemaVersion"] = json!(99);
    let err = score_match_dump(&dump).unwrap_err();
    assert!(err.contains("schemaVersion"), "{err}");
    let mut two = hand_dump();
    let file = two["files"][0].clone();
    two["files"].as_array_mut().unwrap().push(file);
    let err = score_match_dump(&two).unwrap_err();
    assert!(err.contains("exactly one file section"), "{err}");
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/match_truth/fixtures")
            .join(name),
    )
    .unwrap()
}

#[test]
fn the_committed_fixtures_dump_scores_its_constructed_ground_truth() {
    let dump: Value = serde_json::from_str(&fixture("dump.json")).unwrap();
    let card = score_match_dump(&dump).unwrap();
    let f = &card.functions;
    // keepExact, keepRenamed, keepWrapper: the three must-match functions,
    // all matched since exp094.
    assert_eq!(f.must_match, 3);
    assert_eq!(f.matched_of_must, 3);
    assert!(f.missed_must.is_empty());
    // OPEN FINDING 2: changedSmall differs only in literals — a
    // should-match pair the matcher still misses.
    assert_eq!(f.should_match, 1);
    assert_eq!(f.matched_of_should, 0);
    assert_eq!(f.missed_should[0].prior_name, "changedSmall");
    assert!(f.prior_names.iter().any(|n| n == "removedOld"));
    // All three must-statements propose since exp094b.
    assert_eq!(card.statements.must_match, 3);
    assert_eq!(card.statements.proposed_of_must, 3);
    assert_eq!(card.statements.recall, Some(1.0));
}

/// The whole scorecard, byte for byte against what score.ts wrote for the
/// same dump (`fixtures/dump.scorecard.json`, recorded with
/// `JSON.stringify(scoreMatchDump(dump), null, 2)`).
#[test]
fn the_fixtures_scorecard_is_the_ts_scorers_bytes() {
    let dump: Value = serde_json::from_str(&fixture("dump.json")).unwrap();
    let card = score_match_dump(&dump).unwrap();
    let text = humanify_ag_model::js::stringify_pretty(&scorecard_json(&card), 2) + "\n";
    assert_eq!(text, fixture("dump.scorecard.json"));
}

/// The hand-written dump's whole scorecard against score.ts's bytes
/// (`fixtures/hand.dump.json` is score.test.ts's `handDump()` as the TS
/// wrote it; `hand.scorecard.json` is what score.ts made of it).
#[test]
fn the_hand_dumps_scorecard_is_the_ts_scorers_bytes() {
    let dump: Value = serde_json::from_str(&fixture("hand.dump.json")).unwrap();
    assert_eq!(
        dump,
        hand_dump(),
        "the committed dump is this file's hand_dump()"
    );
    let card = score_match_dump(&dump).unwrap();
    let text = humanify_ag_model::js::stringify_pretty(&scorecard_json(&card), 2) + "\n";
    assert_eq!(text, fixture("hand.scorecard.json"));
}
