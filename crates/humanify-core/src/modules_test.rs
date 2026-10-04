//! WP1.5 parity tests — expectations generated from the TS originals by
//! test/parity/wp1.5-probe.mjs (tsx run against src/analysis/*.ts). The
//! comments cite the probe line each expectation came from.

use crate::modules::identify_bun_cjs_factory;

/// TS probe `window[marker with leading decls]`: LEFTMOST binding wins —
/// `var a=1,b=2,x=...` identifies as "a" (the v1 port kept the LAST match).
#[test]
fn identify_takes_the_leftmost_binding() {
    let src = "var a=1,b=2,x=(A,q)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);";
    let h = identify_bun_cjs_factory(src).expect("helper identified");
    assert_eq!(h.name, "a");
    assert_eq!(h.start_offset, 0);
}

/// TS probe `window[two spaces after colon]`: the marker's whitespace is
/// `\s*` — flexible. (The v1 constant `{exports: {}` demanded one space and
/// MISSED real minified bundles, whose marker is `{exports:{}}` — no space.)
#[test]
fn identify_marker_whitespace_is_flexible() {
    let src = "var a=1;var x=(A,q)=>()=>(A||I((A = {exports:  {}}).exports, A), A.exports);";
    let h = identify_bun_cjs_factory(src).expect("helper identified");
    assert_eq!(h.name, "x");
    assert_eq!(h.start_offset, 8);
}

/// TS probe `window[statement before]`: the offset points at the KEYWORD
/// (or comma), not the name.
#[test]
fn identify_offset_points_at_the_keyword() {
    let src = "f();var x=(A,q)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);";
    let h = identify_bun_cjs_factory(src).expect("helper identified");
    assert_eq!(h.name, "x");
    assert_eq!(h.start_offset, 4);
}

/// No marker → None (TS probe `window[no marker]`).
#[test]
fn identify_returns_none_without_the_marker() {
    assert!(identify_bun_cjs_factory("var a=1,b=2,c=3;").is_none());
}

/// The real minified form: `{exports:{}}` with NO space (the oracle
/// bundles' actual marker), and a keyword-declared helper.
#[test]
fn identify_matches_the_real_minified_marker() {
    let src = "var d=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);var tO8=d((q,m)=>{});";
    let h = identify_bun_cjs_factory(src).expect("helper identified");
    assert_eq!(h.name, "d");
    assert_eq!(h.start_offset, 0);
}

// ── classification + banner semantics ────────────────────────────────
// Expectations from the TS probe run (tsx test/parity/wp1.5-probe.mjs on
// the synthetic sources): bannerText is the STRIPPED, TRIMMED text and is
// recorded even when no package parses (statement-level); the in-body
// fallback only reports banners whose package parses; the version EXCLUDES
// the `v` prefix.

use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::modules::{FactoryRecord, classify_bun_modules, name_cjs_factories};
use std::collections::HashMap;

const HELPER: &str = "var x=(I,A)=>()=>(A||I((A = {exports:{}}).exports, A), A.exports);";

/// Parse a fixture and run `f` against its program + semantic — everything
/// lives in one scope, so no arena juggling.
fn with_parsed<T>(
    text: &str,
    f: impl FnOnce(&oxc_ast::ast::Program<'_>, &oxc_semantic::Semantic<'_>) -> T,
) -> T {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "input.js");
    assert!(
        ingest.errors.is_empty(),
        "fixture must parse: {:?}",
        ingest.errors
    );
    f(ingest.program, ingest.semantic())
}

fn classify_of(text: &str) -> Vec<FactoryRecord> {
    // The records own their strings (Spans are Copy), so the arena can
    // die with the ingest at the end of this scope.
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "input.js");
    assert!(ingest.errors.is_empty(), "fixture must parse");
    let tables = SymbolTables::build(ingest.semantic());
    let wrapper = crate::modules::wrapper::find_wrapper_function(ingest.program, ingest.semantic());
    let wrapper_body = wrapper.as_ref().map(|w| w.body_span);
    let classification = classify_bun_modules(
        text,
        ingest.program,
        ingest.semantic(),
        wrapper_body,
        &tables,
    );
    classification.map(|c| c.factories).unwrap_or_default()
}

