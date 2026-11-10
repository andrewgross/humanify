use super::*;

/// A copy of a frozen reference label (cards + run records) with every
/// card's `scorerVersion` set. The frozen cards live on this box only
/// (`/work/dev-harness-proof`); the test FAILS without them, never passes.
fn label(root: &Path, name: &str, scorer: Option<u64>) {
    let src = Path::new("/work/dev-harness-proof/oracles/a/ref-scratch-0f338ffa-r1");
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    for e in std::fs::read_dir(src).expect("the frozen reference cards") {
        let p = e.unwrap().path();
        let n = p.file_name().unwrap().to_str().unwrap().to_string();
        let mut text = std::fs::read_to_string(&p).unwrap();
        if let (Some(v), true) = (
            scorer,
            crate::eval::card::is_scorecard(&JsValue::parse(&text).unwrap_or(JsValue::Null)),
        ) {
            text = text.replacen(
                "{\n  \"pair\"",
                &format!("{{\n  \"scorerVersion\": {v},\n  \"pair\""),
                1,
            );
        }
        std::fs::write(dir.join(n), text).unwrap();
    }
}

#[test]
fn a_mixed_scorer_comparison_is_refused_unless_forced() {
    let root = std::env::temp_dir().join(format!("dev-leaderboard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    label(&root, "v1", None);
    label(&root, "v1b", None);
    label(&root, "v2", Some(2));
    let names = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    let text = leaderboard(&root, &names("v1", "v1b"), None, false).unwrap();
    assert!(text.contains("novel="), "{text}");
    assert!(text.contains("4188 (=)"), "{text}");
    let err = leaderboard(&root, &names("v1", "v2"), None, false).unwrap_err();
    assert!(err.contains("scorer.version"), "{err}");
    let forced = leaderboard(&root, &names("v1", "v2"), None, true).unwrap();
    assert!(forced.contains("!! MIXED"), "{forced}");
}

#[test]
fn cells_say_when_a_delta_is_inside_the_band() {
    assert_eq!(cell(Some(10.0), Some(10.0), false, Some(3.0)), "10 (=)");
    assert_eq!(cell(Some(12.0), Some(10.0), false, Some(3.0)), "12 (~0±3)");
    assert_eq!(cell(Some(15.0), Some(10.0), false, Some(3.0)), "15 (+5)");
    assert_eq!(cell(Some(5.0), Some(10.0), false, None), "5 (-5±?)");
    assert_eq!(cell(None, Some(10.0), false, None), "-");
}

/// Give `name` a fifth pair: a copy of its 2.1.198 card filed as
/// 2.1.179->2.1.181 (numbers doubled so the pair cannot vanish into a
/// total by accident), its summary re-folded over all five cards.
fn add_fifth_pair(root: &Path, name: &str) {
    let dir = root.join(name);
    for (from, to) in [
        ("2.1.198.json", "2.1.181.json"),
        ("2.1.198-run.json", "2.1.181-run.json"),
        ("2.1.198-run-status.json", "2.1.181-run-status.json"),
    ] {
        let text = std::fs::read_to_string(dir.join(from))
            .unwrap()
            .replace("2.1.197->2.1.198", "2.1.179->2.1.181")
            .replace("claude-code-2.1.198", "claude-code-2.1.181");
        std::fs::write(dir.join(to), text).unwrap();
    }
    let summary_path = dir.join("summary.json");
    let summary = JsValue::parse(&std::fs::read_to_string(&summary_path).unwrap()).unwrap();
    let mut cards: Vec<JsValue> = match at(&summary, "pairs") {
        Some(JsValue::Array(a)) => a.clone(),
        _ => panic!("summary.json pairs"),
    };
    let fifth =
        JsValue::parse(&std::fs::read_to_string(dir.join("2.1.181.json")).unwrap()).unwrap();
    cards.insert(2, fifth);
    let totals = crate::eval::summarize::summarize_cards(&cards).totals;
    let JsValue::Object(mut o) = summary else {
        panic!("summary.json is an object")
    };
    o.insert("pairs", JsValue::Array(cards));
    o.insert("totals", JsValue::Object(totals));
    std::fs::write(
        &summary_path,
        crate::json::js_text(&JsValue::Object(o), true),
    )
    .unwrap();
}

#[test]
fn a_five_pair_label_against_a_four_pair_baseline_compares_the_common_four_out_loud() {
    let root = std::env::temp_dir().join(format!("dev-leaderboard-5-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    label(&root, "four", Some(2));
    label(&root, "five", Some(2));
    add_fifth_pair(&root, "five");
    let names = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    // Unforced: compared, over the four common pairs, said out loud.
    let text = leaderboard(&root, &names("four", "five"), None, false).unwrap();
    assert!(text.contains("!! COMMON PAIRS"), "{text}");
    assert!(text.contains("only five: 2.1.179->2.1.181"), "{text}");
    assert!(
        text.contains("4188 (=)"),
        "the fifth pair's statements must not reach the compared total: {text}"
    );
    // Either order: the baseline's extra pair is left out the same way.
    let text = leaderboard(&root, &names("five", "four"), None, false).unwrap();
    assert!(text.contains("4188 (=)"), "{text}");
    // Two five-pair labels: no line, the five-pair totals.
    label(&root, "five2", Some(2));
    add_fifth_pair(&root, "five2");
    let text = leaderboard(&root, &names("five", "five2"), None, false).unwrap();
    assert!(!text.contains("COMMON PAIRS"), "{text}");
}

#[test]
fn bands_measured_over_other_pairs_say_so() {
    let root = std::env::temp_dir().join(format!("dev-leaderboard-bp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    label(&root, "four", Some(2));
    label(&root, "five", Some(2));
    add_fifth_pair(&root, "five");
    let slot = |pairs: &str| {
        format!(
            r#"{{"provenance":{{"provisional":false,"sources":[],"commit":"x","labels":[]{pairs}}},"bands":{{"novel":0}}}}"#
        )
    };
    let file = |pairs: &str| -> NoiseBands {
        serde_json::from_str(&format!(
            r#"{{"provenance":{{"provisional":true,"sources":[],"commit":"","labels":[]}},"bands":{{}},"byMode":{{}},"byScorer":{{"2":{}}}}}"#,
            slot(pairs)
        ))
        .unwrap()
    };
    let names = vec!["four".to_string(), "five".to_string()];
    let five_pairs = r#","pairs":["2.1.85->2.1.86","2.1.118->2.1.119","2.1.179->2.1.181","2.1.197->2.1.198","2.1.215->2.1.216"]"#;
    let text = leaderboard(&root, &names, Some(&file(five_pairs)), false).unwrap();
    assert!(text.contains("bands: MEASURED OVER 5 PAIRS"), "{text}");
    let text = leaderboard(&root, &names, Some(&file("")), false).unwrap();
    assert!(text.contains("records no pair set"), "{text}");
    let four_pairs =
        r#","pairs":["2.1.85->2.1.86","2.1.118->2.1.119","2.1.197->2.1.198","2.1.215->2.1.216"]"#;
    let text = leaderboard(&root, &names, Some(&file(four_pairs)), false).unwrap();
    assert!(!text.contains("MEASURED OVER"), "{text}");
    assert!(!text.contains("records no pair set"), "{text}");
}

/// Give `name`'s summary an `llmDiagnostics` entry per pair (exp206) with
/// `sent` calls and a `p90ms` of `p90` each.
fn add_llm_diagnostics(root: &Path, name: &str, sent: f64, p90: f64) {
    let path = root.join(name).join("summary.json");
    let summary = JsValue::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let diags: Vec<JsValue> = match at(&summary, "pairs") {
        Some(JsValue::Array(a)) => a
            .iter()
            .map(|c| {
                let pair = card_pair(c).unwrap();
                JsValue::parse(&format!(
                    r#"{{"pair":"{pair}","sent":{sent},"p90ms":{p90},"failed":0}}"#
                ))
                .unwrap()
            })
            .collect(),
        _ => panic!("summary.json pairs"),
    };
    let JsValue::Object(mut o) = summary else {
        panic!("summary.json is an object")
    };
    o.insert("llmDiagnostics", JsValue::Array(diags));
    std::fs::write(&path, crate::json::js_text(&JsValue::Object(o), true)).unwrap();
}

#[test]
fn the_llm_diagnostics_print_side_by_side_and_dash_for_a_legacy_label() {
    let root = std::env::temp_dir().join(format!("dev-leaderboard-llm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    label(&root, "legacy", Some(2));
    label(&root, "ledgered", Some(2));
    label(&root, "slower", Some(2));
    add_llm_diagnostics(&root, "ledgered", 100.0, 2000.0);
    add_llm_diagnostics(&root, "slower", 100.0, 6800.0);
    let names: Vec<String> = ["ledgered", "slower", "legacy"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let text = leaderboard(&root, &names, None, false).unwrap();
    assert!(text.contains("LLM call diagnostics"), "{text}");
    assert!(text.contains("DIAGNOSTICS, not KPIs"), "{text}");
    // Four pairs: sent sums, p90 is the worst pair's.
    let slower: Vec<&str> = text.lines().filter(|l| l.starts_with("slower")).collect();
    assert!(
        slower.iter().any(|l| l.contains("6800 (+4800)")),
        "{slower:?}"
    );
    assert!(slower.iter().any(|l| l.contains("400")), "{slower:?}");
    let legacy = text
        .lines()
        .filter(|l| l.starts_with("legacy") && l.split_whitespace().nth(1) == Some("-"))
        .count();
    assert!(
        legacy >= 3,
        "a legacy label prints '-' in every group: {text}"
    );
}

/// Give `name`'s summary a `matchingDiagnostics` entry per pair (exp208).
fn add_matching(root: &Path, name: &str, fn_to_llm: f64, mb_llm: f64) {
    let path = root.join(name).join("summary.json");
    let summary = JsValue::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let diags: Vec<JsValue> = match at(&summary, "pairs") {
        Some(JsValue::Array(a)) => a
            .iter()
            .map(|c| {
                let pair = card_pair(c).unwrap();
                JsValue::parse(&format!(
                    r#"{{"pair":"{pair}","fnTotal":100,"fnToLLM":{fn_to_llm},"mbLLMNamed":{mb_llm}}}"#
                ))
                .unwrap()
            })
            .collect(),
        _ => panic!("summary.json pairs"),
    };
    let JsValue::Object(mut o) = summary else {
        panic!("summary.json is an object")
    };
    o.insert("matchingDiagnostics", JsValue::Array(diags));
    std::fs::write(&path, crate::json::js_text(&JsValue::Object(o), true)).unwrap();
}

#[test]
fn the_matching_counts_print_side_by_side_and_a_same_commit_move_is_flagged() {
    let root = std::env::temp_dir().join(format!("dev-leaderboard-match-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for n in ["r1", "r2", "legacy"] {
        label(&root, n, Some(2));
    }
    add_matching(&root, "r1", 10.0, 4.0);
    add_matching(&root, "r2", 11.0, 9.0);
    let names: Vec<String> = ["r1", "r2", "legacy"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let bands: NoiseBands = serde_json::from_str(
        r#"{"provenance":{"provisional":true,"sources":[],"commit":"","labels":[]},"bands":{},"byMode":{},
        "byScorer":{"2":{"provenance":{"provisional":false,"sources":[],"commit":"x","labels":[]},"bands":{"novel":0},
        "matchingBands":{"mbLLMNamed":8}}}}"#,
    )
    .unwrap();
    let text = leaderboard(&root, &names, Some(&bands), false).unwrap();
    assert!(text.contains("MATCHING"), "{text}");
    // Four pairs: fnToLLM 40 vs 44, mbLLMNamed 16 vs 36 (band ±8).
    let r2: Vec<&str> = text.lines().filter(|l| l.starts_with("r2")).collect();
    assert!(r2.iter().any(|l| l.contains("44 (+4)")), "{r2:?}");
    assert!(
        text.contains("!! MATCHING NOT EQUAL at the same commit: r2 fnToLLM 44 vs r1 40"),
        "{text}"
    );
    assert!(
        text.contains(
            "!! MATCHING outside its band at the same commit: r2 mbLLMNamed 36 vs r1 16 (band ±8)"
        ),
        "{text}"
    );
    let legacy = text
        .lines()
        .filter(|l| l.starts_with("legacy") && l.split_whitespace().nth(1) == Some("-"))
        .count();
    assert!(legacy >= 3, "{text}");
}
