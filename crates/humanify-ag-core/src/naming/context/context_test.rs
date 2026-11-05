//! src/rename/context-builder.test.ts ported at the VIEW level (its two
//! fixtures are about which babel scopes feed usedIdentifiers — here the
//! view carries those scopes, and the order/union rule is what is tested),
//! plus the builder's own rules, each pinned to the TS source line.

use super::*;

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn view() -> ContextView {
    ContextView {
        callees: vec![],
        scope_chain: vec![],
        program_bindings: vec![],
        program_globals: vec![],
        parent_bindings: None,
    }
}

#[test]
fn used_identifiers_include_free_names_ancestors_and_own_bindings() {
    // "var top1 = 1; function f(p) { return myAppGlobal.title + p + top1; }"
    let v = ContextView {
        scope_chain: vec![strs(&["p"])],
        program_bindings: strs(&["top1", "f"]),
        program_globals: strs(&["myAppGlobal"]),
        ..view()
    };
    let ctx = build_context(&v, &[], |_| true);
    for n in ["myAppGlobal", "top1", "p"] {
        assert!(ctx.used_identifiers.iter().any(|u| u == n), "{n}");
    }
}

#[test]
fn used_identifiers_are_an_ordered_union_inner_to_outer_then_globals() {
    let v = ContextView {
        scope_chain: vec![strs(&["b", "a"]), strs(&["c", "a"])],
        program_bindings: strs(&["z", "b", "y"]),
        program_globals: strs(&["g", "z"]),
        ..view()
    };
    let ctx = build_context(&v, &[], |_| true);
    assert_eq!(ctx.used_identifiers, strs(&["b", "a", "c", "z", "y", "g"]));
}

fn callee(
    node_type: &str,
    id: Option<&str>,
    decl: Option<&str>,
    params: Vec<ParamView>,
) -> CalleeView {
    CalleeView {
        node_type: node_type.into(),
        id: id.map(Into::into),
        declarator_id: decl.map(Into::into),
        params,
        body_code: "{\n  one;\n  two;\n  three;\n  four;\n}".into(),
    }
}

#[test]
fn callee_names_follow_the_ts_precedence() {
    let v = ContextView {
        callees: vec![
            callee("FunctionDeclaration", Some("decl"), Some("ignored"), vec![]),
            callee("FunctionExpression", Some("expr"), Some("ignored"), vec![]),
            callee("FunctionExpression", None, Some("viaDeclarator"), vec![]),
            // an arrow's `id` is never read (only declarations/expressions)
            callee(
                "ArrowFunctionExpression",
                Some("arrowId"),
                Some("arrowVar"),
                vec![],
            ),
            callee("ObjectMethod", None, None, vec![]),
        ],
        ..view()
    };
    let names: Vec<String> = build_context(&v, &[], |_| true)
        .callee_signatures
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(
        names,
        strs(&["decl", "expr", "viaDeclarator", "arrowVar", "anonymous"])
    );
}

#[test]
fn callee_params_and_snippet() {
    let v = ContextView {
        callees: vec![callee(
            "FunctionDeclaration",
            Some("f"),
            None,
            vec![
                ParamView::Identifier { name: "a".into() },
                ParamView::Rest {
                    name: "rest".into(),
                },
                ParamView::Assign { name: "d".into() },
                ParamView::Other {
                    code: "{\n  x,\n  y\n}".into(),
                },
            ],
        )],
        ..view()
    };
    let sig = &build_context(&v, &[], |_| true).callee_signatures[0];
    assert_eq!(sig.params, strs(&["a", "...rest", "d", "{\n  x,\n  y\n}"]));
    // getBodySnippet(body, 3): the first three lines of the generated body
    assert_eq!(sig.snippet.as_deref(), Some("{\n  one;\n  two;"));
}

#[test]
fn callsites_pass_through() {
    let sites = strs(&["a(1)", "b(a)"]);
    assert_eq!(build_context(&view(), &sites, |_| true).callsites, sites);
}

fn binding(name: &str, decl: DeclView) -> ParentBinding {
    ParentBinding {
        name: name.into(),
        decl,
    }
}

#[test]
fn context_vars_only_when_the_parent_is_pending_and_non_empty() {
    assert_eq!(build_context(&view(), &[], |_| true).context_vars, None);
    let empty = ContextView {
        parent_bindings: Some(vec![binding("f", DeclView::FunctionOrClass)]),
        ..view()
    };
    assert_eq!(build_context(&empty, &[], |_| true).context_vars, None);
}

#[test]
fn context_vars_rules() {
    let long = format!("var q = \"{}\";", "x".repeat(111)); // 121 chars
    let astral = format!("var r = \"a{}\";", "😀".repeat(54)); // 9 + 1 + 108 + 2 = 120 UTF-16 units
    let astral_long = format!("var s = \"ab{}\";", "😀".repeat(54)); // 121 units (108 chars)
    let v = ContextView {
        parent_bindings: Some(vec![
            binding("f", DeclView::FunctionOrClass),
            binding(
                "a",
                DeclView::Declarator {
                    code: "  var a = 1,\n    b = 2;".into(),
                },
            ),
            binding("skip", DeclView::Other { code: "x".into() }),
            binding(
                "e",
                DeclView::Other {
                    code: String::new(),
                },
            ),
            binding("q", DeclView::Declarator { code: long }),
            binding("r", DeclView::Declarator { code: astral }),
            binding("s", DeclView::Declarator { code: astral_long }),
            binding(
                "t",
                DeclView::Other {
                    code: "\u{feff}t = 1\u{85}".into(),
                },
            ),
        ]),
        ..view()
    };
    let ctx = build_context(&v, &[], |n| n != "skip");
    let vars = ctx.context_vars.unwrap();
    // first line, JS-trimmed (U+FEFF stripped, U+0085 kept), ≤120 UTF-16
    assert_eq!(vars.len(), 3, "{vars:?}");
    assert_eq!(vars[0], "var a = 1,");
    assert!(vars[1].starts_with("var r"));
    assert_eq!(vars[2], "t = 1\u{85}");
}

#[test]
fn context_vars_stop_at_thirty_pushes() {
    let bindings: Vec<ParentBinding> = (0..40)
        .map(|i| {
            binding(
                &format!("v{i}"),
                DeclView::Declarator {
                    code: format!("var v{i};"),
                },
            )
        })
        .collect();
    let v = ContextView {
        parent_bindings: Some(bindings),
        ..view()
    };
    let vars = build_context(&v, &[], |_| true).context_vars.unwrap();
    assert_eq!(vars.len(), 30);
    assert_eq!(vars[29], "var v29;");
}
