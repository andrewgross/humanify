//! The TS spec (src/split/placement-trail.test.ts), case for case.

use super::{PlacementEvidence, PlacementTrail, TrailEntry};
use serde_json::json;

fn entry(index: usize, placed_by: &str, file: &str) -> TrailEntry {
    TrailEntry {
        index,
        names: vec!["alpha".into(), "beta".into()],
        placed_by: placed_by.into(),
        file: file.into(),
        ..TrailEntry::default()
    }
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn describes_every_statement_and_counts_tiers_in_first_seen_order() {
    let mut trail = PlacementTrail::default();
    for (i, t) in ["hash", "hash", "name", "novote", "anchor"]
        .iter()
        .enumerate()
    {
        trail.record(entry(i, t, "core/main.js"));
    }
    assert_eq!(
        trail.tiers,
        vec![
            ("hash".to_string(), 2),
            ("name".to_string(), 1),
            ("novote".to_string(), 1),
            ("anchor".to_string(), 1)
        ]
    );
    let placed: Vec<&str> = trail.rows.iter().map(|r| r.placed_by.as_str()).collect();
    assert_eq!(placed, ["hash", "hash", "name", "novote", "anchor"]);
}

#[test]
fn keeps_the_evidence_that_explains_a_locality_placement() {
    let mut trail = PlacementTrail::default();
    trail.record(TrailEntry {
        index: 7,
        names: strings(&["generateContextUsageMarkdown", "inputData"]),
        placed_by: "conflict".into(),
        file: "src/lsp/skill-hook-registry.js".into(),
        evidence: PlacementEvidence {
            votes: Some(strings(&[
                "src/query-input/context-usage.js",
                "src/logging/socket.js",
            ])),
            all_same: Some(strings(&["src/query-input/context-usage.js"])),
            anchor: None,
        },
        ..TrailEntry::default()
    });
    assert_eq!(
        trail.rows[0].evidence,
        json!({
            "votes": ["src/query-input/context-usage.js", "src/logging/socket.js"],
            "allSame": ["src/query-input/context-usage.js"]
        })
    );
}

#[test]
fn drops_the_bulky_evidence_for_an_uneventful_placement() {
    let mut trail = PlacementTrail::default();
    trail.record(TrailEntry {
        prior_file: Some("core/main.js".into()),
        evidence: PlacementEvidence {
            votes: Some(strings(&["core/main.js"])),
            all_same: Some(strings(&["core/main.js"])),
            anchor: None,
        },
        ..entry(0, "hash", "core/main.js")
    });
    assert_eq!(trail.rows[0].evidence, json!({}));
    assert_eq!(trail.rows[0].prior_file.as_deref(), Some("core/main.js"));
}

#[test]
fn keeps_the_evidence_when_the_statement_moved() {
    let mut trail = PlacementTrail::default();
    trail.record(TrailEntry {
        prior_file: Some("floor/cli-interaction/task-serializer.js".into()),
        evidence: PlacementEvidence {
            votes: Some(strings(&["storage/error-messages/auth-manager.js"])),
            ..PlacementEvidence::default()
        },
        ..entry(0, "name", "storage/error-messages/auth-manager.js")
    });
    assert_eq!(
        trail.rows[0].evidence,
        json!({"votes": ["storage/error-messages/auth-manager.js"]})
    );
}

#[test]
fn records_why_the_hash_tier_abstained() {
    let mut trail = PlacementTrail::default();
    trail.record(TrailEntry {
        hash_miss: Some("absent"),
        ..entry(0, "name", "core/main.js")
    });
    assert_eq!(trail.rows[0].hash_miss.as_deref(), Some("absent"));
}

#[test]
fn records_the_losing_tiers_only_when_one_disagrees() {
    let mut trail = PlacementTrail::default();
    let alt = |pairs: &[(&str, &str)]| {
        Some(
            pairs
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        )
    };
    trail.record(TrailEntry {
        alternatives: alt(&[("name", "a.js"), ("allsame", "a.js")]),
        ..entry(0, "name", "a.js")
    });
    trail.record(TrailEntry {
        alternatives: alt(&[("name", "a.js"), ("anchor", "b.js")]),
        ..entry(1, "name", "a.js")
    });
    assert_eq!(trail.rows[0].alternatives, None);
    assert_eq!(trail.rows[1].alternatives, Some(json!({"anchor": "b.js"})));
}

#[test]
fn truncates_the_declared_names_but_says_how_many() {
    let mut trail = PlacementTrail::default();
    trail.record(TrailEntry {
        names: (0..40).map(|i| format!("name{i}")).collect(),
        ..entry(0, "novote", "core/main.js")
    });
    assert_eq!(trail.rows[0].names.len(), 32);
    assert_eq!(trail.rows[0].name_count, Some(40));
    trail.record(entry(1, "hash", "core/main.js"));
    assert_eq!(trail.rows[1].name_count, None);
}

#[test]
fn the_placement_file_is_sorted_by_span_and_a_missing_span_is_minus_one() {
    let mut trail = PlacementTrail::default();
    trail.record(TrailEntry {
        span: Some((10, 20)),
        ..entry(1, "hash", "a.js")
    });
    trail.record(TrailEntry {
        span: Some((0, 5)),
        ..entry(0, "hash", "a.js")
    });
    trail.record(entry(2, "hash", "a.js"));
    let file = trail.to_placement_file();
    let starts: Vec<i64> = file.placements.iter().map(|r| r.key.start).collect();
    assert_eq!(starts, [-1, 0, 10]);
}

/// `--diagnostics`' `placementTrails` is `placementTrail.report()` as
/// `JSON.stringify` writes it: `tiers` in first-seen order, then every
/// recorded entry with its RAW span (JS string indexes into the shipped
/// text; absent without one), keys in the recorded object's order —
/// the record-site fields, then `nameCount` (appended by the recorder) —
/// and `alternatives` in the tiers' own order (not sorted).
#[test]
fn the_diagnostics_report_is_the_ts_recorder_json() {
    use humanify_model::js::stringify;
    let shipped = "var é = 1;\nvar b = 2;\n";
    let mut trail = PlacementTrail::default();
    trail.record(TrailEntry {
        index: 1,
        // bytes 12..22 ("var b = 2;") are UTF-16 11..21.
        span: Some((12, 22)),
        names: (0..34).map(|i| format!("n{i}")).collect(),
        placed_by: "novote".into(),
        file: "src/b.js".into(),
        prior_file: Some("src/old.js".into()),
        prior_file_from: Some("hash"),
        hash_miss: Some("absent"),
        alternatives: Some(vec![
            ("name".into(), "src/z.js".into()),
            ("anchor".into(), "src/a.js".into()),
            ("ordinal".into(), "src/b.js".into()),
        ]),
        evidence: PlacementEvidence {
            votes: Some(strings(&["src/z.js"])),
            all_same: Some(Vec::new()),
            anchor: Some("src/a.js".into()),
        },
    });
    trail.record(TrailEntry {
        index: 0,
        span: None,
        names: strings(&["x"]),
        placed_by: "fossil-eager".into(),
        file: "src/bootstrap.js".into(),
        ..TrailEntry::default()
    });
    let names: Vec<String> = (0..32).map(|i| format!("\"n{i}\"")).collect();
    let expected = format!(
        concat!(
            r#"{{"tiers":{{"novote":1,"fossil-eager":1}},"trails":["#,
            r#"{{"index":1,"span":{{"start":11,"end":21}},"names":[{}],"placedBy":"novote","#,
            r#""file":"src/b.js","priorFile":"src/old.js","priorFileFrom":"hash","hashMiss":"absent","#,
            r#""alternatives":{{"name":"src/z.js","anchor":"src/a.js"}},"#,
            r#""evidence":{{"votes":["src/z.js"],"allSame":[],"anchor":"src/a.js"}},"nameCount":34}},"#,
            r#"{{"index":0,"names":["x"],"placedBy":"fossil-eager","file":"src/bootstrap.js","evidence":{{}}}}]}}"#
        ),
        names.join(",")
    );
    assert_eq!(stringify(&trail.diagnostics_report(shipped)), expected);
}
