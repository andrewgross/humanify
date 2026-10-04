//! The bundle carry finds the wrapper body through the run's bundle
//! layout (review R9, 2026-10-04) — never by "the first function whose
//! body has exactly N statements", the rule it used to carry its own.

use humanify_model::js::JsValue;
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{ArrowFunctionBody, FunctionBody};
use oxc_semantic::Semantic;
use oxc_span::GetSpan;

use super::{carry_renames_into_bundle, wrapper_body};
use crate::finish::reconcile::PostSplitRename;
use crate::toolchain::BundleLayout;

const LAYOUT: BundleLayout = BundleLayout::SingleWrapperFunction;

/// The RETIRED rule, kept here as the oracle the layout must agree with on
/// a Bun-shaped bundle: the first function (pre-order) whose block body
/// holds exactly `expected` statements.
fn retired_first_function_with_n_statements(
    semantic: &Semantic<'_>,
    expected: usize,
) -> Option<Vec<(u32, u32)>> {
    let spans = |body: &FunctionBody<'_>| -> Vec<(u32, u32)> {
        body.statements
            .iter()
            .map(|s| (s.span().start, s.span().end))
            .collect()
    };
    for node in semantic.nodes().iter() {
        let body = match node.kind() {
            AstKind::Function(f) => f.body.as_deref(),
            AstKind::ArrowFunctionExpression(a) => match &a.body {
                ArrowFunctionBody::FunctionBody(b) => Some(&**b),
                _ => None,
            },
            _ => None,
        };
        if let Some(body) = body
            && body.statements.len() == expected
        {
            return Some(spans(body));
        }
    }
    None
}

/// A Bun-shaped runtime: the bytecode CommonJS wrapper (a BARE function
/// expression, not called, after the `// @bun` banner) holding the whole
/// app — module-level vars, a lazy-init helper, inner functions and
/// arrows of assorted statement counts, a class — at well over the 50
/// wrapper-scope names the layout's gate asks for.
fn bun_shaped_runtime() -> (String, usize) {
    let mut stmts: Vec<String> = Vec::new();
    stmts.push("var __create = Object.create;".into());
    stmts.push(
        "var __commonJS = (cb, mod) => () => (mod || cb((mod = { exports: {} }).exports, mod), mod.exports);"
            .into(),
    );
    for i in 0..40 {
        stmts.push(format!("var state{i} = {i};"));
    }
    for i in 0..12 {
        // Inner bodies of 1..=4 statements — none the wrapper's count.
        let body: Vec<String> = (0..(i % 4) + 1)
            .map(|k| format!("let local{k} = state{i} + {k};"))
            .collect();
        stmts.push(format!(
            "function helper{i}(arg) {{ {} return arg; }}",
            body.join(" ")
        ));
    }
    stmts.push("const arrow = (a) => { const b = a * 2; return b; };".into());
    stmts.push("class Widget { render() { return helper0(1); } }".into());
    stmts.push("module.exports = { Widget, arrow };".into());
    let n = stmts.len();
    let code = format!(
        "// @bun @bytecode @bun-cjs\n(function(exports, require, module, __filename, __dirname) {{\n{}\n}})\n",
        stmts.join("\n")
    );
    (code, n)
}

/// On Bun the wrapper is the outermost function, so the retired rule found
/// it first: the layout's answer is the SAME statement list, span for span.
#[test]
fn on_a_bun_shaped_runtime_the_layout_and_the_retired_rule_pick_the_same_body() {
    let (code, n) = bun_shaped_runtime();
    let allocator = Allocator::default();
    let ingest = crate::finish::relink::parse_or_err(&allocator, &code).expect("parses");
    let semantic = ingest.semantic();
    let retired = retired_first_function_with_n_statements(semantic, n).expect("retired rule");
    let layout = wrapper_body(LAYOUT, ingest.program, semantic, n).expect("layout rule");
    assert_eq!(layout, retired);
    assert_eq!(layout.len(), n);
    // And it is the wrapper's body, not some inner function's.
    let wrapper = LAYOUT
        .recognize_wrapper(ingest.program, semantic)
        .expect("wrapper");
    assert!(
        layout
            .iter()
            .all(|&(s, e)| s >= wrapper.body_span.start && e <= wrapper.body_span.end)
    );
    assert_eq!(
        layout.first().map(|s| s.0),
        Some(code.find("var __create").unwrap() as u32)
    );
}

fn rename(from: &str, to: &str) -> PostSplitRename {
    PostSplitRename {
        file: "a.js".into(),
        from_name: from.into(),
        to_name: to.into(),
        kind: "local",
        votes: 1,
        top_level: false,
        locator: Some((0, 0)),
    }
}

/// When the wrapper's statement count is NOT the ledger's, the retired
/// rule went on to the next function with that count — here an inner
/// function — and carried the rename into the wrong statement. The layout
/// finds THE wrapper, sees the counts disagree, and abstains with a reason
/// that says so.
#[test]
fn a_wrapper_whose_count_disagrees_with_the_ledger_abstains_instead_of_guessing() {
    let bundle = "(function () {\n  var a = 1;\n  var b = 2;\n  var c = 3;\n  function inner() {\n    var x = 1;\n    return x;\n  }\n})();\n";
    let ledger = JsValue::parse(
        "{\"files\":[\"a.js\"],\"order\":[\"a.js\",\"a.js\"],\"emitIndexes\":[0,1]}",
    )
    .unwrap();
    // The retired rule's guess on this text is the inner function.
    let allocator = Allocator::default();
    let ingest = crate::finish::relink::parse_or_err(&allocator, bundle).unwrap();
    let guess = retired_first_function_with_n_statements(ingest.semantic(), 2).unwrap();
    assert_eq!(
        guess.first().map(|s| &bundle[s.0 as usize..s.1 as usize]),
        Some("var x = 1;")
    );

    let renames = [rename("x", "count")];
    let carry = carry_renames_into_bundle(bundle, &ledger, &renames, LAYOUT).unwrap();
    assert_eq!(carry.code, None);
    assert_eq!(carry.carried, 0);
    assert_eq!(
        carry.abstained,
        vec![("wrapper-statement-count-mismatch".to_string(), 1)]
    );
}

/// No wrapper at all under the layout: the carry abstains as it always
/// has (no fallback to the program body).
#[test]
fn a_bundle_without_a_wrapper_abstains_wrapper_body_not_found() {
    let bundle = "var a = 1;\nvar b = 2;\n";
    let ledger = JsValue::parse(
        "{\"files\":[\"a.js\"],\"order\":[\"a.js\",\"a.js\"],\"emitIndexes\":[0,1]}",
    )
    .unwrap();
    let carry =
        carry_renames_into_bundle(bundle, &ledger, &[rename("a", "alpha")], LAYOUT).unwrap();
    assert_eq!(carry.code, None);
    assert_eq!(
        carry.abstained,
        vec![("wrapper-body-not-found".to_string(), 1)]
    );
}
