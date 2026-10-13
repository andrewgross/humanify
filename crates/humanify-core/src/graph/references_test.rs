//! The statement reference graph: one test per edge kind
//! (docs/design/library-discovery.md §4), over small synthetic bundles.

use std::collections::HashSet;

use oxc_allocator::Allocator;

use crate::graph::references::{
    BundlerFacts, EdgeKind, ReferenceGraph, UnitKind, build_reference_graph,
};
use crate::ingest::Ingest;

fn names(list: &[&str]) -> HashSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// The graph of `code`'s top-level statements; `factories` are the texts
/// of declarators that are bundled CommonJS modules.
fn graph_of(code: &str, factories: &[&str], facts: &BundlerFacts) -> ReferenceGraph {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let spans: Vec<oxc_span::Span> = factories
        .iter()
        .map(|f| {
            let at = code.find(f).expect("factory text") as u32;
            oxc_span::Span::new(at, at + f.len() as u32)
        })
        .collect();
    build_reference_graph(code, ingest.semantic(), &ingest.program.body, &spans, facts)
}

/// The unit whose text starts with `head` (the innermost one).
fn unit(g: &ReferenceGraph, code: &str, head: &str) -> u32 {
    let found: Vec<u32> = (0..g.units.len() as u32)
        .filter(|&u| {
            let s = g.units[u as usize].span;
            code[s.start as usize..s.end as usize].starts_with(head)
        })
        .collect();
    *found
        .last()
        .unwrap_or_else(|| panic!("no unit starts with {head:?}: {:?}", g.units))
}

fn has(g: &ReferenceGraph, from: u32, to: u32, kind: EdgeKind) -> bool {
    g.edges.contains(&(from, to, kind))
}

fn linked(g: &ReferenceGraph, from: u32, to: u32) -> bool {
    g.out[from as usize].contains(&to)
}

#[test]
fn a_read_points_at_the_declaring_statement() {
    let code = "var a = 1;\nvar b = a + 1;\nvar c = 2;\n";
    let g = graph_of(code, &[], &BundlerFacts::default());
    assert_eq!(g.units.len(), 3);
    let (a, b, c) = (
        unit(&g, code, "var a"),
        unit(&g, code, "var b"),
        unit(&g, code, "var c"),
    );
    assert!(has(&g, b, a, EdgeKind::Read));
    assert!(!linked(&g, a, b), "a reference has a direction");
    assert!(!linked(&g, c, a) && !linked(&g, a, c));
    assert_eq!(g.inn[a as usize], vec![b]);
}

#[test]
fn an_assignment_defines_the_binding_and_readers_reach_the_writer() {
    let code = "var a;\na = make();\nvar c = a;\nfunction make() { return 1; }\n";
    let g = graph_of(code, &[], &BundlerFacts::default());
    let decl = unit(&g, code, "var a");
    let write = unit(&g, code, "a = make");
    let reader = unit(&g, code, "var c");
    assert!(has(&g, write, decl, EdgeKind::Write));
    assert!(
        has(&g, decl, write, EdgeKind::Write),
        "a binding and its writers are one thing"
    );
    assert!(has(&g, reader, decl, EdgeKind::Read));
    assert!(
        has(&g, reader, write, EdgeKind::Read),
        "the writer is the real definer"
    );
}

#[test]
fn a_hoisted_function_is_reached_before_its_declaration() {
    let code = "var x = f();\nfunction f() { return 1; }\n";
    let g = graph_of(code, &[], &BundlerFacts::default());
    assert!(has(
        &g,
        unit(&g, code, "var x"),
        unit(&g, code, "function f"),
        EdgeKind::Read
    ));
}

#[test]
fn a_split_declaration_gives_each_declarator_its_own_unit() {
    let code = "var a, b;\na = 1;\nvar c = b;\n";
    let g = graph_of(code, &[], &BundlerFacts::default());
    // `var a, b;` is two units: `a` and `b`.
    assert_eq!(g.units.len(), 4, "{:?}", g.units);
    let text = |u: u32| {
        let s = g.units[u as usize].span;
        &code[s.start as usize..s.end as usize]
    };
    let a = (0..g.units.len() as u32)
        .find(|&u| text(u) == "a")
        .expect("a's declarator is a unit");
    let b = (0..g.units.len() as u32)
        .find(|&u| text(u) == "b")
        .expect("b's declarator is a unit");
    assert!(has(&g, unit(&g, code, "a = 1"), a, EdgeKind::Write));
    let reader = unit(&g, code, "var c");
    assert!(has(&g, reader, b, EdgeKind::Read));
    assert!(!linked(&g, reader, a), "reading b is not reading a");
}

