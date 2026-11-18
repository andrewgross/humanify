use super::{Standing, Truth};
use crate::difflabel::InputFile;

fn truth(from: &str, to: &str) -> Truth {
    Truth::compare(
        vec![InputFile::parse("a.js", from)],
        vec![InputFile::parse("a.js", to)],
    )
}

fn at(truth: &Truth, text: &str, needle: &str, to_side: bool) -> Standing {
    let side = if to_side { &truth.to } else { &truth.from };
    side.at("a.js", text.find(needle).unwrap() as u32)
}

#[test]
fn a_renamed_unit_is_unchanged_and_an_edited_one_is_not() {
    let from = "function a(x){return x+1}function b(y){return y*2}var t=a(1);";
    let to = "function q(x){return x+1}function r(y){return y*3}var u=q(1);function n(){}";
    let t = truth(from, to);
    let Standing::Unchanged(pair) = at(&t, from, "x+1", false) else {
        panic!("a is unchanged");
    };
    assert_eq!(at(&t, to, "x+1", true), Standing::Unchanged(pair));
    assert_eq!(at(&t, from, "y*2", false), Standing::Changed);
    assert_eq!(at(&t, to, "y*3", true), Standing::Changed);
    assert_eq!(at(&t, to, "function n", true), Standing::Changed);
    assert!(matches!(at(&t, to, "var u", true), Standing::Unchanged(p) if p != pair));
}

#[test]
fn a_text_that_occurs_twice_cannot_be_paired() {
    let from = "function a(x){return x+1}function b(x){return x+1}";
    let t = truth(from, from);
    assert_eq!(at(&t, from, "x+1", false), Standing::Duplicate);
}

#[test]
fn a_unit_that_now_reads_a_different_outside_name_is_not_unchanged() {
    let from = "function f(){return g()}function g(){return 1}function h(){return 2}";
    let to = "function f(){return h()}function g(){return 1}function h(){return 2}";
    let t = truth(from, to);
    assert_eq!(at(&t, to, "return h()", true), Standing::Duplicate);
    assert!(matches!(
        at(&t, to, "return 2", true),
        Standing::Unchanged(_)
    ));
}

#[test]
fn a_changed_unit_inside_an_unchanged_one_reads_as_the_outer_one() {
    let from = "function f(){return()=>1}";
    let t = truth(from, from);
    let outer = at(&t, from, "function", false);
    assert!(matches!(outer, Standing::Unchanged(_)));
    assert_eq!(at(&t, from, "()=>1", false), outer);
}

#[test]
fn a_byte_outside_every_unit_or_in_an_unknown_file_says_so() {
    let from = "export{};var q=1;";
    let t = truth(from, from);
    assert_eq!(t.from.at("a.js", 2), Standing::OutsideUnits);
    assert_eq!(t.from.at("b.js", 2), Standing::UnknownFile);
}

#[test]
fn an_input_file_is_paired_with_the_file_holding_most_of_its_unchanged_units() {
    let t = Truth::compare(
        vec![
            InputFile::parse(
                "chunk-a.js",
                "function a(x){return x+1}function b(y){return y*2}",
            ),
            InputFile::parse("chunk-b.js", "function c(z){return z-7}"),
        ],
        vec![
            InputFile::parse(
                "chunk-q.js",
                "function a(x){return x+1}function b(y){return y*2}",
            ),
            InputFile::parse("chunk-r.js", "function n(w){return w/9}"),
        ],
    );
    assert_eq!(t.paired_file(false, "chunk-a.js"), Some("chunk-q.js"));
    assert_eq!(t.paired_file(true, "chunk-q.js"), Some("chunk-a.js"));
    assert_eq!(t.paired_file(false, "chunk-b.js"), None);
    assert_eq!(t.paired_file(true, "chunk-r.js"), None);
    assert_eq!(t.paired_files(), [("chunk-a.js", "chunk-q.js")]);
}

#[test]
fn files_with_only_duplicated_units_pair_by_the_texts_they_share() {
    let twin = "function a(x){return x+1}";
    let t = Truth::compare(
        vec![
            InputFile::parse("chunk-a.js", &format!("{twin}function b(y){{return y*2}}")),
            InputFile::parse("chunk-b.js", twin),
        ],
        vec![
            InputFile::parse("chunk-q.js", twin),
            InputFile::parse("chunk-r.js", &format!("{twin}function b(y){{return y*2}}")),
        ],
    );
    assert_eq!(t.paired_file(false, "chunk-a.js"), Some("chunk-r.js"));
    assert_eq!(t.paired_file(false, "chunk-b.js"), Some("chunk-q.js"));
}

#[test]
fn files_that_only_re_export_pair_by_their_export_names() {
    let t = Truth::compare(
        vec![
            InputFile::parse(
                "chunk-a.js",
                "import{x,y}from\"./k.js\";export{x as ONE,y as TWO};",
            ),
            InputFile::parse("chunk-b.js", "import{z}from\"./k.js\";export{z as THREE};"),
        ],
        vec![
            InputFile::parse("chunk-q.js", "import{p}from\"./m.js\";export{p as THREE};"),
            InputFile::parse(
                "chunk-r.js",
                "import{q,r}from\"./m.js\";export{q as ONE,r as TWO};",
            ),
        ],
    );
    assert_eq!(t.paired_file(false, "chunk-a.js"), Some("chunk-r.js"));
    assert_eq!(t.paired_file(false, "chunk-b.js"), Some("chunk-q.js"));
}
