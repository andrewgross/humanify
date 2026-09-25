//! TS original: `src/rename/function-bindings.test.ts` (its 25 cases).

use oxc_allocator::Allocator;

use super::{
    build_owned_binding_map, collect_owned_binding_infos, collect_shadowed_block_bindings,
};
use crate::graph::{Eligibility, build_unified_graph_with_eligibility};
use crate::ingest::Ingest;
use crate::rename::transfer::rows::{FnRow, Rows};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::Anchor;

/// Parse `code` (an ES module, as the TS fixtures), build the graph and the
/// rename state, and hand over the function named `name` (the first
/// function in source order when None).
fn with_fn<R>(code: &str, name: Option<&str>, f: impl FnOnce(&mut RenameState, &FnRow) -> R) -> R {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.mjs");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let semantic = ingest.semantic();
    let graph = build_unified_graph_with_eligibility(
        semantic,
        ingest.program,
        "input.js",
        &[],
        Eligibility::All,
    );
    let mut state = RenameState::new(semantic, Anchor::Fresh);
    let rows = Rows::build(&graph, semantic, state.view());
    let row = match name {
        Some(n) => rows
            .fns
            .iter()
            .find(|r| {
                r.id_symbol
                    .and_then(|s| state.name_of_symbol(s))
                    .is_some_and(|x| x == n)
            })
            .unwrap_or_else(|| panic!("no function named {n}")),
        None => rows
            .fns
            .iter()
            .min_by_key(|r| r.span.start)
            .expect("a function"),
    }
    .clone();
    f(&mut state, &row)
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn info_names(state: &RenameState, row: &FnRow) -> Vec<String> {
    collect_owned_binding_infos(state, row)
        .into_iter()
        .map(|i| i.name)
        .collect()
}

/// Babel's `fnPath.scope.rename(old, new)` in the TS fixtures — an
/// unvalidated rename there; these fixtures' renames all validate.
fn rename_in_fn(state: &mut RenameState, row: &FnRow, old: &str, new: &str) {
    let attempt = state.attempt_validated_rename(
        RenameRequest {
            scope: row.scope,
            old_name: old,
            new_name: new,
            expected: None,
        },
        TrailSpec::Untrailed { why: "fixture" },
    );
    assert!(attempt.applied, "{old}→{new}: {attempt:?}");
}

// ── collectOwnedBindingInfos ────────────────────────────────────────────

#[test]
fn collects_params_vars_lets_and_the_own_name() {
    with_fn(
        "function foo(a, b) { var c = 1; let d = 2; return a + b + c + d; }",
        Some("foo"),
        |s, r| assert_eq!(sorted(info_names(s, r)), ["a", "b", "c", "d", "foo"]),
    );
}

#[test]
fn excludes_nested_function_declaration_names() {
    with_fn(
        "function outer(x) { function inner(y) { return y; } return inner(x); }",
        Some("outer"),
        |s, r| assert_eq!(sorted(info_names(s, r)), ["outer", "x"]),
    );
}

#[test]
fn a_nested_function_names_itself_in_its_own_pass() {
    with_fn(
        "function outer(x) { function inner(y) { return y; } return inner(x); }",
        Some("inner"),
        |s, r| assert_eq!(sorted(info_names(s, r)), ["inner", "y"]),
    );
}

#[test]
fn includes_a_named_function_expression_self_name() {
    with_fn(
        "const f = function orig(n) { return n > 0 ? orig(n - 1) : 0; };",
        Some("orig"),
        |s, r| assert_eq!(sorted(info_names(s, r)), ["n", "orig"]),
    );
}

#[test]
fn collects_arrow_bindings_without_a_self_name() {
    with_fn(
        "const f = (a) => { let b = a; return b; };",
        None,
        |s, r| {
            assert_eq!(sorted(info_names(s, r)), ["a", "b"]);
        },
    );
}

#[test]
fn collects_nested_block_bindings_with_their_owning_scope() {
    let code = "function foo(x) {
      if (x) { let e = 1; use(e); }
      for (const i of x) { use(i); }
      try { go(); } catch (err) { log(err); }
      switch (x) { case 1: { let s = 2; use(s); } }
    }";
    with_fn(code, Some("foo"), |s, r| {
        let infos = collect_owned_binding_infos(s, r);
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        for expected in ["e", "i", "err", "s"] {
            assert!(names.contains(&expected), "missing {expected} in {names:?}");
        }
        for info in &infos {
            assert_eq!(s.binding_in(info.scope, &info.name), Some(info.binding));
        }
    });
}

