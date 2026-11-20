use serde_json::{Value, json};

use super::write::{Input, LayoutError, Previous, Run, Split, Text, layout_of};
use super::{FunctionId, Layout, Match};
use crate::emit::cjs::FileLines;
use crate::format::{FormatOptions, Formatted, format_file_with_origins};
use crate::matching::decisions::MatchDecisions;
use crate::unpack::chunk_link::{ChunkSpan, LinkMap, LinkMark, LinkRun, LinkerText};

const CODE: &str = "function a(){return()=>1}var b=2;";

fn formatted(code: &str) -> Formatted {
    format_file_with_origins(code, &FormatOptions::default(), &[]).unwrap()
}

fn text<'a>(f: &'a Formatted, shipped: &'a str) -> Text<'a> {
    Text {
        fresh: &f.text,
        shipped,
        statements: &f.statement_origins,
        depths: &f.statement_depths,
        functions: &f.function_origins,
        written_line_ends: shipped.matches('\n').count() as u32,
    }
}

fn one_file<'a>(f: &'a Formatted, previous: Option<Previous<'a>>) -> Run<'a> {
    Run {
        input: Input {
            link: None,
            name: "in.js",
            unpack: None,
        },
        text: text(f, &f.text),
        previous,
        named_text: "out.js",
        split: None,
    }
}

fn value(layout: &Layout) -> Value {
    serde_json::from_str(&layout.to_json()).unwrap()
}

#[test]
fn a_text_written_as_one_file_lists_its_statements_and_functions() {
    let f = formatted(CODE);
    assert_eq!(f.text, "function a() {\n  return () => 1;\n}\nvar b = 2;");
    let layout = layout_of(&one_file(&f, None)).unwrap();
    assert_eq!(
        value(&layout),
        json!({
            "format": 1,
            "inputs": ["in.js"],
            "priorLayout": null,
            "namedText": "out.js",
            "files": {"out.js": {"units": [
                {"at": [0, 0], "line": 1, "rows": [[1, 0, "new"], [2, 13], [2, 19, "new"]]},
                {"at": [0, 25], "line": 4},
            ]}},
            "priorAmbiguous": [],
        })
    );
    assert_eq!(Layout::parse(&layout.to_json()).unwrap(), layout);
}

fn matches(f: &Formatted) -> MatchDecisions {
    MatchDecisions {
        fresh: f.function_origins.iter().map(|m| m.0).collect(),
        prior: 2,
        exact: vec![(1, 0)],
        close: vec![],
        ambiguous: vec![0],
    }
}

#[test]
fn a_matched_function_names_its_partner_in_the_previous_layout() {
    let before = formatted("var z=()=>2;function a(){return 1}");
    let previous = layout_of(&one_file(&before, None)).unwrap().to_json();
    let f = formatted(CODE);
    let decided = matches(&f);
    let layout = layout_of(&one_file(
        &f,
        Some(Previous {
            matches: &decided,
            layout: Some(&previous),
        }),
    ))
    .unwrap();
    let got = value(&layout);
    assert_eq!(
        got["files"]["out.js"]["units"][0]["rows"],
        json!([[1, 0, "exact", 0, 12], [2, 13], [2, 19, "new"]])
    );
    assert_eq!(got["priorAmbiguous"], json!([[0, 6]]));
    assert_eq!(got["priorLayout"].as_str().map(str::len), Some(16));
}

#[test]
fn a_previous_layout_with_another_function_count_is_refused() {
    let before = formatted("function a(){return 1}");
    let previous = layout_of(&one_file(&before, None)).unwrap().to_json();
    let f = formatted(CODE);
    let decided = matches(&f);
    let got = layout_of(&one_file(
        &f,
        Some(Previous {
            matches: &decided,
            layout: Some(&previous),
        }),
    ));
    assert_eq!(
        got,
        Err(LayoutError::PreviousFunctions {
            layout: 1,
            matcher: 2
        })
    );
}

#[test]
fn a_match_against_a_tree_with_no_layout_names_no_partner() {
    let f = formatted(CODE);
    let decided = matches(&f);
    let layout = layout_of(&one_file(
        &f,
        Some(Previous {
            matches: &decided,
            layout: None,
        }),
    ))
    .unwrap();
    let got = value(&layout);
    assert_eq!(
        got["files"]["out.js"]["units"][0]["rows"],
        json!([[1, 0, "exact", null, null], [2, 13], [2, 19, "new"]])
    );
    assert_eq!(got["priorLayout"], json!(null));
}

