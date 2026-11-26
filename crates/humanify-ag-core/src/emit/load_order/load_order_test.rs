//! Ported from src/split/load-order.test.ts, plus the Babel-shape cases
//! the oxc walk has to translate (parens, chains, meta properties, labels,
//! private names).

use humanify_ag_testkit::strings;
use std::collections::HashSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, Statement};

use super::{
    LoadOrderFacts, LoadOrderOptions, analyze_load_order, bundle_load_order_facts,
    order_respecting_load_order,
};
use crate::ingest::Ingest;

fn facts_with(src: &str, pure: &[&str]) -> Vec<LoadOrderFacts> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, src, "t.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let opts = LoadOrderOptions {
        pure_call_names: pure.iter().map(|s| s.to_string()).collect(),
        target_writing_call_names: HashSet::new(),
    };
    analyze_load_order(&ingest.program.body, &opts)
}

fn facts(src: &str) -> Vec<LoadOrderFacts> {
    facts_with(src, &[])
}

fn bundle_facts(src: &str) -> Vec<LoadOrderFacts> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, src, "t.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let json = crate::ingest::program_estree_json(ingest.program);
    let body = json["body"].as_array().expect("body");
    let lazy = crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild.lazy_init_helpers(body);
    bundle_load_order_facts(&ingest.program.body, ingest.semantic(), &lazy, false)
}

#[test]
fn a_function_declaration_is_hoisted_with_no_reads_writes_or_effects() {
    let f = &facts("function render(x) { return helper(x) + sideEffect(); }")[0];
    assert!(f.hoisted);
    assert!(!f.effects);
    assert!(f.reads.is_empty() && f.writes.is_empty());
}

#[test]
fn a_literal_declaration_is_a_pure_write() {
    let f = &facts("var TAG = \"[enforce]\";")[0];
    assert!(!f.hoisted && !f.effects);
    assert_eq!(f.writes, strings(&["TAG"]));
    assert!(f.reads.is_empty());
}

#[test]
fn a_function_body_is_not_read_but_an_initializer_is() {
    let f = &facts("var run = () => { return moduleState + other(); };")[0];
    assert!(!f.effects);
    assert_eq!(f.writes, strings(&["run"]));
    assert!(f.reads.is_empty());
    let g = &facts("var derived = base.field;")[0];
    assert_eq!(g.writes, strings(&["derived"]));
    assert_eq!(g.reads, strings(&["base"]));
}

#[test]
fn an_unknown_call_is_an_effect_and_a_verified_pure_one_is_not() {
    assert!(facts("var conn = openSocket();")[0].effects);
    let f = &facts_with(
        "var cfg = lazyInitializer(() => buildConfig(globalSettings));",
        &["lazyInitializer"],
    )[0];
    assert!(!f.effects);
    assert_eq!(f.writes, strings(&["cfg"]));
    assert_eq!(f.reads, strings(&["lazyInitializer"]));
}

#[test]
fn the_pure_wrapper_is_recognised_through_the_sequence_callee_form() {
    let f = &facts_with(
        "var cfg = (0, resourceLifecycle.lazyInitializer)(() => build());",
        &["lazyInitializer"],
    )[0];
    assert!(!f.effects);
    assert_eq!(f.reads, strings(&["resourceLifecycle"]));
    let g = &facts_with(
        "var cfg = (0, resourceLifecycle.runNow)(() => build());",
        &["lazyInitializer"],
    )[0];
    assert!(g.effects);
}

#[test]
fn bare_var_is_free_and_bare_let_is_a_write() {
    let f = &facts("var pending, queued;")[0];
    assert!(!f.effects);
    assert!(f.writes.is_empty());
    assert_eq!(facts("let pending;")[0].writes, strings(&["pending"]));
}

