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