/// Probe: `banner[...]` (leading-comment form) — one row per TS expectation.
#[test]
fn banner_leading_comment_matches_ts() {
    let cases: &[(&str, &str, Option<&str>, Option<&str>)] = &[
        (
            "@azure/msal-common v15.13.1",
            "@azure/msal-common v15.13.1",
            Some("@azure/msal-common"),
            Some("15.13.1"),
        ),
        ("Sharp", "Sharp", None, None),
        ("Copyright 2013", "Copyright 2013", None, None),
        ("highlight.js", "highlight.js", Some("highlight.js"), None),
        ("license", "license", None, None),
        ("see license", "see license", None, None),
        ("pkg", "pkg", None, None),
        ("pkg;", "pkg;", None, None),
        (
            "react-dom 18.2.0",
            "react-dom 18.2.0",
            Some("react-dom"),
            Some("18.2.0"),
        ),
    ];
    for (banner, text, pkg, version) in cases {
        let src = format!("{HELPER} /*! {banner} */ var tO8=x((q,m)=>{{module.exports=1;}});");
        let factories = classify_of(&src);
        let f = factories.first().expect("factory classified");
        assert_eq!(
            f.banner_text.as_deref(),
            Some(*text),
            "bannerText for {banner}"
        );
        assert_eq!(f.banner_package.as_deref(), *pkg, "pkg for {banner}");
        assert_eq!(
            f.banner_version.as_deref(),
            *version,
            "version for {banner}"
        );
    }
}

/// Probe: `inbody[...]` — the in-body fallback only reports banners whose
/// package parses.
#[test]
fn banner_in_body_fallback_matches_ts() {
    let cases: &[(&str, Option<&str>, Option<&str>)] = &[
        (
            "@azure/msal-common v15.13.1",
            Some("@azure/msal-common"),
            Some("15.13.1"),
        ),
        ("Sharp", None, None),
        ("Copyright 2013", None, None),
        ("highlight.js", Some("highlight.js"), None),
        ("react-dom 18.2.0", Some("react-dom"), Some("18.2.0")),
    ];
    for (banner, pkg, version) in cases {
        let src = format!("{HELPER} var tO8=x((q,m)=>{{/*! {banner} */ module.exports=1;}});");
        let factories = classify_of(&src);
        let f = factories.first().expect("factory classified");
        assert_eq!(f.banner_package.as_deref(), *pkg, "pkg for {banner}");
        assert_eq!(
            f.banner_version.as_deref(),
            *version,
            "version for {banner}"
        );
    }
}

/// The trailing form: a bang comment between two factory statements
/// attaches to the PRIOR statement (Babel trailingComments) — the gap scan
/// picks it up for the FOLLOWING statement (the TS collectBanner's
/// priorStatementPath branch).
#[test]
fn banner_between_statements_attaches_to_the_next() {
    let src = format!(
        "{HELPER} var tO8=x((q,m)=>{{module.exports=1;}}); /*! @r/pkg v1.0 */ var tO9=x((q,m)=>{{module.exports=2;}});"
    );
    let factories = classify_of(&src);
    assert_eq!(factories.len(), 2);
    assert_eq!(factories[0].banner_package.as_deref(), None);
    assert_eq!(factories[1].banner_package.as_deref(), Some("@r/pkg"));
}

/// Real minified shapes: helper declared among comma-joined declarators,
/// factory bodies with no spaces.
#[test]
fn classify_real_minified_shape() {
    let src = "var d=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports),tO8=d((q,m)=>{module.exports=1;}),tO9=d((q,m)=>{module.exports=2;});";
    let factories = classify_of(src);
    assert_eq!(factories.len(), 2);
    assert_eq!(factories[0].factory_var, "tO8");
    assert_eq!(factories[1].factory_var, "tO9");
    // byteRange covers the DECLARATOR, not the statement.
    let d0 = &src[factories[0].span.start as usize..factories[0].span.end as usize];
    assert!(d0.starts_with("tO8=d("), "declarator slice: {d0}");
    // contentHash: 16 hex of sha256 over the declarator slice.
    assert_eq!(factories[0].content_hash.len(), 16);
    // lineRange is 1-indexed.
    assert_eq!(factories[0].line_range.0, 1);
}

// ── the naming cascade ───────────────────────────────────────────────

