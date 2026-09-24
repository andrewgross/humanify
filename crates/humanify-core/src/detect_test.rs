//! Detection tests (WPB.1). Three layers:
//!
//! 1. `detect.test.ts` ported fixture-for-fixture (`detect_bundle`).
//! 2. `signals/signals.test.ts` ported fixture-for-fixture (the per-signal
//!    detectors).
//! 3. The JS-regex-semantics cases the TS tests never needed because the
//!    TS gets them from V8 for free: `\s` is the ECMAScript WhiteSpace +
//!    LineTerminator set (NOT Unicode White_Space — U+FEFF yes, U+0085 no),
//!    `\b` is ASCII-only, `.` stops at all four line terminators, and the
//!    scan windows (16K, 200) count UTF-16 code units, not bytes.

use humanify_model::detection::{BundlerType, DetectionTier, MinifierType};

use crate::detect::detect_bundle;
use crate::detect::signals::{
    detect_browserify, detect_bun_bundler, detect_bun_minifier, detect_esbuild,
    detect_esbuild_minifier, detect_parcel, detect_swc_minifier, detect_terser, detect_webpack,
};

// ---- layer 1: detect.test.ts ------------------------------------------------

const WEBPACK: &str = "
/******/ (function(modules) { // webpackBootstrap
/******/   var installedModules = {};
/******/   function __webpack_require__(moduleId) {
/******/     if(installedModules[moduleId]) return installedModules[moduleId].exports;
";

const BROWSERIFY: &str = r#"
(function(){function r(e,n,t){function o(i,f){if(!n[i]){if(!e[i]){var c="function"==typeof require&&require;if(!f&&c)return c(i,!0);if(u)return u(i,!0);var a=new Error("Cannot find module '"+i+"'");throw a.code="MODULE_NOT_FOUND",a}var p=n[i]={exports:{}};e[i][0].call(p.exports,function(r){var n=e[i][1][r];return o(n||r)},p,p.exports,r,e,n,t)}return n[i].exports}for(var u="function"==typeof require&&require,i=0;i<t.length;i++)o(t[i]);return o})()({1:[function(require,module,exports){
var installedModules = {};
"#;

const ESBUILD: &str = "
var __defProp = Object.defineProperty;
var __export = (target, all) => { for (var name in all) __defProp(target, name, { get: all[name], enumerable: true }); };
var __commonJS = (cb, mod) => function() { return mod || (0, cb[Object.keys(cb)[0]])(mod = { exports: {} }), mod.exports; };
var __toESM = (mod) => __defProp(mod, \"__esModule\", { value: true });
";

const PARCEL: &str = "
parcelRequire = (function (modules, cache, entry, globalName) {
  var previousRequire = typeof parcelRequire === 'function' && parcelRequire;
  function newRequire(name, jumped) {
    if (!cache[name]) {
";

const BUN: &str = r#"import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,"default",{enumerable:!0,value:I}));"#;

const PLAIN: &str = "
function greet(name) {
  console.log(\"Hello, \" + name + \"!\");
}
greet(\"world\");
";

const SWC_MINIFIED: &str = "function _class_call_check(instance, Constructor) {
  if (!(instance instanceof Constructor)) throw new TypeError(\"Cannot call a class as a function\");
}
var _lib = _interop_require_default(require(\"lib\"));
var _default = void 0;";

const BUN_MINIFIED: &str = "var $a0=1,$b1=2,$c2=3,$d3=4,$e4=5,$f5=6,$g6=7,$h7=8,$i8=9,$j9=10,$kA=11,$lB=12;var u=void 0,t=!0;";

const ESBUILD_MINIFIED: &str = "// app.js
var a = void 0, b = !0, c = !1;";

const GENERIC_MINIFIED: &str = "var a=void 0,b=!0,c=!1;return a?b:c;";

fn assert_bundler(code: &str, kind: BundlerType, tier: DetectionTier) {
    let r = detect_bundle(code);
    assert_eq!(r.bundler.kind, kind);
    assert_eq!(r.bundler.tier, tier);
}

#[test]
fn detects_webpack_bundles() {
    assert_bundler(WEBPACK, BundlerType::Webpack, DetectionTier::Definitive);
}

#[test]
fn detects_browserify_bundles() {
    assert_bundler(
        BROWSERIFY,
        BundlerType::Browserify,
        DetectionTier::Definitive,
    );
}

#[test]
fn detects_esbuild_bundles() {
    assert_bundler(ESBUILD, BundlerType::Esbuild, DetectionTier::Definitive);
}

#[test]
fn detects_parcel_bundles() {
    assert_bundler(PARCEL, BundlerType::Parcel, DetectionTier::Definitive);
}

#[test]
fn detects_bun_cjs_bundles() {
    assert_bundler(BUN, BundlerType::Bun, DetectionTier::Definitive);
}

#[test]
fn returns_unknown_for_plain_js() {
    assert_bundler(PLAIN, BundlerType::Unknown, DetectionTier::Unknown);
}

#[test]
fn collects_all_matching_signals() {
    let r = detect_bundle(WEBPACK);
    assert!(!r.signals.is_empty());
    assert!(
        r.signals
            .iter()
            .filter_map(|s| s.bundler)
            .all(|b| b == BundlerType::Webpack)
    );
}

#[test]
fn no_cross_contamination_for_definitive_signals() {
    let cases = [
        (BundlerType::Webpack, WEBPACK),
        (BundlerType::Browserify, BROWSERIFY),
        (BundlerType::Esbuild, ESBUILD),
        (BundlerType::Parcel, PARCEL),
        (BundlerType::Bun, BUN),
    ];
    for (name, fixture) in cases {
        let r = detect_bundle(fixture);
        let mut unique: Vec<BundlerType> = r
            .signals
            .iter()
            .filter(|s| s.tier == DetectionTier::Definitive)
            .filter_map(|s| s.bundler)
            .collect();
        unique.dedup();
        assert_eq!(unique, vec![name], "{name:?} fixture");
    }
}

#[test]
fn classifies_swc_by_snake_case_helpers_not_terser() {
    assert_eq!(detect_bundle(SWC_MINIFIED).minifier.kind, MinifierType::Swc);
}

#[test]
fn classifies_bun_minified_despite_void_0() {
    assert_eq!(detect_bundle(BUN_MINIFIED).minifier.kind, MinifierType::Bun);
}

#[test]
fn classifies_esbuild_minified_despite_void_0() {
    assert_eq!(
        detect_bundle(ESBUILD_MINIFIED).minifier.kind,
        MinifierType::Esbuild
    );
}

#[test]
fn falls_back_to_terser_at_unknown_tier() {
    let r = detect_bundle(GENERIC_MINIFIED);
    assert_eq!(r.minifier.kind, MinifierType::Terser);
    assert_eq!(r.minifier.tier, DetectionTier::Unknown);
}

#[test]
fn reports_no_minifier_for_non_minified_code() {
    let r = detect_bundle(PLAIN);
    assert_eq!(r.minifier.kind, MinifierType::Unknown);
    assert_eq!(r.minifier.tier, DetectionTier::Unknown);
}

// ---- layer 2: signals.test.ts ----------------------------------------------

#[test]
fn webpack_signals() {
    let s = detect_webpack("var m = __webpack_require__(1);");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].bundler, Some(BundlerType::Webpack));
    assert_eq!(s[0].tier, DetectionTier::Definitive);
    assert_eq!(detect_webpack("var __webpack_modules__ = {};").len(), 1);
    let s = detect_webpack("(self.webpackChunkapp = self.webpackChunkapp || [])");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].pattern, "webpackChunk");
    assert!(detect_webpack("var __webpack_modules__ = {}; __webpack_require__(0);").len() >= 2);
    assert!(detect_webpack("console.log(\"hello\")").is_empty());
}

#[test]
fn browserify_signals() {
    let s = detect_browserify(
        "e[i][0].call(p.exports,function(r){var n=e[i][1][r];return o(n||r)},p,p.exports}",
    );
    assert!(s.iter().any(|x| x.bundler == Some(BundlerType::Browserify)));
    let s = detect_browserify("var installedModules = {};");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].bundler, Some(BundlerType::Browserify));
    assert_eq!(s[0].tier, DetectionTier::Definitive);
    assert!(
        detect_browserify("var installedModules = {}; function __webpack_require__(id) {}")
            .is_empty()
    );
    assert!(detect_browserify("console.log(\"hello\")").is_empty());
}

#[test]
fn esbuild_bundler_signals() {
    for code in [
        "var init_foo = __commonJS({",
        "var react = __toESM(require(\"react\"));",
        "module.exports = __toCommonJS(exports);",
        "var __export = (target, all) => { for (var name in all) {} };",
    ] {
        let s = detect_esbuild(code);
        assert_eq!(s.len(), 1, "{code}");
        assert_eq!(s[0].bundler, Some(BundlerType::Esbuild));
        assert_eq!(s[0].tier, DetectionTier::Definitive);
    }
    assert!(detect_esbuild("console.log(\"hello\")").is_empty());
}

#[test]
fn parcel_signals() {
    let s = detect_parcel("var parcelRequire;");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].bundler, Some(BundlerType::Parcel));
    assert_eq!(s[0].tier, DetectionTier::Definitive);
    let s = detect_parcel("var loader = require(\"_bundle_loader\");");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].bundler, Some(BundlerType::Parcel));
    assert!(detect_parcel("console.log(\"hello\")").is_empty());
}

const BUN_PREAMBLE: &str = r#"import{createRequire as Glq}from"node:module";var m6=Glq(import.meta.url);var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);"#;

#[test]
fn bun_bundler_signals() {
    let s = detect_bun_bundler(BUN_PREAMBLE);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].bundler, Some(BundlerType::Bun));
    assert_eq!(s[0].tier, DetectionTier::Definitive);
    let bytecode_cjs = "// @bun @bytecode @bun-cjs\n(function(exports, require, module, __filename, __dirname) {var vGc=require(\"module\"),K7h=vGc.createRequire(\"/\");var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);});";
    let s = detect_bun_bundler(bytecode_cjs);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].bundler, Some(BundlerType::Bun));
    assert_eq!(s[0].tier, DetectionTier::Definitive);
    assert!(detect_bun_bundler("var doc = \"see @bun docs\";\nfunction f(){}").is_empty());
}

#[test]
fn bun_bundler_requires_both_patterns() {
    assert!(
        detect_bun_bundler(
            "var x = (I, A) => () => (A || I((A = {exports: {}}).exports, A), A.exports);"
        )
        .is_empty()
    );
    assert!(
        detect_bun_bundler(
            r#"import{createRequire as X}from"node:module";var r=X(import.meta.url);var y=r("fs");"#
        )
        .is_empty()
    );
    let esbuild_code = "var __commonJS = (cb, mod) => function() { return mod || (0, cb[Object.keys(cb)[0]])(mod = { exports: {} }), mod.exports; };";
    assert!(detect_bun_bundler(esbuild_code).is_empty());
    assert!(detect_bun_bundler("console.log(\"hello\")").is_empty());
}

#[test]
fn terser_signals() {
    let s = detect_terser("if (x === void 0) return;");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].minifier, Some(MinifierType::Terser));
    assert_eq!(s[0].tier, DetectionTier::Unknown);
    let s = detect_terser("return !0;");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].pattern, "!0/!1 boolean coercion");
    assert_eq!(s[0].tier, DetectionTier::Unknown);
    assert!(detect_terser("const x = true; return undefined;").is_empty());
}

