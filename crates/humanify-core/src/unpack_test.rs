//! Unpack tests (WPB.2), ported fixture-for-fixture from
//! `src/unpack/adapters/bun.test.ts` and `src/unpack/select-adapter.test.ts`,
//! plus the regex-floor and require-tracing cases the TS gets from V8 (the
//! patterns are emulated by hand here — `unpack::bun`'s docs say which).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::detect::detect_bundle;
use crate::modules::CarriedName;
use crate::modules::vendor_names::{VendorNameRequest, VendorNamer};
use crate::toolchain::Reason;
use crate::unpack::bun::{
    BunUnpackOptions, ExtractedModule, PriorVendor, extract_factory_bodies, find_prior_tree_root,
    identify_bun_require, load_prior_vendor, rewrite_require_calls, unpack_bun,
};
use crate::unpack::webcrack::parse_shim_output;
use crate::unpack::{UnpackAdapter, choose_adapter};
use humanify_model::detection::BundlerType;

const BUN_BUNDLE: &str = concat!(
    "import{createRequire as Glq}from\"node:module\";\n",
    "var m6=Glq(import.meta.url);\n",
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
    "var mod_a=x((exports,module)=>{\n",
    "  var dep=m6(\"node:path\");\n",
    "  function helper(){return dep.join(\"a\",\"b\")}\n",
    "  module.exports=helper;\n",
    "});\n",
    "var mod_b=x((exports)=>{\n",
    "  exports.value=42;\n",
    "});\n",
    "var main=mod_a();"
);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "humanify-unpack-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn unpack(code: &str, dir: &Path) {
    unpack_bun(code, dir, BunUnpackOptions::default()).expect("unpack runs");
}

fn read_manifest(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join("vendor/_bun-modules.json")).unwrap())
        .unwrap()
}

