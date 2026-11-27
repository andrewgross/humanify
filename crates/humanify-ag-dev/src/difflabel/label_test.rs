use std::collections::BTreeMap;

use super::{Label, Labeller};
use crate::difflabel::{InputFile, Tree, Truth};

const FROM_INPUT: &str = "function a(x){return x+1}function b(y){return y*2}";
const TO_INPUT: &str = "function q(x){return x+1}function r(y){return y*3}";

fn tree(files: &[(&str, &str)], rows: &str) -> Tree {
    let files: BTreeMap<String, Vec<u8>> = files
        .iter()
        .map(|(p, t)| (p.to_string(), t.as_bytes().to_vec()))
        .collect();
    Tree::from_parts(
        files,
        Some(&crate::difflabel::layout_of_rows(&["a.js"], rows)),
    )
    .unwrap()
}

fn truth() -> Truth {
    Truth::compare(
        vec![InputFile::parse("a.js", FROM_INPUT)],
        vec![InputFile::parse("a.js", TO_INPUT)],
    )
}

fn from_tree() -> Tree {
    tree(
        &[(
            "src/m.js",
            "const dep = require(\"./d.js\");\nfunction alpha(x) {\n  return x + 1;\n}\nfunction beta(y) {\n  return y * 2;\n}\nexports.alpha = alpha;\n",
        )],
        r#"{"src/m.js":[[1],[2,0,0],[5,0,25],[8]]}"#,
    )
}

#[test]
fn a_line_of_unchanged_input_is_labelled_by_how_its_text_differs() {
    let to = tree(
        &[
            ("src/m.js", "function beta(y) {\n  return y * 3;\n}\n"),
            (
                "src/n.js",
                "const dep2 = require(\"./d.js\");\nfunction alpha(value) {\n  return 1 + value;\n}\n",
            ),
            ("README.md", "hello\n"),
        ],
        r#"{"src/m.js":[[1,0,25]],"src/n.js":[[1],[2,0,0]]}"#,
    );
    let (truth, from) = (truth(), from_tree());
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(true, "src/n.js", 1), Label::RequireEdge);
    assert_eq!(labeller.label(true, "src/n.js", 2), Label::Renamed);
    assert_eq!(labeller.label(true, "src/n.js", 3), Label::Reshaped);
    assert_eq!(labeller.label(true, "src/n.js", 4), Label::MovedFile);
    assert_eq!(labeller.label(true, "src/m.js", 2), Label::InputChanged);
    assert_eq!(labeller.label(true, "README.md", 1), Label::NoOrigin);
    assert_eq!(labeller.label(false, "src/m.js", 2), Label::Renamed);
    assert_eq!(labeller.label(false, "src/m.js", 6), Label::InputChanged);
    assert_eq!(labeller.label(false, "src/m.js", 8), Label::Export);
}

#[test]
fn the_same_text_in_the_same_file_only_moved_within_it() {
    let to = tree(
        &[(
            "src/m.js",
            "function beta(y) {\n  return y * 3;\n}\nfunction alpha(x) {\n  return x + 1;\n}\n",
        )],
        r#"{"src/m.js":[[1,0,25],[4,0,0]]}"#,
    );
    let (truth, from) = (truth(), from_tree());
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(true, "src/m.js", 5), Label::MovedInFile);
    assert_eq!(labeller.label(false, "src/m.js", 3), Label::MovedInFile);
}

#[test]
fn a_unit_printed_on_another_number_of_lines_is_compared_as_a_set() {
    let to = tree(
        &[(
            "src/m.js",
            "function alpha(value) {\n  // kept\n  return value + 1;\n}\n",
        )],
        r#"{"src/m.js":[[1,0,0]]}"#,
    );
    let (truth, from) = (truth(), from_tree());
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(true, "src/m.js", 1), Label::Renamed);
    assert_eq!(labeller.label(true, "src/m.js", 2), Label::Reshaped);
    assert_eq!(labeller.label(true, "src/m.js", 4), Label::MovedInFile);
}

#[test]
fn an_unchanged_unit_the_other_tree_has_no_lines_for_says_so() {
    let to = tree(&[("src/m.js", "x\n")], r#"{"src/m.js":[[1]]}"#);
    let (truth, from) = (truth(), from_tree());
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(false, "src/m.js", 3), Label::NotInOtherTree);
    assert_eq!(labeller.label(true, "src/m.js", 1), Label::PipelineWritten);
}

