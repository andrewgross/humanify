use humanify_ag_core::layout_record::{FileEntry, FunctionId, Layout, Match, Row, Unit};

use super::{Standings, locate, locate_init_wrappers};
use crate::difflabel::{InputFile, Truth};

const FROM: &str = "function a(x){return x+1}function b(y){return y*2}";
const TO: &str = "function r(y){return y*2}function n(){}function q(x){return x+1}";

/// A layout whose named text holds these functions, one unit each.
pub fn layout(inputs: &[&str], functions: &[(u32, FunctionId, Match)]) -> Layout {
    let unit = |(i, (input, id, matched)): (usize, &(u32, FunctionId, Match))| Unit {
        at: Some((*input, None)),
        line: i as u32 + 1,
        rows: vec![Row::Function {
            line: i as u32 + 1,
            id: id.clone(),
            matched: matched.clone(),
        }],
        ..Unit::default()
    };
    let entry = FileEntry {
        units: functions.iter().enumerate().map(unit).collect(),
    };
    Layout {
        format: 1,
        inputs: inputs.iter().map(|n| n.to_string()).collect(),
        prior_layout: None,
        named_text: "named.js".into(),
        files: [("named.js".to_string(), entry)].into(),
        prior_ambiguous: Vec::new(),
    }
}

pub fn at(text: &str, needle: &str) -> FunctionId {
    FunctionId::Byte(text.find(needle).unwrap() as u32)
}

fn new(id: FunctionId) -> (u32, FunctionId, Match) {
    (0, id, Match::New)
}

#[test]
fn a_function_sits_where_its_input_byte_is() {
    let truth = Truth::compare(
        vec![InputFile::parse("index.js", FROM)],
        vec![InputFile::parse("index.js", TO)],
    );
    let from = layout(
        &["index.js"],
        &[new(at(FROM, "function a")), new(at(FROM, "function b"))],
    );
    let to = layout(
        &["index.js"],
        &[
            new(FunctionId::Linker("init+9".into())),
            new(at(TO, "function r")),
            new(at(TO, "function n")),
            new(at(TO, "function q")),
        ],
    );
    let prior = locate(&from, &truth.from).functions;
    let located = locate(&to, &truth.to);
    let pairs =
        |side: &[crate::matchcard::score::Located]| side.iter().map(|f| f.pair).collect::<Vec<_>>();
    let (a_pair, b_pair) = (prior[0].pair.unwrap(), prior[1].pair.unwrap());
    assert_eq!(
        pairs(&located.functions),
        [None, Some(b_pair), None, Some(a_pair)]
    );
    assert_eq!(located.by_own_byte, 3);
    assert_eq!(
        located.standings,
        Standings::from([("changed", 1), ("no input byte", 1), ("unchanged", 2)])
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
    let to = layout(
        &["index.js"],
        &[new(FunctionId::Byte(0)), new(at(to_text, "function(x)"))],
    );
    let fresh = locate(&to, &truth.to).functions;
    assert_eq!(fresh[0].pair, None);
    assert!(fresh[1].pair.is_some(), "the inner function is unchanged");
}

#[test]
fn a_method_named_by_its_key_sits_in_its_own_function() {
    let from_text = "class K{m(x){return x+1}n(){return 1}}";
    let to_text = "class K{m(x){return x+1}n(){return 2}}";
    let truth = Truth::compare(
        vec![InputFile::parse("index.js", from_text)],
        vec![InputFile::parse("index.js", to_text)],
    );
    let to = layout(
        &["index.js"],
        &[new(at(to_text, "m(x)")), new(at(to_text, "n()"))],
    );
    let fresh = locate(&to, &truth.to).functions;
    assert!(fresh[0].pair.is_some(), "the method is unchanged");
    assert_eq!(fresh[1].pair, None);
}

#[test]
fn an_init_wrapper_sits_in_the_input_file_the_linker_wrote_it_for() {
    let linker = |input, id: &str| (input, FunctionId::Linker(id.into()), Match::New);
    let tree = layout(
        &["index.js", "chunk-a.js", "chunk-b.js"],
        &[
            linker(1, "init+9"),
            linker(2, "init+9"),
            linker(1, "exports+4"),
            (0, FunctionId::Byte(30), Match::New),
        ],
    );
    let pair_of = |file: &str| (file != "chunk-b.js").then_some(file.len() as u32);
    let got = locate_init_wrappers(&tree, &pair_of);
    let pairs: Vec<_> = got.iter().map(|f| f.pair).collect();
    assert_eq!(pairs, [Some(10), None, None, None]);
}