fn factories(manifest: &Value) -> Vec<Value> {
    manifest["factories"].as_array().unwrap().clone()
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn is_lib_hash_js(file_name: &str) -> bool {
    file_name
        .strip_prefix("vendor/lib_")
        .and_then(|r| r.strip_suffix(".js"))
        .is_some_and(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// A namer answering from a closure, recording every batch's keys.
struct FnNamer<F: FnMut(&VendorNameRequest) -> Option<String>> {
    answer: F,
    asked: Vec<Vec<String>>,
}

impl<F: FnMut(&VendorNameRequest) -> Option<String>> VendorNamer for FnNamer<F> {
    fn name_batch(&mut self, requests: Vec<VendorNameRequest>) -> Vec<Option<String>> {
        self.asked
            .push(requests.iter().map(|r| r.key.clone()).collect());
        requests.iter().map(&mut self.answer).collect()
    }
}

// ---- select-adapter.test.ts -----------------------------------------------

const WEBPACK: &str = "\n/******/ (function(modules) { // webpackBootstrap\n/******/   var installedModules = {};\n/******/   function __webpack_require__(moduleId) {\n/******/     if(installedModules[moduleId]) return installedModules[moduleId].exports;\n";
const BROWSERIFY: &str = "\n(function(){function r(e,n,t){function o(i,f){if(!n[i]){if(!e[i]){var c=\"function\"==typeof require&&require;if(!f&&c)return c(i,!0);if(u)return u(i,!0);var a=new Error(\"Cannot find module '\"+i+\"'\");throw a.code=\"MODULE_NOT_FOUND\",a}var p=n[i]={exports:{}};e[i][0].call(p.exports,function(r){var n=e[i][1][r];return o(n||r)},p,p.exports,r,e,n,t)}return n[i].exports}for(var u=\"function\"==typeof require&&require,i=0;i<t.length;i++)o(t[i]);return o})()({1:[function(require,module,exports){\nvar installedModules = {};\n";
const ESBUILD: &str = "\nvar __defProp = Object.defineProperty;\nvar __export = (target, all) => { for (var name in all) __defProp(target, name, { get: all[name], enumerable: true }); };\nvar __commonJS = (cb, mod) => function() { return mod || (0, cb[Object.keys(cb)[0]])(mod = { exports: {} }), mod.exports; };\nvar __toESM = (mod) => __defProp(mod, \"__esModule\", { value: true });\n";
const BUN_HEAD: &str = "import{createRequire as Glq}from\"node:module\";var m6=Glq(import.meta.url);var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,\"default\",{enumerable:!0,value:I}));";
const PLAIN: &str =
    "\nfunction greet(name) {\n  console.log(\"Hello, \" + name + \"!\");\n}\ngreet(\"world\");\n";

#[test]
fn selects_webcrack_for_webpack() {
    assert_eq!(
        choose_adapter(&detect_bundle(WEBPACK), None).piece.name(),
        "webcrack"
    );
}

#[test]
fn selects_webcrack_for_browserify() {
    assert_eq!(
        choose_adapter(&detect_bundle(BROWSERIFY), None)
            .piece
            .name(),
        "webcrack"
    );
}

#[test]
fn selects_bun_for_bun_cjs() {
    assert_eq!(
        choose_adapter(&detect_bundle(BUN_HEAD), None).piece.name(),
        "bun"
    );
}

#[test]
fn selects_esbuild_for_esbuild() {
    // The pre-exp075-port behavior was passthrough (no esbuild reader);
    // the TS-era reference (ac56eac0) established the module form.
    assert_eq!(
        choose_adapter(&detect_bundle(ESBUILD), None).piece.name(),
        "esbuild"
    );
}

#[test]
fn selects_passthrough_for_unknown() {
    assert_eq!(
        choose_adapter(&detect_bundle(PLAIN), None).piece.name(),
        "passthrough"
    );
}

#[test]
fn respects_bundler_override() {
    assert_eq!(
        choose_adapter(&detect_bundle(PLAIN), Some(BundlerType::Webpack))
            .piece
            .name(),
        "webcrack"
    );
}

#[test]
fn override_to_unknown_is_ignored() {
    assert_eq!(
        choose_adapter(&detect_bundle(WEBPACK), Some(BundlerType::Unknown))
            .piece
            .name(),
        "webcrack"
    );
}

#[test]
fn the_adapter_choice_says_why() {
    assert_eq!(
        choose_adapter(&detect_bundle(BUN_HEAD), None).reason,
        Reason::Detected
    );
    assert_eq!(
        choose_adapter(&detect_bundle(PLAIN), Some(BundlerType::Webpack)).reason,
        Reason::Flag
    );
    assert_eq!(
        choose_adapter(&detect_bundle(PLAIN), None).reason,
        Reason::Fallback
    );
    // A forced bundler with no adapter of its own lands on passthrough
    // (it supports everything) — a fallback, not the flag's choice.
    assert_eq!(
        choose_adapter(&detect_bundle(BUN_HEAD), Some(BundlerType::Rollup)),
        crate::toolchain::Chosen {
            piece: UnpackAdapter::Passthrough,
            reason: Reason::Fallback
        }
    );
    assert!(UnpackAdapter::Bun.provides_module_fossils());
    assert!(UnpackAdapter::Esbuild.provides_module_fossils());
    assert!(!UnpackAdapter::Webcrack.provides_module_fossils());
    assert!(!UnpackAdapter::Passthrough.provides_module_fossils());
}

/// The vendor-record stamps come from the registry: every adapter that
/// writes the record is found by its stamp, nothing else is.
#[test]
fn vendor_record_stamps_round_trip_through_the_registry() {
    for a in crate::unpack::ADAPTERS {
        match a.vendor_record_stamp() {
            Some(stamp) => assert_eq!(UnpackAdapter::of_vendor_record_stamp(stamp), Some(a)),
            None => assert_eq!(UnpackAdapter::of_vendor_record_stamp(a.name()), None),
        }
    }
    assert_eq!(UnpackAdapter::Bun.vendor_record_stamp(), Some("bun"));
    assert_eq!(
        UnpackAdapter::Esbuild.vendor_record_stamp(),
        Some("esbuild")
    );
    assert_eq!(UnpackAdapter::of_vendor_record_stamp("webcrack"), None);
}

// ---- adapters/bun.test.ts --------------------------------------------------

#[test]
fn supports_bun_and_not_webpack() {
    assert!(UnpackAdapter::Bun.supports(&detect_bundle(BUN_HEAD)));
    assert!(!UnpackAdapter::Bun.supports(&detect_bundle(WEBPACK)));
}

#[test]
fn extracts_factory_bodies_into_separate_files_with_stable_names() {
    let t = TempDir::new("extract");
    let outcome = unpack_bun(BUN_BUNDLE, &t.0, BunUnpackOptions::default()).unwrap();
    let manifest = read_manifest(&t.0);
    assert_eq!(manifest["adapter"], "bun");
    assert_eq!(manifest["runtimeFile"], "runtime.js");
    let entries = factories(&manifest);
    assert_eq!(entries.len(), 2, "one manifest entry per factory");
    let written: Vec<String> = outcome
        .result
        .files
        .iter()
        .map(|f| {
            f.path
                .strip_prefix(&t.0)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    for e in &entries {
        assert_eq!(s(e, "nameSource"), "fallback");
        assert!(is_lib_hash_js(s(e, "fileName")), "{}", s(e, "fileName"));
        assert_eq!(s(e, "structuralHash").len(), 16);
        assert!(written.contains(&s(e, "fileName").to_string()));
    }
    assert!(written.contains(&"runtime.js".to_string()));
}

#[test]
fn does_not_serialize_the_rerollable_factory_var() {
    let t = TempDir::new("novar");
    unpack(BUN_BUNDLE, &t.0);
    let raw = fs::read_to_string(t.0.join("vendor/_bun-modules.json")).unwrap();
    assert!(!raw.contains("factoryVar"), "{raw}");
}

#[test]
fn still_reads_a_prior_manifest_that_carries_factory_var() {
    let t = TempDir::new("legacy");
    unpack(BUN_BUNDLE, &t.0);
    let path = t.0.join("vendor/_bun-modules.json");
    let mut manifest: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    for (i, f) in manifest["factories"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        f["factoryVar"] = Value::String(format!("legacy_{i}"));
    }
    fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    let names = load_prior_vendor(&t.0.join("humanified.js"))
        .and_then(|p| p.names)
        .expect("a legacy manifest must still yield carry-over names");
    assert!(!names.is_empty());
}

#[test]
fn rewrites_the_require_variable_to_require() {
    let t = TempDir::new("require");
    unpack(BUN_BUNDLE, &t.0);
    let entries = factories(&read_manifest(&t.0));
    let body = fs::read_to_string(t.0.join(s(&entries[0], "fileName"))).unwrap();
    assert!(body.contains("require(\"node:path\")"), "{body}");
    assert!(!body.contains("m6("), "{body}");
}

#[test]
fn collects_runtime_code_with_stable_factory_references() {
    let t = TempDir::new("runtime");
    unpack(BUN_BUNDLE, &t.0);
    let entries = factories(&read_manifest(&t.0));
    let id = s(&entries[0], "runtimeIdentifier");
    assert!(!id.is_empty(), "mod_a must expose a runtimeIdentifier");
    let runtime = fs::read_to_string(t.0.join("runtime.js")).unwrap();
    assert!(runtime.contains(&format!("{id}()")), "{runtime}");
    assert!(!runtime.contains("mod_a()"), "{runtime}");
}

#[test]
fn identifier_is_the_sanitized_file_stem() {
    let t = TempDir::new("stem");
    unpack(BUN_BUNDLE, &t.0);
    for e in factories(&read_manifest(&t.0)) {
        let file = s(&e, "fileName");
        let base = Path::new(file).file_stem().unwrap().to_string_lossy();
        let sanitized: String = base
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        assert_eq!(s(&e, "runtimeIdentifier"), sanitized);
    }
}

#[test]
fn same_identifier_across_versions_with_rerolled_tokens() {
    let v2 = concat!(
        "import{createRequire as Wq9}from\"node:module\";\n",
        "var n7=Wq9(import.meta.url);\n",
        "var y=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var extra=y((exports)=>{\n",
        "  exports.brandNew=true;\n",
        "});\n",
        "var q7=y((exports,module)=>{\n",
        "  var w2=n7(\"node:path\");\n",
        "  function p9(){return w2.join(\"a\",\"b\")}\n",
        "  module.exports=p9;\n",
        "});\n",
        "var r2=y((exports)=>{\n",
        "  exports.value=42;\n",
        "});\n",
        "var z9=q7();"
    );
    let a = TempDir::new("v1");
    let b = TempDir::new("v2");
    unpack(BUN_BUNDLE, &a.0);
    unpack(v2, &b.0);
    let m1 = factories(&read_manifest(&a.0));
    let m2 = factories(&read_manifest(&b.0));
    let hash = s(&m1[0], "structuralHash");
    let twin = m2
        .iter()
        .find(|f| s(f, "structuralHash") == hash)
        .expect("same-content factory must share a structural hash");
    let id = s(&m1[0], "runtimeIdentifier");
    assert_eq!(s(twin, "runtimeIdentifier"), id);
    let runtime2 = fs::read_to_string(b.0.join("runtime.js")).unwrap();
    assert!(runtime2.contains(&format!("{id}()")), "{runtime2}");
}

#[test]
fn rewrites_cross_factory_references_inside_bodies() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var mod_a=x((exports,module)=>{\n",
        "  module.exports=function base(){return 7};\n",
        "});\n",
        "var mod_c=x((exports)=>{\n",
        "  exports.wrapped=mod_a()();\n",
        "});\n",
        "var main=mod_c();"
    );
    let t = TempDir::new("cross");
    unpack(bundle, &t.0);
    let entries = factories(&read_manifest(&t.0));
    let id_a = s(&entries[0], "runtimeIdentifier");
    let body_c = fs::read_to_string(t.0.join(s(&entries[1], "fileName"))).unwrap();
    assert!(body_c.contains(&format!("{id_a}()")), "{body_c}");
    assert!(!body_c.contains("mod_a"), "{body_c}");
}

#[test]
fn leaves_shadowing_local_bindings_untouched() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var mod_a=x((exports)=>{\n",
        "  exports.value=1;\n",
        "});\n",
        "function shadow(){var mod_a=5;return mod_a+1}\n",
        "var main=mod_a();\n",
        "console.log(shadow());"
    );
    let t = TempDir::new("shadow");
    unpack(bundle, &t.0);
    let entries = factories(&read_manifest(&t.0));
    let id = s(&entries[0], "runtimeIdentifier");
    let runtime = fs::read_to_string(t.0.join("runtime.js")).unwrap();
    assert!(
        runtime.contains("var mod_a=5") && runtime.contains("return mod_a+1"),
        "{runtime}"
    );
    assert!(runtime.contains(&format!("{id}()")), "{runtime}");
}

#[test]
fn handles_code_without_a_factory_helper() {
    let t = TempDir::new("plain");
    let outcome = unpack_bun("console.log(\"hello\");", &t.0, BunUnpackOptions::default()).unwrap();
    assert_eq!(outcome.result.files.len(), 1);
    assert_eq!(
        outcome.result.files[0].path.file_name().unwrap(),
        "index.js"
    );
    assert!(outcome.manifest.is_none());
}

#[test]
fn works_with_different_helper_names() {
    let bundle = concat!(
        "import{createRequire as OBq}from\"node:module\";\n",
        "var r5=OBq(import.meta.url);\n",
        "var C=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var foo=C((exports)=>{\n",
        "  exports.x=r5(\"node:fs\");\n",
        "});\n",
        "var bar=C((exports)=>{\n",
        "  exports.y=2;\n",
        "});"
    );
    let t = TempDir::new("helper");
    unpack(bundle, &t.0);
    let entries = factories(&read_manifest(&t.0));
    assert_eq!(entries.len(), 2);
    let body = fs::read_to_string(t.0.join(s(&entries[0], "fileName"))).unwrap();
    assert!(body.contains("require(\"node:fs\")"), "{body}");
}

#[test]
fn uses_the_banner_package_as_the_file_name() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "/*! axios v1.2.3 */\n",
        "var foo=x((exports,module)=>{\n",
        "  module.exports=function axios(){};\n",
        "});\n",
        "var main=foo();"
    );
    let t = TempDir::new("banner");
    unpack(bundle, &t.0);
    let entries = factories(&read_manifest(&t.0));
    assert_eq!(entries.len(), 1);
    assert_eq!(s(&entries[0], "nameSource"), "banner");
    assert_eq!(s(&entries[0], "bannerPackage"), "axios");
    assert_eq!(s(&entries[0], "bannerVersion"), "1.2.3");
    assert_eq!(s(&entries[0], "fileName"), "vendor/axios@1.2.3.js");
}

#[test]
fn llm_names_fallback_factories_via_the_namer() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "/*! axios v1.2.3 */\n",
        "var withBanner=x((exports,module)=>{ module.exports=function axios(){}; });\n",
        "var unknownOne=x((exports)=>{ exports.load=function load(s){return s+\"YAMLException\";}; });\n",
        "var unknownTwo=x((exports)=>{ exports.render=function render(t){return t+\"template\";}; });\n",
        "var main=withBanner();"
    );
    let t = TempDir::new("llm");
    let mut namer = FnNamer {
        answer: |r: &VendorNameRequest| {
            Some(if r.evidence.contains("YAMLException") {
                "js-yaml".to_string()
            } else {
                "not a name!!".to_string()
            })
        },
        asked: Vec::new(),
    };
    unpack_bun(
        bundle,
        &t.0,
        BunUnpackOptions {
            namer: Some(&mut namer),
            ..Default::default()
        },
    )
    .unwrap();
    let entries = factories(&read_manifest(&t.0));
    assert_eq!(entries.len(), 3);
    assert_eq!(namer.asked.len(), 1, "one batch for the fallback factories");
    assert_eq!(namer.asked[0].len(), 2, "banner-named factory excluded");
    let by_source = |src: &str| entries.iter().find(|f| s(f, "nameSource") == src).unwrap();
    assert_eq!(s(by_source("banner"), "fileName"), "vendor/axios@1.2.3.js");
    assert_eq!(s(by_source("llm"), "fileName"), "vendor/js-yaml.js");
    assert_eq!(s(by_source("llm"), "name"), "js-yaml");
    assert!(is_lib_hash_js(s(by_source("fallback"), "fileName")));
}

#[test]
fn strips_a_trailing_js_from_banner_names() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "/*! highlight.js */\n",
        "var hl=x((exports,module)=>{\n",
        "  module.exports=function highlight(){};\n",
        "});\n",
        "var main=hl();"
    );
    let t = TempDir::new("hljs");
    unpack(bundle, &t.0);
    let entries = factories(&read_manifest(&t.0));
    assert_eq!(s(&entries[0], "nameSource"), "banner");
    assert_eq!(s(&entries[0], "fileName"), "vendor/highlight.js");
}

