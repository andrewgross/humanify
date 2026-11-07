//! score.test.ts's canonical() vectors (exp092, exp094 and the 2026-09-29
//! number-blur study), ported one for one.

use super::{canonical, canonical_loose as lo, line_diff_mass};

#[test]
fn wrapper_spelling_is_normalized_function_expression_equals_arrow() {
    assert_eq!(
        canonical("function (a, b) {\n  return a + b;\n}"),
        canonical("(a, b) => {\n  return a + b;\n}")
    );
    assert_eq!(
        canonical("async function (a) {\n  return a;\n}"),
        canonical("async (a) => {\n  return a;\n}")
    );
    // The statement slice carries its `var name =` prefix.
    assert_eq!(
        canonical("var w = function (a) {\n  return a;\n};"),
        canonical("var w = (a) => {\n  return a;\n};")
    );
}

#[test]
fn the_erasure_refuses_a_flip_the_flip_would_rebind() {
    assert_ne!(
        canonical("function () {\n  return this.token;\n}"),
        canonical("() => {\n  return this.token;\n}")
    );
    assert_ne!(
        canonical("function () {\n  return arguments.length;\n}"),
        canonical("() => {\n  return arguments.length;\n}")
    );
    // Occurrences the flip does NOT rebind keep the erasure.
    assert_eq!(
        canonical("function () {\n  return x.arguments;\n}"),
        canonical("() => {\n  return x.arguments;\n}")
    );
    assert_eq!(
        canonical("function () {\n  return function () {\n    return this;\n  };\n}"),
        canonical("() => {\n  return function () {\n    return this;\n  };\n}")
    );
    assert_eq!(
        canonical(
            "function (a) {\n  class C {\n    m() {\n      return this.x;\n    }\n    f = this.y;\n  }\n  return new C(a);\n}"
        ),
        canonical(
            "(a) => {\n  class C {\n    m() {\n      return this.x;\n    }\n    f = this.y;\n  }\n  return new C(a);\n}"
        )
    );
}

#[test]
fn consistent_identifier_renaming_is_erased() {
    assert_eq!(
        canonical("function (px, qy) {\n  return px * qy + 7;\n}"),
        canonical("function (xx, yy) {\n  return xx * yy + 7;\n}")
    );
    assert_eq!(
        canonical("function foo(a) {\n  return foo.length + a;\n}"),
        canonical("function bar(b) {\n  return bar.length + b;\n}")
    );
}

#[test]
fn inconsistent_renaming_is_not_the_same_function() {
    assert_ne!(
        canonical("function (px, qy) {\n  return px + px;\n}"),
        canonical("function (a, b) {\n  return a + b;\n}")
    );
}

#[test]
fn string_contents_are_opaque_not_identifiers() {
    assert_ne!(
        canonical("function () {\n  return \"alpha\";\n}"),
        canonical("function () {\n  return \"beta\";\n}")
    );
    assert_eq!(
        canonical("function (aa) {\n  return aa + \"zz\";\n}"),
        canonical("function (b) {\n  return b + \"zz\";\n}")
    );
}

#[test]
fn structure_and_literals_are_kept() {
    assert_ne!(
        canonical("function (n) {\n  return n * 2;\n}"),
        canonical("function (n) {\n  return n * 3;\n}")
    );
    assert_ne!(
        canonical("function (a, b) {\n  return a + b;\n}"),
        canonical("function (a, b) {\n  return a - b;\n}")
    );
}

#[test]
fn the_loose_form_blurs_literals_but_keeps_operators() {
    assert_eq!(
        lo("function (n) {\n  return n * 2 + 50;\n}"),
        lo("function (n) {\n  return n * 3 + 100;\n}")
    );
    assert_eq!(
        lo("function () {\n  return \"aa\";\n}"),
        lo("function () {\n  return \"bb\";\n}")
    );
    assert_ne!(
        lo("function (a, b) {\n  return a + b;\n}"),
        lo("function (a, b) {\n  return a - b;\n}")
    );
}

#[test]
fn template_interpolations_are_code_not_string_content() {
    assert_eq!(
        canonical("function (count) {\n  return `${count} items`;\n}"),
        canonical("function (totalShown) {\n  return `${totalShown} items`;\n}")
    );
    assert_eq!(
        canonical("function (px) {\n  var msg = `got ${px}`;\n  return px;\n}"),
        canonical("function (user) {\n  var msg = `got ${user}`;\n  return user;\n}")
    );
    assert_eq!(
        canonical("function (a) {\n  return `${({ k: a }).k} end`;\n}"),
        canonical("function (b) {\n  return `${({ k: b }).k} end`;\n}")
    );
    assert_ne!(
        canonical("function () {\n  return `a${1}x`;\n}"),
        canonical("function () {\n  return `a${1}y`;\n}")
    );
}

#[test]
fn the_loose_form_blurs_each_template_quasi_by_length() {
    assert_eq!(
        lo("function (x) {\n  return `${x} done`;\n}"),
        lo("function (userCount) {\n  return `${userCount} done`;\n}")
    );
    assert_ne!(
        lo("function (n) {\n  return `${n} aa`;\n}"),
        lo("function (n) {\n  return `${n} aaaa`;\n}")
    );
}