#[test]
fn linker_text_is_matched_against_the_paired_chunks_linker_text() {
    let from = tree(
        &[(
            "src/m.js",
            "var a, b;\n(0, one.initOne)();\n(0, two.initTwo)();\nfunction alpha(x) {\n}\n",
        )],
        r#"{"src/m.js":[[1,0,null],[4,0,0]]}"#,
    );
    let to = tree(
        &[(
            "src/m.js",
            "(0, uno.initUno)();\n(0, two.initTwo)();\n(0, three.initThree)();\nvar a, b;\nfunction alpha(x) {\n}\n",
        )],
        r#"{"src/m.js":[[1,0,null],[5,0,0]]}"#,
    );
    let truth = truth();
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(false, "src/m.js", 1), Label::LinkerMoved);
    assert_eq!(labeller.label(false, "src/m.js", 2), Label::LinkerRenamed);
    assert_eq!(labeller.label(true, "src/m.js", 1), Label::LinkerRenamed);
    assert_eq!(labeller.label(true, "src/m.js", 2), Label::ImportsReordered);
    assert_eq!(
        labeller.label(false, "src/m.js", 3),
        Label::ImportsReordered
    );
    assert_eq!(labeller.label(true, "src/m.js", 3), Label::LinkerChanged);
}

#[test]
fn a_require_line_is_matched_against_the_same_files_require_lines() {
    let from = tree(
        &[(
            "src/m.js",
            "const one = require(\"./one.js\");\nconst two = require(\"./two.js\");\nconst gone = require(\"./gone.js\");\n",
        )],
        r#"{"src/m.js":[[1]]}"#,
    );
    let to = tree(
        &[(
            "src/m.js",
            "const two = require(\"./two.js\");\nconst uno = require(\"./one.js\");\nconst fresh = require(\"./fresh.js\");\n",
        )],
        r#"{"src/m.js":[[1]]}"#,
    );
    let truth = truth();
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(false, "src/m.js", 1), Label::RequireRenamed);
    assert_eq!(labeller.label(false, "src/m.js", 2), Label::RequireMoved);
    assert_eq!(labeller.label(false, "src/m.js", 3), Label::RequireEdge);
    assert_eq!(labeller.label(true, "src/m.js", 2), Label::RequireRenamed);
    assert_eq!(labeller.label(true, "src/m.js", 3), Label::RequireEdge);
}

/// A tree with one file of `calls` the linker wrote for input file 0,
/// and one file per `(path, init name, input file)`.
fn tree_of_inits(inputs: &[&str], calls: &str, inits: &[(&str, &str, u32)]) -> Tree {
    use serde_json::json;
    let mut files = BTreeMap::from([("src/m.js".to_string(), calls.as_bytes().to_vec())]);
    let mut layout = BTreeMap::from([(
        "src/m.js".to_string(),
        json!({"units": [{"at": [0, null], "linker": "init", "line": 1, "names": ["initM"]}]}),
    )]);
    for (path, name, input) in inits {
        files.insert(path.to_string(), format!("var {name};\n").into_bytes());
        let unit = json!({"at": [input, null], "linker": "init", "line": 1, "names": [name]});
        layout.insert(path.to_string(), json!({"units": [unit]}));
    }
    let layout = json!({
        "format": 1, "inputs": inputs, "priorLayout": null, "namedText": "",
        "files": layout, "priorAmbiguous": [],
    });
    Tree::from_parts(files, Some(&layout.to_string())).unwrap()
}

#[test]
fn an_init_call_is_matched_by_the_chunk_it_calls() {
    let chunk =
        |n: u32| format!("function f{n}(x){{return x+{n}}}function g{n}(y){{return y*{n}}}");
    let input = |names: [&str; 3], bodies: [u32; 3]| -> Vec<InputFile> {
        let files = names.iter().zip(bodies);
        files.map(|(n, b)| InputFile::parse(n, &chunk(b))).collect()
    };
    let truth = Truth::compare(
        input(["m.js", "one.js", "gone.js"], [1, 2, 3]),
        input(["m.js", "uno.js", "fresh.js"], [1, 2, 4]),
    );
    let from = tree_of_inits(
        &["m.js", "one.js", "gone.js"],
        "(0, one.initOne)();\n(0, gone.initGone)();\n",
        &[("src/one.js", "initOne", 1), ("src/gone.js", "initGone", 2)],
    );
    let to = tree_of_inits(
        &["m.js", "uno.js", "fresh.js"],
        "(0, fresh.initFresh)();\n(0, uno.initUno)();\n",
        &[
            ("src/uno.js", "initUno", 1),
            ("src/fresh.js", "initFresh", 2),
        ],
    );
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(false, "src/m.js", 1), Label::LinkerRenamed);
    assert_eq!(labeller.label(true, "src/m.js", 2), Label::LinkerRenamed);
    assert_eq!(labeller.label(false, "src/m.js", 2), Label::InitCallChanged);
    assert_eq!(labeller.label(true, "src/m.js", 1), Label::InitCallChanged);
}

#[test]
fn unchanged_input_at_another_indentation_was_nested_differently() {
    let to = tree(
        &[(
            "src/m.js",
            "function alpha(x) {\n    return x + 1;\n}\nfunction beta(y) {\n  return y * 3;\n}\n",
        )],
        r#"{"src/m.js":[[1,0,0],[4,0,25]]}"#,
    );
    let (truth, from) = (truth(), from_tree());
    let labeller = Labeller::new(&truth, &from, &to);
    assert_eq!(labeller.label(true, "src/m.js", 2), Label::InputReindented);
    assert_eq!(labeller.label(false, "src/m.js", 3), Label::InputReindented);
}
