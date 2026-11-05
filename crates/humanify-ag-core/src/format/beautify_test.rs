//! The formatter's CORRECTNESS fixes (findings #42, #44, #45, #46): since
//! the cutover the formatter no longer reproduces the TS beautifier's
//! bytes, so its semantic bugs are fixed here. Each case pins the exact
//! stage-6 output for the finding's repro input and — where the program
//! can run on its own — runs the original and the formatted text under
//! node and compares what they print (the formatter must never change a
//! program's meaning).

use std::process::Command;

use super::{FormatOptions, format};

fn full(code: &str) -> String {
    format(code, &FormatOptions::default()).unwrap_or_else(|e| panic!("format failed: {e}"))
}

/// Run `code` as an ES module under node; its stdout (or the error).
fn run_node(code: &str) -> String {
    let out = Command::new("node")
        .args(["--input-type=module", "-e", code])
        .output()
        .expect("node is on PATH (the gate runs under it)");
    format!(
        "exit {:?}\n{}{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
            .lines()
            .find(|l| l.contains("Error"))
            .unwrap_or("")
    )
}

/// The formatted program prints exactly what the original prints.
fn same_behaviour(code: &str) -> String {
    let formatted = full(code);
    let (a, b) = (run_node(code), run_node(&formatted));
    assert_eq!(
        a, b,
        "behaviour changed:\n--- original\n{code}\n--- formatted\n{formatted}"
    );
    formatted
}

// -- #42: `a ?? b;` is a NULLISH test, not a falsy one --------------------------

#[test]
fn nullish_statement_tests_for_null_not_falsiness() {
    assert_eq!(
        full("H._zod ?? (H._zod = {});"),
        "if (H._zod == null) {\n  H._zod = {};\n}"
    );
    let out = same_behaviour(
        "const H = { _zod: 0 }, G = { _zod: '' }, K = {};\n\
         H._zod ?? (H._zod = {}); G._zod ?? (G._zod = {}); K._zod ?? (K._zod = {});\n\
         console.log(JSON.stringify([H, G, K]));",
    );
    assert!(out.contains("== null"), "{out}");
}

#[test]
fn nullish_chain_keeps_the_inner_nullish_and_parenthesizes() {
    assert_eq!(full("a ?? b ?? c;"), "if ((a ?? b) == null) {\n  c;\n}");
    same_behaviour("let n = 0; const a = null, b = false; a ?? b ?? (n = 1); console.log(n);");
}

#[test]
fn nullish_statement_evaluates_its_left_side_once() {
    same_behaviour(
        "let n = 0; function f() { n++; return 0; } f() ?? console.log('rhs'); console.log(n);",
    );
}

#[test]
fn and_or_statements_are_unchanged() {
    assert_eq!(full("a && b;"), "if (a) {\n  b;\n}");
    assert_eq!(full("a || b;"), "if (!a) {\n  b;\n}");
}

// -- #42: `void <n>` → `undefined` only where `undefined` is the global --------

#[test]
fn void_number_is_kept_under_a_local_undefined() {
    assert_eq!(
        full("function f(undefined) { return void 0; }"),
        "function f(undefined) {\n  return void 0;\n}"
    );
    same_behaviour("function f(undefined) { return void 0; } console.log(String(f(5)));");
}

#[test]
fn void_number_still_becomes_undefined_elsewhere() {
    assert_eq!(full("f(void 0);"), "f(undefined);");
}

// -- #45: the `.concat` fold evaluates every argument exactly once ----------------

#[test]
fn concat_fold_in_a_nullish_statement_does_not_duplicate_the_argument() {
    assert_eq!(
        full("`u`.concat(f()) ?? z;"),
        "if (`u${f()}` == null) {\n  z;\n}"
    );
    assert_eq!(
        full("while (x) a, `u`.concat(y) ?? z;"),
        "while (x) {\n  a;\n  if (`u${y}` == null) {\n    z;\n  }\n}"
    );
    same_behaviour(
        "let n = 0; function f() { n++; return 'x'; } `u`.concat(f()) ?? 0; console.log(n);",
    );
    same_behaviour(
        "let n = 0; function f() { n++; return 'x'; } `u`.concat(f()) || 0; console.log(n);",
    );
}

// -- the `.concat` fold's template raw text ------------------------------------------

