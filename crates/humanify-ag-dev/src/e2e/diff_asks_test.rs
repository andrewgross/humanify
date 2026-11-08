//! The ask diff's semantics (ported from test/ask-trace-harness.test.ts):
//! the exit-nonzero-empty-diff contract use (a) — a no-op change — depends
//! on.

use humanify_ag_model::js::{JsObject, JsValue};

use super::*;

/// A row like the TS test's `row()`, with `over` replacing fields.
fn row(over: &[(&str, JsValue)]) -> JsObject {
    let mut o = JsObject::new();
    o.insert("seq", JsValue::Number(0.0));
    o.insert("site", JsValue::str("naming"));
    o.insert("scope", JsValue::str("fn-a"));
    o.insert("scopeKind", JsValue::str("fn"));
    o.insert("reason", JsValue::str("fresh"));
    o.insert("isRetry", JsValue::Bool(false));
    o.insert("priorContext", JsValue::Bool(false));
    o.insert("round", JsValue::Number(1.0));
    o.insert("identifiers", JsValue::str_array(&["Ka"]));
    o.insert("usedNamesCount", JsValue::Number(3.0));
    o.insert("promptVariant", JsValue::str("batch"));
    for (k, v) in over {
        o.insert(*k, v.clone());
    }
    o
}

fn retry(cause: &str, seq: f64) -> JsObject {
    row(&[
        ("seq", JsValue::Number(seq)),
        ("round", JsValue::Number(2.0)),
        ("reason", JsValue::str("retry")),
        ("isRetry", JsValue::Bool(true)),
        ("retryCause", JsValue::str(cause)),
    ])
}

fn scopes(rows: &[&JsObject]) -> Vec<String> {
    rows.iter()
        .map(|r| {
            r.get("scope")
                .and_then(JsValue::as_str)
                .unwrap()
                .to_string()
        })
        .collect()
}

#[test]
fn identical_logs_flag_nothing_even_reordered_duplicates_pair_by_multiset() {
    let a = vec![row(&[]), retry("NameTaken", 1.0)];
    let b = vec![
        row(&[("seq", JsValue::Number(5.0))]),
        retry("NameTaken", 9.0),
    ];
    let d = diff_asks(&a, &b);
    assert!(d.identical);
    assert!(d.only_a.is_empty() && d.only_b.is_empty() && d.changed.is_empty());
    let same = vec![row(&[])];
    assert!(diff_asks(&same, &same).identical);
}

#[test]
fn asks_only_in_a_or_only_in_b_are_reported_by_scope() {
    let a = vec![
        row(&[]),
        row(&[
            ("scope", JsValue::str("fn-b")),
            ("identifiers", JsValue::str_array(&["Zz"])),
        ]),
    ];
    let b = vec![
        row(&[]),
        row(&[
            ("scope", JsValue::str("fn-c")),
            ("identifiers", JsValue::str_array(&["Yy"])),
        ]),
    ];
    let d = diff_asks(&a, &b);
    assert!(!d.identical);
    assert_eq!(scopes(&d.only_a), vec!["fn-b"]);
    assert_eq!(scopes(&d.only_b), vec!["fn-c"]);
    let text = format_delta("A", "B", &a, &b, &d);
    assert_eq!(
        text,
        "A: A — 2 ask(s)\nB: B — 2 ask(s)\n  fresh: A 2 / B 2\n\
         IDENTICAL: no — only-in-A 1, only-in-B 1, changed 0\n\n— ONLY IN A (1) —\n  \
         [naming] fn-b (round 1, fresh) [Zz]\n\n— ONLY IN B (1) —\n  [naming] fn-c (round 1, fresh) [Yy]"
    );
}

#[test]
fn identical_asks_that_changed_a_compared_field_pair_naming_the_field() {
    let a = vec![row(&[
        ("wave", JsValue::Number(1.0)),
        ("priorContext", JsValue::Bool(false)),
    ])];
    let b = vec![row(&[
        ("wave", JsValue::Number(2.0)),
        ("priorContext", JsValue::Bool(true)),
    ])];
    let d = diff_asks(&a, &b);
    assert!(!d.identical);
    assert!(d.only_a.is_empty());
    assert_eq!(d.changed.len(), 1);
    let mut fields = d.changed[0].fields.clone();
    fields.sort();
    assert_eq!(fields, vec!["priorContext", "wave"]);
    let text = format_delta("A", "B", &a, &b, &d);
    assert!(text.contains("\n— CHANGED (1) —\n"));
    assert!(
        text.ends_with(
            "  [naming] fn-a (round 1, fresh) [Ka]: priorContext false → true, wave 1 → 2"
        )
    );
}

#[test]
fn a_different_retry_cause_is_a_different_ask_not_a_changed_one() {
    let a = vec![retry("NameTaken", 0.0)];
    let b = vec![retry("InvalidSuggestion", 0.0)];
    let d = diff_asks(&a, &b);
    assert!(!d.identical);
    assert!(d.changed.is_empty());
    assert_eq!((d.only_a.len(), d.only_b.len()), (1, 1));
    let text = format_delta("A", "B", &a, &b, &d);
    assert!(text.contains("  [naming] fn-a (round 2, retry=NameTaken retry) [Ka]"));
}

#[test]
fn a_log_reads_one_row_per_nonblank_line_and_names_a_bad_one() {
    let dir = std::env::temp_dir().join(format!("dev-diff-asks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let good = dir.join("a.jsonl");
    std::fs::write(&good, "{\"seq\":0}\n\n{\"seq\":1}\n").unwrap();
    assert_eq!(read_asks(&good).unwrap().len(), 2);
    let bad = dir.join("b.jsonl");
    std::fs::write(&bad, "{\"seq\":0}\nnope\n").unwrap();
    let e = read_asks(&bad).unwrap_err();
    assert!(e.contains("b.jsonl:2: not a JSONL row"), "{e}");
    std::fs::remove_dir_all(&dir).unwrap();
}
