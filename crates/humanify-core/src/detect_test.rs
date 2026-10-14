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

use humanify_model::detection::{
    BundlerType, ConflictResolution, DetectionTier, MinifierType, SignalStrength,
};

use crate::detect::signals::{
    detect_browserify, detect_bun_bundler, detect_bun_minifier, detect_esbuild,
    detect_esbuild_minifier, detect_parcel, detect_swc_minifier, detect_terser, detect_webpack,
};
use crate::detect::{detect_bundle, pick_bundler};

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
    // One common word: a weak token, `likely` (review R19).
    assert_eq!(s[0].tier, DetectionTier::Likely);
    assert_eq!(s[0].strength, Some(SignalStrength::WeakToken));
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
    // The module helper alone is not Bun's Shape: it is the plain form
    // esbuild writes too, so only the shared rank (spec I2).
    let helper_alone = detect_bun_bundler(
        "var x = (I, A) => () => (A || I((A = {exports: {}}).exports, A), A.exports);",
    );
    assert_eq!(helper_alone.len(), 1);
    assert_eq!(helper_alone[0].strength, Some(SignalStrength::SharedShape));
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

// ---- layer 4: the verdict is decided by strength, not list order ------------
// (toolchain review R3/R19, 2026-10-04; the rules are on `SignalStrength`)

/// A real `bun build --target=bun --format=cjs` (Bun 1.3.14, unminified):
/// the `// @bun @bun-cjs` banner AND esbuild's helper names, which Bun's
/// runtime writes too (test/e2e/fixtures/bun-bundle/README.md).
const REAL_BUN_BUILDS: [&str; 2] = [
    include_str!("../../../test/e2e/fixtures/bun-bundle/build/v1.0.0/build/index.js"),
    include_str!("../../../test/e2e/fixtures/bun-bundle/build/v1.1.0/build/index.js"),
];

#[test]
fn a_real_unminified_bun_build_is_detected_as_bun() {
    for build in REAL_BUN_BUILDS {
        let r = detect_bundle(build);
        // The build carries both bundlers' signals: the conflict is real.
        assert!(
            r.signals
                .iter()
                .any(|s| s.bundler == Some(BundlerType::Esbuild))
        );
        assert_eq!(r.bundler.kind, BundlerType::Bun);
        assert_eq!(r.bundler.tier, DetectionTier::Definitive);
        let conflict = r.bundler.conflict.expect("two bundlers named: a conflict");
        assert_eq!(conflict.resolution, ConflictResolution::Strength);
        assert_eq!(conflict.candidates[0].bundler, BundlerType::Bun);
        assert_eq!(conflict.candidates[0].strength, SignalStrength::Banner);
        assert_eq!(conflict.candidates[1].bundler, BundlerType::Esbuild);
        assert_eq!(
            conflict.candidates[1].strength,
            SignalStrength::SharedHelperName
        );
    }
}

#[test]
fn a_bun_banner_outranks_esbuild_helper_names() {
    // R3's sample: list order (esbuild before bun) used to win.
    let code = "// @bun\nvar __commonJS = (cb, mod) => () => (mod || cb((mod = { exports: {} }).exports, mod), mod.exports);\n";
    let r = detect_bundle(code);
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    assert_eq!(r.bundler.tier, DetectionTier::Definitive);
}

#[test]
fn the_ranking_is_banner_runtime_name_shape_shared_helper_weak_token() {
    use SignalStrength as S;
    assert!(S::Banner > S::OwnRuntimeName);
    assert!(S::OwnRuntimeName > S::Shape);
    assert!(S::Shape > S::SharedShape);
    assert!(S::SharedShape > S::SharedHelperName);
    assert!(S::SharedHelperName > S::WeakToken);
    assert_eq!(S::WeakToken.tier(), DetectionTier::Likely);
    for s in [
        S::SharedHelperName,
        S::SharedShape,
        S::Shape,
        S::OwnRuntimeName,
        S::Banner,
    ] {
        assert_eq!(s.tier(), DetectionTier::Definitive);
    }
}

