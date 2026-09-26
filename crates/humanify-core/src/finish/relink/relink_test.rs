//! Ported from src/split/bun-relink.test.ts (the text-level cases; the
//! executed ones live in the TS suite and the boot gate). Every expected
//! string was produced by the TS functions themselves
//! (`relinkFactoryReferences` / `wrapExtractedFactory`, 2026-09-25).

use super::{BUN_RELINK_RUNTIME, FactoryLookup, relink_factory_references, wrap_extracted_factory};

fn lookup(entries: &[(&str, &str)]) -> FactoryLookup {
    entries
        .iter()
        .map(|(id, file)| (id.to_string(), file.to_string()))
        .collect()
}

fn two() -> FactoryLookup {
    lookup(&[("lib_aaaa", "lib_aaaa.js"), ("lib_bbbb", "pkg/axios.js")])
}

fn relink(code: &str, file: &str) -> String {
    relink_factory_references(code, file, &two()).unwrap()
}

#[test]
fn injects_a_require_header_and_rewrites_refs_to_live_f_reads() {
    assert_eq!(
        relink(
            "var x = lib_aaaa();\nvar y = lib_bbbb().default;\n",
            "core/deep/app.js"
        ),
        "const lib_aaaa = require(\"../../lib_aaaa.js\");\nconst lib_bbbb = require(\"../../pkg/axios.js\");\nvar x = lib_aaaa.f();\nvar y = lib_bbbb.f().default;\n"
    );
}

#[test]
fn computes_dot_slash_paths_from_the_output_root() {
    assert_eq!(
        relink("var z = lib_aaaa();\n", "runtime-part.js"),
        "const lib_aaaa = require(\"./lib_aaaa.js\");\nvar z = lib_aaaa.f();\n"
    );
}

#[test]
fn a_locally_bound_factory_id_is_not_relinked() {
    let code = "function f(lib_aaaa) { return lib_aaaa(); }\n";
    assert_eq!(relink(code, "a.js"), code);
    // A catch param binds in its clause; a class declaration binds the
    // name in the program scope (the call before it is still bound).
    let code = "try {} catch (lib_aaaa) { lib_aaaa(); } lib_bbbb(); class lib_bbbb {}\n";
    assert_eq!(relink(code, "a.js"), code);
}

#[test]
fn a_block_function_is_block_scoped_in_babel() {
    // Babel keeps `function` in a block BLOCK-scoped (oxc hoists it), so
    // the call after the block is free.
    assert_eq!(
        relink("if (a) { function lib_aaaa() {} } lib_aaaa();\n", "a.js"),
        "const lib_aaaa = require(\"./lib_aaaa.js\");\nif (a) { function lib_aaaa() {} } lib_aaaa.f();\n"
    );
}

#[test]
fn each_factory_is_required_once() {
    assert_eq!(
        relink("lib_aaaa(); lib_aaaa(); lib_aaaa();\n", "a.js"),
        "const lib_aaaa = require(\"./lib_aaaa.js\");\nlib_aaaa.f(); lib_aaaa.f(); lib_aaaa.f();\n"
    );
}

#[test]
fn the_directive_prologue_stays_first() {
    assert_eq!(
        relink("\"use strict\";\nvar x = lib_aaaa();\n", "a.js"),
        "\"use strict\";\nconst lib_aaaa = require(\"./lib_aaaa.js\");\nvar x = lib_aaaa.f();\n"
    );
    // The LAST directive's end, semicolon-less directives included.
    assert_eq!(
        relink("\"use strict\"\n\"x\";lib_aaaa()", "a.js"),
        "\"use strict\"\n\"x\";\nconst lib_aaaa = require(\"./lib_aaaa.js\");lib_aaaa.f()"
    );
}

#[test]
fn no_reference_returns_the_code_unchanged() {
    for code in ["var x = 1 + 2;\n", "", "/* c */"] {
        assert_eq!(relink(code, "a.js"), code);
    }
}

