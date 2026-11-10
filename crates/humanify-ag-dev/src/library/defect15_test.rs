use super::*;
use crate::library::truth::{Src, Truth};

fn lits(code: &str) -> Lits {
    unit_literals(code).expect("the unit parses")
}

fn file(path: &str, text: &str) -> Lits {
    file_literals(path, text).expect("the file parses")
}

/// The cp950 shape (exp210): a JSON table module filed under node-fetch's
/// index.js. node-fetch's own text holds none of the table's strings.
const TABLE_UNIT: &str = r#"yWe.exports=[["a140","　，、。"],["a180","～…‥"],["a1c0","abécd"],["a240","←↑→"],["a280","table-row-one"],["a2ab","table-row-two"],["a2e3","table-row-three"],["a2ef","table-row-four"],["a340","table-row-five"]];"#;
const NODE_FETCH: &str = r#"
"use strict";
var Body = require("./lib/body");
function fetch(url, opts) {
  if (!fetch.Promise) throw new Error("native promise missing, set fetch.Promise");
  return new fetch.Promise(function (resolve, reject) {
    var request = new Request(url, opts);
    request.headers.set("Accept-Encoding", "gzip,deflate");
  });
}
module.exports = fetch;
"#;

#[test]
fn a_table_filed_under_an_unrelated_file_is_a_defect() {
    let unit = lits(TABLE_UNIT);
    let nf = file("node_modules/node-fetch/index.js", NODE_FETCH);
    let ev = judge(&unit, &[&nf]);
    assert!(ev.evidence >= MIN_EVIDENCE, "{ev:?}");
    assert_eq!(ev.held, 0, "{ev:?}");
    assert!(ev.is_defect());
}

#[test]
fn the_same_table_under_its_own_json_file_is_no_defect_escapes_and_all() {
    // The source JSON spells the strings raw where the bundle escaped
    // them: one extractor decodes both, so the file holds the unit.
    let unit = lits(TABLE_UNIT);
    let own = file(
        "node_modules/iconv-lite/encodings/tables/cp950.json",
        r#"[["a140","　，、。"],["a180","～…‥"],["a1c0","abécd"],["a240","←↑→"],["a280","table-row-one"],["a2ab","table-row-two"],["a2e3","table-row-three"],["a2ef","table-row-four"],["a340","table-row-five"]]"#,
    );
    let ev = judge(&unit, &[&own]);
    assert_eq!(ev.held, ev.evidence, "{ev:?}");
    assert!(!ev.is_defect());
}

#[test]
fn code_whose_file_holds_its_names_is_no_defect() {
    let unit = lits(
        r#"function q(u,o){if(!q.Promise)throw new Error("native promise missing, set fetch.Promise");return new q.Promise(function(r,j){var x=new Request(u,o);x.headers.set("Accept-Encoding","gzip,deflate")})}"#,
    );
    let nf = file("node_modules/node-fetch/index.js", NODE_FETCH);
    let ev = judge(&unit, &[&nf]);
    assert!(!ev.is_defect(), "{ev:?}");
}

#[test]
fn too_little_evidence_is_never_a_defect() {
    // Three strings absent from the file: below MIN_EVIDENCE, no verdict.
    let unit = lits(r#"x.exports={alpha:"first-value",beta:"second-value"};"#);
    let nf = file("node_modules/node-fetch/index.js", NODE_FETCH);
    let ev = judge(&unit, &[&nf]);
    assert_eq!(ev.held, 0);
    assert!(ev.evidence < MIN_EVIDENCE, "{ev:?}");
    assert!(!ev.is_defect());
}

#[test]
fn a_file_that_holds_a_quarter_is_no_defect() {
    // 8 distinctive strings, 2 of them in the file: 0.25 > LACK.
    let unit = lits(
        r#"x.exports=["Accept-Encoding","gzip,deflate","word-three","word-four","word-five","word-six","word-seven","word-eight"];"#,
    );
    let nf = file("node_modules/node-fetch/index.js", NODE_FETCH);
    let ev = judge(&unit, &[&nf]);
    assert_eq!((ev.evidence, ev.held), (8, 2), "{ev:?}");
    assert!(!ev.is_defect());
}

#[test]
fn numbers_and_short_names_are_not_evidence() {
    let unit = lits(r#"x.exports=[12345,67890,"abc",{ab:1},"long-enough"];"#);
    assert_eq!(distinctive(&unit), vec!["S:long-enough"]);
}

#[test]
fn a_body_statement_with_a_bare_return_still_reads() {
    let l = lits(r#"return a.headers.get("content-type-header")"#);
    assert!(l.contains("S:content-type-header"), "{l:?}");
}

#[test]
fn a_source_that_does_not_parse_cannot_be_judged() {
    let mut texts = EntryTexts::new(
        Vec::new(),
        "",
        vec!["a.js".into(), "b.js".into()],
        vec![
            Some("var a = 'fine-value';".into()),
            Some("this is ) not js".into()),
        ],
    );
    assert!(texts.all_lits(&[0]).is_some());
    assert!(texts.all_lits(&[0, 1]).is_none());
}

fn truth(segments: Vec<(i64, i64)>, labels: &[&str]) -> Truth {
    Truth {
        method: "sourcemap".into(),
        segments,
        sources: labels
            .iter()
            .map(|l| Src {
                label: l.to_string(),
                version: None,
                file: None,
            })
            .collect(),
        app_package: None,
        rows: Vec::new(),
        strata: Default::default(),
    }
}

#[test]
fn label_sources_are_the_label_s_voting_files_most_bytes_first() {
    // segments: [0,10) src0 (pkg a), [10,40) src2 (a), [40,50) src1 (b)
    let t = truth(vec![(0, 0), (10, 2), (40, 1)], &["a", "b", "a"]);
    assert_eq!(label_sources(&t, 0, 50, "a"), vec![2, 0]);
    assert_eq!(label_sources(&t, 0, 50, "b"), vec![1]);
    assert_eq!(label_sources(&t, 12, 30, "a"), vec![2]);
}

#[test]
fn a_package_root_is_the_innermost_node_modules_entry() {
    assert_eq!(
        package_root("/a/node_modules/x/node_modules/@s/y/lib/z.js").as_deref(),
        Some("/a/node_modules/x/node_modules/@s/y")
    );
    assert_eq!(
        package_root("node_modules/q/index.js").as_deref(),
        Some("node_modules/q")
    );
    assert_eq!(package_root("/a/src/app.js"), None);
}
