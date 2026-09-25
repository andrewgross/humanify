//! contentAnchorVerdicts + changedLineFraction, case for case with the TS
//! (test/parity/wp51-anchor.json, from test/parity/wp51-anchor-probe.ts).

use super::{PriorStatement, changed_line_fraction, content_anchor_verdicts, literals_of};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../../../test/parity/wp51-anchor.json");

#[test]
fn every_ts_case_agrees() {
    let rows: Vec<Value> = serde_json::from_str(VECTORS).expect("vectors");
    assert!(rows.len() > 300);
    let mut failures = Vec::new();
    for (k, row) in rows.iter().enumerate() {
        let prior: Vec<PriorStatement> = row["prior"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| PriorStatement {
                text: p["text"].as_str().unwrap(),
                file: p["file"].as_str().unwrap(),
            })
            .collect();
        let fresh: Vec<&str> = row["fresh"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap())
            .collect();
        let got = content_anchor_verdicts(&prior, &fresh);
        let mut got_rows: Vec<Value> = (0..fresh.len())
            .filter_map(|i| {
                got.get(&i)
                    .map(|v| serde_json::json!([i, v.file, v.near_identical]))
            })
            .collect();
        got_rows.sort_by_key(|v| v[0].as_u64());
        if Value::Array(got_rows.clone()) != row["verdicts"] {
            failures.push(format!(
                "case {k}: ts {} rust {:?}",
                row["verdicts"], got_rows
            ));
        }
        for (i, f) in fresh.iter().enumerate() {
            let want = row["changed"][i].as_f64().unwrap();
            let have = changed_line_fraction(f, prior[0].text);
            if want.to_bits() != have.to_bits() {
                failures.push(format!("case {k} fresh {i}: changed ts {want} rust {have}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn rare_literals_follow_the_js_regex() {
    // An escape inside a quote ends the run; the escaped quote then opens a
    // new one. Astral chars count two UTF-16 units toward {12,}.
    assert_eq!(
        literals_of(r#"g("a distinctive quote \" escaped here")"#),
        vec![" escaped here".to_string()]
    );
    assert_eq!(
        literals_of("\"😀😀😀😀😀😀\""),
        vec!["😀😀😀😀😀😀".to_string()]
    );
    assert!(literals_of("\"😀😀😀😀😀\"").is_empty(), "10 units < 12");
    assert!(literals_of("\"line one\nline two is long\"").is_empty());
}