#[test]
fn babel_reference_positions_only() {
    // Assignment targets (destructuring included) are constant violations
    // in Babel, not references; member properties and keys are not
    // references; update targets and for-in heads ARE. A shorthand VALUE is
    // a reference too — it is EXPANDED (finding #29), see below.
    assert_eq!(
        relink(
            "lib_aaaa = 1; o.lib_aaaa; ({ lib_aaaa: 1 }); lib_bbbb++;\n",
            "a.js"
        ),
        "const lib_bbbb = require(\"./pkg/axios.js\");\nlib_aaaa = 1; o.lib_aaaa; ({ lib_aaaa: 1 }); lib_bbbb.f++;\n"
    );
    assert_eq!(
        relink(
            "for (lib_aaaa in o); [lib_bbbb] = x; ({lib_aaaa} = y); ({lib_bbbb});\n",
            "a.js"
        ),
        "const lib_aaaa = require(\"./lib_aaaa.js\");\nconst lib_bbbb = require(\"./pkg/axios.js\");\nfor (lib_aaaa.f in o); [lib_bbbb] = x; ({lib_aaaa} = y); ({lib_bbbb: lib_bbbb.f});\n"
    );
}

#[test]
fn a_shorthand_property_value_is_expanded_not_broken() {
    // Finding #29: splicing `.f` after a shorthand value wrote
    // `({lib_aaaa.f})`, a syntax error. The shorthand is expanded so the
    // key keeps its name and the value reads the thunk.
    let out = relink("var o = ({lib_aaaa});\n", "a.js");
    assert_eq!(
        out,
        "const lib_aaaa = require(\"./lib_aaaa.js\");\nvar o = ({lib_aaaa: lib_aaaa.f});\n"
    );
    let allocator = oxc_allocator::Allocator::default();
    assert!(super::parse_or_err(&allocator, &out).is_ok(), "{out}");
    // Mixed with other properties and a default-valued pattern elsewhere.
    let out = relink("f({a, lib_bbbb, b: lib_aaaa});\n", "a.js");
    assert_eq!(
        out,
        "const lib_aaaa = require(\"./lib_aaaa.js\");\nconst lib_bbbb = require(\"./pkg/axios.js\");\nf({a, lib_bbbb: lib_bbbb.f, b: lib_aaaa.f});\n"
    );
}

#[test]
fn a_leading_comment_keeps_the_header_at_the_first_statement() {
    assert_eq!(
        relink("// hi\nlib_aaaa();\n", "a.js"),
        "// hi\n\nconst lib_aaaa = require(\"./lib_aaaa.js\");lib_aaaa.f();\n"
    );
}

#[test]
fn wraps_a_factory_body_on_a_stable_exports_f() {
    let out = wrap_extracted_factory(
        "(exports, module) => { module.exports = 42; }",
        "lib_bbbb.js",
        &lookup(&[("lib_aaaa", "lib_aaaa.js")]),
    )
    .unwrap();
    assert_eq!(
        out,
        "const { __commonJS } = require(\"./.humanify/__bun-runtime.js\");\nexports.f = __commonJS((exports, module) => { module.exports = 42; });\n"
    );
}

#[test]
fn a_wrapped_body_gets_its_cross_module_requires_first() {
    let out = wrap_extracted_factory(
        "  (exports, module) => { module.exports = lib_aaaa() + 1; }\n",
        "sub/lib_cccc.js",
        &lookup(&[("lib_aaaa", "lib_aaaa.js")]),
    )
    .unwrap();
    assert_eq!(
        out,
        "const lib_aaaa = require(\"../lib_aaaa.js\");\nconst { __commonJS } = require(\"../.humanify/__bun-runtime.js\");\nexports.f = __commonJS((exports, module) => { module.exports = lib_aaaa.f() + 1; });\n"
    );
}

#[test]
fn a_parse_error_is_an_error_not_a_silent_pass() {
    // Babel's parseSync throws; the TS pass fails ("Post-split step failed").
    assert!(relink_factory_references("var = lib_aaaa(;\n", "a.js", &two()).is_err());
}

#[test]
fn the_runtime_text_is_the_ts_constant() {
    assert!(BUN_RELINK_RUNTIME.starts_with("// Bun CJS/ESM factory helpers"));
    assert!(BUN_RELINK_RUNTIME.ends_with("module.exports = { __commonJS, __esm };\n"));
}