#[test]
fn collects_bindings_when_params_have_defaults() {
    with_fn(
        "function foo(a = 1) { var b = 2; return a + b; }",
        Some("foo"),
        |s, r| assert_eq!(sorted(info_names(s, r)), ["a", "b", "foo"]),
    );
}

#[test]
fn keeps_only_the_first_same_named_sibling_block_binding() {
    with_fn(
        "function f() { { let e = 1; use(e); } { let e = 2; use2(e); } }",
        Some("f"),
        |s, r| {
            assert_eq!(info_names(s, r).iter().filter(|n| *n == "e").count(), 1);
        },
    );
}

#[test]
fn excludes_a_nested_fn_decl_name_with_default_params() {
    with_fn(
        "function foo(a = 1) { function inner(y) { return y; } return inner(a); }",
        Some("foo"),
        |s, r| assert_eq!(sorted(info_names(s, r)), ["a", "foo"]),
    );
}

#[test]
fn excludes_a_block_scoped_nested_fn_decl_name() {
    with_fn(
        "function foo(x) { if (x) { function g() { return 1; } return g(); } return 0; }",
        Some("foo"),
        |s, r| assert!(!info_names(s, r).contains(&"g".to_string())),
    );
}

#[test]
fn does_not_collect_bindings_owned_by_nested_functions() {
    with_fn(
        "function outer(x) { const g = (z) => { let w = z; return w; }; return g(x); }",
        Some("outer"),
        |s, r| assert_eq!(sorted(info_names(s, r)), ["g", "outer", "x"]),
    );
}

// ── buildOwnedBindingMap ────────────────────────────────────────────────

fn map_names(state: &RenameState, row: &FnRow) -> Vec<String> {
    build_owned_binding_map(state, row)
        .into_iter()
        .map(|(n, _)| n)
        .collect()
}

#[test]
fn the_transfer_map_carries_nested_fn_decl_names() {
    with_fn(
        "function outer(x) { function inner(y) { return y; } return inner(x); }",
        Some("outer"),
        |s, r| {
            let map = build_owned_binding_map(s, r);
            let (_, scope) = map.iter().find(|(n, _)| n == "inner").expect("inner");
            assert!(s.binding_in(*scope, "inner").is_some());
        },
    );
}

#[test]
fn the_transfer_map_carries_the_nfe_self_name() {
    with_fn(
        "const f = function orig(n) { return n > 0 ? orig(n - 1) : 0; };",
        Some("orig"),
        |s, r| {
            let map = build_owned_binding_map(s, r);
            let (_, scope) = map.iter().find(|(n, _)| n == "orig").expect("orig");
            assert!(s.binding_in(*scope, "orig").is_some());
        },
    );
}

#[test]
fn every_name_maps_to_the_scope_owning_its_binding() {
    let code = "function foo(a, b = 2) {
      var c = 1;
      let d = 2;
      if (a) { var hoisted = 3; let blockLocal = 4; use(blockLocal); }
      for (const i of b) { use(i); }
      try { go(); } catch (err) { log(err); }
      function inner(y) { return y; }
      return inner(a + c + d + hoisted);
    }";
    with_fn(code, Some("foo"), |s, r| {
        let map = build_owned_binding_map(s, r);
        for expected in [
            "a",
            "b",
            "c",
            "d",
            "hoisted",
            "blockLocal",
            "i",
            "err",
            "inner",
            "foo",
        ] {
            let (_, scope) = map
                .iter()
                .find(|(n, _)| n == expected)
                .unwrap_or_else(|| panic!("missing {expected}"));
            assert!(s.binding_in(*scope, expected).is_some(), "{expected}");
        }
    });
}