#[test]
fn esbuild_minifier_signals() {
    let s = detect_esbuild_minifier("// index.js\nvar x = 1;");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].minifier, Some(MinifierType::Esbuild));
    assert_eq!(s[0].tier, DetectionTier::Likely);
    assert!(detect_esbuild_minifier("var x = 1;").is_empty());
}

#[test]
fn bun_minifier_signals() {
    let code = "$a0 = 1; $bC = 2; $cD = 3; $dE = 4; $eF = 5; $fG = 6; $gH = 7; $hI = 8; $iJ = 9; $jK = 10; $kL = 11;";
    let s = detect_bun_minifier(code);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].minifier, Some(MinifierType::Bun));
    assert_eq!(s[0].tier, DetectionTier::Likely);
    assert!(detect_bun_minifier("$a0 = 1; $bC = 2;").is_empty());
}

#[test]
fn swc_minifier_signals() {
    let s = detect_swc_minifier("function _class_call_check(instance, Constructor) {}");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].minifier, Some(MinifierType::Swc));
    assert_eq!(s[0].tier, DetectionTier::Likely);
    let s = detect_swc_minifier("var _lib = _interop_require_default(require(\"lib\"));");
    assert_eq!(s.len(), 1);
    assert!(detect_swc_minifier("function _classCallCheck(i, C) {}").is_empty());
    assert!(detect_swc_minifier("var a = _toConsumableArray(b);").is_empty());
    assert!(detect_swc_minifier("var x = _extends({}, y);").is_empty());
    assert!(detect_swc_minifier("_inherits(Sub, Super);").is_empty());
    assert!(detect_swc_minifier("const x = 1; return x;").is_empty());
}

