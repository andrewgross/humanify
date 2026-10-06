//! The split stage end to end on a regime the four oracle pairs never
//! reach (lesson 17): a runnable-emit DECLINE.

use std::collections::HashMap;

use serde_json::Value;

use super::{InputGate, SplitOptions, stable_split};
use crate::place::ledger::StableSplitLedger;
use crate::place::method::MarkerOffer;
use crate::place::placement_dump::Regime;

/// Finding #40: a prior ledger whose names place `xa` in a.js and `yb`,
/// `zb` in b.js makes a load-time reference cycle (a.js reads b.js's `yb`
/// at load, b.js reads a.js's `xa`) — the runnable emit declines, the
/// byte-exact review tree is written. The persisted ledger used to keep
/// the aliases the TS emit had assigned before it threw; aliases are per
/// importer now and carried from the prior tree's require lines (finding
/// #88), so the ledger records none — and a review tree has no require
/// lines to carry.
#[test]
fn a_declined_emit_writes_the_review_tree_and_records_no_aliases() {
    let vectors: Vec<Value> =
        serde_json::from_str(include_str!("../../../../../test/parity/wp53-cjs.json"))
            .expect("vectors");
    let v = vectors
        .iter()
        .find(|v| v["name"] == "load-time cycle declines")
        .expect("the cycle vector");
    let code = v["code"].as_str().expect("code");
    let order: Vec<String> = v["order"]
        .as_array()
        .expect("order")
        .iter()
        .map(|s| s.as_str().expect("file").to_string())
        .collect();
    let mut name_to_files: HashMap<String, Vec<String>> = HashMap::new();
    name_to_files.insert("xa".into(), vec!["a.js".into()]);
    name_to_files.insert("yb".into(), vec!["b.js".into()]);
    name_to_files.insert("zb".into(), vec!["b.js".into()]);
    for i in 0..60 {
        name_to_files.insert(format!("padFiller{i}"), vec!["pad/fill.js".into()]);
    }
    let prior = StableSplitLedger {
        version: 1,
        files: vec!["a.js".into(), "b.js".into(), "pad/fill.js".into()],
        name_to_files,
        order,
        hashes: None,
        emit_hashes: None,
        emit_names: None,
        emit_indexes: None,
        hash_version: None,
        fossil_modules: None,
    };
    let outcome = stable_split(
        code,
        SplitOptions {
            layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
            module_wrappers: crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
            markers: MarkerOffer::NotProvided,
            prior: Some(&prior),
            prior_tree: None,
            carry: None,
            namer: None,
            reviser: None,
            placement: Default::default(),
            align: Default::default(),
            registrar_exemption_disabled: false,
            split_pure: false,
            trail: None,
            vendor_captures: &[],
            vendor_fresh: None,
            input_gate: InputGate::TextAtHand,
        },
    )
    .expect("the split runs");
    assert_eq!(
        outcome.declined.as_deref(),
        v["declined"].as_str(),
        "the runnable emit declines with the TS's reason"
    );
    assert!(outcome.runnable.is_none(), "the review tree is written");
    let ledger = outcome.ledger.as_object().expect("a ledger object");
    assert!(
        ledger.get("aliases").is_none(),
        "the ledger records no aliases"
    );
    assert!(
        ledger.get("emitIndexes").is_none(),
        "the layout was never recorded"
    );
}

/// A wrapper bundle for the vendor-bridge tests (finding #60): statements
/// 0-1 declare the ESM init/namespace pair a vendored body reads. The
/// padding clears the wrapper's 50-binding recognition gate.
const BRIDGE_BUNDLE: &str = concat!(
    "(function (exports, require, module) {\n",
    "  var ns = {};\n",
    "  var initM = function () { ns.hi = function () { return 42; }; };\n",
    "  exports.setup = initM;\n",
    "  exports.read = function () { return (initM(), ns).hi(); };\n",
    "  var padFiller0=0,padFiller1=0,padFiller2=0,padFiller3=0,padFiller4=0,\n",
    "  padFiller5=0,padFiller6=0,padFiller7=0,padFiller8=0,padFiller9=0,\n",
    "  padFiller10=0,padFiller11=0,padFiller12=0,padFiller13=0,padFiller14=0,\n",
    "  padFiller15=0,padFiller16=0,padFiller17=0,padFiller18=0,padFiller19=0,\n",
    "  padFiller20=0,padFiller21=0,padFiller22=0,padFiller23=0,padFiller24=0,\n",
    "  padFiller25=0,padFiller26=0,padFiller27=0,padFiller28=0,padFiller29=0,\n",
    "  padFiller30=0,padFiller31=0,padFiller32=0,padFiller33=0,padFiller34=0,\n",
    "  padFiller35=0,padFiller36=0,padFiller37=0,padFiller38=0,padFiller39=0,\n",
    "  padFiller40=0,padFiller41=0,padFiller42=0,padFiller43=0,padFiller44=0,\n",
    "  padFiller45=0,padFiller46=0,padFiller47=0,padFiller48=0,padFiller49=0;\n",
    "});\n",
);