#[test]
fn hash_fallback_name_and_test() {
    assert_eq!(
        crate::modules::hash_fallback_name("abcdef1234567890"),
        "lib_abcdef12"
    );
    assert!(crate::modules::is_hash_fallback_name("lib_abcdef12"));
    assert!(!crate::modules::is_hash_fallback_name("lib_abc"));
    assert!(!crate::modules::is_hash_fallback_name("react"));
}

/// Probe `banner[...]` + `nameCounts`: banner wins, then fallback.
#[test]
fn naming_cascade_banner_then_fallback() {
    // A statement-level banner applies to ALL of the statement's
    // declarators (the TS bannerForStatement is shared) — the banner and
    // fallback mix needs separate STATEMENTS.
    let src = format!(
        "{HELPER} /*! @azure/msal-common v15.13.1 */ var tO8=x((q,m)=>{{module.exports=1;}}); var tO9=x((q,m)=>{{module.exports=2;}});"
    );
    let leaked: &'static str = Box::leak(src.clone().into_boxed_str());
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, leaked, "input.js");
    let tables = SymbolTables::build(ingest.semantic());
    let mut classification = crate::modules::classify_bun_modules(
        leaked,
        ingest.program,
        ingest.semantic(),
        None,
        &tables,
    )
    .expect("helper present");
    assert_eq!(classification.factories.len(), 2);
    let counts = name_cjs_factories(&mut classification, leaked, None);
    assert_eq!(counts.banner, 1);
    assert_eq!(counts.fallback, 1);
    assert_eq!(counts.url, 0);
    assert_eq!(
        classification.factories[0].name.as_deref(),
        Some("@azure/msal-common@15.13.1")
    );
    assert!(matches!(
        classification.factories[0].name_source,
        Some(crate::modules::NameSource::Banner)
    ));
    // The fallback name comes from the STRUCTURAL hash (8 chars).
    let expected = format!("lib_{}", &classification.factories[1].structural_hash[..8]);
    assert_eq!(
        classification.factories[1].name.as_deref(),
        Some(expected.as_str())
    );
}

/// A statement-level banner applies to ALL the statement's declarators
/// (the TS bannerForStatement is computed once per statement).
#[test]
fn banner_applies_to_every_declarator_of_its_statement() {
    let src = format!(
        "{HELPER} /*! @r/pkg v1.0 */ var tA=x((q,m)=>{{module.exports=1;}}),tB=x((q,m)=>{{module.exports=2;}});"
    );
    let factories = classify_of(&src);
    assert_eq!(factories.len(), 2);
    assert_eq!(factories[0].banner_package.as_deref(), Some("@r/pkg"));
    assert_eq!(factories[1].banner_package.as_deref(), Some("@r/pkg"));
}

/// Carry-over: an INTACT prior group carries positionally; a changed group
/// earns a fresh name.
#[test]
fn naming_cascade_carry_over_requires_intact_group() {
    let src = format!(
        "{HELPER} var tA=x((q,m)=>{{module.exports=1;}}),tB=x((q,m)=>{{module.exports=1;}});"
    );
    let leaked: &'static str = Box::leak(src.clone().into_boxed_str());
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse(&allocator, leaked, "input.js");
    let tables = SymbolTables::build(ingest.semantic());
    let mut classification = crate::modules::classify_bun_modules(
        leaked,
        ingest.program,
        ingest.semantic(),
        None,
        &tables,
    )
    .expect("helper present");
    // Two structurally identical factories → one group of 2.
    let hash = classification.factories[0].structural_hash.clone();
    assert_eq!(classification.factories[1].structural_hash, hash);
    // INTACT prior group of 2 → positional carry.
    let mut prior = HashMap::new();
    prior.insert(
        hash.clone(),
        vec![
            crate::modules::CarriedName::new("lib_prior0", Some("fallback")),
            crate::modules::CarriedName::new("lib_prior1", Some("fallback")),
        ],
    );
    let counts = name_cjs_factories(&mut classification, leaked, Some(&prior));
    assert_eq!(counts.carry_over, 2);
    assert_eq!(
        classification.factories[0].name.as_deref(),
        Some("lib_prior0")
    );
    assert_eq!(
        classification.factories[1].name.as_deref(),
        Some("lib_prior1")
    );
    // CHANGED prior group (1 entry) → no carry → fallback.
    let mut classification = crate::modules::classify_bun_modules(
        leaked,
        ingest.program,
        ingest.semantic(),
        None,
        &tables,
    )
    .expect("helper present");
    let mut prior = HashMap::new();
    prior.insert(
        hash,
        vec![crate::modules::CarriedName::new(
            "lib_prior0",
            Some("fallback"),
        )],
    );
    let counts = name_cjs_factories(&mut classification, leaked, Some(&prior));
    assert_eq!(counts.carry_over, 0);
    assert_eq!(counts.fallback, 2);
}