#[test]
fn registration_calls_assignments_and_spreads() {
    let f = &facts("defineModuleExports(memoryModule, { get: () => x });")[0];
    assert!(f.effects);
    assert!(f.reads.contains(&"memoryModule".to_string()));
    let g = &facts("cachedValue = 1;")[0];
    assert!(!g.effects);
    assert_eq!(g.writes, strings(&["cachedValue"]));
    assert!(facts("target.field = 1;")[0].effects);
    assert!(facts("var merged = { ...source };")[0].effects);
}

#[test]
fn classes_and_control_flow() {
    let f = &facts("class Panel extends Base { render() { return sideFx(); } }")[0];
    assert!(!f.effects);
    assert_eq!(f.writes, strings(&["Panel"]));
    assert_eq!(f.reads, strings(&["Base"]));
    assert!(facts("class Registry { static all = collectAll(); }")[0].effects);
    assert!(facts("if (flag) { boot(); }")[0].effects);
    assert!(facts("try { boot(); } catch {}")[0].effects);
    assert!(facts("for (var i = 0; i < 3; i++) tick();")[0].effects);
}

#[test]
fn babel_identifiers_the_oxc_shape_hides_are_reads() {
    // A label, a catch param and a nested declarator id are all Babel
    // Identifier nodes the generic walk reaches.
    let f = &facts("outer: for (var k of list) { break outer; }")[0];
    assert_eq!(f.reads, strings(&["outer", "k", "list"]));
    let g = &facts("try { a(); } catch (err) { b(err); }")[0];
    assert_eq!(g.reads, strings(&["a", "err", "b"]));
}

#[test]
fn parens_and_optional_chains_are_transparent() {
    let f = &facts_with("var x = (lazy)(() => 1);", &["lazy"])[0];
    assert!(!f.effects);
    let g = &facts_with("var y = ns?.lazy(() => 1);", &["lazy"])[0];
    assert!(!g.effects);
    assert_eq!(g.reads, strings(&["ns"]));
}

#[test]
fn destructuring_targets_read_and_destructuring_declarations_are_effects() {
    let f = &facts("({ a, b: c } = src);")[0];
    assert!(f.effects);
    assert_eq!(f.reads, strings(&["a", "c", "src"]));
    let g = &facts("var { p, q } = obj;")[0];
    assert!(g.effects);
    assert_eq!(g.writes, strings(&["p", "q"]));
}

const HELPER: &str = "var qz = (gen, cached) => () => (gen && (cached = gen(gen = 0)), cached);";

#[test]
fn the_lazy_init_helper_is_verified_by_shape_not_by_name() {
    let src = format!("{HELPER}\nvar mod = qz(() => loadHeavyThing());");
    let f = bundle_facts(&src);
    assert!(!f[1].effects);
    assert_eq!(f[1].writes, strings(&["mod"]));
    let src2 = "var qz = makeThing;\nvar mod = qz(() => loadHeavyThing());";
    assert!(bundle_facts(src2)[1].effects);
}

/// Unminified esbuild's lazy-init helper (`__esm`, real 0.27.2 output,
/// formatted): the same helper the fossil layout reads, so the load order
/// admits its calls too (toolchain review R12, spec I27 — one owner).
const ESBUILD_HELPER: &str = "var __esm = (fn, res) => function __init() {\n  return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;\n};";

#[test]
fn esbuilds_lazy_init_helper_is_admitted_by_the_same_shape() {
    let src = format!(
        "{ESBUILD_HELPER}\nvar init_mod = __esm({{ \"src/mod.js\"() {{ loadHeavyThing(); }} }});"
    );
    let f = bundle_facts(&src);
    assert!(!f[1].effects);
    assert_eq!(f[1].writes, strings(&["init_mod"]));
}

const REGISTRAR: &str = "var defineModuleExports = (targetObject, sourceObject) => {
    for (var propKey in sourceObject) defineProperty(targetObject, propKey, {
      get: sourceObject[propKey],
      enumerable: true,
      configurable: true,
      set: BoundIdentityProperty.bind(sourceObject, propKey)
    });
  };";

