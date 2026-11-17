use super::*;

fn card(pair: &str) -> JsValue {
    JsValue::parse(&format!(r#"{{"pair":"{pair}","labels":{{}}}}"#)).unwrap()
}

#[test]
fn numeric_pair_order() {
    let mut v = vec!["2.1.215->2.1.216", "2.1.85->2.1.86", "2.1.118->2.1.119"];
    v.sort_by(|a, b| numeric_compare(a, b));
    assert_eq!(
        v,
        ["2.1.85->2.1.86", "2.1.118->2.1.119", "2.1.215->2.1.216"]
    );
}

#[test]
fn the_cold_self_hop_is_judged_only_against_its_own_version() {
    let s = JsValue::parse(r#"{"version":"2.1.216","diffLines":200,"coldCache":"fresh"}"#).unwrap();
    let r = || Some(("2.1.216", vec![2, 32, 184]));
    assert!(cold_range_verdict(&s, r()).starts_with("!! cold self-hop 2.1.216: 200 ln — OUTSIDE"));
    let s2 = JsValue::parse(r#"{"version":"2.1.198","diffLines":5}"#).unwrap();
    assert!(cold_range_verdict(&s2, r()).contains("not judged"));
}

/// exp206: a pair whose scored run left a call ledger gets a digest in
/// `llmDiagnostics` and a row in the printed diagnostics; a pair without
/// one prints `-` and gets no digest (never a zero).
#[test]
fn the_llm_diagnostics_come_from_each_pairs_stats_file() {
    let dir = std::env::temp_dir().join(format!("dev-summarize-llm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("b.stats.json"),
        r#"{"llmCalls":{"outcomes":{"total":{"calls":12,"failed":1}},"transport":{"total":{"sent":9}},"timing":{}}}"#,
    )
    .unwrap();
    std::fs::write(dir.join("c.stats.json"), r#"{"reask":{}}"#).unwrap();
    let cards = [card("a->b"), card("b->c")];
    let (digests, lines) = llm_section(&dir, &cards);
    assert_eq!(digests.len(), 1);
    assert_eq!(number_at(&digests[0], "calls"), Some(12.0));
    assert_eq!(number_at(&digests[0], "failed"), Some(1.0));
    let text = lines.join("\n");
    assert!(text.contains("LLM call diagnostics"), "{text}");
    let bc = text.lines().find(|l| l.starts_with("b->c")).unwrap();
    assert!(bc.split_whitespace().skip(1).all(|c| c == "-"), "{bc}");
}

/// exp208: the matching counts come from the same stats file; a pair
/// whose stats carry no coverage prints `-` and gets no digest.
#[test]
fn the_matching_counts_come_from_each_pairs_stats_file() {
    let dir = std::env::temp_dir().join(format!("dev-summarize-match-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("b.stats.json"),
        r#"{"coverage":{"functions":{"total":40,"llm":3,"failed":1,"cached":30}},"resolutionStats":{"unmatched":2}}"#,
    )
    .unwrap();
    std::fs::write(dir.join("c.stats.json"), r#"{"reask":{}}"#).unwrap();
    let cards = [card("a->b"), card("b->c")];
    let (digests, lines) = matching_section(&dir, &cards);
    assert_eq!(digests.len(), 1);
    assert_eq!(number_at(&digests[0], "fnAsked"), Some(4.0));
    let text = lines.join("\n");
    assert!(text.contains("MATCHING"), "{text}");
    let bc = text.lines().find(|l| l.starts_with("b->c")).unwrap();
    assert!(bc.split_whitespace().skip(1).all(|c| c == "-"), "{bc}");
}

#[test]
fn the_summary_tables_each_pairs_lines_by_group_and_flags_a_card_that_does_not_add_up() {
    let dir = std::env::temp_dir().join(format!("dev-summarize-groups-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let card = |pair: &str, says: u32| {
        format!(
            r#"{{"pair":"{pair}","scorerVersion":4,"changedLines":10,"diffSays":{says},"labels":{{}},
            "groups":{{"real":{{"lines":6,"outsideMetadata":5}},"noise":{{"lines":3,"outsideMetadata":3}},
            "wiring":{{"lines":1,"outsideMetadata":1}},"unknown":{{"lines":0,"outsideMetadata":0}}}}}}"#
        )
    };
    std::fs::write(dir.join("2.json"), card("1->2", 10)).unwrap();
    std::fs::write(dir.join("3.json"), card("2->3", 11)).unwrap();
    let reference = crate::committed::eval::self_hop_reference(&crate::paths::repo_root())
        .unwrap()
        .value;
    let text = summarize(&dir, "label", &reference).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let row = |name: &str| -> Vec<String> {
        let line = text.lines().find(|l| l.starts_with(name)).expect(&text);
        line.split_whitespace()
            .skip(1)
            .map(str::to_string)
            .collect()
    };
    assert_eq!(row("1->2"), ["9", "5", "3", "1", "0"]);
    assert_eq!(row("TOTAL"), ["18", "10", "6", "2", "0"]);
    assert!(
        text.contains("!! LABELS DO NOT ADD UP to the diff's count for: 2->3"),
        "{text}"
    );
}
