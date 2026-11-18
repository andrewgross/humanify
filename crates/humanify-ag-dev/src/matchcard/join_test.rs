use std::collections::BTreeMap;

use super::{Standings, locate};
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

    let (prior, _) = locate(&from, &truth.from, &[2, 3, 4]);
    let (fresh, standings) = locate(&to, &truth.to, &[1, 2, 3, 4, 5, 6]);

    let pairs = |side: &[super::Located]| side.iter().map(|f| f.pair).collect::<Vec<_>>();
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