#[test]
fn the_export_registrar_writes_its_target_and_is_not_a_barrier() {
    let src = format!(
        "{REGISTRAR}\nvar exportsObj = {{}};\ndefineModuleExports(exportsObj, {{ foo: () => 1 }});"
    );
    let f = bundle_facts(&src);
    assert!(!f[2].effects);
    assert!(f[2].writes.contains(&"exportsObj".to_string()));
    assert!(!f[2].reads.contains(&"exportsObj".to_string()));
    let got = order_respecting_load_order(&[0, 1, 2], &[2, 1, 0], &f);
    let pos = |x: usize| got.iter().position(|&s| s == x).unwrap();
    assert!(
        pos(1) < pos(2),
        "declaration must precede registration: {got:?}"
    );
}

#[test]
fn a_load_time_reader_of_the_registrar_target_stays_after_it() {
    let src = format!(
        "{REGISTRAR}\nvar exportsObj = {{}};\ndefineModuleExports(exportsObj, {{ foo: () => 1 }});\nvar copy = exportsObj.foo;"
    );
    let f = bundle_facts(&src);
    let got = order_respecting_load_order(&[0, 1, 2, 3], &[3, 2, 1, 0], &f);
    let pos = |x: usize| got.iter().position(|&s| s == x).unwrap();
    assert!(pos(2) < pos(3), "{got:?}");
}

#[test]
fn an_unrelated_call_stays_a_barrier() {
    let src = format!(
        "{REGISTRAR}\nvar exportsObj = {{}};\ndoSomethingElse(exportsObj, {{ foo: () => 1 }});"
    );
    assert!(bundle_facts(&src)[2].effects);
}

fn order(src: &str, desired: &[usize], pure: &[&str]) -> Vec<usize> {
    let f = facts_with(src, pure);
    let slots: Vec<usize> = (0..f.len()).collect();
    order_respecting_load_order(&slots, desired, &f)
}

#[test]
fn pure_declarations_permute_freely() {
    let src = "var a = \"one\";\nvar b = 2;\nvar c = () => {};\nvar d = /re/;";
    for desired in [[3, 2, 1, 0], [1, 3, 0, 2], [2, 0, 3, 1]] {
        assert_eq!(order(src, &desired, &[]), desired.to_vec());
    }
}

#[test]
fn dependences_and_barriers_hold() {
    assert_eq!(
        order("var total = 5;\nvar doubled = total * 2;", &[1, 0], &[]),
        vec![0, 1]
    );
    assert_eq!(
        order("var view = state;\nvar state = compute;", &[1, 0], &[]),
        vec![0, 1]
    );
    assert_eq!(
        order("counter = 1;\ncounter = 2;", &[1, 0], &[]),
        vec![0, 1]
    );
    assert_eq!(order("boot();\nshutdown();", &[1, 0], &[]), vec![0, 1]);
    let crash = "var mod = {};\ndefineModuleExports(mod, { get: () => v });\nvar tail = \"t\";";
    assert_eq!(order(crash, &[2, 1, 0], &[]), vec![0, 1, 2]);
    assert_eq!(order(crash, &[1, 0, 2], &[]), vec![0, 1, 2]);
}

#[test]
fn a_hoisted_function_crosses_a_barrier() {
    let src = "var mod = {};\ndefineModuleExports(mod, {});\nfunction tail() {}";
    assert_eq!(order(src, &[2, 0, 1], &[]), vec![2, 0, 1]);
}

#[test]
fn a_bare_var_crosses_a_barrier() {
    // `var b;` does nothing when the module loads: it may sit after the
    // call it preceded in the bundle.
    assert_eq!(
        order("var b;\nsideEffect();\nvar c = 1;", &[1, 2, 0], &[]),
        [1, 2, 0]
    );
    // A `let` has a dead zone, so its place is observable: it stays.
    assert_eq!(
        order("let b;\nsideEffect();\nvar c = 1;", &[1, 2, 0], &[]),
        [0, 1, 2]
    );
}