#[test]
fn a_split_tree_lists_each_file_in_written_order() {
    let f = formatted("(function(){function a(){return()=>1}var b=2})();");
    assert_eq!(
        f.text,
        "(function () {\n  function a() {\n    return () => 1;\n  }\n  var b = 2;\n})();"
    );
    let shipped = f.text.replace("function a", "function alpha");
    let at = |needle: &str| shipped.find(needle).unwrap() as u32;
    let spans = [
        (at("function alpha"), at("  }") + 3),
        (at("var b"), at("var b") + 10),
    ];
    let files = [FileLines {
        path: "src/a.js".into(),
        header_lines: 1,
        statements: vec![(1, 1), (0, 3)],
    }];
    let run = Run {
        text: text(&f, &shipped),
        named_text: ".humanify/humanified.js",
        split: Some(Split {
            spans: &spans,
            files: &files,
            line_count: &|_| 6,
        }),
        ..one_file(&f, None)
    };
    let got = value(&layout_of(&run).unwrap());
    assert_eq!(
        got["files"],
        json!({
            ".humanify/humanified.js": {"units": [{"at": [0, 0], "line": 1, "rows": [
                [1, 1, "new"], [2, 12], [2, 12, "new"], [3, 25], [3, 31, "new"], [5, 37],
            ]}]},
            "src/a.js": {"units": [
                {"at": [0, 37], "line": 3},
                {"at": [0, 12], "line": 4, "rows": [[5, 25]]},
            ]},
        })
    );
}

#[test]
fn a_statement_written_with_other_lines_is_one_unit_without_rows() {
    let f = formatted(CODE);
    let end = f.text.find("}\n").unwrap() as u32 + 1;
    let spans = [(0, end), (end + 1, f.text.len() as u32)];
    let files = [FileLines {
        path: "src/a.js".into(),
        header_lines: 0,
        statements: vec![(0, 5), (1, 1)],
    }];
    let run = Run {
        named_text: ".humanify/humanified.js",
        split: Some(Split {
            spans: &spans,
            files: &files,
            line_count: &|path| if path == "src/a.js" { 6 } else { 3 },
        }),
        ..one_file(&f, None)
    };
    let got = value(&layout_of(&run).unwrap());
    assert_eq!(
        got["files"]["src/a.js"]["units"],
        json!([{"at": [0, 0], "line": 1}, {"at": [0, 25], "line": 6}])
    );
}

/// Two chunks linked by hand: `a.js` holds `x();`, and the linker wrote
/// `b.js` an init that calls `a.js`'s and runs `y();`.
fn linked() -> (&'static str, LinkMap) {
    let code = "var ia=w(()=>{x()});var ib=w(()=>{ia();y()});";
    let at = |needle: &str| code.find(needle).unwrap() as u32;
    let mark = |from: &str, len: usize, chunk, text| LinkMark {
        out: at(from),
        len: len as u32,
        chunk,
        text,
    };
    let run = |from: &str, chunk, src| LinkRun {
        out: at(from),
        chunk,
        src,
        len: 3,
        edited: false,
    };
    let map = LinkMap {
        files: vec!["a.js".into(), "b.js".into()],
        runs: vec![run("x()", 0, 7), run("y()", 1, 40)],
        spans: vec![
            ChunkSpan {
                out: 0,
                end: at("var ib"),
                chunk: 0,
            },
            ChunkSpan {
                out: at("var ib"),
                end: code.len() as u32,
                chunk: 1,
            },
        ],
        marks: vec![
            mark("var ia", "var ia=w(()=>{".len(), 0, LinkerText::Init),
            mark("});var ib", 3, 0, LinkerText::Init),
            mark("var ib", "var ib=w(()=>{".len(), 1, LinkerText::Init),
            mark("ia();", 5, 1, LinkerText::Call { target: 0 }),
            LinkMark {
                out: code.rfind("});").unwrap() as u32,
                len: 3,
                chunk: 1,
                text: LinkerText::Init,
            },
        ],
    };
    (code, map)
}