/// URL naming: one distinctive github repo, or one pkg domain.
#[test]
fn url_naming_distinctive_repo() {
    assert_eq!(
        crate::modules::extract_distinctive_repo_name("x=github.com/left-pad/left-pad;y"),
        Some("left-pad".to_string())
    );
    // Two different repos → abstain.
    assert_eq!(
        crate::modules::extract_distinctive_repo_name("a=github.com/o/r1;b=github.com/o2/r2"),
        None
    );
    // One pkg domain.
    assert_eq!(
        crate::modules::extract_distinctive_repo_name("see pkg.dev docs"),
        Some("pkg".to_string())
    );
    // None at all.
    assert_eq!(
        crate::modules::extract_distinctive_repo_name("no refs here"),
        None
    );
}

// ── wrapper detection ────────────────────────────────────────────────

#[test]
fn wrapper_small_iife_is_not_a_wrapper() {
    let src = "var a=1;(function(){var b=2;return b;})();";
    assert!(with_parsed(src, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s).is_none()
    }));
}

#[test]
fn wrapper_giant_iife_is_detected() {
    // 60 bindings inside the IIFE body — over the threshold of 50.
    let decls: Vec<String> = (0..60).map(|i| format!("var v{i}={i};")).collect();
    let src = "(function(){".to_string() + &decls.concat() + "return v0;})();";
    let w = with_parsed(&src, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s).expect("wrapper detected")
    });
    assert_eq!(w.binding_count, 60);
    // The body block's slice is the IIFE's inner statements.
    let body = &src[w.body_span.start as usize..w.body_span.end as usize];
    // The body block's span includes the braces (babel's BlockStatement too).
    assert!(body.starts_with("{var v0="));
}

#[test]
fn wrapper_negated_and_call_forms() {
    let decls: Vec<String> = (0..55).map(|i| format!("var w{i}={i};")).collect();
    let joined = decls.concat();
    let negated = "!function(){".to_string() + &joined + "return w0;}();";
    assert!(with_parsed(&negated, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s).is_some()
    }));
    let call_form = "(function(){".to_string() + &joined + "return w0;}).call(this);";
    assert!(with_parsed(&call_form, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s).is_some()
    }));
    // Bun CJS bytecode: a bare function expression statement (not called).
    let bare = "(function(){".to_string() + &joined + "return w0;});";
    assert!(with_parsed(&bare, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s).is_some()
    }));
}

#[test]
fn the_input_bundle_gate_reads_the_original_text() {
    use crate::modules::wrapper::original_bundle_binding_count;
    // Being a bundled app is a property of the INPUT: the split's ≥50
    // threshold reads the run's ORIGINAL bundle (what the unpack stage
    // saw), never the post-extraction runtime it is handed.
    let decls: Vec<String> = (0..55).map(|i| format!("var v{i}={i};")).collect();
    let bundled = "(function(){".to_string() + &decls.concat() + "return v0;})();";
    assert_eq!(
        original_bundle_binding_count(&bundled).expect("bundled input"),
        55
    );
    // A plain script fails the gate loud, as does a small per-module IIFE
    // (the WP1.5 negative) — the same ≥50 threshold, measured on the
    // input.
    let err = original_bundle_binding_count("var a=1;console.log(a);").unwrap_err();
    assert!(err.contains("no recognizable bundle wrapper"), "{err}");
    let err = original_bundle_binding_count("(function(){var b=1;return b;})();").unwrap_err();
    assert!(err.contains("no recognizable bundle wrapper"), "{err}");
}