#[test]
fn concat_fold_keeps_string_escapes_in_the_template_raw() {
    // Was `` `a<LF>b\c${x}` `` — the cooked value used as raw: the `\\`
    // became `\c` (cooked "c"), the backslash lost.
    assert_eq!(full(r#""a\nb\\c".concat(x);"#), r#"`a\nb\\c${x}`;"#);
    same_behaviour(r#"const x = 1; console.log(JSON.stringify("a\nb\\c\tA\x42'\"".concat(x)));"#);
}

#[test]
fn concat_fold_escapes_a_backtick_and_dollar_brace() {
    // #44 (a): was a throw ("Invalid raw"), failing the whole file.
    assert_eq!(full(r#""a`b${c}".concat(x);"#), r#"`a\`b\${c}${x}`;"#);
    same_behaviour(r#"const x = 1; console.log("a`b${c}$d\${".concat(x));"#);
}

#[test]
fn template_concat_appends_a_string_tail_once() {
    // Was `` `x${y}z!!` `` — the tail appended twice.
    assert_eq!(full(r#"`x${y}z`.concat("!");"#), r#"`x${y}z!`;"#);
    assert_eq!(full(r#"`${y}`.concat("!");"#), r#"`${y}!`;"#);
    same_behaviour(r#"const y = 1; console.log(`x${y}z`.concat("!`${\\"));"#);
}

#[test]
fn chained_concat_folds_keep_escapes_and_order() {
    same_behaviour(
        r#"const a = 1, b = 2; console.log("p\\".concat(a).concat("`q").concat(b).concat("r\n"));"#,
    );
}

#[test]
fn string_concat_string_folds_to_one_string() {
    assert_eq!(full(r#""a`".concat("b\\");"#), r#""a`b\\";"#);
}

// -- #44 (b): a labeled multi-declarator `var` ----------------------------------------

#[test]
fn labeled_var_in_a_loop_body_formats_and_keeps_its_label() {
    for code in [
        "do lbl: var a = 1, b = 2; while (x);",
        "while (x) lbl: var a = 1, b = 2;",
        "for (;;) lbl: var a = 1, b = 2;",
    ] {
        let out = full(code);
        assert!(out.contains("lbl:"), "{code} → {out}");
    }
    same_behaviour("let i = 0; do lbl: var a = 1, b = 2; while (i++ < 2); console.log(a, b, i);");
}

#[test]
fn labeled_var_in_an_if_branch_keeps_its_label() {
    assert_eq!(
        full("if (x) lbl: var a = 1, b = 2;"),
        "if (x) {\n  lbl: var a = 1,\n    b = 2;\n}"
    );
    assert_eq!(full("lbl: var a = 1, b = 2;"), "lbl: var a = 1,\n  b = 2;");
}

#[test]
fn unlabeled_var_in_a_loop_body_still_splits() {
    same_behaviour("let i = 0; do var a = 1, b = 2; while (i++ < 2); console.log(a, b, i);");
}

// -- #46: `@license` / `@preserve` comments are kept ----------------------------

#[test]
fn license_comments_are_kept_as_a_file_header() {
    assert_eq!(
        full("/*! @license MIT */\nvar a = 1; /* @preserve keep */ f(a); // plain\n"),
        "/*! @license MIT */\n/* @preserve keep */\nvar a = 1;\nf(a);"
    );
    assert_eq!(
        full("#!/usr/bin/env node\n// @license X\nf();"),
        "#!/usr/bin/env node\n// @license X\nf();"
    );
}

// -- makeNumbersLonger keeps the number's VALUE --------------------------------------

#[test]
fn longer_numbers_keep_their_value_with_a_separator() {
    // Was `a(NaN, 1_000, NaN)` — the TS's `Number(raw)` rejects `_`.
    assert_eq!(full("a(1_0e3, 1_000, 0xe_1);"), "a(10000, 1_000, 225);");
    same_behaviour("console.log(1_0e3, 0xe_1, 5e3);");
}

// -- evaluation ORDER and COUNT: hoisting out of a loop, dropping an operand ------------

#[test]
fn void_in_a_for_update_is_not_hoisted_out_of_the_loop() {
    // Was `a(); for (;; undefined)` — the update ran once, not per turn.
    assert_eq!(
        full("for (;; void a()) b();"),
        "for (;; void a()) {\n  b();\n}"
    );
    same_behaviour("let n = 0; for (let i = 0; i < 3; void n++) i++; console.log(n);");
}

#[test]
fn void_in_a_do_while_test_is_not_hoisted_before_the_body() {
    same_behaviour("let s = ''; do { s += 'b'; } while (void (s += 'a')); console.log(s);");
}

#[test]
fn void_statement_under_a_local_undefined_runs_its_argument_once() {
    // Was `b(); void b();` — the argument hoisted AND kept.
    assert_eq!(
        full("{ let undefined = 1; void b(); }"),
        "{\n  let undefined = 1;\n  void b();\n}"
    );
    same_behaviour(
        "let n = 0; const b = () => n++; { let undefined = 1; void b(); } console.log(n);",
    );
    same_behaviour(
        "let n = 0; const b = () => n++; try { throw 1; } catch (undefined) { void b(); } console.log(n);",
    );
}

#[test]
fn void_of_a_template_keeps_the_template_expressions() {
    // Was `g(undefined)` — `b()` dropped.
    same_behaviour("let n = 0; const b = () => n++; console.log(void `a${b()}`, n);");
}

#[test]
fn sequence_in_a_loop_test_is_not_hoisted_out_of_the_loop() {
    // Was `n++; while (…)` — the first expression ran once, not per test.
    same_behaviour("let n = 0, k = 0; while ((n++, n < 3 && k++ < 10)); console.log(n, k);");
    same_behaviour("let n = 0, k = 0; do k++; while ((n++, n < 3 && k < 10)); console.log(n, k);");
}

#[test]
fn comparison_flip_keeps_a_template_expression_first() {
    // Was `g() === `${f()}`` — g ran before f.
    same_behaviour(
        "let s = ''; const f = () => { s += 'f'; }, g = () => { s += 'g'; }; `${f()}` === g(); console.log(s);",
    );
    assert_eq!(full("1 < a;"), "a > 1;");
    // A regex flips under `==` (one side converted) but not under `<`.
    assert_eq!(full("/r/ == z;"), "z == /r/;");
    assert_eq!(full("/r/ < z;"), "/r/ < z;");
}