#[test]
fn groups_a_packages_modules_into_a_folder() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "/*! axios v1.0.0 */\n",
        "var a=x((exports)=>{ exports.value = function f(a) { return a + 1; }; });\n",
        "/*! axios v1.0.0 */\n",
        "var b=x((exports)=>{ exports.value = function f(a,b) { return a * b; }; });\n",
        "/*! axios v1.0.0 */\n",
        "var c=x((exports)=>{ exports.value = function f(a,b,c) { return a * b * c; }; });\n",
        "var main=a();"
    );
    let t = TempDir::new("group");
    unpack(bundle, &t.0);
    let entries = factories(&read_manifest(&t.0));
    assert_eq!(entries.len(), 3);
    let mut names = Vec::new();
    for e in &entries {
        let f = s(e, "fileName");
        let stem = f
            .strip_prefix("vendor/axios@1.0.0/lib_")
            .and_then(|r| r.strip_suffix(".js"))
            .unwrap_or_else(|| panic!("expected vendor/axios@1.0.0/lib_<hash>.js, got {f}"));
        assert_eq!(stem.len(), 8);
        names.push(f.to_string());
    }
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 3, "distinct files in the folder");
    assert_eq!(
        fs::read_dir(t.0.join("vendor/axios@1.0.0"))
            .unwrap()
            .count(),
        3
    );
}

#[test]
fn keeps_a_single_module_package_flat() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "/*! axios v2.0.0 */\n",
        "var a=x((exports)=>{ exports.value = 1; });\n",
        "/*! lodash v4.0.0 */\n",
        "var b=x((exports)=>{ exports.value = 2; });\n",
        "var main=a();"
    );
    let t = TempDir::new("flat");
    unpack(bundle, &t.0);
    let mut names: Vec<String> = factories(&read_manifest(&t.0))
        .iter()
        .map(|f| s(f, "fileName").to_string())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec!["vendor/axios@2.0.0.js", "vendor/lodash@4.0.0.js"]
    );
}

#[test]
fn runtime_identifier_is_stable_when_a_folder_is_introduced() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "/*! axios v1.0.0 */\n",
        "var a=x((exports)=>{ exports.value = function f(a){return a+1}; });\n",
        "/*! axios v1.0.0 */\n",
        "var b=x((exports)=>{ exports.value = function f(a,b){return a*b}; });\n",
        "var main=a()+b();"
    );
    let t = TempDir::new("folderid");
    unpack(bundle, &t.0);
    let runtime = fs::read_to_string(t.0.join("runtime.js")).unwrap();
    for f in factories(&read_manifest(&t.0)) {
        let id = s(&f, "runtimeIdentifier");
        assert!(!id.is_empty());
        assert!(!id.contains("axios"), "{id}");
        assert!(runtime.contains(&format!("{id}(")), "{runtime}");
    }
}

#[test]
fn disambiguates_names_differing_only_in_case() {
    let bundle = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "/*! Ab v1.0.0 */\n",
        "var a=x((exports)=>{ exports.value = 1; });\n",
        "/*! aB v1.0.0 */\n",
        "var b=x((exports)=>{ exports.value = 2; });\n",
        "var main=a();"
    );
    let t = TempDir::new("case");
    unpack(bundle, &t.0);
    let names: Vec<String> = factories(&read_manifest(&t.0))
        .iter()
        .map(|f| s(f, "fileName").to_string())
        .collect();
    assert_eq!(names, vec!["vendor/Ab@1.0.0.js", "vendor/aB@1.0.0-2.js"]);
}

// ---- the esbuild adapter (exp075 — real esbuild 0.27.2 wrapper shapes) ------

/// An unminified esbuild 0.27.2 bundle, byte-shaped as a fresh build emits
/// it: the `__commonJS` helper's `{ exports: {} }` marker is SPACED (bun's
/// tight marker does not match), and every CJS factory arrives as an
/// OBJECT with one keyed method whose KEY IS the module's original source
/// path.
const ESBUILD_OBJECT_BUNDLE: &str = concat!(
    "var __defProp = Object.defineProperty;\n",
    "var __getOwnPropNames = Object.getOwnPropertyNames;\n",
    "var __commonJS = ((cb, mod) => function __require() {\n",
    "  return mod || (0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports;\n",
    "});\n",
    "// src/libs/cjs-dep.js\n",
    "var require_cjs_dep = __commonJS({\n",
    "  \"src/libs/cjs-dep.js\"(exports, module2) {\n",
    "    var version = \"1.2.3\";\n",
    "    function helper(x) {\n",
    "      return x * 2;\n",
    "    }\n",
    "    module2.exports = { version, helper };\n",
    "  }\n",
    "});\n",
    "// src/main.js\n",
    "function main(n) {\n",
    "  var dep = require_cjs_dep();\n",
    "  return dep.helper(n);\n",
    "}\n",
    "console.log(main(5));\n",
);

#[test]
fn esbuild_object_factories_extract_with_their_source_paths() {
    let t = TempDir::new("esbuild");
    let outcome = unpack_bun(
        ESBUILD_OBJECT_BUNDLE,
        &t.0,
        BunUnpackOptions {
            adapter: UnpackAdapter::Esbuild,
            ..BunUnpackOptions::default()
        },
    )
    .expect("unpack runs");
    let manifest = read_manifest(&t.0);
    assert_eq!(manifest["adapter"], "esbuild");
    assert_eq!(manifest["runtimeFile"], "runtime.js");
    let entries = factories(&manifest);
    assert_eq!(entries.len(), 1, "one CJS factory");
    assert_eq!(s(&entries[0], "sourcePath"), "src/libs/cjs-dep.js");
    assert!(is_lib_hash_js(s(&entries[0], "fileName")));
    // The vendor body is the METHOD's function — the object wrapper and
    // its path key are stripped, the body's own code kept. A method's
    // span carries no `function` keyword, so the vendored body gains one
    // (it must be a complete expression: the relink stage re-parses it).
    let body = fs::read_to_string(t.0.join(s(&entries[0], "fileName"))).unwrap();
    assert!(body.starts_with("function "), "{body}");
    assert!(body.contains("x * 2"), "{body}");
    assert!(!body.contains("src/libs/cjs-dep.js"), "{body}");
    assert!(!body.contains("__commonJS"), "{body}");
    // The factory's bundle reference in the runtime is the stable
    // identifier, never the factory var.
    let id = s(&entries[0], "runtimeIdentifier");
    assert!(!id.is_empty(), "esbuild factories get an identifier");
    let runtime = fs::read_to_string(t.0.join("runtime.js")).unwrap();
    assert!(runtime.contains(&format!("{id}()")), "{runtime}");
    assert!(!runtime.contains("require_cjs_dep()"), "{runtime}");
    // The outcome's bundle order keeps the factory var — bundle order is
    // same-version bookkeeping, not a carried field.
    assert_eq!(
        outcome
            .bundle_order
            .iter()
            .map(|r| r.factory_var.as_str())
            .collect::<Vec<_>>(),
        vec!["require_cjs_dep"]
    );
}