#[test]
fn linker_text_is_named_by_its_kind_and_a_startup_function_lists_its_body() {
    let (code, map) = linked();
    let f = formatted(code);
    let run = Run {
        input: Input {
            link: Some(&map),
            name: "a.js",
            unpack: None,
        },
        ..one_file(&f, None)
    };
    let got = value(&layout_of(&run).unwrap());
    assert_eq!(got["inputs"], json!(["a.js", "b.js"]));
    assert_eq!(
        got["files"]["out.js"]["units"],
        json!([
            {"at": [0, null], "linker": "init", "line": 1,
             "rows": [[1, "init+9", "new"]],
             "body": [{"at": [0, 7], "line": 2}]},
            {"at": [1, null], "linker": "init", "line": 4,
             "rows": [[4, "init+9", "new"]],
             "body": [
                {"at": [1, null], "linker": "call", "target": 0, "line": 5},
                {"at": [1, 40], "line": 6},
             ]},
        ])
    );
}

#[test]
fn a_wrapper_with_no_input_text_lists_the_statements_inside_it() {
    let (inner, mut map) = linked();
    let code = format!("(function(){{var h=()=>{{k()}};{inner}}})();");
    let shift = "(function(){var h=()=>{k()};".len() as u32;
    map.runs.iter_mut().for_each(|r| r.out += shift);
    map.marks.iter_mut().for_each(|m| m.out += shift);
    for span in &mut map.spans {
        span.out += shift;
        span.end += shift;
    }
    let f = formatted(&code);
    let run = Run {
        input: Input {
            link: Some(&map),
            name: "a.js",
            unpack: None,
        },
        ..one_file(&f, None)
    };
    let got = value(&layout_of(&run).unwrap());
    let units = &got["files"]["out.js"]["units"];
    assert_eq!(units.as_array().map(Vec::len), Some(1));
    assert_eq!(units[0]["at"], json!(null));
    assert_eq!(units[0]["rows"], json!([[1, "program+1", "new"]]));
    let body = units[0]["body"].as_array().unwrap();
    assert_eq!(
        body[0],
        json!({"at": null, "line": 2, "rows": [[2, "program+18", "new"]]}),
        "a helper is one unit: only the outermost wrapper lists what is inside it"
    );
    assert_eq!(body[1]["linker"], json!("init"));
    assert_eq!(body[1]["body"], json!([{"at": [0, 7], "line": 6}]));
    assert_eq!(body.len(), 3);
}

#[test]
fn linker_text_the_linker_did_not_name_is_refused() {
    let (code, mut map) = linked();
    map.marks.remove(3);
    let f = formatted(code);
    let run = Run {
        input: Input {
            link: Some(&map),
            name: "a.js",
            unpack: None,
        },
        ..one_file(&f, None)
    };
    assert!(matches!(
        layout_of(&run),
        Err(LayoutError::LinkerTextUnnamed { .. })
    ));
}

#[test]
fn a_named_text_with_other_lines_is_refused() {
    let f = formatted(CODE);
    let run = Run {
        text: text(&f, "one line"),
        ..one_file(&f, None)
    };
    assert_eq!(
        layout_of(&run),
        Err(LayoutError::LinesDiffer {
            fresh: 4,
            shipped: 1
        })
    );
}

#[test]
fn a_layout_of_another_format_is_refused() {
    let got = Layout::parse(r#"{"format": 2, "files": []}"#);
    assert_eq!(
        got.unwrap_err().to_string(),
        "layout format 2: this binary reads format 1"
    );
}

#[test]
fn the_functions_of_a_file_read_back_in_written_order() {
    let (code, map) = linked();
    let f = formatted(code);
    let run = Run {
        input: Input {
            link: Some(&map),
            name: "a.js",
            unpack: None,
        },
        ..one_file(&f, None)
    };
    let layout = layout_of(&run).unwrap();
    let got: Vec<_> = layout.files["out.js"]
        .functions()
        .map(|f| (f.input, f.id.clone(), f.matched.clone(), f.line))
        .collect();
    let init = || FunctionId::Linker("init+9".into());
    assert_eq!(
        got,
        [(0, init(), Match::New, 1), (1, init(), Match::New, 4)]
    );
}
