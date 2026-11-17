use serde_json::{Value, json};

use super::{ProvenanceError, Sources, provenance_json};
use crate::emit::cjs::FileLines;
use crate::unpack::chunk_link::{ChunkSpan, LinkMap, LinkRun};

const FRESH: &str =
    "(function () {\n  var a = 1;\n  function f() {\n    return a;\n  }\n  g();\n});\n";

fn origin(fresh_needle: &str, unpacked: u32) -> (u32, u32) {
    (FRESH.find(fresh_needle).unwrap() as u32, unpacked)
}

fn span(shipped: &str, from: &str, to: &str) -> (u32, u32) {
    let start = shipped.find(from).unwrap();
    let end = shipped[start..].find(to).unwrap() + start + to.len();
    (start as u32, end as u32)
}

fn files(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap()["files"].clone()
}

#[test]
fn each_statement_line_reads_back_to_its_input_byte() {
    let shipped = FRESH
        .replace("var a", "var count")
        .replace("return a", "return count")
        .replace("f()", "read()");
    let origins = [
        origin("var a", 100),
        origin("function f", 200),
        origin("return a", 215),
        origin("g()", 300),
    ];
    let link = LinkMap {
        files: vec!["index.js".into(), "chunk-x.js".into()],
        runs: vec![
            LinkRun {
                out: 100,
                chunk: 0,
                src: 10,
                len: 50,
                edited: false,
            },
            LinkRun {
                out: 200,
                chunk: 1,
                src: 0,
                len: 50,
                edited: false,
            },
        ],
        spans: vec![
            ChunkSpan {
                out: 90,
                end: 160,
                chunk: 0,
            },
            ChunkSpan {
                out: 190,
                end: 400,
                chunk: 1,
            },
        ],
    };
    let sources = Sources {
        link: Some(&link),
        input_name: "index.js",
        unpack: None,
        fresh: FRESH,
        origins: &origins,
        shipped: &shipped,
        spans: &[
            span(&shipped, "var count", ";"),
            span(&shipped, "function read", "  }"),
            span(&shipped, "g()", ";"),
        ],
    };
    let emitted = [
        FileLines {
            path: "src/a.js".into(),
            header_lines: 2,
            statements: vec![(1, 3), (0, 1)],
        },
        FileLines {
            path: "src/b.js".into(),
            header_lines: 0,
            statements: vec![(2, 1)],
        },
    ];
    let text = provenance_json(&sources, &emitted, &|path| {
        if path == "src/a.js" { 7 } else { 1 }
    })
    .unwrap();
    assert_eq!(
        files(&text),
        json!({
            "src/a.js": [[1], [4, 1, 0], [5, 1, 15], [7, 0, 10]],
            "src/b.js": [[1, 1, null]],
        })
    );
}

#[test]
fn a_named_text_with_other_lines_is_refused() {
    let sources = Sources {
        link: None,
        input_name: "in.js",
        unpack: None,
        fresh: FRESH,
        origins: &[],
        shipped: "one line",
        spans: &[],
    };
    assert_eq!(
        provenance_json(&sources, &[], &|_| 0),
        Err(ProvenanceError::LinesDiffer {
            fresh: 8,
            shipped: 1
        })
    );
}