#[test]
fn numeric_literals_are_one_token_a_bigint_suffix_is_never_an_identifier() {
    assert_eq!(
        canonical(
            "function (index) {\n  let counter = 21;\n  while (counter > 0n) {\n    counter--;\n  }\n}"
        ),
        canonical("function (n) {\n  let a = 21;\n  while (a > 0n) {\n    a--;\n  }\n}")
    );
    assert_eq!(
        canonical("function (x) {\n  return x + 1024n;\n}"),
        "(_0) => {\n  return _0 + 1024n;\n}"
    );
    assert_eq!(
        lo("function (x) {\n  return x + 1024n;\n}"),
        lo("function (x) {\n  return x + 999999n;\n}")
    );
}

#[test]
fn a_regex_containing_a_quote_does_not_desync_the_string_scanner() {
    assert_eq!(
        canonical(
            "function (px) {\n  var re = /\"/;\n  return (\"done \" + px).replace(re, \"x\");\n}"
        ),
        canonical(
            "function (user) {\n  var re = /\"/;\n  return (\"done \" + user).replace(re, \"x\");\n}"
        )
    );
    assert_ne!(
        canonical("function () {\n  var re = /a\"x/;\n}"),
        canonical("function () {\n  var re = /b\"x/;\n}")
    );
    assert_eq!(
        canonical("function (s) {\n  return /\"\\d\"/.test(s);\n}"),
        canonical("function (str) {\n  return /\"\\d\"/.test(str);\n}")
    );
    assert_ne!(
        canonical("function (a, b) {\n  return a / b;\n}"),
        canonical("function (a, b) {\n  return a * b;\n}")
    );
    assert_eq!(
        canonical("function (total) {\n  return total / 2;\n}"),
        canonical("function (sum) {\n  return sum / 2;\n}")
    );
}

// Beyond score.test.ts: the exact spellings, read off the TS canonicalizer
// (`npx tsx -e` over canonical.ts), so the port is held to bytes, not only
// to equalities.

#[test]
fn exact_spellings_match_the_ts_canonicalizer() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "var w = async function (a, b) {\n\n\n  return `x${a}y${ {k:b}.k }` + 'q' + /[/]\"/g.source;\n};",
            "var _0 = async (_1, _2) => {\n  return «x»${_1}«y»${ {_3:_2}._3 }«» + 'q' + /[/]\"/g._4;\n};",
            "var _0 = async (_1, _2) => {\n  return #s1${_1}#s1${ {_3:_2}._3 }#s0 + #s1 + /[/]\"/g._4;\n};",
        ),
        (
            "function () {\n  return this.x / 2 / 3;\n}",
            "function () {\n  return this._0 / 2 / 3;\n}",
            "function () {\n  return this._0 / # / #;\n}",
        ),
        (
            "  const  f = function (\n  a\n) { return typeof a === \"\\u00e9\\\"x\" ? 0x1F : 1.5e3; }",
            "  const  _0 = (\n  _1\n) => { return typeof _1 === \"\\u00e9\\\"x\" ? 0x1F : 1.5e3; }",
            "  const  _0 = (\n  _1\n) => { return typeof _1 === #s9 ? # : #; }",
        ),
        (
            "function (a) { return a ? \"é😀\" : `😀${a}é`; }",
            "(_0) => { return _0 ? \"é😀\" : «😀»${_0}«é»; }",
            "(_0) => { return _0 ? #s3 : #s2${_0}#s1; }",
        ),
        (
            "function (a) {\r\n  return a\t+\t'unterminated\n  ;}",
            "function (_0) {\n  return _0 + 'unterminated\n  ;}",
            "function (_0) {\n  return _0 + #s11\n  ;}",
        ),
        (
            "var x = function* () { return 1; };",
            "var _0 = function* () { return 1; };",
            "var _0 = function* () { return #; };",
        ),
        (
            "function () { return new.target; }",
            "function () { return new._0; }",
            "function () { return new._0; }",
        ),
    ];
    for (input, strict, loose) in cases {
        assert_eq!(canonical(input), *strict, "strict: {input}");
        assert_eq!(lo(input), *loose, "loose: {input}");
    }
}

#[test]
fn line_diff_mass_counts_lines_with_multiplicity() {
    assert_eq!(line_diff_mass("a\nb\nb", "a\nb\nc"), 2);
    assert_eq!(line_diff_mass("x", "x"), 0);
}

/// The ONE place the port answers differently from canonical.ts, on
/// purpose: the head-erasure refusal is the pipeline's rule
/// (`hash::wrapper_spelling`), which looks for `new.target` in the
/// function's OWN scope, while the TS arm refused on a `new.target`
/// anywhere in the subtree. A nested classic function binds its own
/// `new.target`, so the flip cannot change it: the TS refusal only missed
/// ground truth (its designed error direction), never invented it.
#[test]
fn a_nested_new_target_does_not_refuse_the_erasure_unlike_the_ts_arm() {
    let nested = "function () { return function () { return new.target; }; }";
    // TS: "function () { return function () { return new._0; }; }"
    assert_eq!(
        canonical(nested),
        "() => { return function () { return new._0; }; }"
    );
    // Its OWN new.target still refuses, as in the TS.
    assert_eq!(
        canonical("function () { return new.target; }"),
        "function () { return new._0; }"
    );
}