#[test]
fn every_bundler_signal_carries_its_strength() {
    use SignalStrength as S;
    let cases: [(&str, S); 6] = [
        (WEBPACK, S::OwnRuntimeName),
        (PARCEL, S::OwnRuntimeName),
        (ESBUILD, S::SharedHelperName),
        (BUN, S::Shape),
        ("// @bun\n", S::Banner),
        ("var installedModules = {};", S::WeakToken),
    ];
    for (code, strength) in cases {
        let r = detect_bundle(code);
        let bundler_signals: Vec<_> = r.signals.iter().filter(|s| s.bundler.is_some()).collect();
        assert!(!bundler_signals.is_empty(), "{code}");
        assert!(
            bundler_signals.iter().all(|s| s.strength == Some(strength)),
            "{code}"
        );
    }
    let b = detect_bundle(BROWSERIFY);
    assert!(b.signals.iter().any(|s| s.strength == Some(S::Shape)));
    // Minifier signals carry no bundler strength.
    assert!(
        detect_bundle(GENERIC_MINIFIED)
            .signals
            .iter()
            .all(|s| s.strength.is_none())
    );
}

#[test]
fn the_verdict_does_not_depend_on_signal_order() {
    let code = "// @bun\nvar __commonJS = 1; var x = __toESM(y);";
    let mut signals = detect_bundle(code).signals;
    let forward = pick_bundler(&signals);
    signals.reverse();
    let backward = pick_bundler(&signals);
    assert_eq!(forward.kind, BundlerType::Bun);
    assert_eq!(forward.kind, backward.kind);
    assert_eq!(forward.tier, backward.tier);
    assert_eq!(
        forward.conflict.map(|c| c.resolution),
        backward.conflict.map(|c| c.resolution)
    );
}

#[test]
fn installed_modules_alone_is_only_a_likely_browserify() {
    let r = detect_bundle("var installedModules = {};");
    assert_eq!(r.bundler.kind, BundlerType::Browserify);
    assert_eq!(r.bundler.tier, DetectionTier::Likely);
    assert!(r.bundler.conflict.is_none());
}

#[test]
fn a_definitive_signal_outranks_a_weak_token_of_another_bundler() {
    // A Bun banner plus a stray `installedModules`: Bun, with the weak
    // token listed as the outranked candidate.
    let r = detect_bundle("// @bun\nvar installedModules = {};");
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    assert_eq!(r.bundler.tier, DetectionTier::Definitive);
    let c = r.bundler.conflict.expect("two bundlers named");
    assert_eq!(c.resolution, ConflictResolution::Strength);
    assert_eq!(c.candidates.len(), 2);
    assert_eq!(c.candidates[1].strength, SignalStrength::WeakToken);
}

#[test]
fn a_tie_at_the_strongest_rank_is_unknown_and_says_so() {
    // Two bundlers' own runtime names: list order used to say webpack.
    let r = detect_bundle("var parcelRequire; __webpack_require__(1);");
    assert_eq!(r.bundler.kind, BundlerType::Unknown);
    assert_eq!(r.bundler.tier, DetectionTier::Unknown);
    let c = r.bundler.conflict.expect("a tie is a conflict");
    assert_eq!(c.resolution, ConflictResolution::Tie);
    let mut named: Vec<_> = c.candidates.iter().map(|k| k.bundler).collect();
    named.sort_by_key(|b| format!("{b:?}"));
    assert_eq!(named, vec![BundlerType::Parcel, BundlerType::Webpack]);
}