// ---- layer 3: JS regex semantics -------------------------------------------

#[test]
fn js_whitespace_is_ecmascript_not_unicode() {
    // U+FEFF is JS \s (and not Unicode White_Space); U+0085 is the reverse.
    assert_eq!(detect_esbuild("var\u{feff}__export = 1").len(), 1);
    assert_eq!(detect_esbuild("var\u{a0}\u{3000}__export = 1").len(), 1);
    assert!(detect_esbuild("var\u{85}__export = 1").is_empty());
    assert_eq!(detect_browserify("x[0].call(m.exports\u{2028}}").len(), 1);
    assert!(detect_browserify("x[0].call(m.exports\u{85}}").is_empty());
}

#[test]
fn js_word_boundary_is_ascii_only() {
    // `é` is not an ASCII word char, so `\b` holds between it and `_`.
    assert_eq!(detect_esbuild("é__commonJS(").len(), 1);
    assert!(detect_esbuild("a__commonJS(").is_empty());
    assert!(detect_esbuild("__commonJS2(").is_empty());
    assert_eq!(detect_esbuild("__commonJSé(").len(), 1);
    assert_eq!(detect_terser("x=!0é").len(), 1);
    assert!(detect_terser("x=!0a").is_empty());
}

#[test]
fn js_dot_stops_at_every_line_terminator() {
    assert!(detect_esbuild_minifier("// a\u{2028}b.js\n").is_empty());
    assert!(detect_esbuild_minifier("// a\rb.js\n").is_empty());
    assert!(detect_esbuild_minifier("// .js\n").is_empty());
    assert_eq!(detect_esbuild_minifier("// x.js\n").len(), 1);
    assert_eq!(detect_esbuild_minifier("// é\u{1F600}.js\n").len(), 1);
}