#[test]
fn esbuilds_minified_form_extracts_through_the_bun_marker() {
    // Minified esbuild reverts to the bare-function factory and minifies
    // the helper name away — the tight `{exports:{}}` marker identifies it
    // (bun-first identification), and NO source path is claimed.
    let minified = concat!(
        "var l=(r,e)=>()=>(e||r((e={exports:{}}).exports,e),e.exports);\n",
        "var c=l((k,u)=>{u.exports={value:1}});\n",
        "var done=c();\n",
    );
    let t = TempDir::new("esbuild-min");
    unpack_bun(
        minified,
        &t.0,
        BunUnpackOptions {
            adapter: UnpackAdapter::Esbuild,
            ..BunUnpackOptions::default()
        },
    )
    .expect("unpack runs");
    let entries = factories(&read_manifest(&t.0));
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0].get("sourcePath").is_none(),
        "no source path survives minification"
    );
}

#[test]
fn bun_bundles_still_unpack_under_the_bun_adapter_stamp() {
    // The combined helper identification changed nothing for bun: the same
    // bundle, the same manifest bytes, adapter "bun".
    let t = TempDir::new("bun-still");
    unpack(BUN_BUNDLE, &t.0);
    let manifest = read_manifest(&t.0);
    assert_eq!(manifest["adapter"], "bun");
    assert!(
        factories(&manifest)
            .iter()
            .all(|f| f.get("sourcePath").is_none())
    );
}

// ---- cross-version vendor name carry-over ----------------------------------

const UNKNOWN_BUNDLE: &str = concat!(
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
    "var unknownOne=x((exports)=>{ exports.load=function load(s){return s+\"YAMLException\";}; });\n",
    "var main=unknownOne();"
);

fn run_with(
    code: &str,
    dir: &Path,
    prior: Option<HashMap<String, Vec<String>>>,
    answer: fn(&VendorNameRequest) -> Option<String>,
) -> Vec<Vec<String>> {
    let mut namer = FnNamer {
        answer,
        asked: Vec::new(),
    };
    unpack_bun(
        code,
        dir,
        BunUnpackOptions {
            namer: Some(&mut namer),
            prior: prior.map(|names| {
                PriorVendor::from_names(
                    names
                        .into_iter()
                        .map(|(h, v)| {
                            (
                                h,
                                v.into_iter().map(|n| CarriedName::new(n, None)).collect(),
                            )
                        })
                        .collect(),
                )
            }),
            ..Default::default()
        },
    )
    .unwrap();
    namer.asked
}

#[test]
fn reuses_the_prior_name_ahead_of_the_llm() {
    let first = TempDir::new("carry1");
    run_with(UNKNOWN_BUNDLE, &first.0, None, |_| Some("js-yaml".into()));
    let m1 = factories(&read_manifest(&first.0));
    assert_eq!(s(&m1[0], "nameSource"), "llm");
    let hash = s(&m1[0], "structuralHash").to_string();

    let second = TempDir::new("carry2");
    let prior = HashMap::from([(hash, vec!["js-yaml".to_string()])]);
    let asked = run_with(UNKNOWN_BUNDLE, &second.0, Some(prior), |_| {
        Some("yaml-parser".into())
    });
    let m2 = factories(&read_manifest(&second.0));
    assert_eq!(s(&m2[0], "name"), "js-yaml");
    assert_eq!(s(&m2[0], "nameSource"), "carry-over");
    assert_eq!(s(&m2[0], "fileName"), "vendor/js-yaml.js");
    assert!(asked.is_empty(), "a carried factory never reaches the LLM");
}

/// The manifest records where the NAME came from, not how THIS run got it:
/// a library named "llm" on a fresh run and carried on the next hop keeps a
/// byte-identical manifest entry (finding #71 — the per-hop "llm" ->
/// "carry-over" flip was ~3,000 manifest lines per version with no change
/// behind them). How this run got the name stays in run state: the
/// cascade's carry-over count still reports the carry.
#[test]
fn a_carried_library_keeps_a_byte_identical_manifest_entry() {
    let hop = |dir: &Path, prior: Option<PriorVendor>| {
        let mut namer = FnNamer {
            answer: |_| Some("js-yaml".into()),
            asked: Vec::new(),
        };
        let outcome = unpack_bun(
            UNKNOWN_BUNDLE,
            dir,
            BunUnpackOptions {
                namer: Some(&mut namer),
                prior,
                ..Default::default()
            },
        )
        .unwrap();
        (outcome, namer.asked.len())
    };
    let first = TempDir::new("stable1");
    let (out1, asked1) = hop(&first.0, None);
    assert_eq!(asked1, 1);
    assert_eq!(out1.name_counts.as_ref().unwrap().carry_over, 0);
    let m1 = factories(&read_manifest(&first.0));
    assert_eq!(s(&m1[0], "nameSource"), "llm");

    let second = TempDir::new("stable2");
    let prior = load_prior_vendor(&first.0.join("humanified.js")).expect("prior tree");
    let (out2, asked2) = hop(&second.0, Some(prior));
    assert_eq!(asked2, 0, "a carried factory never reaches the LLM");
    assert_eq!(out2.name_counts.as_ref().unwrap().carry_over, 1);
    let m2 = factories(&read_manifest(&second.0));
    assert_eq!(
        serde_json::to_string(&m2[0]).unwrap(),
        serde_json::to_string(&m1[0]).unwrap()
    );
}

#[test]
fn falls_back_to_the_llm_for_an_unknown_library() {
    let t = TempDir::new("carry3");
    let prior = HashMap::from([("0".repeat(16), vec!["some-other-lib".to_string()])]);
    run_with(UNKNOWN_BUNDLE, &t.0, Some(prior), |_| {
        Some("js-yaml".into())
    });
    let m = factories(&read_manifest(&t.0));
    assert_eq!(s(&m[0], "nameSource"), "llm");
    assert_eq!(s(&m[0], "name"), "js-yaml");
}

#[test]
fn keeps_a_carried_hash_name_flat() {
    let twins = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var one=x((exports)=>{ exports.v = function pick(a){ return a[0]; }; });\n",
        "var two=x((exports)=>{ exports.v = function pick(a){ return a[0]; }; });\n",
        "var main=one();"
    );
    let first = TempDir::new("flat1");
    run_with(twins, &first.0, None, |_| None);
    let m1 = factories(&read_manifest(&first.0));
    let mut flat: Vec<String> = m1.iter().map(|f| s(f, "fileName").to_string()).collect();
    flat.sort();
    assert!(
        flat.iter().all(|n| {
            let n = n
                .strip_suffix("-2.js")
                .map_or(n.clone(), |b| format!("{b}.js"));
            is_lib_hash_js(&n)
        }),
        "{flat:?}"
    );
    let mut prior: HashMap<String, Vec<String>> = HashMap::new();
    for f in &m1 {
        prior
            .entry(s(f, "structuralHash").to_string())
            .or_default()
            .push(s(f, "name").to_string());
    }
    let second = TempDir::new("flat2");
    run_with(twins, &second.0, Some(prior), |_| None);
    let mut again: Vec<String> = factories(&read_manifest(&second.0))
        .iter()
        .map(|f| s(f, "fileName").to_string())
        .collect();
    again.sort();
    assert_eq!(again, flat);
}

#[test]
fn keeps_hash_colliding_shims_on_their_own_prior_names() {
    let shims = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var depOne=x((exports,module)=>{ module.exports=function one(a){return a+1}; });\n",
        "var depTwo=x((exports,module)=>{ module.exports=function two(a,b,c){return a*b*c}; });\n",
        "var shimOne=x((exports,module)=>{ module.exports=depOne(); });\n",
        "var shimTwo=x((exports,module)=>{ module.exports=depTwo(); });\n",
        "var main=shimOne();"
    );
    let first = TempDir::new("shim1");
    run_with(shims, &first.0, None, |_| None);
    let m1 = factories(&read_manifest(&first.0));
    let mut counts: HashMap<String, usize> = HashMap::new();
    for f in &m1 {
        *counts
            .entry(s(f, "structuralHash").to_string())
            .or_default() += 1;
    }
    let shared = m1
        .iter()
        .map(|f| s(f, "structuralHash").to_string())
        .find(|h| counts[h] == 2)
        .expect("the two shims share a structuralHash");
    let second = TempDir::new("shim2");
    let prior = HashMap::from([(
        shared.clone(),
        vec!["retry".to_string(), "lodash".to_string()],
    )]);
    run_with(shims, &second.0, Some(prior), |_| None);
    let carried: Vec<String> = factories(&read_manifest(&second.0))
        .iter()
        .filter(|f| s(f, "structuralHash") == shared)
        .map(|f| s(f, "name").to_string())
        .collect();
    assert_eq!(carried, vec!["retry", "lodash"]);
}