fn bridge_options<'a>(
    captures: &'a [crate::modules::vendor_names::ManifestCapture],
    fresh: Option<&'a str>,
) -> SplitOptions<'a, 'a> {
    SplitOptions {
        layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
        module_wrappers: crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
        markers: MarkerOffer::NotProvided,
        prior: None,
        prior_tree: None,
        carry: None,
        namer: None,
        reviser: None,
        placement: Default::default(),
        align: Default::default(),
        registrar_exemption_disabled: false,
        split_pure: false,
        trail: None,
        vendor_captures: captures,
        vendor_fresh: fresh,
        // The bridge bundle's own text clears the ≥50 gate — the threshold
        // reads the text at hand here (see the gated tests below for the
        // original-bundle path).
        input_gate: InputGate::TextAtHand,
    }
}

fn capture(name: &str) -> crate::modules::vendor_names::ManifestCapture {
    crate::modules::vendor_names::ManifestCapture {
        name: name.to_string(),
    }
}

/// Finding #60: the unpack's capture records — raw names a vendored body
/// READS — resolve against the FRESH text (still raw-named, aligned
/// statement-for-statement with the shipped one) and the split's placement:
/// (raw name → owner file + post-rename accessor). The owner file EXPORTS
/// the accessor whether or not any app file references the binding,
/// because the reader is a vendor body the emit cannot see. Here the
/// shipped text carries the renames (`ns` → `nsRenamed`), exactly as the
/// naming stage leaves it.
#[test]
fn vendor_captures_resolve_to_owner_files_with_forced_accessors() {
    let shipped = BRIDGE_BUNDLE
        .replace("ns", "nsRenamed")
        .replace("initM", "initMRenamed");
    let outcome = stable_split(
        &shipped,
        bridge_options(&[capture("ns"), capture("initM")], Some(BRIDGE_BUNDLE)),
    )
    .expect("the split runs");
    assert!(
        outcome.declined.is_none(),
        "{:?}",
        outcome.declined.as_deref().unwrap_or_default()
    );
    let mut bridges = outcome.vendor_bridges.clone();
    bridges.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(bridges.len(), 2);
    assert_eq!(bridges[0].name, "initM");
    assert_eq!(bridges[0].accessor, "initMRenamed");
    assert_eq!(bridges[1].name, "ns");
    assert_eq!(bridges[1].accessor, "nsRenamed");
    assert_eq!(
        bridges[0].file, bridges[1].file,
        "both statements share the owner file"
    );
    let owner = &bridges[0].file;
    let emitted = outcome
        .files
        .iter()
        .find(|(p, _)| p == owner)
        .expect("the owner file was emitted");
    assert_eq!(
        emitted
            .1
            .matches("Object.defineProperty(module.exports, \"initMRenamed\"")
            .count(),
        1
    );
    assert_eq!(
        emitted
            .1
            .matches("Object.defineProperty(module.exports, \"nsRenamed\"")
            .count(),
        1
    );
}

// ── the ≥50 wrapper gate reads the run's ORIGINAL input bundle ────────
//
// Being a bundled app is a property of the INPUT. The split is handed the
// POST-EXTRACTION runtime, where the vendor extraction has already
// spliced out one wrapper-scope declaration per vendored module, so a
// mid-size app whose vendor half dominates (the esbuild lane's real test
// app: semver+ms+mitt, 32 bindings left after extraction) falls under the
// frozen WP1.5 threshold although its input clears it comfortably. The
// threshold reads the ORIGINAL text the unpack stage saw; the WRAPPER
// GRAMMAR still reads the text at hand (a non-bundle cannot mint phantom
// modules — pinned below).

/// One esbuild-shaped CJS dep (the `__commonJS` object form): one
/// wrapper-scope binding pre-extraction, spliced out WHOLE by the vendor
/// extraction.
fn cjs_dep_decl(i: usize) -> String {
    format!(
        "  var require_mod{i:02} = __commonJS({{ \"src/vendor/mod{i:02}.cjs\"(\
exports, module) {{ var base = {i}; module.exports = {{ base }}; }} }});\n"
    )
}

