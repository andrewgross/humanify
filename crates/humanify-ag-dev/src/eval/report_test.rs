use super::*;

fn st(hash: &str, text: &str) -> Stmt {
    Stmt {
        hash: hash.into(),
        text: text.into(),
        lines: super::super::stmts::line_count(text),
        declared: vec![],
    }
}

#[test]
fn locale_numbers() {
    assert_eq!(fmt_int(0), "0");
    assert_eq!(fmt_int(1234567), "1,234,567");
    assert_eq!(fmt_int(-1000), "-1,000");
}

#[test]
fn noise_shapes() {
    assert_eq!(
        noise_shape("var a = b.c;", "var x = b.c;"),
        "internal-rename"
    );
    assert_eq!(noise_shape("use(foo);", "use(bar);"), "outer-echo");
    assert_eq!(noise_shape("x.foo = 1;", "x.bar = 1;"), "property-drift");
    assert_eq!(noise_shape("f(a, b);", "f(a);"), "misaligned");
}

#[test]
fn the_ledger_buckets_every_fresh_line() {
    let prior = vec![
        st("h1", "a();"),
        st("h2", "var x = 1;"),
        st(
            "h3",
            "function old(alpha, beta) {\n  return alpha + beta;\n}",
        ),
    ];
    let fresh = vec![
        st("h1", "a();"),
        st("h2", "var y = 1;"),
        st(
            "h4",
            "function old(alpha, beta) {\n  return alpha * beta;\n}",
        ),
        st("h5", "brandNew();"),
    ];
    let d = diff_ledger(&fresh, &prior, None);
    assert_eq!(d.clean_ln, 1);
    assert_eq!(d.modified_pairs, 1);
    assert_eq!(d.modified_edited_ln, 1);
    assert_eq!(d.pure_added_ln, 1);
    assert_eq!(d.fresh_total_ln, 6);
    assert_eq!(d.shapes[0].shape, "internal-rename");
    let html = render_html(
        &serde_json::json!({"identifierLedger": {"totalBindings": 10, "transferSettled": {"exact": 7}, "llmNamed": 2, "notRenamed": 1, "remainingMinted": 1}}),
        &KeyOrders::new(),
        Some(&d),
    )
    .unwrap();
    assert!(html.contains("<b>10</b>TOTAL bindings"));
    assert!(html.contains("<tr><td>exact</td><td class=n>7</td><td class=n>70.00%</td></tr>"));
    assert!(render_html(&serde_json::json!({}), &KeyOrders::new(), None).is_err());
}

#[test]
fn count_ties_keep_the_record_key_order() {
    let v = serde_json::json!({"retry": 5, "coverage-sweep": 5, "a": 9});
    let order = vec![
        "retry".to_string(),
        "coverage-sweep".to_string(),
        "a".to_string(),
    ];
    let got: Vec<String> = sorted_counts(&v, Some(&order))
        .into_iter()
        .map(|x| x.0)
        .collect();
    assert_eq!(got, ["a", "retry", "coverage-sweep"]);
}