#[test]
fn the_small_esbuild_fixture_pins_the_original_bundle_boundary() {
    // The committed small fixture is the boundary shape: its ORIGINAL
    // clears the frozen ≥50 wrapper threshold while its POST-EXTRACTION
    // runtime (one wrapper-scope binding spliced out per vendored CJS
    // module) sits under it — the failure the input-bundle gate lifts. A
    // regen that shrinks the vendor half below the threshold makes e2e
    // fail loud; a regen that fattens the app half past it silently stops
    // exercising the boundary, so this pin reads the counts off the
    // committed build.
    let text =
        include_str!("../../../test/e2e/fixtures/esbuild-bundle-small/build/v1.0.0/build/index.js");
    let deps = text.matches("= __commonJS({").count();
    assert!(deps >= 1, "the fixture's vendor half is CJS modules");
    let w = with_parsed(text, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s)
            .expect("the fixture's original is a bundled app")
            .binding_count
    });
    assert!(
        w >= 50,
        "the fixture's ORIGINAL must clear the frozen threshold (has {w})"
    );
    assert!(
        w - deps < 50,
        "the fixture's post-extraction runtime ({} = {w} − {deps} vendored) \
must sit UNDER the threshold — otherwise the fixture exercises nothing",
        w - deps
    );
}

#[test]
fn wrapper_directive_prologue_does_not_hide_the_bundle_iife() {
    // esbuild's --format=iife output (the default bundle form) opens with
    // a `"use strict";` directive prologue BEFORE the IIFE — the wrapper
    // must be found through it (the classification container and the
    // split both read this).
    let decls: Vec<String> = (0..55).map(|i| format!("var u{i}={i};")).collect();
    let strict = format!(
        "\"use strict\";\n(() => {{{}return u0;}})();",
        decls.concat()
    );
    let w = with_parsed(&strict, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s).expect("wrapper behind a directive")
    });
    assert_eq!(w.binding_count, 55);
    // A directive does not license extra statements: the wrapper must
    // still be the ONLY real statement.
    let extra = format!(
        "\"use strict\";var a=1;(()=>{{{}return u0;}})();",
        decls.concat()
    );
    assert!(with_parsed(&extra, |p, s| {
        crate::modules::wrapper::find_wrapper_function(p, s).is_none()
    }));
}

// ── soundness (eval/with taint) ──────────────────────────────────────

#[test]
fn with_statement_taints_enclosing_function() {
    let src = "function f(){ with (obj) { x = 1; } } function g(){ return 1; }";
    let taint = with_parsed(src, |_p, s| {
        crate::modules::soundness::collect_eval_with_taint(s)
    });
    assert_eq!(taint.site_count, 1);
    assert!(taint.module_tainted);
    assert_eq!(taint.tainted_functions.len(), 1);
    // The tainted span is f's, and the freeze predicate agrees.
    let f_span = taint.tainted_functions[0];
    assert!(crate::modules::soundness::is_binding_eval_taint_frozen(
        Some(f_span),
        &taint
    ));
    assert!(crate::modules::soundness::is_binding_eval_taint_frozen(
        None, &taint
    ));
}

#[test]
fn direct_eval_taints_but_local_eval_does_not() {
    let src = "function h(){ eval(\"x\"); } function i(){ const eval = () => 1; eval(); }";
    let taint = with_parsed(src, |_p, s| {
        crate::modules::soundness::collect_eval_with_taint(s)
    });
    // One site (h's eval); i's eval is a local binding — an ordinary function.
    assert_eq!(taint.site_count, 1);
    assert_eq!(taint.tainted_functions.len(), 1);
    // A module-level direct eval makes the module tainted with no functions.
    let src = "eval(\"x\");";
    let taint = with_parsed(src, |_p, s| {
        crate::modules::soundness::collect_eval_with_taint(s)
    });
    assert_eq!(taint.site_count, 1);
    assert!(taint.tainted_functions.is_empty());
    assert!(taint.module_tainted);
    // Indirect eval (window.eval, (0,eval)) is NOT a site.
    let src = "window.eval(\"x\"); (0, eval)(\"y\");";
    let taint = with_parsed(src, |_p, s| {
        crate::modules::soundness::collect_eval_with_taint(s)
    });
    assert_eq!(taint.site_count, 0);
    assert!(!taint.module_tainted);
}

// ── known globals ────────────────────────────────────────────────────