/// A mid-size bundle whose vendor half dominates: `deps` CJS modules plus
/// a small ESM app half, esbuild's iife shape behind the `"use strict"`
/// directive prologue. Wrapper-scope bindings: 3 helpers + `deps`
/// requires + `deps` interop imports + the app half.
fn dominant_vendor_bundle(deps: usize) -> String {
    let mut s = String::from("\"use strict\";\n(() => {\n");
    s.push_str("  var __defProp = Object.defineProperty;\n");
    s.push_str(
        "  var __commonJS = (cb, mod) => function __require() {\n\
             return (mod || (0, cb[Object.getOwnPropertyNames(cb)[0]])((mod = { exports: {} }).exports, mod), mod.exports);\n\
           };\n",
    );
    s.push_str("  var __toESM = (mod) => mod;\n");
    for i in 0..deps {
        s.push_str(&cjs_dep_decl(i));
    }
    for i in 0..deps {
        s.push_str(&format!(
            "  var import_mod{i:02} = __toESM(require_mod{i:02}());\n"
        ));
    }
    s.push_str("  var REPORT = [];\n  for (const line of REPORT) { console.log(line); }\n})();\n");
    s
}

/// The same bundle's POST-EXTRACTION runtime: every `require_modNN`
/// declaration is gone (vendored) and its references were rewritten to
/// the vendored bodies' file identifiers, exactly as the unpack stage
/// leaves the runtime it hands the naming stage and the split.
fn dominant_vendor_runtime(deps: usize) -> String {
    let mut s = String::from("\"use strict\";\n(() => {\n");
    s.push_str("  var __defProp = Object.defineProperty;\n");
    s.push_str("  var __commonJS = (cb, mod) => function __require() { return mod; };\n");
    s.push_str("  var __toESM = (mod) => mod;\n");
    for i in 0..deps {
        s.push_str(&format!(
            "  var import_mod{i:02} = __toESM(vendored_mod{i:02}());\n"
        ));
    }
    s.push_str("  var REPORT = [];\n  for (const line of REPORT) { console.log(line); }\n})();\n");
    s
}

fn gated_options<'a>(original_bundle: Option<&'a str>) -> SplitOptions<'a, 'a> {
    SplitOptions {
        layout: crate::toolchain::BundleLayout::SingleWrapperFunction,
        module_wrappers: crate::toolchain::ModuleWrapperGrammar::BunAndEsbuild,
        markers: MarkerOffer::NotProvided,
        prior: None,
        prior_tree: None,
        carry: None,
        namer: None,
        reviser: None,
        placement: Default::default(),
        align: Default::default(),
        registrar_exemption_disabled: false,
        split_pure: false,
        trail: None,
        vendor_captures: &[],
        vendor_fresh: None,
        input_gate: original_bundle.map_or(InputGate::TextAtHand, InputGate::Original),
    }
}

/// The wrapper binding counts that make the shape: the INPUT clears the
/// frozen ≥50 threshold, the post-extraction runtime the split is HANDED
/// sits under it.
fn wrapper_binding_count(text: &str) -> Option<usize> {
    let allocator = oxc_allocator::Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, text, "gate.js");
    assert!(
        ingest.errors.is_empty(),
        "fixture must parse: {:?}",
        ingest.errors
    );
    crate::modules::wrapper::find_wrapper_function(ingest.program, ingest.semantic())
        .map(|w| w.binding_count)
}

#[test]
fn a_mid_size_bundle_whose_vendor_half_dominates_splits() {
    let original = dominant_vendor_bundle(24);
    let runtime = dominant_vendor_runtime(24);
    assert!(
        wrapper_binding_count(&original).is_some_and(|n| n >= 50),
        "the input bundle clears the frozen threshold"
    );
    assert!(
        wrapper_binding_count(&runtime).is_none(),
        "the post-extraction runtime sits UNDER the threshold (the old gate fails it)"
    );
    // The historical gate, on the runtime alone, still fails loud — this
    // is the failure the original-bundle gate exists to lift.
    let err = match stable_split(&runtime, gated_options(None)) {
        Err(e) => e,
        Ok(_) => panic!("the runtime alone does not clear the ≥50 gate"),
    };
    assert!(err.contains("no recognizable bundle wrapper"), "{err}");
    // The same runtime splits when the run hands the ORIGINAL input the
    // unpack stage saw: being a bundled app is a property of the INPUT.
    let outcome = stable_split(&runtime, gated_options(Some(&original)))
        .expect("the mid-size app splits on the strength of its input");
    assert!(outcome.stats.statements > 1);
}