#[test]
fn a_namespace_helper_and_an_init_time_member_write_define_their_target() {
    let code = "var N = {};\nrt(N, { x: () => y });\nN.extra = 1;\nvar y = 1;\nvar z = N.x;\nfunction configure() { N.later = 2; }\n";
    let facts = BundlerFacts {
        namespace_fillers: names(&["rt"]),
        ..BundlerFacts::default()
    };
    let g = graph_of(code, &[], &facts);
    let ns = unit(&g, code, "var N");
    let fill = unit(&g, code, "rt(N");
    let extra = unit(&g, code, "N.extra");
    let reader = unit(&g, code, "var z");
    let configure = unit(&g, code, "function configure");
    assert!(has(&g, fill, ns, EdgeKind::Write) && has(&g, ns, fill, EdgeKind::Write));
    assert!(has(&g, extra, ns, EdgeKind::Write) && has(&g, ns, extra, EdgeKind::Write));
    assert!(
        has(&g, reader, fill, EdgeKind::Read),
        "N's readers reach the module that fills it"
    );
    assert!(has(&g, fill, unit(&g, code, "var y"), EdgeKind::Read));
    // Run-time configuration is a READ: the library does not depend on it.
    assert!(has(&g, configure, ns, EdgeKind::Read));
    assert!(!linked(&g, ns, configure));
    assert!(!has(&g, reader, configure, EdgeKind::Read));
}

#[test]
fn interop_call_shapes_reach_the_module_and_its_namespace() {
    let code = "var F = Q((e, m) => { m.exports = 1; });\nvar N;\nvar I = b(() => { N = { k: 1 }; });\nvar x = A(F(), 1);\nvar y = (I(), In(N));\nvar z = () => Promise.resolve().then(() => (I(), N));\n";
    let facts = BundlerFacts {
        lazy_init: names(&["b"]),
        ..BundlerFacts::default()
    };
    let g = graph_of(code, &["F = Q((e, m) => { m.exports = 1; })"], &facts);
    let factory = unit(&g, code, "var F");
    assert_eq!(g.units[factory as usize].kind, UnitKind::Factory);
    let init = unit(&g, code, "var I");
    let fill = unit(&g, code, "N = {");
    let ns = unit(&g, code, "var N");
    assert!(has(&g, unit(&g, code, "var x"), factory, EdgeKind::Read));
    for reader in [unit(&g, code, "var y"), unit(&g, code, "var z")] {
        assert!(has(&g, reader, init, EdgeKind::Read));
        assert!(has(&g, reader, ns, EdgeKind::Read));
        assert!(has(&g, reader, fill, EdgeKind::Read));
    }
}

#[test]
fn a_lazy_module_owns_its_body_statements() {
    let code = "var N;\nvar I = b(() => {\n  N = g();\n  h();\n});\nfunction g() { return 1; }\nfunction h() {}\n";
    let facts = BundlerFacts {
        lazy_init: names(&["b"]),
        ..BundlerFacts::default()
    };
    let g = graph_of(code, &[], &facts);
    let init = unit(&g, code, "var I");
    let fill = unit(&g, code, "N = g()");
    let call = unit(&g, code, "h();");
    assert_eq!(g.units[init as usize].kind, UnitKind::LazyModule);
    assert_eq!(
        g.units[fill as usize].kind,
        UnitKind::LazyBody { module: init }
    );
    assert!(has(&g, init, fill, EdgeKind::Body) && has(&g, init, call, EdgeKind::Body));
    assert!(has(&g, fill, unit(&g, code, "function g"), EdgeKind::Read));
    assert!(has(&g, call, unit(&g, code, "function h"), EdgeKind::Read));
    // The wrapper owns only its overhead.
    let whole = g.units[init as usize].span.size();
    let body = g.units[fill as usize].span.size() + g.units[call as usize].span.size();
    assert_eq!(g.units[init as usize].bytes, whole - body);
}