#[test]
fn signals_of_one_bundler_are_no_conflict() {
    let r = detect_bundle(ESBUILD);
    assert_eq!(r.bundler.kind, BundlerType::Esbuild);
    assert!(r.bundler.conflict.is_none());
    // ... and the serialized verdict keeps its pre-ranking shape.
    let json = serde_json::to_string(&r.bundler).unwrap();
    assert_eq!(json, r#"{"type":"esbuild","tier":"definitive"}"#);
}

#[test]
fn a_conflict_serializes_with_its_candidates() {
    let r = detect_bundle("// @bun\nvar __commonJS = 1;");
    assert_eq!(
        serde_json::to_string(&r.bundler).unwrap(),
        r#"{"type":"bun","tier":"definitive","conflict":{"resolution":"strength","candidates":[{"bundler":"bun","pattern":"// @bun banner","strength":"banner"},{"bundler":"esbuild","pattern":"__commonJS","strength":"shared-helper-name"}]}}"#
    );
}

// ---- layer 5: the module helper's SHAPE (spec I2) ---------------------------
// `--minify` renames every esbuild helper, so the names above vanish; the
// module helper's shape does not (`modules::helper_shape`). A form only
// esbuild writes (the `try` guard of 0.28, the indirect `(0, cb[…])` call)
// or the plain form beside esbuild's own `__export` names esbuild; the plain
// form alone is written by both bundlers, and an unsure input is Bun's.

/// The committed esbuild 0.27.2 `--bundle --minify --format=iife` build
/// (test/e2e/fixtures/esbuild-minified): the plain helper + esbuild's
/// `__export` (`{get:e[r],enumerable:!0}` — Bun's adds `configurable`).
const REAL_ESBUILD_MINIFIED: &str =
    include_str!("../../../test/e2e/fixtures/esbuild-minified/build/v1.0.0/build/index.js");

/// esbuild 0.28.2 `--minify`: the guarded helper (shape lab, 2026-10-12).
const ESBUILD_028_MINIFIED: &str = "var r=Object.defineProperty;var g=(n,e,o)=>()=>{if(o)throw o[0];try{return n&&(e=n(n=0)),e}catch(l){throw o=[l],l}};var a=(n,e)=>()=>{try{return e||n((e={exports:{}}).exports,e),e.exports}catch(o){throw e=0,o}},h=(n,e)=>{for(var o in e)r(n,o,{get:e[o],enumerable:!0})};var u=a(c=>{c.label=\"x\"});";

/// wrangler 3.114.0's cli.js re-minified by esbuild 0.28.2 (experiments/095
/// discovery, Results §5): the indirect function form, every helper joined.
const WRANGLER_REMINIFIED: &str = "(function(){var cye=Object.create,fp=Object.defineProperty,UD=Object.getOwnPropertyNames,a=(e,t)=>fp(e,\"name\",{value:t,configurable:!0}),q=(e,t)=>function(){return e&&(t=(0,e[UD(e)[0]])(e=0)),t},ee=(e,t)=>function(){return t||(0,e[UD(e)[0]])((t={exports:{}}).exports,t),t.exports};var x=ee((t,n)=>{n.exports=1});})();";

/// Bun 1.3.14 `build --target=node --format=cjs --minify`: no banner, no
/// `createRequire` import — only the plain helper (shape lab, 2026-10-12).
const BUN_MINIFIED_NO_BANNER: &str = "var{defineProperty:t,getOwnPropertyNames:p,getOwnPropertyDescriptor:i}=Object,b=Object.prototype.hasOwnProperty;var s,g=(e,n)=>()=>(n||e((n={exports:{}}).exports,n),n.exports);var h=(e)=>e;var y=(e,n)=>{for(var o in n)t(e,o,{get:n[o],enumerable:!0,configurable:!0,set:q.bind(n,o)})};var a=g((r)=>{r.label=\"cjsdep\"});";

#[test]
fn minified_esbuild_is_detected_by_its_helper_shapes() {
    for (code, what) in [
        (
            REAL_ESBUILD_MINIFIED,
            "0.27.2 --minify (plain helper + __export)",
        ),
        (ESBUILD_028_MINIFIED, "0.28.2 --minify (guarded helper)"),
        (WRANGLER_REMINIFIED, "re-minified (indirect helper)"),
    ] {
        let r = detect_bundle(code);
        assert_eq!(r.bundler.kind, BundlerType::Esbuild, "{what}");
        assert_eq!(r.bundler.tier, DetectionTier::Definitive, "{what}");
        let esbuild = detect_esbuild(code);
        assert!(
            esbuild
                .iter()
                .any(|s| s.strength == Some(SignalStrength::Shape)),
            "{what}: an esbuild shape signal"
        );
    }
}

#[test]
fn the_plain_helper_alone_is_bun_when_unsure() {
    let r = detect_bundle(BUN_MINIFIED_NO_BANNER);
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    assert_eq!(r.bundler.tier, DetectionTier::Definitive);
    // Bun's own `__export` (with `configurable` + `set`) is Bun's shape,
    // and not esbuild's.
    let bun = detect_bun_bundler(BUN_MINIFIED_NO_BANNER);
    assert_eq!(bun.len(), 1);
    assert_eq!(bun[0].strength, Some(SignalStrength::Shape));
    assert!(detect_esbuild(BUN_MINIFIED_NO_BANNER).is_empty());
    // The plain helper with no registrar at all: the shared shape, Bun's.
    let plain = "var g=(e,n)=>()=>(n||e((n={exports:{}}).exports,n),n.exports);var a=g((r)=>{r.label=\"cjsdep\"});";
    let r = detect_bundle(plain);
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    let bun = detect_bun_bundler(plain);
    assert_eq!(bun[0].strength, Some(SignalStrength::SharedShape));
}

#[test]
fn an_esbuild_shape_outranks_the_shared_shape() {
    // The 0.27.2 build carries the plain helper Bun writes too: both
    // bundlers are named, and esbuild's own shape settles it.
    let r = detect_bundle(REAL_ESBUILD_MINIFIED);
    let conflict = r.bundler.conflict.expect("both bundlers named");
    assert_eq!(conflict.resolution, ConflictResolution::Strength);
    assert_eq!(conflict.candidates[0].bundler, BundlerType::Esbuild);
    assert_eq!(conflict.candidates[1].bundler, BundlerType::Bun);
    assert_eq!(conflict.candidates[1].strength, SignalStrength::SharedShape);
}

#[test]
fn the_helper_shapes_leave_banner_and_unminified_verdicts_alone() {
    // A Bun banner still returns the banner alone.
    for build in REAL_BUN_BUILDS {
        assert_eq!(detect_bun_bundler(build).len(), 1);
        assert_eq!(detect_bundle(build).bundler.kind, BundlerType::Bun);
    }
    // Unminified esbuild: still esbuild (its helper names AND now its
    // indirect helper shape), and no Bun signal.
    let unminified =
        include_str!("../../../test/e2e/fixtures/esbuild-cjs/build/v1.0.0/build/index.js");
    let r = detect_bundle(unminified);
    assert_eq!(r.bundler.kind, BundlerType::Esbuild);
    assert!(detect_bun_bundler(unminified).is_empty());
}

// ---- layer 6: the OUTER bundle's runtime (exp096 held-out census) ---------
// A bundle's own runtime comes first; once the module helper is CALLED the
// text is module bodies — vendored code that may itself have been built by
// esbuild or webpack, or ship a bundle as a string. Bun writes no banner for
// `--target node`, so on those builds a dependency's runtime used to name
// the bundler. The fixtures are hand-written, after the shapes Bun 1.4.0
// `--minify --target node` writes (exp096 corpus).

/// Bun `--target node --minify`'s prologue: the plain helper and Bun's own
/// `__export` registrar (`configurable` + a bound `set`).
const BUN_NODE_PROLOGUE: &str = "var a=Object.create;var{getPrototypeOf:b,defineProperty:c,getOwnPropertyNames:d}=Object,e=Object.prototype.hasOwnProperty;function g(t){return this[t]}var f=(t,n)=>()=>(n||t((n={exports:{}}).exports,n),n.exports);var h=(t)=>t;function k(t,n){this[t]=h.bind(null,n)}var m=(t,n)=>{for(var o in n)c(t,o,{get:n[o],enumerable:!0,configurable:!0,set:k.bind(n,o)})};";

#[test]
fn an_esbuild_registrar_inside_a_module_body_is_not_the_bundler() {
    let code = format!(
        "{BUN_NODE_PROLOGUE}var p=f(function(x,y){{var q=Object.defineProperty,r=(u,v)=>{{for(var w in v)q(u,w,{{get:v[w],enumerable:!0}})}};y.exports=1}});"
    );
    let r = detect_bundle(&code);
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    let bun = detect_bun_bundler(&code);
    assert_eq!(
        bun[0].strength,
        Some(SignalStrength::Shape),
        "Bun's registrar is Bun's own shape"
    );
}

#[test]
fn a_bundle_shipped_as_a_string_is_not_the_bundler() {
    let code = "var f=(t,n)=>()=>(n||t((n={exports:{}}).exports,n),n.exports);var p=f(function(x,y){y.exports='var q=Object.defineProperty,r=(u,v)=>{for(var w in v)q(u,w,{get:v[w],enumerable:!0})};'});";
    assert_eq!(detect_bundle(code).bundler.kind, BundlerType::Bun);
}

#[test]
fn a_webpack_name_inside_a_module_body_is_not_the_bundler() {
    let code = "var f=(t,n)=>()=>(n||t((n={exports:{}}).exports,n),n.exports);var p=f(function(x,y){y.exports=function(){return __webpack_require__(1)}});";
    let r = detect_bundle(code);
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    assert!(r.bundler.conflict.is_none(), "{:?}", r.bundler.conflict);
}

#[test]
fn of_two_registrars_in_the_prologue_the_first_is_the_bundlers() {
    // Bun ESM (`createRequire`) with a hoisted ESM dependency that carries
    // an esbuild-like registrar of its own, before any module call.
    let code = format!(
        "import{{createRequire as z}}from\"node:module\";{BUN_NODE_PROLOGUE}var s=z(import.meta.url);var i=(A,e)=>{{let t={{}};for(var r in A)c(t,r,{{get:A[r],enumerable:!0}});return t}};var p=f(function(x,y){{y.exports=1}});"
    );
    let r = detect_bundle(&code);
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    assert!(
        detect_esbuild(&code)
            .iter()
            .all(|s| s.strength != Some(SignalStrength::Shape))
    );
}

#[test]
fn bun_s_sourcemap_debug_id_names_bun() {
    // Bun ends every debug ID with "dun!dun!" in hex.
    let r = detect_bundle("module.exports={};\n\n//# debugId=C1145FCE225F6B6464756E2164756E21\n");
    assert_eq!(r.bundler.kind, BundlerType::Bun);
    assert_eq!(r.bundler.tier, DetectionTier::Definitive);
    // Any other debug ID is not Bun's.
    let other = detect_bundle("module.exports={};\n//# debugId=C1145FCE225F6B640123456789ABCDEF\n");
    assert_eq!(other.bundler.kind, BundlerType::Unknown);
    // Nor is the mark anywhere but the trailing comment.
    let inner =
        detect_bundle("var s='//# debugId=C1145FCE225F6B6464756E2164756E21';\nmodule.exports=s;\n");
    assert_eq!(inner.bundler.kind, BundlerType::Unknown);
}

#[test]
fn a_bun_registrar_inside_an_esbuild_module_body_leaves_esbuild() {
    let code = "var c=Object.defineProperty;var f=(t,n)=>()=>(n||t((n={exports:{}}).exports,n),n.exports),m=(t,n)=>{for(var o in n)c(t,o,{get:n[o],enumerable:!0})};var p=f(function(x,y){var q=(u,v)=>{for(var w in v)c(u,w,{get:v[w],enumerable:!0,configurable:!0,set:k.bind(v,w)})};y.exports=1});";
    assert_eq!(detect_bundle(code).bundler.kind, BundlerType::Esbuild);
}

#[test]
fn a_webpack_bootstrap_with_a_bundled_dependency_stays_webpack() {
    let code = "(function(modules){function __webpack_require__(id){var module={exports:{}};modules[id](module,module.exports,__webpack_require__);return module.exports}return __webpack_require__(1)})({1:function(module,exports,__webpack_require__){var f=(t,n)=>()=>(n||t((n={exports:{}}).exports,n),n.exports);var p=f(function(x,y){y.exports=1});module.exports=p()}});";
    let r = detect_bundle(code);
    assert_eq!(r.bundler.kind, BundlerType::Webpack);
}

// ---- exp105: minified webpack 5 and rollup, by SHAPE ---------------------------

/// Real builds of one tiny app with the index builder's recipes
/// (test/fixtures/bundlers/README.md).
const WEBPACK5_TERSER: &str =
    include_str!("../../../test/fixtures/bundlers/webpack5-terser.js.fixture");
const ROLLUP4_TERSER: &str =
    include_str!("../../../test/fixtures/bundlers/rollup4-terser.js.fixture");

fn verdict(code: &str) -> (BundlerType, DetectionTier) {
    let r = detect_bundle(code);
    (r.bundler.kind, r.bundler.tier)
}

#[test]
fn a_minified_webpack_5_build_is_webpack_by_its_module_table() {
    // terser renames every runtime name: nothing but the shape is left
    assert!(!WEBPACK5_TERSER.contains("__webpack_require__"));
    assert_eq!(
        verdict(WEBPACK5_TERSER),
        (BundlerType::Webpack, DetectionTier::Definitive)
    );
    let s = detect_webpack(WEBPACK5_TERSER);
    assert!(s.iter().all(|s| s.strength == Some(SignalStrength::Shape)));
}

#[test]
fn every_module_table_form_opens_a_webpack_bootstrap() {
    for code in [
        // methods (webpack 5.100+), arrows, functions, a one-parameter arrow
        "(()=>{var e={585(e){e.exports=1}},t={};",
        "(()=>{var e={123:(e,t,r)=>{\"use strict\";t.a=1}},t={};",
        "!function(){var e={7:function(e,t,r){e.exports=2}};",
        "(()=>{\"use strict\";var e={9:e=>{e.exports=3}};",
        "#!/usr/bin/env node\n/*! For license information please see out.js.LICENSE.txt */\n(()=>{var e={1(e){}}})();",
        "(function(){var e={42:function(e){e.exports=4}};",
    ] {
        assert_eq!(
            verdict(code),
            (BundlerType::Webpack, DetectionTier::Definitive),
            "{code}"
        );
    }
}

#[test]
fn webpack_s_require_function_and_definer_are_shapes_too() {
    // webpack 4: the runtime comes first, the modules are its argument
    let v4 = "!function(e){var t={};function n(r){if(t[r])return t[r].exports;var o=t[r]={i:r,l:!1,exports:{}};return e[r].call(o.exports,o,o.exports,n),o.l=!0,o.exports}n.m=e}([function(e,t){e.exports=1}]);";
    assert_eq!(
        verdict(v4),
        (BundlerType::Webpack, DetectionTier::Definitive)
    );
    // webpack 5 with every module concatenated: no table, the definer stays
    let flat = "(()=>{\"use strict\";var e={d:(t,o)=>{for(var n in o)e.o(o,n)&&!e.o(t,n)&&Object.defineProperty(t,n,{enumerable:!0,get:o[n]})},o:(e,t)=>Object.prototype.hasOwnProperty.call(e,t)};var t={};e.d(t,{a:()=>r});const r=1;module.exports=t})();";
    assert_eq!(
        verdict(flat),
        (BundlerType::Webpack, DetectionTier::Definitive)
    );
}

#[test]
fn an_object_of_functions_that_is_not_a_bootstrap_is_not_webpack() {
    for code in [
        // not the first statement: an app's own lookup table
        "var a=1;var e={1(e){return e}};",
        // string keys
        "(()=>{var e={a(e){return e}}})();",
        // a plain IIFE
        "(()=>{var e=1;console.log(e)})();",
        PLAIN,
    ] {
        assert_eq!(verdict(code).0, BundlerType::Unknown, "{code}");
    }
}

#[test]
fn a_minified_rollup_build_is_rollup_by_the_commonjs_plugin_s_shapes() {
    assert_eq!(
        verdict(ROLLUP4_TERSER),
        (BundlerType::Rollup, DetectionTier::Definitive)
    );
}

#[test]
fn the_commonjs_plugin_s_helpers_and_guards_name_rollup() {
    for code in [
        // getDefaultExportFromCjs, minified and as rollup writes it
        "\"use strict\";function e(e){return e&&e.__esModule&&Object.prototype.hasOwnProperty.call(e,\"default\")?e.default:e}",
        "'use strict';\nfunction getDefaultExportFromCjs (x) {\n\treturn x && x.__esModule && Object.prototype.hasOwnProperty.call(x, 'default') ? x['default'] : x;\n}\n",
        // commonjsGlobal
        "var c=\"undefined\"!=typeof globalThis?globalThis:\"undefined\"!=typeof window?window:\"undefined\"!=typeof global?global:\"undefined\"!=typeof self?self:{};",
        "var commonjsGlobal = typeof globalThis !== 'undefined' ? globalThis : typeof window !== 'undefined' ? window : typeof global !== 'undefined' ? global : typeof self !== 'undefined' ? self : {};",
        // two guarded require functions
        "var r,t,s,n;var a=function(){if(t)return r;t=1;return r=1}(),b=function(){if(n)return s;n=1;return s=2}();",
        "var ms, hasRequiredMs;\nfunction requireMs () {\n\tif (hasRequiredMs) return ms;\n\thasRequiredMs = 1;\n\treturn ms = 1;\n}\nvar x, hasRequiredX;\nfunction requireX () {\n\tif (hasRequiredX) return x;\n\thasRequiredX = 1;\n\treturn x = 2;\n}\n",
    ] {
        assert_eq!(
            verdict(code),
            (BundlerType::Rollup, DetectionTier::Definitive),
            "{code}"
        );
    }
}

#[test]
fn one_guard_alone_proves_nothing() {
    let once = "var r,t;function a(){if(t)return r;t=1;return r=1}";
    assert_eq!(verdict(once).0, BundlerType::Unknown);
}

#[test]
fn rollup_s_shapes_inside_another_bundler_s_output_are_not_rollup() {
    // a Bun bundle that hoisted a rollup-built dist: the module helper says
    // this is not rollup's output, whatever its modules hold
    let bun = format!(
        "{BUN}function e(e){{return e&&e.__esModule&&Object.prototype.hasOwnProperty.call(e,\"default\")?e.default:e}}"
    );
    assert_eq!(verdict(&bun).0, BundlerType::Bun);
    // a webpack bundle whose first module is rollup output
    let wp = "(()=>{var e={1(e){function t(e){return e&&e.__esModule&&Object.prototype.hasOwnProperty.call(e,\"default\")?e.default:e}e.exports=t}}})();";
    assert_eq!(verdict(wp).0, BundlerType::Webpack);
}

#[test]
fn the_real_bun_and_esbuild_builds_keep_their_verdicts() {
    for build in REAL_BUN_BUILDS {
        let s = detect_bundle(build).signals;
        assert!(
            !s.iter()
                .any(|s| matches!(s.bundler, Some(BundlerType::Webpack | BundlerType::Rollup))),
            "a Bun build named webpack or rollup"
        );
    }
}