#[test]
fn a_non_bundled_input_still_fails_the_split_loudly() {
    // A plain script: no wrapper anywhere — the tight WP1.5 grammar, with
    // or without an input gate. The gate must not become pass-always.
    let plain = "var a = 1;\nvar b = 2;\nconsole.log(a + b);\n";
    for original in [None, Some(plain)] {
        let err = match stable_split(plain, gated_options(original)) {
            Err(e) => e,
            Ok(_) => panic!("a plain script never splits"),
        };
        assert!(err.contains("no recognizable bundle wrapper"), "{err}");
    }
    // A small IIFE input: the grammar passes, but the INPUT's binding
    // count is under the threshold — the same ≥50 logic, now measured on
    // the input the unpack stage saw.
    let small_iife = "(function(){var b=1;return b;})();\n";
    let err = match stable_split(small_iife, gated_options(Some(small_iife))) {
        Err(e) => e,
        Ok(_) => panic!("a small-IIFE input never splits"),
    };
    assert!(err.contains("no recognizable bundle wrapper"), "{err}");
}

#[test]
fn the_wrapper_grammar_still_reads_the_runtime_even_with_an_input_gate() {
    // The original clears the gate, but the text handed to the split is
    // not one wrapper IIFE: the grammar is measured on the text at hand —
    // a non-bundle cannot be lifted into a split tree by its input.
    let original = dominant_vendor_bundle(24);
    let stray = "var a = 1;\nvar b = 2;\nconsole.log(a + b);\n";
    let err = match stable_split(stray, gated_options(Some(&original))) {
        Err(e) => e,
        Ok(_) => panic!("the grammar gates the text at hand"),
    };
    assert!(err.contains("no recognizable bundle wrapper"), "{err}");
}

/// A capture the split cannot resolve would leave a free name in a vendor
/// file — the exact hole finding #51 closed — so it fails the split
/// loudly, never silently: an unknown raw name, a name the fresh text
/// declares twice, a fresh text that no longer aligns with the shipped
/// one, or no fresh text at all.
#[test]
fn an_unresolvable_vendor_capture_fails_the_split() {
    let err = match stable_split(
        BRIDGE_BUNDLE,
        bridge_options(&[capture("gone")], Some(BRIDGE_BUNDLE)),
    ) {
        Err(e) => e,
        Ok(_) => panic!("an unknown raw name fails"),
    };
    assert!(err.contains("gone"), "{err}");
    let err = match stable_split(BRIDGE_BUNDLE, bridge_options(&[capture("ns")], None)) {
        Err(e) => e,
        Ok(_) => panic!("captures without a fresh text fail"),
    };
    assert!(err.contains("fresh"), "{err}");
    // A fresh text whose statement list does not align with the shipped
    // one (here: one statement fewer) is refused, never guessed through.
    let misaligned = BRIDGE_BUNDLE.replace("  var ns = {};\n", "");
    let err = match stable_split(
        BRIDGE_BUNDLE,
        bridge_options(&[capture("ns")], Some(&misaligned)),
    ) {
        Err(e) => e,
        Ok(_) => panic!("a fresh text that disagrees with the shipped one fails"),
    };
    assert!(err.contains("align"), "{err}");
    // The wrapper is ONE scope: the same raw name declared by two of its
    // statements is ambiguous and refused.
    let dup = BRIDGE_BUNDLE.replace("var ns = {};", "var ns = {};\n  var ns = 1;");
    let err = match stable_split(&dup, bridge_options(&[capture("ns")], Some(&dup))) {
        Err(e) => e,
        Ok(_) => panic!("a name declared twice is refused"),
    };
    assert!(err.contains("exactly one"), "{err}");
}

/// A wrapper bundle for the fresh grouping (the clustered regime): enough
/// pad bindings to clear the ≥50 gate, plus `decls` — the statements under
/// test.
fn clustered_bundle(decls: &str) -> String {
    let mut s = String::from("(function () {\n");
    s.push_str(decls);
    for i in 0..55 {
        s.push_str(&format!("  var pad{i:02} = {i};\n"));
    }
    s.push_str("  console.log(pad00);\n})();\n");
    s
}

fn vendor_files(outcome: &super::SplitOutcome) -> Vec<String> {
    outcome
        .files
        .iter()
        .map(|(f, _)| f.clone())
        .filter(|f| f.starts_with("vendor/"))
        .collect()
}