#[test]
fn esbuild_banner_window_is_200_utf16_units() {
    // 97 astral chars = 194 units; "// a.js\n" then straddles unit 200.
    let pad: String = "\u{1F600}".repeat(97);
    assert!(detect_esbuild_minifier(&format!("{pad}// a.js\n")).is_empty());
    let pad: String = "\u{1F600}".repeat(96); // 192 units: banner ends at 200
    assert_eq!(detect_esbuild_minifier(&format!("{pad}// a.js\n")).len(), 1);
}

#[test]
fn scan_window_is_16k_utf16_units() {
    // 8,190 astral chars = 16,380 units; `parcelRequire` then crosses 16,384.
    let pad: String = "\u{1F600}".repeat(8190);
    let r = detect_bundle(&format!("{pad}parcelRequire"));
    assert_eq!(r.bundler.kind, BundlerType::Unknown);
    let r = detect_bundle(&format!("{pad}!0"));
    assert_eq!(r.minifier.kind, MinifierType::Terser);
    // A surrogate pair straddling the cut: JS keeps a lone high surrogate,
    // which is a non-word char exactly like end-of-input.
    let pad: String = "a".repeat(16_381);
    let r = detect_bundle(&format!("{pad}!0\u{1F600}"));
    assert_eq!(r.minifier.kind, MinifierType::Terser);
}

#[test]
fn create_require_import_cannot_cross_a_brace() {
    let code = r#"import{a}x{createRequire}from"node:module";var q={exports:{}};"#;
    assert!(detect_bun_bundler(code).is_empty());
    let code = "import \u{feff}{ x, createRequire as C } from 'node:module';var q={exports:\t{}};";
    assert_eq!(detect_bun_bundler(code).len(), 1);
}

#[test]
fn bun_banner_is_anchored_at_input_start() {
    assert_eq!(
        detect_bun_bundler("\u{feff}\n  //\u{a0}@bun @bytecode").len(),
        1
    );
    assert!(detect_bun_bundler("x\n// @bun").is_empty());
    assert!(detect_bun_bundler("// @bunx").is_empty());
    assert_eq!(detect_bun_bundler("// @bun-cjs").len(), 1);
}

#[test]
fn result_serializes_as_the_ts_json() {
    let r = detect_bundle("var a=void 0;var parcelRequire;");
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"bundler":{"type":"parcel","tier":"definitive"},"minifier":{"type":"terser","tier":"unknown"},"signals":[{"source":"parcel","pattern":"parcelRequire","bundler":"parcel","tier":"definitive"},{"source":"terser","pattern":"void 0","minifier":"terser","tier":"unknown"}]}"#
    );
}
