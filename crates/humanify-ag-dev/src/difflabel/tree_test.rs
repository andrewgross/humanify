use std::collections::BTreeMap;

use super::{Origin, Tree};

fn tree() -> Tree {
    let files = BTreeMap::from([
        ("src/a.js".to_string(), b"one\ntwo\nthree\nfour\n".to_vec()),
        ("src/b.bin".to_string(), b"\0\x01".to_vec()),
        ("notes.txt".to_string(), b"x".to_vec()),
    ]);
    let layout = r#"{"format":1,"inputs":["index.js","chunk-a.js"],"priorLayout":null,
        "namedText":"a.js","priorAmbiguous":[],
        "files":{"src/a.js":{"units":[
            {"at":[1,40],"line":2,"rows":[[2,40,"new"],[3,40]]},
            {"at":[1,null],"linker":"init","line":4,"body":[{"at":[1,null],"linker":"call","target":0,"line":4}]}]}}}"#;
    Tree::from_parts(files, Some(layout)).unwrap()
}

#[test]
fn a_line_reads_the_row_it_falls_in() {
    let t = tree();
    assert_eq!(t.origin("src/a.js", 1), Some(Origin::Pipeline));
    assert_eq!(t.origin("src/a.js", 2), Some(Origin::At(1, 40)));
    assert_eq!(t.origin("src/a.js", 3), Some(Origin::At(1, 40)));
    assert_eq!(t.origin("src/a.js", 4), Some(Origin::Linker(1)));
    assert_eq!(t.origin("notes.txt", 1), None);
    assert_eq!(t.input_name(1), "chunk-a.js");
}

#[test]
fn a_file_gives_its_lines_and_says_when_it_is_not_text() {
    let t = tree();
    assert_eq!(t.line("src/a.js", 3), Some("three"));
    assert_eq!(t.line_count("src/a.js"), Some(4));
    assert_eq!(t.line_count("notes.txt"), Some(1));
    assert_eq!(t.line_count("src/b.bin"), None);
    assert_eq!(t.rows("src/a.js").len(), 3);
}

#[test]
fn a_tree_without_a_layout_record_has_no_origins() {
    let files = BTreeMap::from([("a.js".to_string(), b"x\n".to_vec())]);
    let t = Tree::from_parts(files, None).unwrap();
    assert_eq!(t.origin("a.js", 1), None);
}

#[test]
fn a_layout_of_another_format_is_refused() {
    let got = Tree::from_parts(BTreeMap::new(), Some(r#"{"format":7}"#));
    assert_eq!(
        got.err().as_deref(),
        Some(".humanify/layout.json: layout format 7: this binary reads format 1")
    );
}