#[test]
fn runtime_helpers_and_what_only_they_use_are_marked_runtime() {
    let code = "var oEt = Object.defineProperty;\nvar rt = (e, t) => { for (var r in t) oEt(e, r, { get: t[r] }); };\nvar N = {};\nrt(N, { a: () => 1 });\nvar app = () => N.a;\n";
    let facts = BundlerFacts {
        namespace_fillers: names(&["rt"]),
        runtime: names(&["rt"]),
        ..BundlerFacts::default()
    };
    let g = graph_of(code, &[], &facts);
    let runtime: Vec<bool> = g.units.iter().map(|u| u.runtime).collect();
    assert_eq!(runtime, vec![true, true, false, false, false]);
    // Edges into the runtime still exist; discovery treats them as external.
    assert!(has(
        &g,
        unit(&g, code, "rt(N"),
        unit(&g, code, "var rt"),
        EdgeKind::Read
    ));
}

fn facts_of(code: &str) -> BundlerFacts {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    BundlerFacts::recognise(
        code,
        &ingest.program.body,
        crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
        crate::toolchain::InteropHelpers::Bun,
    )
}

fn sorted(set: &HashSet<String>) -> Vec<&str> {
    let mut v: Vec<&str> = set.iter().map(String::as_str).collect();
    v.sort_unstable();
    v
}

/// Bun's runtime prelude, as Claude Code 2.1.216 ships it (minified).
const BUN_PRELUDE: &str = r#"var rfm=Object.create;var{getPrototypeOf:nfm,defineProperty:oEt,getOwnPropertyNames:bai,getOwnPropertyDescriptor:ofm}=Object,Kkn=Object.prototype.hasOwnProperty;function Ykn(e){return this[e]}var Oi=(e,t,r)=>{var n=t&&typeof t==="object"||typeof t==="function"?bai(t):[];for(let o of n)if(!Kkn.call(e,o)&&o!=="default")oEt(e,o,{get:Ykn.bind(t,o),enumerable:!0});if(r){for(let o of n)if(!Kkn.call(r,o)&&o!=="default")oEt(r,o,{get:Ykn.bind(t,o),enumerable:!0});return r}},ifm,sfm,A=(e,t,r)=>{var n=e!=null&&typeof e==="object";if(n){var o=t?ifm??=new WeakMap:sfm??=new WeakMap,i=o.get(e);if(i)return i}r=e!=null?rfm(nfm(e)):{};let s=t||!e||!e.__esModule?oEt(r,"default",{value:e,enumerable:!0}):r;if(e&&typeof e==="object"||typeof e==="function"){for(let a of bai(e))if(!Kkn.call(s,a))oEt(s,a,{get:Ykn.bind(e,a),enumerable:!0})}if(n)o.set(e,s);return s},In=(e)=>{var t=(fal??=new WeakMap).get(e),r;if(t)return t;if(t=oEt({},"__esModule",{value:!0}),e&&typeof e==="object"||typeof e==="function"){for(var n of bai(e))if(!Kkn.call(t,n))oEt(t,n,{get:Ykn.bind(e,n),enumerable:!(r=ofm(e,n))||r.enumerable})}return fal.set(e,t),t},fal,Q=(e,t)=>()=>(t||e((t={exports:{}}).exports,t),t.exports);var afm=(e)=>e;function lfm(e,t){this[e]=afm.bind(null,t)}var rt=(e,t)=>{for(var r in t)oEt(e,r,{get:t[r],enumerable:!0,configurable:!0,set:lfm.bind(t,r)})};var b=(e,t)=>()=>(e&&(t=e(e=0)),t);var z=Symbol.for("react.memo_cache_sentinel"),Qs=Symbol.for("react.early_return_sentinel");var mal=Q((e,t)=>{t.exports=1});var N={};var I=b(()=>{rt(N,{x:()=>1})});function app(){return z}var ajb=async(e,t,r)=>{let o=t?"fullscreen":"default";if(r==="")return e(`Current renderer: ${o}`,{display:"system"}),null;return app()};"#;

#[test]
fn bun_runtime_helpers_are_recognised_by_shape() {
    let f = facts_of(BUN_PRELUDE);
    assert_eq!(sorted(&f.lazy_init), vec!["b"]);
    assert_eq!(sorted(&f.namespace_fillers), vec!["Oi", "rt"]);
    assert_eq!(
        sorted(&f.runtime),
        vec!["A", "In", "Oi", "Q", "Qs", "b", "rt", "z"]
    );
    // Built on these facts, the runtime is the helpers plus what only they
    // use; the module, its namespace and the app are not.
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, BUN_PRELUDE, "input.js");
    let g = build_reference_graph(
        BUN_PRELUDE,
        ingest.semantic(),
        &ingest.program.body,
        &[],
        &f,
    );
    let runtime_text: Vec<&str> = g
        .units
        .iter()
        .filter(|u| u.runtime)
        .map(|u| &BUN_PRELUDE[u.span.start as usize..u.span.end as usize])
        .map(|t| t.split(['=', '(']).next().unwrap_or(t))
        .collect();
    for helper in [
        "var rfm",
        "Oi",
        "A",
        "In",
        "Q",
        "var afm",
        "function lfm",
        "var rt",
        "var b",
        "z",
        "Qs",
        "Kkn",
        "function Ykn",
    ] {
        assert!(
            runtime_text.iter().any(|t| t.trim() == helper),
            "{helper} in {runtime_text:?}"
        );
    }
    for app in ["var N", "var I", "function app"] {
        assert!(
            !runtime_text.iter().any(|t| t.trim() == app),
            "{app} is not runtime"
        );
    }
}