/// The lists are a VERBATIM port of the TS — the committed fixture
/// (generated from src/analysis/known-globals.ts by the WP1.5 work) is the
/// exact-equality oracle. One re-cut since (toolchain review R21,
/// 2026-10-04): the TS `node` list's five CommonJS names moved, unchanged,
/// to their own `commonjs` environment read from the toolchain's one list
/// — same membership, so `is_known_global` answers exactly as before.
#[test]
fn known_globals_match_the_ts_lists_exactly() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/parity/known-globals.json"
    );
    let raw = std::fs::read_to_string(path).expect("fixture present");
    let ts: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(&raw).expect("fixture parses");
    for (key, names) in &ts {
        let (rust_key, rust_names) = crate::modules::known_globals::GLOBAL_ENVIRONMENTS
            .iter()
            .find(|(k, _)| k == key)
            .unwrap_or_else(|| panic!("environment {key} missing"));
        let mine: Vec<&str> = rust_names.to_vec();
        let theirs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        assert_eq!(format!("{rust_key:?}"), format!("{key:?}"));
        assert_eq!(mine, theirs, "environment {key} diverges");
    }
}

#[test]
fn known_globals_cover_the_historical_misses() {
    // The first version of the TS list omitted these and misreported 16
    // unreachable free references as 2 — they must stay.
    for name in ["Bun", "btoa", "crypto", "Blob"] {
        assert!(
            crate::modules::known_globals::is_known_global(name),
            "{name}"
        );
    }
    assert!(crate::modules::known_globals::known_globals(&[]).len() >= 100);
}

// ── esbuild's module form (exp075) ───────────────────────────────────

/// Real esbuild 0.27.2 output shapes (verified against a fresh build):
/// the CJS helper is a two-param arrow whose thunk REQUIRES `mod`, and an
/// unminified build wraps every lazy module — CJS and ESM alike — in an
/// OBJECT with one keyed method whose KEY IS the original source path.
/// The helper's `{ exports: {} }` marker is SPACED, which bun's tight
/// `{exports:` marker does not match: the esbuild helper is identified by
/// its preserved declaration NAME (`__commonJS`).
const ESBUILD_CJS_HELPER: &str = concat!(
    "var __commonJS = ((cb, mod) => function __require() {\n",
    "  return mod || (0, cb[Object.getOwnPropertyNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;\n",
    "});\n",
);

/// The unminified factory: `var require_x = __commonJS({ "src/x.js"(exports, module) {...} });`
const ESBUILD_OBJECT_FACTORY: &str = concat!(
    "var require_cjs_dep = __commonJS({\n",
    "  \"src/libs/cjs-dep.js\"(exports, module2) {\n",
    "    module2.exports = { value: 1 };\n",
    "  }\n",
    "});\n",
);

#[test]
fn classifies_esbuilds_object_form_factory_with_its_source_path() {
    let src = format!("{ESBUILD_CJS_HELPER}{ESBUILD_OBJECT_FACTORY}");
    let factories = classify_of(&src);
    assert_eq!(factories.len(), 1, "the object form is a factory");
    let f = &factories[0];
    assert_eq!(f.factory_var, "require_cjs_dep");
    assert_eq!(f.source_path.as_deref(), Some("src/libs/cjs-dep.js"));
    assert_eq!(f.structural_hash.len(), 16, "the inner function is hashed");
}

#[test]
fn esbuilds_minified_form_is_the_bare_function_factory() {
    // Minified esbuild reverts to bun's shape — bare function first arg —
    // and the helper name is minified away; the tight `{exports:{}}`
    // marker identifies it exactly as bun's does.
    let src = concat!(
        "var l=(r,e)=>()=>(e||r((e={exports:{}}).exports,e),e.exports);\n",
        "var c=l((k,u)=>{u.exports={value:1}});\n",
    );
    let factories = classify_of(src);
    assert_eq!(factories.len(), 1);
    assert_eq!(factories[0].factory_var, "c");
    assert!(
        factories[0].source_path.is_none(),
        "no source path survives minification"
    );
}

#[test]
fn bun_bundles_never_pick_up_the_esbuild_helper_name() {
    // The bun-first helper identification is unchanged: a bundle whose
    // factories call bun's helper keeps classifying by the tight marker
    // even when the text mentions an esbuild helper name.
    let src = format!("{HELPER} var tO8=x((q,m)=>{{module.exports=1;}}); var __commonJS = 1;");
    let factories = classify_of(&src);
    assert_eq!(factories.len(), 1);
    assert_eq!(factories[0].factory_var, "tO8");
}
