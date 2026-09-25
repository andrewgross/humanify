//! Unpack tests (WPB.2), ported fixture-for-fixture from
//! `src/unpack/adapters/bun.test.ts` and `src/unpack/select-adapter.test.ts`,
//! plus the regex-floor and require-tracing cases the TS gets from V8 (the
//! patterns are emulated by hand here — `unpack::bun`'s docs say which).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::detect::detect_bundle;
use crate::modules::vendor_names::{VendorNameRequest, VendorNamer};
use crate::unpack::bun::{
    BunUnpackOptions, ExtractedModule, PriorVendor, extract_factory_bodies, find_prior_tree_root,
    identify_bun_require, load_prior_vendor, rewrite_require_calls, unpack_bun,
};
use crate::unpack::webcrack::parse_shim_output;
use crate::unpack::{UnpackAdapter, select_adapter, select_unpack_adapter};
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
        select_adapter(&detect_bundle(WEBPACK), None).name(),
        "webcrack"
    );
}

#[test]
fn selects_webcrack_for_browserify() {
    assert_eq!(
        select_adapter(&detect_bundle(BROWSERIFY), None).name(),
        "webcrack"
    );
}

#[test]
fn selects_bun_for_bun_cjs() {
    assert_eq!(select_adapter(&detect_bundle(BUN_HEAD), None).name(), "bun");
}

#[test]
fn selects_passthrough_for_esbuild() {
    assert_eq!(
        select_adapter(&detect_bundle(ESBUILD), None).name(),
        "passthrough"
    );
}

#[test]
fn selects_passthrough_for_unknown() {
    assert_eq!(
        select_adapter(&detect_bundle(PLAIN), None).name(),
        "passthrough"
    );
}

#[test]
fn respects_bundler_override() {
    assert_eq!(
        select_adapter(&detect_bundle(PLAIN), Some(BundlerType::Webpack)).name(),
        "webcrack"
    );
}

#[test]
fn override_to_unknown_is_ignored() {
    assert_eq!(
        select_adapter(&detect_bundle(WEBPACK), Some(BundlerType::Unknown)).name(),
        "webcrack"
    );
}

#[test]
fn select_by_name_and_unknown_name_errors() {
    assert_eq!(select_unpack_adapter("bun"), Ok(UnpackAdapter::Bun));
    assert_eq!(
        select_unpack_adapter("nope"),
        Err("No unpack adapter named \"nope\"".to_string())
    );
    assert!(UnpackAdapter::Bun.provides_module_fossils());
    assert!(!UnpackAdapter::Webcrack.provides_module_fossils());
    assert!(!UnpackAdapter::Passthrough.provides_module_fossils());
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
            prior: prior.map(PriorVendor::from_names),
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
    assert_eq!(names.get(&hash), Some(&vec!["js-yaml".to_string()]));
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
            },
            ExtractedModule {
                name: "b".into(),
                body_start: 41,
                body_end: 59,
                decl_start: 32,
                decl_end: 60,
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
    assert!(prior.ts_era.is_some(), "no hashVersion = TS era");
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
    assert_eq!(read_manifest(&fresh.0)["hashVersion"], 2);
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