#[test]
fn loads_prior_vendor_names_from_a_prior_tree() {
    let t = TempDir::new("priortree");
    run_with(UNKNOWN_BUNDLE, &t.0, None, |_| Some("js-yaml".into()));
    let hash = s(&factories(&read_manifest(&t.0))[0], "structuralHash").to_string();
    fs::create_dir_all(t.0.join(".humanify")).unwrap();
    let prior_file = t.0.join(".humanify/humanified.js");
    fs::write(&prior_file, "// prior").unwrap();
    let names = load_prior_vendor(&prior_file)
        .and_then(|p| p.names)
        .expect("discovered");
    assert_eq!(
        names.get(&hash),
        Some(&vec![CarriedName::new("js-yaml", Some("llm"))])
    );
    assert_eq!(find_prior_tree_root(&prior_file), Some(t.0.clone()));
}

#[test]
fn no_prior_names_without_a_vendor_manifest() {
    let t = TempDir::new("bare");
    fs::write(t.0.join("humanified.js"), "// prior").unwrap();
    assert!(load_prior_vendor(&t.0.join("humanified.js")).is_none());
}

// ---- the regex floor + require tracing (V8 semantics by hand) ---------------

#[test]
fn regex_floor_extracts_between_the_outermost_parens() {
    let code = "var x=H;var a = H ((e)=>{f(1)});let  b=H(function(){g((2))})\nconst c=Hx(1)";
    let modules = extract_factory_bodies(code, "H");
    assert_eq!(
        modules,
        vec![
            ExtractedModule {
                name: "a".into(),
                body_start: 19,
                body_end: 30,
                decl_start: 8,
                decl_end: 32,
                object_method: false,
            },
            ExtractedModule {
                name: "b".into(),
                body_start: 41,
                body_end: 59,
                decl_start: 32,
                decl_end: 60,
                object_method: false,
            },
        ]
    );
    assert_eq!(&code[19..30], "(e)=>{f(1)}");
    // `Hx(` is not the helper followed by `\s*\(`.
    assert!(modules.iter().all(|m| m.name != "c"));
}

#[test]
fn regex_floor_takes_a_dollar_helper_literally() {
    let code = "var $a=$h(()=>1);";
    let modules = extract_factory_bodies(code, "$h");
    assert_eq!(modules.len(), 1);
    assert_eq!(modules[0].name, "$a");
    assert_eq!(
        modules[0].decl_end,
        code.len(),
        "the trailing `;` is spliced"
    );
}

#[test]
fn require_tracing_follows_the_create_require_alias() {
    assert_eq!(identify_bun_require(BUN_BUNDLE).as_deref(), Some("m6"));
    // Single-quoted module specifier and spaced declaration.
    let spaced = "import { foo, createRequire as Req } from 'node:module';\nconst  r =  Req(import.meta.url);";
    assert_eq!(identify_bun_require(spaced).as_deref(), Some("r"));
    // The LAST createRequire alias inside the braces wins (greedy `[^}]*`).
    let twice = "import{createRequire as A,createRequire as B}from\"node:module\";var q=B(import.meta.url);var p=A(import.meta.url);";
    assert_eq!(identify_bun_require(twice).as_deref(), Some("q"));
    // Wrong module → no alias → no require var.
    assert!(
        identify_bun_require("import{createRequire as A}from\"node:fs\";var q=A(import.meta.url);")
            .is_none()
    );
    // A `}` before createRequire ends the brace group.
    assert!(
        identify_bun_require(
            "import{a}from\"x\";createRequire as A}from\"node:module\";var q=A(import.meta.url);"
        )
        .is_none()
    );
}

#[test]
fn require_rewrite_respects_the_ascii_word_boundary() {
    assert_eq!(
        rewrite_require_calls("m6(\"a\");xm6(\"b\");é m6(\"c\");_m6(1)", "m6"),
        "require(\"a\");xm6(\"b\");é require(\"c\");_m6(1)"
    );
    // `é` is a non-word char under ASCII `\b`, so `ém6(` IS bounded.
    assert_eq!(rewrite_require_calls("ém6(1)", "m6"), "érequire(1)");
}

// ---- identifyBunCjsFactory's lookback window (UTF-16 units) ----------------

/// The TS looks back `LOOKBACK_CHARS = 2000` UTF-16 code units from the
/// marker (`source.slice(match.index - 2000, match.index)`). Counted in
/// BYTES the window is shorter on non-ASCII text — and slicing at a byte
/// offset inside a multi-byte char panicked.
#[test]
fn factory_helper_lookback_counts_utf16_units() {
    let with_pad = |k: usize| format!("var h={}{{exports:{{}}}}", "é".repeat(k));
    let found = crate::modules::identify_bun_cjs_factory(&with_pad(1994));
    assert_eq!(
        found.map(|h| h.name).as_deref(),
        Some("h"),
        "`var h=` + 1994 units sits exactly inside the 2000-unit window"
    );
    assert!(
        crate::modules::identify_bun_cjs_factory(&with_pad(1995)).is_none(),
        "one unit further and the window starts inside `var`"
    );
}

#[test]
fn factory_helper_lookback_never_splits_a_char() {
    let code = format!(
        "import{{createRequire as Glq}}from\"node:module\";var m6=Glq(import.meta.url);/*{}X*/var x=(I,A)=>()=>(A||I((A={{exports:{{}}}}).exports,A),A.exports);var m=x((e,t)=>{{t.exports=1}});",
        "é".repeat(1100)
    );
    let t = TempDir::new("lookback");
    let outcome = unpack_bun(&code, &t.0, BunUnpackOptions::default()).unwrap();
    assert_eq!(outcome.manifest.map(|m| m.factories.len()), Some(1));
}

// ---- a TS-era prior manifest: carried by CONTENT, never by hash bytes -------

const SHIMS: &str = concat!(
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
    "var depOne=x((exports,module)=>{ module.exports=function one(a){return a+1}; });\n",
    "var shimOne=x((exports,module)=>{ module.exports=depOne(); });\n",
    "var shimTwo=x((exports,module)=>{ module.exports=depOne(); });\n",
    "var main=shimOne();"
);

/// The next release: Bun rerolled every factory var, the code is the same.
const SHIMS_REROLLED: &str = concat!(
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
    "var qA=x((exports,module)=>{ module.exports=function one(a){return a+1}; });\n",
    "var zB=x((exports,module)=>{ module.exports=qA(); });\n",
    "var kC=x((exports,module)=>{ module.exports=qA(); });\n",
    "var main=zB();"
);

/// Unpack SHIMS as the prior release, then make its manifest a TS-era one:
/// no `hashVersion`, every structural hash replaced by `hash_of` (group for
/// group), and the names a real run would have carried.
fn ts_era_prior(tag: &str, hash_of: fn(&str) -> String) -> (TempDir, PathBuf) {
    let t = TempDir::new(tag);
    unpack(SHIMS, &t.0);
    let path = t.0.join("vendor/_bun-modules.json");
    let mut manifest = read_manifest(&t.0);
    assert!(manifest.get("hashVersion").is_some(), "{manifest}");
    manifest.as_object_mut().unwrap().remove("hashVersion");
    for f in manifest["factories"].as_array_mut().unwrap() {
        let name = match f.get("hashOrdinal").and_then(Value::as_u64) {
            None => "dep-one",
            Some(0) => "shim-a",
            Some(_) => "shim-b",
        };
        f["name"] = Value::String(name.into());
        f["nameSource"] = Value::String("carry-over".into());
        let ts = hash_of(f["structuralHash"].as_str().unwrap());
        f["structuralHash"] = Value::String(ts);
    }
    fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    fs::create_dir_all(t.0.join(".humanify")).unwrap();
    let prior_file = t.0.join(".humanify/humanified.js");
    fs::write(&prior_file, "// prior").unwrap();
    (t, prior_file)
}

