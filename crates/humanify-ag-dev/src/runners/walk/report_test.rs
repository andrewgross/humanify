use humanify_ag_model::js::JsValue;

use super::*;

#[test]
fn numbers_print_as_en_us_locale_strings() {
    assert_eq!(fmt(None), "-");
    assert_eq!(fmt(Some(0.0)), "0");
    assert_eq!(fmt(Some(1234567.0)), "1,234,567");
    assert_eq!(fmt(Some(-1500.0)), "-1,500");
    assert_eq!(fmt(Some(12.5)), "12.5");
}

fn row(from: &str, to: &str, noise: f64, turnover: (usize, usize)) -> Row {
    let card = JsValue::parse(&format!(
        "{{\"churn\":{{\"layout\":{{\"noise\":{noise}}},\"vendor\":{{\"noise\":0}}}}}}"
    ))
    .unwrap();
    Row {
        from: Some(from.into()),
        to: to.into(),
        hop: JsValue::parse("{}").unwrap(),
        card: Some(card),
        card_error: None,
        lines: None,
        turnover: Some(turnover),
    }
}

#[test]
fn a_hop_far_above_the_median_is_flagged_with_the_rule_that_fired() {
    let rows = vec![
        row("1", "2", 1000.0, (0, 800)),
        row("2", "3", 1100.0, (2, 800)),
        row("3", "4", 60000.0, (182, 882)),
        row("4", "5", 900.0, (85, 833)),
        row("5", "6", 1200.0, (1, 800)),
    ];
    let out = hops_to_look_at("rust", &rows).join("\n");
    assert!(out.contains("**3→4** — noiseLines 60,000"), "{out}");
    assert!(
        out.contains("vendor hash turnover 182 of 882 files (20.6%)"),
        "{out}"
    );
    assert!(out.contains("**4→5** — vendor hash turnover 85"), "{out}");
    assert!(
        !out.contains("**1→2**") && !out.contains("**2→3**") && !out.contains("**5→6**"),
        "{out}"
    );
}

#[test]
fn hash_named_vendor_files_are_the_turnover_population() {
    assert!(super::is_hash_named("module-e972b138.js"));
    assert!(super::is_hash_named("lib_aec02bcd.js"));
    assert!(!super::is_hash_named("index.js"));
    assert!(!super::is_hash_named("module-E972B138.js"));
    assert!(!super::is_hash_named("module-e972b13.js"));
}