#[test]
fn the_transfer_map_keeps_a_nested_fn_decl_with_default_params() {
    with_fn(
        "function foo(a = 1) { function inner(y) { return y; } return inner(a); }",
        Some("foo"),
        |s, r| assert!(map_names(s, r).contains(&"inner".to_string())),
    );
}

#[test]
fn the_transfer_map_keeps_the_first_sibling_block_scope() {
    with_fn(
        "function f() { { let e = 1; use(e); } { let e = 2; use2(e); } }",
        Some("f"),
        |s, r| assert_eq!(sorted(map_names(s, r)), ["e", "f"]),
    );
}

// ── collectShadowedBlockBindings ────────────────────────────────────────

fn eligible(name: &str) -> bool {
    name.len() <= 2
}

fn shadowed(state: &RenameState, row: &FnRow) -> Vec<String> {
    collect_shadowed_block_bindings(state, row, eligible)
        .into_iter()
        .map(|i| i.name)
        .collect()
}

#[test]
fn finds_a_catch_binding_that_shadows_a_parameter() {
    with_fn(
        "function f(t) { try { } catch(t) { console.log(t); } }",
        None,
        |s, r| {
            rename_in_fn(s, r, "t", "component");
            assert_eq!(shadowed(s, r), ["t"]);
        },
    );
}

#[test]
fn finds_for_loop_consts_that_shadow_parameters() {
    let code = "function f(o, r) {
      for (let i = 0; i < 10; i++) {
        const o = compute(i);
        const r = transform(o);
        emit(r);
      }
      return o + r;
    }";
    with_fn(code, None, |s, row| {
        rename_in_fn(s, row, "o", "currentIndexOffset");
        rename_in_fn(s, row, "r", "transformedValue");
        let names = shadowed(s, row);
        assert!(names.contains(&"o".to_string()), "{names:?}");
        assert!(names.contains(&"r".to_string()), "{names:?}");
    });
}

#[test]
fn finds_an_if_block_let_that_shadows_a_parameter() {
    let code = "function f(x) {
      let y = x * 2;
      if (y > 10) {
        let x = transform(y);
        use(x);
      }
      return y;
    }";
    with_fn(code, None, |s, r| {
        rename_in_fn(s, r, "x", "input");
        assert!(shadowed(s, r).contains(&"x".to_string()));
    });
}

#[test]
fn reports_sibling_blocks_reusing_a_name_each() {
    let code = "function f(a) {
      if (a.length > 0) { let r = a[0]; process(r); }
      if (a.length > 1) { let r = a[1]; process(r); }
    }";
    with_fn(code, None, |s, row| {
        assert_eq!(shadowed(s, row).iter().filter(|n| *n == "r").count(), 2);
    });
}

#[test]
fn finds_switch_case_block_bindings() {
    let code = "function f(n) {
      switch(n) {
        case 1: { let r = computeA(); return r; }
        case 2: { let r = computeB(); return r; }
      }
    }";
    with_fn(code, None, |s, row| {
        assert_eq!(shadowed(s, row).iter().filter(|n| *n == "r").count(), 2);
    });
}

#[test]
fn finds_multiple_catch_bindings() {
    let code = "function f(e) {
      try { a(); } catch(e) { log(e); }
      try { b(); } catch(e) { log(e); }
    }";
    with_fn(code, None, |s, row| {
        rename_in_fn(s, row, "e", "error");
        assert_eq!(shadowed(s, row).iter().filter(|n| *n == "e").count(), 2);
    });
}

#[test]
fn skips_descriptive_names() {
    with_fn(
        "function f(t) { try { } catch(error) { console.log(error); } }",
        None,
        |s, r| {
            rename_in_fn(s, r, "t", "component");
            assert!(shadowed(s, r).is_empty());
        },
    );
}

#[test]
fn does_not_descend_into_nested_functions() {
    with_fn(
        "function f(t) { try { } catch(t) {} function g(x) { try { } catch(x) {} } }",
        None,
        |s, r| {
            rename_in_fn(s, r, "t", "component");
            assert_eq!(shadowed(s, r), ["t"]);
        },
    );
}