fn names_in_bundle_order(
    dir: &Path,
    outcome: &crate::unpack::bun::BunUnpackOutcome,
) -> Vec<String> {
    let by_file: HashMap<String, String> = factories(&read_manifest(dir))
        .iter()
        .map(|f| (s(f, "fileName").to_string(), s(f, "name").to_string()))
        .collect();
    outcome
        .bundle_order
        .iter()
        .map(|r| by_file[&r.file_name].clone())
        .collect()
}

#[test]
fn a_ts_era_prior_carries_its_names_by_content() {
    // TS bytes: nothing like the Rust's.
    let (_prior, prior_file) = ts_era_prior("tsera-content", |h| format!("{:0>16}", h.len()));
    let fresh = TempDir::new("tsera-content-fresh");
    let prior = load_prior_vendor(&prior_file).expect("a prior tree");
    assert!(prior.stale_era.is_some(), "no hashVersion = TS era");
    let outcome = unpack_bun(
        SHIMS_REROLLED,
        &fresh.0,
        BunUnpackOptions {
            prior: Some(prior),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        names_in_bundle_order(&fresh.0, &outcome),
        vec!["dep-one", "shim-a", "shim-b"]
    );
    let rekey = outcome.rekey.expect("re-keyed");
    assert_eq!((rekey.groups_joined, rekey.factories_joined), (2, 3));
    // fixed:{was: 3, why: exp094 bumped FACTORY_HASH_VERSION — a safe arrow
    // serializes under the FunctionExpression token now, so the manifest's
    // ARROW-factory hashes are a new era's bytes (function-expression
    // factories keep theirs across the era; the gate, not the bytes,
    // carries the refusal)}.
    assert_eq!(read_manifest(&fresh.0)["hashVersion"], 4);
}

#[test]
fn a_ts_era_prior_never_joins_by_hash_bytes() {
    // The TS bytes happen to EQUAL the Rust's, but the vendor files are
    // gone: with no content to read, nothing may carry.
    let (prior_tree, prior_file) = ts_era_prior("tsera-bytes", |h| h.to_string());
    for f in factories(&read_manifest(&prior_tree.0)) {
        fs::remove_file(prior_tree.0.join(s(&f, "fileName"))).unwrap();
    }
    let fresh = TempDir::new("tsera-bytes-fresh");
    let outcome = unpack_bun(
        SHIMS_REROLLED,
        &fresh.0,
        BunUnpackOptions {
            prior: load_prior_vendor(&prior_file),
            ..Default::default()
        },
    )
    .unwrap();
    let names = names_in_bundle_order(&fresh.0, &outcome);
    assert!(names.iter().all(|n| n.starts_with("lib_")), "{names:?}");
}

// ---- an OLDER `hashVersion` prior (exp093): the whole era machinery --------

/// The vendor-manifest `hashVersion` the release BEFORE exp093 stamped (the
/// number-magnitude-blur era). A manifest stamped with ANY version other
/// than this run's must take the same never-by-hash path a TS manifest
/// takes — carried by CONTENT, or not at all. The version gate is the only
/// protection for the worst case: a number-free factory's hash bytes are
/// IDENTICAL across the era boundary, so a stale manifest would silently
/// hash-join its nameless/renamed entries whenever the bytes happen to
/// collide, while its number-bearing entries (new bytes) silently mint —
/// a half-carried prior with no warning.
const PRIOR_ERA_HASH_VERSION: u64 = 2;

/// A prior manifest stamped [`PRIOR_ERA_HASH_VERSION`]: the names a real run
/// would have carried, and hashes left exactly as the current run computes
/// them — the STRONGEST form of the gate test: the stored bytes are
/// byte-joinable, and the era mismatch must refuse the join anyway.
fn older_era_prior(tag: &str) -> (TempDir, PathBuf) {
    let t = TempDir::new(tag);
    unpack(SHIMS, &t.0);
    let path = t.0.join("vendor/_bun-modules.json");
    let mut manifest = read_manifest(&t.0);
    manifest
        .as_object_mut()
        .unwrap()
        .insert("hashVersion".into(), Value::from(PRIOR_ERA_HASH_VERSION));
    for f in manifest["factories"].as_array_mut().unwrap() {
        let name = match f.get("hashOrdinal").and_then(Value::as_u64) {
            None => "dep-one",
            Some(0) => "shim-a",
            Some(_) => "shim-b",
        };
        f["name"] = Value::String(name.into());
        f["nameSource"] = Value::String("carry-over".into());
    }
    fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    fs::create_dir_all(t.0.join(".humanify")).unwrap();
    let prior_file = t.0.join(".humanify/humanified.js");
    fs::write(&prior_file, "// prior").unwrap();
    (t, prior_file)
}

#[test]
fn an_older_hashversion_prior_is_rekeyed_by_content_never_by_hash() {
    let (_prior, prior_file) = older_era_prior("older-era-content");
    let prior = load_prior_vendor(&prior_file).expect("a prior tree");
    // THE era gate: a version-mismatched manifest is never read by hash,
    // even when its bytes would join (`names` stays None, the content
    // re-key path takes over).
    assert!(
        prior.stale_era.is_some(),
        "a hashVersion {} manifest must not hash-join under this run",
        PRIOR_ERA_HASH_VERSION
    );
    assert!(prior.names.is_none(), "no hash-byte carry, ever");
    let fresh = TempDir::new("older-era-content-fresh");
    let outcome = unpack_bun(
        SHIMS_REROLLED,
        &fresh.0,
        BunUnpackOptions {
            prior: Some(prior),
            ..Default::default()
        },
    )
    .unwrap();
    // The names still carried — via the CONTENT re-key, proven by its stats.
    assert_eq!(
        names_in_bundle_order(&fresh.0, &outcome),
        vec!["dep-one", "shim-a", "shim-b"]
    );
    let rekey = outcome.rekey.expect("re-keyed by content");
    assert_eq!((rekey.groups_joined, rekey.factories_joined), (2, 3));
}

#[test]
fn an_older_hashversion_prior_without_content_mints_everything() {
    // The bytes join, the version refuses, the vendor files are GONE: a
    // hash-byte match is not a fallback — nothing carries, everything
    // mints a fresh `lib_<hash>` name.
    let (prior_tree, prior_file) = older_era_prior("older-era-mint");
    for f in factories(&read_manifest(&prior_tree.0)) {
        fs::remove_file(prior_tree.0.join(s(&f, "fileName"))).unwrap();
    }
    let fresh = TempDir::new("older-era-mint-fresh");
    let outcome = unpack_bun(
        SHIMS_REROLLED,
        &fresh.0,
        BunUnpackOptions {
            prior: load_prior_vendor(&prior_file),
            ..Default::default()
        },
    )
    .unwrap();
    let names = names_in_bundle_order(&fresh.0, &outcome);
    assert!(names.iter().all(|n| n.starts_with("lib_")), "{names:?}");
}

#[test]
fn webcrack_shim_output_parses_files_and_metadata() {
    let out = parse_shim_output(
        "noise\n{\"files\":[{\"path\":\"/o/a.js\",\"metadata\":{\"id\":\"0\",\"modulePath\":\"./node_modules/x/i.js\",\"isEntry\":false}},{\"path\":\"/o/b.js\"}],\"bundleType\":\"webpack\"}\n",
    )
    .unwrap();
    assert_eq!(out.files.len(), 2);
    assert_eq!(
        out.files[0].metadata.as_ref().unwrap().module_path,
        "./node_modules/x/i.js"
    );
    assert!(out.files[1].metadata.is_none());
}

/// `--disable manifest-prior-order` reverts the WHOLE of exp047
/// (manifest-order.ts): no `hashOrdinal` stamps, no prior-order reorder.
#[test]
fn the_manifest_prior_order_switch_reverts_exp047() {
    let twins = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var one=x((exports)=>{ exports.v = function pick(a){ return a[0]; }; });\n",
        "var two=x((exports)=>{ exports.v = function pick(a){ return a[0]; }; });\n",
        "var main=one()+two();"
    );
    let on = TempDir::new("mpo-on");
    unpack_bun(twins, &on.0, BunUnpackOptions::default()).unwrap();
    let with = factories(&read_manifest(&on.0));
    assert!(
        with.iter().all(|f| f.get("hashOrdinal").is_some()),
        "twins are stamped"
    );
    let off = TempDir::new("mpo-off");
    unpack_bun(
        twins,
        &off.0,
        BunUnpackOptions {
            manifest_prior_order_disabled: true,
            ..Default::default()
        },
    )
    .unwrap();
    let without = factories(&read_manifest(&off.0));
    assert!(without.iter().all(|f| f.get("hashOrdinal").is_none()));
}