/// esbuild's runtime prelude (unminified).
const ESBUILD_PRELUDE: &str = r#"var __defProp = Object.defineProperty;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __esm = (fn, res) => function __init() {
  return fn && (res = (0, fn[__getOwnPropNames(fn)[0]])(fn = 0)), res;
};
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: true });
  }
  return to;
};
var __reExport = (target, mod, secondTarget) => (__copyProps(target, mod, "default"), secondTarget && __copyProps(secondTarget, mod, "default"));
var ns = {};
var init_mod = __esm({
  "src/mod.js"() {
    __export(ns, { x: () => x });
  }
});
var x = 1;
"#;

#[test]
fn esbuild_runtime_helpers_are_recognised_by_shape() {
    let f = facts_of(ESBUILD_PRELUDE);
    assert_eq!(sorted(&f.lazy_init), vec!["__esm"]);
    assert_eq!(sorted(&f.namespace_fillers), vec!["__export", "__reExport"]);
    // esbuild's object-method init form is a lazy module too.
    let g = graph_of(ESBUILD_PRELUDE, &[], &f);
    let init = unit(&g, ESBUILD_PRELUDE, "var init_mod");
    assert_eq!(g.units[init as usize].kind, UnitKind::LazyModule);
    let fill = unit(&g, ESBUILD_PRELUDE, "__export(ns");
    assert_eq!(
        g.units[fill as usize].kind,
        UnitKind::LazyBody { module: init }
    );
    assert!(has(
        &g,
        fill,
        unit(&g, ESBUILD_PRELUDE, "var ns"),
        EdgeKind::Write
    ));
}

/// esbuild 0.28.2 `--minify --keep-names`: `__defNormalProp` (p), `__name`
/// (n), `__require` (u), `__publicField` (s), then app code.
const ESBUILD_MINIFIED: &str = r#"var c=Object.defineProperty;var p=(r,e,t)=>e in r?c(r,e,{enumerable:!0,configurable:!0,writable:!0,value:t}):r[e]=t;var n=(r,e)=>c(r,"name",{value:e,configurable:!0}),u=(r=>typeof require<"u"?require:typeof Proxy<"u"?new Proxy(r,{get:(e,t)=>(typeof require<"u"?require:e)[t]}):r)(function(r){if(typeof require<"u")return require.apply(this,arguments);throw Error('Dynamic require of "'+r+'" is not supported')});var s=(r,e,t)=>p(r,typeof e!="symbol"?e+"":e,t);var a=class a{constructor(){s(this,"x",1)}bar(){return u(process.argv[2])}};n(a,"Foo"),s(a,"y",2);var i=a;function b(){return()=>1}n(b,"named");var k=(r,e)=>r.name===e;"#;

#[test]
fn esbuild_name_require_and_public_field_helpers_are_runtime_by_shape() {
    let f = facts_of(ESBUILD_MINIFIED);
    assert_eq!(sorted(&f.runtime), vec!["n", "s", "u"]);
    assert_eq!(
        sorted(&f.namespace_fillers),
        vec!["n"],
        "__name defines its first argument"
    );
    let g = graph_of(ESBUILD_MINIFIED, &[], &f);
    let runtime: Vec<&str> = g
        .units
        .iter()
        .filter(|u| u.runtime)
        .map(|u| &ESBUILD_MINIFIED[u.span.start as usize..u.span.end as usize])
        .map(|t| t.split('=').next().unwrap_or(t).trim_start_matches("var "))
        .collect();
    assert_eq!(runtime, vec!["c", "p", "n", "u", "s"]);
    // The name setter defines the function it names.
    let named = unit(&g, ESBUILD_MINIFIED, "function b");
    let setter = unit(&g, ESBUILD_MINIFIED, "n(b");
    assert!(has(&g, setter, named, EdgeKind::Write));
}
