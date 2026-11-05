//! `assertConcatEquivalence` (stable-split.ts) — the review tree must
//! re-slice, file by file and in ledger order, into exactly the bundle's
//! statements (finding #41: the Rust ran the split without it).

use super::assert_concat_equivalence;

const CODE: &str = "\"use strict\";\nvar a = 1;\nfunction f() { return a; }\nvar b = f();\n";

fn spans() -> Vec<(u32, u32)> {
    vec![(0, 13), (14, 24), (25, 51), (52, 64)]
}

fn order() -> Vec<String> {
    ["x.js", "y.js", "x.js", "y.js"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn tree(x: &str, y: &str) -> Vec<(String, String)> {
    vec![("x.js".into(), x.into()), ("y.js".into(), y.into())]
}

#[test]
fn the_review_tree_reconstructs_the_bundle_statements() {
    // A leading bare string re-parses as a DIRECTIVE: still a statement.
    let t = tree(
        "\"use strict\";\nfunction f() { return a; }\n",
        "var a = 1;\nvar b = f();\n",
    );
    assert_eq!(
        assert_concat_equivalence(&t, &order(), &spans(), CODE),
        Ok(())
    );
}

#[test]
fn a_tree_that_lost_or_changed_a_statement_is_refused_with_the_ts_message() {
    let changed = tree(
        "\"use strict\";\nfunction f() { return a + 1; }\n",
        "var a = 1;\nvar b = f();\n",
    );
    assert_eq!(
        assert_concat_equivalence(&changed, &order(), &spans(), CODE),
        Err("stable split: emitted tree does not reconstruct the source statements (tree/ledger invariant violated)".to_string())
    );
    let short = tree("\"use strict\";\n", "var a = 1;\nvar b = f();\n");
    assert_eq!(
        assert_concat_equivalence(&short, &order(), &spans(), CODE),
        Err("reconstruct: x.js is short of statement 1".to_string())
    );
    let extra = tree(
        "\"use strict\";\nfunction f() { return a; }\nvar c = 2;\n",
        "var a = 1;\nvar b = f();\n",
    );
    assert_eq!(
        assert_concat_equivalence(&extra, &order(), &spans(), CODE),
        Err("reconstruct: x.js has 1 statement(s) beyond the ledger".to_string())
    );
}