// ---- finding #51: vendor bodies that reference bundle-scope bindings -------

/// A Bun bundle whose factory bodies reach OUTSIDE themselves for
/// something other than another factory: `mod_a` calls the bundle's
/// `__toESM` (`u`), `mod_c` runs an app ESM module's init and converts its
/// namespace (`(initM(), Rq(ns))` — Bun's `require()` of an ESM module), and
/// `mod_d` depends on `mod_c`. The bundle prints `[43,"hi!"]`.
const BUN_BUNDLE_SCOPE_REFS: &str = concat!(
    "var cr=Object.create,gp=Object.getPrototypeOf,dp=Object.defineProperty,gn=Object.getOwnPropertyNames,gd=Object.getOwnPropertyDescriptor,hp=Object.prototype.hasOwnProperty;\n",
    "function ap(k){return this[k]}\n",
    "var c1,c2,u=(H,_,q)=>{var $=H!=null&&typeof H===\"object\";if($){var K=_?c1??=new WeakMap:c2??=new WeakMap,O=K.get(H);if(O)return O}q=H!=null?cr(gp(H)):{};let T=_||!H||!H.__esModule?dp(q,\"default\",{value:H,enumerable:!0}):q;for(let z of gn(H))if(!hp.call(T,z))dp(T,z,{get:ap.bind(H,z),enumerable:!0});if($)K.set(H,T);return T};\n",
    "var cm,Rq=(H)=>{var _=(cm??=new WeakMap).get(H),q;if(_)return _;if(_=dp({},\"__esModule\",{value:!0}),H&&typeof H===\"object\"||typeof H===\"function\"){for(var $ of gn(H))if(!hp.call(_,$))dp(_,$,{get:ap.bind(H,$),enumerable:!(q=gd(H,$))||q.enumerable})}return cm.set(H,_),_};\n",
    "var G=(H,_)=>()=>(H&&(_=H(H=0)),_);\n",
    "var J_=(H,_)=>{for(var q in _)dp(H,q,{get:_[q],enumerable:!0,configurable:!0})};\n",
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
    "var mod_b=x((exports)=>{exports.value=42;});\n",
    "var mod_a=x((exports,module)=>{module.exports=u(mod_b()).value+1;});\n",
    "function hi(){return \"hi\"}\n",
    "var ns={};\n",
    "var initM=G(()=>{J_(ns,{hi:()=>hi})});\n",
    "var mod_c=x((exports,module)=>{module.exports=(initM(),Rq(ns)).hi();});\n",
    "var mod_d=x((exports,module)=>{module.exports=mod_c()+\"!\";});\n",
    "console.log(JSON.stringify([mod_a(),mod_d()]));\n",
);

/// The runnable graph the finish builds from an unpacked tree, at unit
/// scale: every vendor file wrapped (`wrap_extracted_factory`), the runtime
/// relinked (`relink_factory_references`) and the factory-helper shim
/// written — then RUN by Node. Node's stdout, or its stderr as the error.
///
/// Finding #60's unit-scale bridge: no naming stage ran, so each capture's
/// accessor keeps its raw name and the owner is the runtime itself; the
/// harness gives the runtime the accessor exports the split's emit writes
/// for the owner file in the pipeline.
fn run_relinked(dir: &Path) -> Result<String, String> {
    use crate::finish::relink::{
        BUN_RELINK_RUNTIME, FactoryLookup, VendorBridge, relink_factory_references,
        wrap_extracted_factory,
    };
    let interop = crate::toolchain::InteropHelpers::Bun;
    let entries = factories(&read_manifest(dir));
    let lookup: FactoryLookup = entries
        .iter()
        .map(|e| {
            (
                s(e, "runtimeIdentifier").to_string(),
                s(e, "fileName").to_string(),
            )
        })
        .collect();
    let mut bridges: Vec<VendorBridge> = Vec::new();
    let mut accessor_lines: Vec<String> = Vec::new();
    for e in &entries {
        for c in e["captures"].as_array().into_iter().flatten() {
            let name = c["name"].as_str().expect("a capture name");
            if !bridges.iter().any(|b| b.name == name) {
                bridges.push(VendorBridge {
                    name: name.to_string(),
                    file: "runtime.js".to_string(),
                    accessor: name.to_string(),
                });
                accessor_lines.push(format!(
                    "Object.defineProperty(module.exports, \"{name}\", {{ get: () => {name}, enumerable: true, configurable: true }});"
                ));
            }
        }
    }
    for e in &entries {
        let file = s(e, "fileName");
        let body = fs::read_to_string(dir.join(file)).unwrap();
        let (wrapped, _) = wrap_extracted_factory(&body, file, &lookup, &bridges, interop).unwrap();
        fs::write(dir.join(file), wrapped).unwrap();
    }
    let runtime = fs::read_to_string(dir.join("runtime.js")).unwrap();
    let mut relinked = relink_factory_references(&runtime, "runtime.js", &lookup).unwrap();
    if !accessor_lines.is_empty() {
        relinked = format!("{}\n{relinked}", accessor_lines.join("\n"));
    }
    fs::write(dir.join("runtime.js"), &relinked).unwrap();
    let shim = dir.join(interop.runtime_file());
    fs::create_dir_all(shim.parent().unwrap()).unwrap();
    fs::write(shim, BUN_RELINK_RUNTIME).unwrap();
    node(&dir.join("runtime.js"))
}

fn node(file: &Path) -> Result<String, String> {
    let out = std::process::Command::new("node")
        .arg(file)
        .output()
        .expect("node is on PATH (npm run check runs under it)");
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).into_owned())
    }
}

#[test]
fn the_scope_refs_bundle_itself_runs() {
    // The control: the input prints what the unpacked tree must print.
    let t = TempDir::new("scope-refs-control");
    let path = t.0.join("bundle.js");
    fs::write(&path, BUN_BUNDLE_SCOPE_REFS).unwrap();
    assert_eq!(node(&path).as_deref(), Ok("[43,\"hi!\"]\n"));
}

#[test]
fn a_vendor_body_never_references_a_bundle_scope_binding() {
    // Finding #51: only FACTORY references were rewritten and relinked, so
    // `u` (the bundle's __toESM) and `initM` / `Rq` / `ns` stayed free in
    // the vendor files — a ReferenceError the moment the factory ran.
    // Finding #60: the `initM`/`ns` READS are now bridged through the
    // owner file's live accessor, so the whole graph runs from vendor.
    let t = TempDir::new("scope-refs");
    unpack(BUN_BUNDLE_SCOPE_REFS, &t.0);
    assert_eq!(run_relinked(&t.0).as_deref(), Ok("[43,\"hi!\"]\n"));
}

#[test]
fn app_scope_reads_are_vendored_with_capture_records() {
    // Finding #60: a factory body's READ of a wrapper-scope binding (the
    // ESM init + namespace pair is the shape real bundles hit — Bun's
    // `require()` of an in-bundle ESM module, e.g. @aws-sdk/client-sts'
    // machinery at 2.1.182) no longer keeps the factory in the app. The
    // body is extracted to vendor and the read is recorded in the manifest
    // as a capture of the RAW name — the split resolves the name against
    // the fresh (pre-rename) text, takes the post-rename accessor from the
    // aligned shipped statement, and the finish bridges the read through
    // the owner file's live accessor.
    let t = TempDir::new("scope-refs-bridge");
    unpack(BUN_BUNDLE_SCOPE_REFS, &t.0);
    let manifest = read_manifest(&t.0);
    let entries = factories(&manifest);
    assert_eq!(entries.len(), 4, "nothing stays in the app: {entries:?}");
    let runtime = fs::read_to_string(t.0.join("runtime.js")).unwrap();
    assert!(!runtime.contains("var mod_c="), "{runtime}");
    assert!(!runtime.contains("var mod_d="), "{runtime}");
    // mod_a's __toESM capture still names the shim's helper.
    assert!(
        entries.iter().any(|e| {
            fs::read_to_string(t.0.join(s(e, "fileName")))
                .map(|b| b.contains("__toESM("))
                .unwrap_or(false)
        }),
        "{entries:?}"
    );
    // mod_c: the ESM pair reads are captures of their raw names.
    let mod_c = entries
        .iter()
        .find(|e| {
            fs::read_to_string(t.0.join(s(e, "fileName")))
                .map(|b| b.contains("__toCommonJS(ns)"))
                .unwrap_or(false)
        })
        .expect("mod_c is vendored");
    let mut captures: Vec<String> = mod_c["captures"]
        .as_array()
        .expect("mod_c records its app-scope reads")
        .iter()
        .map(|c| c["name"].as_str().expect("name").to_string())
        .collect();
    captures.sort();
    assert_eq!(
        captures,
        vec!["initM".to_string(), "ns".to_string()],
        "{entries:?}"
    );
}

