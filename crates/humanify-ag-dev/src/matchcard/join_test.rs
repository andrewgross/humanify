use std::collections::BTreeMap;

use super::{NAMED_TEXT, Standings, locate, locate_init_wrappers};
use crate::difflabel::{InputFile, Tree, Truth};

const FROM: &str = "function a(x){return x+1}function b(y){return y*2}";
const TO: &str = "function r(y){return y*2}function n(){}function q(x){return x+1}";

fn tree(rows: &str) -> Tree {
    let provenance = format!(
        r#"{{"version":1,"rows":"","inputFiles":["index.js"],"files":{{".humanify/humanified.js":{rows}}}}}"#
    );
    Tree::from_parts(BTreeMap::new(), Some(&provenance)).unwrap()
}

#[test]
fn a_function_sits_where_its_line_came_from() {
    let truth = Truth::compare(
        vec![InputFile::parse("index.js", FROM)],
        vec![InputFile::parse("index.js", TO)],
    );
    let b = FROM.find("function b").unwrap();
    let (n, q) = (
        TO.find("function n").unwrap(),
        TO.find("function q").unwrap(),
    );
    let from = tree(&format!("[[1],[2,0,0],[4,0,{b}]]"));
    let to = tree(&format!("[[1],[2,0,0],[3,0,{n}],[4,0,{q}],[6,0,null]]"));

    let prior = locate(&from, &truth.from, &[2, 3, 4]).functions;
    let located = locate(&to, &truth.to, &[1, 2, 3, 4, 5, 6]);
    let (fresh, standings) = (located.functions, located.standings);

    let pairs =
        |side: &[crate::matchcard::score::Located]| side.iter().map(|f| f.pair).collect::<Vec<_>>();
    let (a_pair, b_pair) = (prior[0].pair.unwrap(), prior[2].pair.unwrap());
    assert_eq!(pairs(&prior), [Some(a_pair), Some(a_pair), Some(b_pair)]);
    assert_eq!(
        pairs(&fresh),
        [None, Some(b_pair), None, Some(a_pair), Some(a_pair), None]
    );
    assert!(fresh[3].order < fresh[4].order, "text order within one row");
    assert_eq!(
        standings,
        Standings::from([("changed", 1), ("no input byte", 2), ("unchanged", 3)])
    );
}

#[test]
fn a_function_inside_a_changed_one_is_judged_by_its_own_text() {
    let from_text = "function out(){var a=function(x){return x+1};return a(5)}";
    let to_text = "function out(){var a=function(x){return x+1};return a(6)}";
    let truth = Truth::compare(
        vec![InputFile::parse("index.js", from_text)],
        vec![InputFile::parse("index.js", to_text)],
    );
    let rows = format!(
        "[[1,0,0],[2,0,{}],[3,0,{}]]",
        to_text.find("var a").unwrap(),
        to_text.find("return a").unwrap()
    );
    let located = locate(&tree(&rows), &truth.to, &[1, 2]);
    let (fresh, standings) = (located.functions, located.standings);
    assert_eq!(located.by_own_byte, 2);
    assert_eq!(fresh[0].pair, None);
    assert!(fresh[1].pair.is_some(), "the inner function is unchanged");
    assert_eq!(
        standings,
        Standings::from([("changed", 1), ("unchanged", 1)])
    );
}

#[test]
fn an_init_wrapper_sits_in_the_input_file_the_linker_wrote_it_for() {
    let text = "var initA = __esm(() => {\n  a();\n});\nvar initB = lazy(async () => {\n  b();\n});\nvar map = {\n  get: () => a\n};\nvar initC = __esm(() => {});\n";
    let provenance = r#"{"version":1,"rows":"","inputFiles":["index.js","chunk-a.js","chunk-b.js"],
        "files":{".humanify/humanified.js":[[1,1,null],[2,1,5],[3,1,null],[4,2,null],[5,2,9],[6,2,null],[7,0,null],[10,0,30]]}}"#;
    let files = BTreeMap::from([(NAMED_TEXT.to_string(), text.as_bytes().to_vec())]);
    let tree = Tree::from_parts(files, Some(provenance)).unwrap();
    let pair_of = |file: &str| (file != "chunk-b.js").then_some(file.len() as u32);
    let got = locate_init_wrappers(&tree, &[1, 4, 8, 10], &pair_of);
    let pairs: Vec<_> = got.iter().map(|f| f.pair).collect();
    assert_eq!(pairs, [Some(10), None, None, None]);
}
