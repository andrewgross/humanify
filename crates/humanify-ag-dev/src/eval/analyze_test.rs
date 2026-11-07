use super::*;
use crate::eval::card::{number_at, scorer_version};

/// A bundle of `body` statements, its split tree and ledger: every
/// statement filed in `files[i]`.
fn write_tree(root: &Path, body: &[&str], files: &[&str], srcs: &[(&str, &str)]) {
    let _ = std::fs::remove_dir_all(root);
    std::fs::create_dir_all(root.join(".humanify")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("vendor")).unwrap();
    let filler: String = (0..50).map(|i| format!("var v{i} = {i};\n")).collect();
    let code = format!("(function () {{\n{filler}{}\n}})();\n", body.join("\n"));
    std::fs::write(root.join(".humanify/humanified.js"), code).unwrap();
    let mut order: Vec<String> = (0..50).map(|_| "src/filler.js".to_string()).collect();
    order.extend(files.iter().map(|s| s.to_string()));
    let hashes: Vec<String> = (0..order.len()).map(|i| format!("h{i}")).collect();
    let ledger = serde_json::json!({
        "version": 1, "files": [], "order": order, "hashes": hashes,
        "nameToFiles": {"greet": [files[0]]}
    });
    std::fs::write(root.join(".humanify/split-ledger.json"), ledger.to_string()).unwrap();
    for (rel, text) in srcs {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

#[test]
fn a_pair_scores_to_a_v2_card_whose_partitions_add_up() {
    let base = std::env::temp_dir().join(format!("dev-analyze-{}", std::process::id()));
    write_tree(
        &base.join("prior"),
        &["function greet(a) {\n  return a;\n}", "use(1);"],
        &["src/a.js", "src/a.js"],
        &[("src/a.js", "function greet(a) {\n  return a;\n}\nuse(1);\n")],
    );
    write_tree(
        &base.join("fresh"),
        &[
            "function hello(b) {\n  return b;\n}",
            "use(1);",
            "brandNew(2);",
        ],
        &["src/a.js", "src/a.js", "vendor/x/m.js"],
        &[
            ("src/a.js", "function hello(b) {\n  return b;\n}\nuse(1);\n"),
            ("vendor/x/m.js", "brandNew(2);\n"),
        ],
    );
    let stats = base.join("stats.json");
    std::fs::write(
        &stats,
        r#"{"coverage":{"functions":{"total":3,"cached":2,"llm":1,"closeMatch":0},"mintedCensus":{"total":1,"names":["zz"]}}}"#,
    )
    .unwrap();
    let card = score_pair(&PairInputs {
        pair: "1->2".into(),
        fresh_tree: base.join("fresh"),
        prior_tree: base.join("prior"),
        stats,
    })
    .unwrap();
    assert_eq!(scorer_version(&card), 2);
    assert_eq!(number_at(&card, "churn.statements.novel"), Some(1.0));
    assert_eq!(
        number_at(&card, "churn.statements.unchangedChurned"),
        Some(1.0)
    );
    assert_eq!(number_at(&card, "churn.lines.namingNoiseLines"), Some(3.0));
    assert_eq!(number_at(&card, "churn.layout.naming"), Some(4.0));
    // A whole added file is its text's `split("\n")` count (2 with the
    // trailing newline), as the TS charged it.
    assert_eq!(number_at(&card, "churn.vendor.real"), Some(2.0));
    assert_eq!(
        number_at(&card, "determinism.functions.pctDeterministic"),
        Some(66.67)
    );
    assert_eq!(partition_failures(&card), Vec::<String>::new());
}