#[test]
fn a_written_app_scope_binding_still_keeps_the_factory_in_the_app() {
    // The bridge is READ-only: a body that WRITES a wrapper-scope binding
    // cannot reach it through a get accessor, so the factory stays in the
    // app (finding #51's sound floor) — and so does everything that
    // references it.
    const BUN_WRITE_CAPTURE: &str = concat!(
        "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
        "var shared=1;\n",
        "var mod_w=x((exports)=>{shared=2;exports.value=shared;});\n",
        "var mod_r=x((exports)=>{exports.value=mod_w().value;});\n",
        "console.log(mod_r().value);\n",
    );
    let t = TempDir::new("scope-refs-write");
    unpack(BUN_WRITE_CAPTURE, &t.0);
    // Every factory stays in the app, so nothing is extractable — the
    // adapter falls to the passthrough floor and writes no manifest.
    assert!(
        !t.0.join("vendor/_bun-modules.json").exists(),
        "no factory was extracted"
    );
    let index = fs::read_to_string(t.0.join("index.js")).unwrap();
    assert!(index.contains("var mod_w=x("), "{index}");
    assert!(index.contains("var mod_r=x("), "{index}");
}

// ---- finding #90: carry by content, app text assets -------------------------

/// A vendor module whose one string changes LENGTH between releases: its
/// structural hash changes, so the exact-hash carry misses it.
fn yaml_bundle(message: &str) -> String {
    format!(
        concat!(
            "var x=(I,A)=>()=>(A||I((A={{exports:{{}}}}).exports,A),A.exports);\n",
            "var yamlish=x((exports)=>{{ exports.load=function load(s){{if(typeof s!==\"string\")",
            "throw new TypeError(\"{}\");var out=[];for(var i=0;i<s.length;i++){{out.push(s.charCodeAt(i))}}",
            "return out.join(\",\")+\"YAMLException: unexpected end of the stream\";}};",
            " exports.dump=function dump(o){{return JSON.stringify(o,null,2)+\"\\n---\\n\"}};",
            " exports.safeLoadAll=function safeLoadAll(docs,iterator){{return docs.split(\"---\").map(iterator)}}; }});\n",
            "var other=x((exports)=>{{ exports.render=function render(t){{return \"<div>\"+t+\"</div>\"}}; }});\n",
            "var main=yamlish();var o=other();"
        ),
        message
    )
}

fn hop_with(
    code: &str,
    dir: &Path,
    prior_dir: Option<&Path>,
    answer: fn(&VendorNameRequest) -> Option<String>,
) -> Vec<Vec<String>> {
    let mut namer = FnNamer {
        answer,
        asked: Vec::new(),
    };
    unpack_bun(
        code,
        dir,
        BunUnpackOptions {
            namer: Some(&mut namer),
            prior: prior_dir.and_then(|p| load_prior_vendor(&p.join("humanified.js"))),
            ..Default::default()
        },
    )
    .unwrap();
    namer.asked
}

fn entry_with<'a>(entries: &'a [Value], needle: &str, dir: &Path) -> &'a Value {
    entries
        .iter()
        .find(|e| {
            fs::read_to_string(dir.join(s(e, "fileName"))).is_ok_and(|body| body.contains(needle))
        })
        .unwrap_or_else(|| panic!("no vendor file holds {needle}: {entries:?}"))
}

#[test]
fn a_module_whose_string_changed_length_keeps_its_name_file_and_identifier() {
    let first = TempDir::new("pair1");
    hop_with(
        &yaml_bundle("expected a YAML document string"),
        &first.0,
        None,
        |r| {
            Some(if r.evidence.contains("YAMLException") {
                "js-yaml".into()
            } else {
                "html-render".into()
            })
        },
    );
    let m1 = factories(&read_manifest(&first.0));
    let before = entry_with(&m1, "YAMLException", &first.0).clone();
    assert_eq!(s(&before, "fileName"), "vendor/js-yaml.js");

    let second = TempDir::new("pair2");
    let asked = hop_with(
        &yaml_bundle("expected a YAML document as a string"),
        &second.0,
        Some(&first.0),
        |_| Some("yaml-parser".into()),
    );
    let m2 = factories(&read_manifest(&second.0));
    let after = entry_with(&m2, "YAMLException", &second.0);
    assert_ne!(
        s(after, "structuralHash"),
        s(&before, "structuralHash"),
        "the length change moved the hash"
    );
    assert!(
        asked.is_empty(),
        "the paired module is never re-asked: {asked:?}"
    );
    assert_eq!(s(after, "name"), "js-yaml");
    assert_eq!(
        s(after, "nameSource"),
        "llm",
        "the label is the prior's (#71)"
    );
    assert_eq!(s(after, "fileName"), "vendor/js-yaml.js");
    assert_eq!(
        s(after, "runtimeIdentifier"),
        s(&before, "runtimeIdentifier"),
        "the app code's name for the module is unchanged"
    );
    let runtime = fs::read_to_string(second.0.join("runtime.js")).unwrap();
    assert!(
        runtime.contains(&format!("{}()", s(&before, "runtimeIdentifier"))),
        "{runtime}"
    );
}

#[test]
fn an_ambiguous_content_pair_is_left_to_the_model() {
    // Two prior modules equally like the fresh one: no margin, no carry.
    let twin = |tag: &str| {
        format!(
            "var {tag}=x((exports)=>{{ exports.load=function load(s){{if(typeof s!==\"string\")throw new TypeError(\"bad input\");return s.split(\",\").map(function(v){{return v.trim()+\"YAMLException {tag}\"}})}}; }});\n"
        )
    };
    let head = "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n";
    let prior_code = format!(
        "{head}{}{}var a=one();var b=two();",
        twin("one"),
        twin("two")
    );
    let first = TempDir::new("ambig1");
    hop_with(&prior_code, &first.0, None, |r| {
        Some(
            if r.evidence.contains("one") {
                "lib-one"
            } else {
                "lib-two"
            }
            .into(),
        )
    });
    let fresh_code = format!("{head}{}var a=six();", twin("six"));
    let second = TempDir::new("ambig2");
    let asked = hop_with(&fresh_code, &second.0, Some(&first.0), |_| {
        Some("fresh-name".into())
    });
    assert_eq!(asked.iter().map(Vec::len).sum::<usize>(), 1, "{asked:?}");
    let m2 = factories(&read_manifest(&second.0));
    assert_eq!(s(&m2[0], "name"), "fresh-name");
}

const TEXT_ASSETS_BUNDLE: &str = concat!(
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
    "var envText=x((exports,module)=>{module.exports=\"## Environment\\n\\nDocs live at https://pypa.io/en/latest for the sandbox.\"});\n",
    "var libText=x((exports,module)=>{module.exports=`lorem ipsum dolor sit amet, the library's own template`});\n",
    "var lib=x((exports,module)=>{module.exports=function render(){return libText()}});\n",
    "var main=envText();var l=lib();"
);

#[test]
fn an_app_text_module_is_an_app_asset_named_from_its_text() {
    let t = TempDir::new("assets");
    let asked = hop_with(TEXT_ASSETS_BUNDLE, &t.0, None, |_| Some("react".into()));
    let entries = factories(&read_manifest(&t.0));
    let env = entry_with(&entries, "## Environment", &t.0);
    assert_eq!(s(env, "fileName"), "src/_assets/environment.js");
    assert_eq!(s(env, "name"), "environment");
    assert_eq!(s(env, "nameSource"), "asset");
    // Asked of nobody: the namer saw only the two vendor modules.
    assert_eq!(asked.iter().map(Vec::len).sum::<usize>(), 2, "{asked:?}");
    // A text module a vendor module requires is that library's own text.
    let lib_text = entry_with(&entries, "lorem ipsum", &t.0);
    assert!(
        s(lib_text, "fileName").starts_with("vendor/"),
        "{lib_text:?}"
    );
    // The app still reaches the asset through its identifier.
    let runtime = fs::read_to_string(t.0.join("runtime.js")).unwrap();
    assert!(
        runtime.contains(&format!("{}()", s(env, "runtimeIdentifier"))),
        "{runtime}"
    );
}