#[test]
fn pure_lazy_init_declarations_move() {
    let src = "var first = lazyInitializer(() => a());\nvar second = lazyInitializer(() => b());\nvar third = lazyInitializer(() => c());";
    assert_eq!(order(src, &[2, 0, 1], &["lazyInitializer"]), vec![2, 0, 1]);
}

#[test]
fn always_a_permutation_that_crosses_no_barrier() {
    let src = "var mod = {};\nfunction f() {}\ndefineModuleExports(mod, {});\nvar lit = \"x\";\nvar derived = lit + mod;\nfunction g() {}";
    let f = facts(src);
    let slots: Vec<usize> = (0..f.len()).collect();
    // The TS's seeded shuffler (an LCG), 50 trials.
    let mut state: u64 = 12345;
    for _ in 0..50 {
        let mut desired = slots.clone();
        for i in (1..desired.len()).rev() {
            state = (state * 1_103_515_245 + 12_345) % 2_147_483_648;
            let j = ((state as f64 / 2_147_483_648.0) * (i + 1) as f64).floor() as usize;
            desired.swap(i, j);
        }
        let out = order_respecting_load_order(&slots, &desired, &f);
        let mut sorted = out.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, slots);
        let pos = |x: usize| out.iter().position(|&s| s == x).unwrap();
        for &b in &out {
            if !f[b].effects {
                continue;
            }
            for &s in &out {
                if s != b && !f[s].hoisted {
                    assert_eq!(s < b, pos(s) < pos(b), "{s} crossed barrier {b}");
                }
            }
        }
    }
}

#[test]
fn a_declaration_that_runs_no_code_is_a_write_and_no_barrier() {
    let all = bundle_facts(
        "var a = Symbol('a'); var b = Object.freeze({ k: [1] }); var c = new Set(['x']); class D extends Error { static n = 1; } var e = /x/;",
    );
    for (f, name) in all.iter().zip(["a", "b", "c", "D", "e"]) {
        assert!(!f.effects, "{name}");
        assert!(f.reads.is_empty(), "{name}: {:?}", f.reads);
        assert_eq!(f.writes, strings(&[name]));
    }
}

#[test]
fn a_built_in_name_the_bundle_binds_is_a_call_like_any_other() {
    let all = bundle_facts("function Symbol(x) { return x; } var a = Symbol('a');");
    assert!(all[1].effects);
}

#[test]
fn a_require_of_a_node_built_in_is_a_write_and_no_barrier() {
    let all = bundle_facts(
        "var a = require('path'); var b = require('node:fs/promises'); var c = { default: require('os') }; var d = require('left-pad'); var e = require(a);",
    );
    for (f, name) in all.iter().zip(["a", "b", "c"]) {
        assert!(!f.effects, "{name}");
        assert_eq!(f.writes, strings(&[name]));
    }
    assert!(all[3].effects);
    assert!(all[4].effects);
}

#[test]
fn a_require_the_bundle_binds_is_a_call_like_any_other() {
    let all = bundle_facts("function require(x) { return x; } var a = require('path');");
    assert!(all[1].effects);
}

#[test]
fn a_wrapper_parameter_named_require_is_the_programs_require() {
    let src =
        "(function (exports, require) { var a = require('path'); var b = require('left-pad'); });";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, src, "t.js");
    let Statement::ExpressionStatement(e) = &ingest.program.body[0] else {
        panic!("an expression statement");
    };
    let Expression::FunctionExpression(f) = crate::babel_view::unparen(&e.expression) else {
        panic!("a function");
    };
    let body = &f.body.as_ref().expect("a body").statements;
    let lazy = HashSet::new();
    let all = bundle_load_order_facts(body, ingest.semantic(), &lazy, false);
    assert!(!all[0].effects);
    assert!(all[1].effects);
}