/// Toolchain review R7: the fresh grouping's vendor bucket takes the
/// module helper from the run's module wrapper grammar (P3), never from
/// its own "most-used higher-order callee" tally. An app's own wrapper
/// used twice (a minified `forwardRef`/`memo`, `K7(function …)`) is not a
/// bundled module: its statements stay in the app.
#[test]
fn the_fresh_grouping_does_not_vendor_an_app_wrapper_the_grammar_rejects() {
    let code = clustered_bundle(
        "  var K7 = (render) => render;\n\
         \x20 var Card = K7(function (props) { return props.a; });\n\
         \x20 var Row = K7(function (props) { return props.b; });\n",
    );
    let outcome = stable_split(&code, gated_options(None)).expect("splits");
    assert_eq!(vendor_files(&outcome), Vec::<String>::new());
}

/// The same bucket still takes what the grammar recognises: Bun's module
/// helper (the tight `{exports:{}}` marker) wrapping two factories.
#[test]
fn the_fresh_grouping_vendors_the_modules_the_grammar_recognises() {
    let code = clustered_bundle(
        "  var U = (A, q) => () => (q || A((q = {exports:{}}).exports, q), q.exports);\n\
         \x20 var reqA = U((e, m) => { m.exports = 1; });\n\
         \x20 var reqB = U((e, m) => { m.exports = 2; });\n",
    );
    let outcome = stable_split(&code, gated_options(None)).expect("splits");
    assert_eq!(
        vendor_files(&outcome).len(),
        2,
        "{:?}",
        vendor_files(&outcome)
    );
}

// ── the split method comes from what the bundle CONTAINS (C1/C2) ──────

/// Options for a run whose toolchain OFFERS the module markers (every Bun
/// and esbuild bundle) — the split decides from the bundle itself.
fn offered_options<'a>() -> SplitOptions<'a, 'a> {
    SplitOptions {
        markers: MarkerOffer::Offered,
        ..gated_options(None)
    }
}

const LAZY_HELPER: &str = "  var __esm = (fn, res) => () => (fn && (res = fn(fn = 0)), res);\n";

/// C1: a Bun app with plain static imports — no lazy-init module at all.
/// The bundler offers markers, the bundle records none: on main the split
/// chose the marker method by BUNDLER and failed after naming ("records
/// no module fossils"); now it takes the fresh grouping.
#[test]
fn a_bundle_with_no_markers_splits_by_the_fresh_grouping() {
    let code = clustered_bundle(LAZY_HELPER);
    let outcome = stable_split(&code, offered_options()).expect("the split runs");
    assert_eq!(outcome.method.regime, Regime::Cluster);
    let c = outcome.method.coverage.expect("measured");
    assert_eq!(c.modules, 0);
    assert!(
        outcome.method.reason.contains("no module markers"),
        "{}",
        outcome.method.reason
    );
}

/// C2: a mixed bundle — eager code before the one lazy module, more after
/// it — falls under the threshold and gets the fresh grouping for all.
#[test]
fn a_mixed_bundle_splits_by_the_fresh_grouping() {
    let code = clustered_bundle(&format!(
        "{LAZY_HELPER}  var eagerTable = buildTable(4);\n  setupEager(eagerTable);\n\
         \x20 var lateValue;\n  var lateInit = __esm(() => {{ lateValue = readConfig(1); }});\n"
    ));
    let outcome = stable_split(&code, offered_options()).expect("the split runs");
    assert_eq!(outcome.method.regime, Regime::Cluster);
    let c = outcome.method.coverage.expect("measured");
    assert_eq!(c.modules, 1);
    assert_eq!(c.glued_statements, 2);
}

/// Claude Code's shape — every module lazy, a one-call entry tail — keeps
/// the marker method.
#[test]
fn an_all_lazy_bundle_keeps_the_marker_method() {
    let mut decls = String::from(LAZY_HELPER);
    for i in 0..30 {
        decls.push_str(&format!(
            "  function read{i:02}() {{ return value{i:02} + {i}; }}\n\
             \x20 var value{i:02};\n\
             \x20 var init{i:02} = __esm(() => {{ value{i:02} = readConfig({i}); }});\n"
        ));
    }
    let code = format!("(function () {{\n{decls}  init00();\n}})();\n");
    let outcome = stable_split(&code, offered_options()).expect("the split runs");
    assert_eq!(outcome.method.regime, Regime::Fossil);
    assert_eq!(outcome.method.coverage.map(|c| c.modules), Some(30));
}
