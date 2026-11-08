use super::*;

fn card(pair: &str, novel: f64, layout: bool) -> JsValue {
    let mut j = format!(
        r#"{{"pair":"{pair}","scorerVersion":2,"determinism":{{"functions":{{"pctDeterministic":99,"pctReachingLLM":1}},"mintedLeftovers":3}},
        "churn":{{"statements":{{"total":10,"unchangedClean":7,"unchangedChurned":1,"novel":{novel}}},"lines":{{"namingNoiseLines":4,"realLines":9}},
        "relocations":{{"sameNameMovedFile":1,"novelNames":2,"freshNames":20}},"tree":{{"relocatedStatements":1}}"#
    );
    if layout {
        j.push_str(r#","layout":{"churnLines":100,"buildConstantLines":4,"real":50,"noise":10,"naming":8,"alias":1,"reorder":1,"spellingIdenticalLines":0,"churnLinesClean":96,"realClean":46}"#);
    }
    j.push_str(r#","surfaces":{"names":{"app":{"novelNames":2},"vendor":{"novelNames":0},"crossBoundary":{"novelNames":0}}}}}"#);
    JsValue::parse(&j).unwrap()
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
fn totals_fold_every_scored_field_and_count_partial_blocks() {
    let t = summarize_cards(&[card("a->b", 2.0, true), card("b->c", 3.0, false)]);
    let tot = JsValue::Object(t.totals.clone());
    assert_eq!(number_at(&tot, "novel"), Some(5.0));
    assert_eq!(number_at(&tot, "stmts"), Some(20.0));
    assert_eq!(number_at(&tot, "layoutChurnLines"), Some(100.0));
    assert_eq!(number_at(&tot, "layoutRealClean"), Some(46.0));
    assert_eq!(
        t.totals.get("split.names.app.novelNames"),
        Some(&JsValue::Number(4.0))
    );
    assert_eq!(
        (t.layout_cards, t.clean_cards, t.surface_cards, t.tree_cards),
        (1, 1, 2, 2)
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

#[test]
fn the_surface_table_reads_the_flat_split_totals() {
    let mut out = Vec::new();
    let c = card("a->b", 2.0, true);
    let t = summarize_cards(std::slice::from_ref(&c));
    print_surfaces(&[c], &JsValue::Object(t.totals), &mut out);
    let row = out.iter().find(|l| l.starts_with("newName")).unwrap();
    assert!(row.contains(" 2 ") && !row.contains(" - "), "{row}");
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
    let cards = [card("a->b", 1.0, false), card("b->c", 1.0, false)];
    let (digests, lines) = llm_section(&dir, &cards);
    assert_eq!(digests.len(), 1);
    assert_eq!(number_at(&digests[0], "calls"), Some(12.0));
    assert_eq!(number_at(&digests[0], "failed"), Some(1.0));
    let text = lines.join("\n");
    assert!(text.contains("LLM call diagnostics"), "{text}");
    let bc = text.lines().find(|l| l.starts_with("b->c")).unwrap();
    assert!(bc.split_whitespace().skip(1).all(|c| c == "-"), "{bc}");
}
