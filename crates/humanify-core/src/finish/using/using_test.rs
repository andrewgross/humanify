//! The `using` desugar against the real TS's outputs
//! (test/parity/wp54-desugar.json, written by
//! test/parity/wp54-desugar-probe.ts) — every branch of the plugin and the
//! retainLines generator the port reproduces — plus the pass-through
//! cases of src/split/using-desugar.test.ts.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::{desugar_using, print_retaining_lines};

fn fixture() -> Vec<Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-desugar.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn desugar_matches_the_ts_bytes() {
    let rows = fixture();
    assert!(rows.len() >= 16);
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let code = row["code"].as_str().unwrap();
        let got = desugar_using(code);
        if let Some(reason) = row.get("refused").and_then(Value::as_str) {
            assert!(
                got.is_err(),
                "{name}: the Rust must refuse ({reason}), got {got:?}"
            );
            continue;
        }
        let want = row["out"].as_str().map(str::to_string);
        assert_eq!(got.unwrap(), want, "{name}");
    }
}

#[test]
fn files_without_a_using_declaration_are_left_alone() {
    assert_eq!(
        desugar_using("var a = 1;\nmodule.exports.a = a;\n").unwrap(),
        None
    );
    // The word in a string or a comment is not a declaration.
    assert_eq!(
        desugar_using("const s = \"using x = y\"; // using z\n").unwrap(),
        None
    );
    // `$using` is bounded (`$` is not a word char); `usingX` is not.
    assert_eq!(desugar_using("var usingX = 1;\n").unwrap(), None);
}

#[test]
fn a_parse_error_is_an_error() {
    assert!(desugar_using("function f() { using = ; }\n").is_err());
}

#[test]
fn the_printer_alone_reprints_like_babel() {
    // retainLines: `x =>` gains parens, a statement's first line keeps its
    // number, a trailing newline is trimmed.
    assert_eq!(
        print_retaining_lines("f(x => x + 1);\n\nvar a = 1,\n  b = 2;\n").unwrap(),
        "f((x) => x + 1);\n\nvar a = 1,\n  b = 2;"
    );
}
