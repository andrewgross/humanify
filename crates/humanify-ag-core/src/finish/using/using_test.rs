//! The `using` desugar against the real TS's programs
//! (test/parity/wp54-desugar.json, written by
//! test/parity/wp54-desugar-probe.ts) — every branch of the plugin and the
//! retainLines generator the port reproduces — plus the pass-through
//! cases of src/split/using-desugar.test.ts.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use oxc_allocator::Allocator;

use super::{desugar_using, print_retaining_lines};
use crate::finish::relink::parse_or_err;
use crate::format::convert::Converter;
use crate::format::printer::{Mode, Printer};

fn fixture() -> Vec<Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-desugar.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// Babel's program for `code`, printed without positions (the beautify
/// printer keeps no blank lines and no layout of the input), so two texts
/// that differ only in layout print the same.
fn canonical(code: &str) -> String {
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, code).unwrap();
    let mut converter = Converter::new(code);
    let program = converter.program(ingest.program).unwrap();
    Printer::new(&converter.tree, Mode::Beautify).generate(program)
}

/// The desugar splices into the input's own text, so its BYTES are not
/// Babel's; its PROGRAM is: the same statements, the same try statements,
/// the same helpers, the same uids (printed alike once layout is gone).
#[test]
fn desugar_builds_the_babel_program() {
    let rows = fixture();
    assert!(rows.len() >= 16);
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let code = row["code"].as_str().unwrap();
        let got = desugar_using(code);
        match row.get("refused").and_then(Value::as_str) {
            // Splicing keeps every comment where it was: nothing to refuse.
            Some("comments") => {
                let out = got.unwrap().expect("a using declaration");
                assert!(out.contains("// a comment"), "{name}: {out}");
                assert!(out.contains("const a = _usingCtx.u(g());"), "{name}: {out}");
            }
            Some(reason) => assert!(
                got.is_err(),
                "{name}: the Rust must refuse ({reason}), got {got:?}"
            ),
            None => {
                let got = got.unwrap();
                let want = row["out"].as_str();
                assert_eq!(got.is_some(), want.is_some(), "{name}");
                if let (Some(got), Some(want)) = (got, want) {
                    assert_eq!(canonical(&got), canonical(want), "{name}");
                }
            }
        }
    }
}

/// Every line the desugar does not have to change stays byte for byte:
/// no reprint (an unparenthesized arrow parameter keeps its spelling, a
/// call's own line breaks stay), no re-indent inside the new `try`. The
/// helper gets a line of its own above the file.
#[test]
fn only_the_lines_the_desugar_must_change_are_touched() {
    let code = "\
function f(a) {
  const g = x => x + 1;
  using h = open(a);
  using k = (open(a), next(a));
  return g(h.read(
      a));
}
if (c) {
  using m = () => 1;
} else {
  f(1);
}
";
    let out = desugar_using(code).unwrap().unwrap();
    let (helpers, rest) = out.split_once('\n').unwrap();
    // Babel unshifts each helper: the last one added comes first.
    assert!(
        helpers.starts_with("function _setFunctionName("),
        "{helpers}"
    );
    assert!(helpers.contains("function _usingCtx2() {"), "{helpers}");
    assert_eq!(
        rest,
        "\
function f(a) {try {var _usingCtx = _usingCtx2();
  const g = x => x + 1;
  const h = _usingCtx.u(open(a));
  const k = _usingCtx.u((open(a), next(a)));
  return g(h.read(
      a));
} catch (_) {_usingCtx.e = _;} finally {_usingCtx.d();}}
if (c) try {var _usingCtx3 = _usingCtx2();
  const m = _usingCtx3.u(_setFunctionName(() => 1, \"m\"));
} catch (_) {_usingCtx3.e = _;} finally {_usingCtx3.d();} else {
  f(1);
}
"
    );
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
