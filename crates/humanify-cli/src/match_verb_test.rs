//! `humanify match` — the fixture-backed tests for the match verb: a
//! fixed two-version input where the ground truth is known by
//! construction (functions `addValues` / `sumTo` are byte-identical in
//! both versions, `onlyInOld` is gone from the new version,
//! `onlyInNew` is brand new), plus determinism, the dump's contract
//! with the ground-truth harness (every graph row carries its source
//! slice, so the harness never re-parses JavaScript), and the RAW-bundle
//! work dir: a Bun bundle's vendor files are bare CJS-factory bodies
//! that do not parse standalone, and the verb must hand the matcher the
//! same wrapped shape the split stage ships (`exports.f = …`).

use serde_json::Value;

use crate::match_verb::{MatchVerbArgs, match_dump};

const NEW_VERSION: &str = "function addValues(first, second) {
  return first + second;
}
function sumTo(limit) {
  var total = 0;
  for (var i = 0; i < limit; i++) {
    total += i;
  }
  return total;
}
function onlyInNew(flag) {
  return flag ? 1 : 0;
}
";

const OLD_VERSION: &str = "function addValues(first, second) {
  return first + second;
}
function sumTo(limit) {
  var total = 0;
  for (var i = 0; i < limit; i++) {
    total += i;
  }
  return total;
}
function onlyInOld(first, second) {
  return first ? second : 0;
}
";

/// The old version run through the stage-6 formatter — the closest
/// no-LLM equivalent of a humanified prior (the harness does the same).
fn formatted_prior(dir: &std::path::Path) -> String {
    let text = humanify_core::format::format(
        OLD_VERSION,
        &humanify_core::format::FormatOptions::default(),
    )
    .expect("the fixture formats");
    let path = dir.join("prior.js");
    std::fs::write(&path, text).expect("write prior fixture");
    path.display().to_string()
}

/// The dump for the fixed pair, with both texts on disk under `dir`.
fn dump_for(dir: &std::path::Path) -> Value {
    let input = dir.join("input.js");
    std::fs::write(&input, NEW_VERSION).expect("write input fixture");
    match_dump(&MatchVerbArgs {
        input: &input.display().to_string(),
        prior_version: &formatted_prior(dir),
        sequential: false,
        bundler: None,
        minifier: None,
        webcrack_shim: None,
        work_dir: Some(&dir.join("work").display().to_string()),
        keep_work_dir: true,
    })
    .expect("the match verb runs on the fixed fixture")
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("humanify-match-verb-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fixture dir");
    dir
}

fn names(rows: &Value) -> Vec<String> {
    rows.as_array()
        .expect("inventory rows are an array")
        .iter()
        .map(|r| r["name"].as_str().expect("a name").to_string())
        .collect()
}

/// The one file section (the passthrough adapter unpacks to one file).
fn one_file(dump: &Value) -> &Value {
    let files = dump["files"].as_array().expect("files is an array");
    assert_eq!(files.len(), 1, "one unpacked file: {dump}");
    &files[0]
}

#[test]
fn identical_functions_are_matched_with_a_tier() {
    let dir = temp_dir("pairs");
    let dump = dump_for(&dir);
    let file = one_file(&dump);
    let pairs = file["functions"]["pairs"].as_array().expect("pairs array");
    assert!(
        pairs.len() >= 2,
        "addValues and sumTo must both match: {pairs:?}"
    );
    for pair in pairs {
        let tier = pair["tier"].as_str().expect("every pair names its tier");
        assert!(!tier.is_empty());
    }
    // The pair set is exactly the two functions both versions share.
    let prior_rows = &file["functions"]["prior"];
    let mut matched: Vec<String> = pairs
        .iter()
        .map(|p| {
            let idx = p["prior"].as_u64().expect("a prior index") as usize;
            names(prior_rows)[idx].clone()
        })
        .collect();
    matched.sort();
    assert_eq!(matched, ["addValues", "sumTo"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn vanished_prior_function_is_unmatched_and_new_one_is_unpaired() {
    let dir = temp_dir("unmatched");
    let dump = dump_for(&dir);
    let file = one_file(&dump);
    let functions = &file["functions"];
    let unmatched: Vec<String> = functions["unmatched"]
        .as_array()
        .expect("unmatched array")
        .iter()
        .map(|u| {
            let idx = u.as_u64().expect("an inventory index") as usize;
            names(&functions["prior"])[idx].clone()
        })
        .collect();
    assert_eq!(unmatched, ["onlyInOld"], "the removed function is reported");
    // The brand-new function exists on the fresh side and no pair
    // references it.
    assert!(
        names(&functions["fresh"]).contains(&"onlyInNew".to_string()),
        "the new function is in the fresh inventory"
    );
    let fresh_paired: Vec<String> = functions["pairs"]
        .as_array()
        .expect("pairs array")
        .iter()
        .map(|p| {
            let idx = p["fresh"].as_u64().expect("a fresh index") as usize;
            names(&functions["fresh"])[idx].clone()
        })
        .collect();
    assert!(
        !fresh_paired.contains(&"onlyInNew".to_string()),
        "a function that exists only in the new version is never paired"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_inventory_row_carries_its_source_slice() {
    let dir = temp_dir("slices");
    let dump = dump_for(&dir);
    let file = one_file(&dump);
    let fresh_text = file["freshText"].as_str().expect("the formatted text");
    for side in [&file["functions"]["prior"], &file["functions"]["fresh"]] {
        for row in side.as_array().expect("inventory rows") {
            let slice = row["slice"].as_str().expect("a source slice");
            assert!(!slice.is_empty(), "no empty function slices");
            assert!(
                fresh_text.contains(slice.trim()) || slice.contains("return"),
                "slices come from the matched texts"
            );
        }
    }
    // The addValues slices are byte-identical across the two versions —
    // the ground-truth predicate the harness applies.
    let slice_of = |rows: &Value, name: &str| -> String {
        rows.as_array()
            .expect("rows")
            .iter()
            .find(|r| r["name"] == name)
            .map(|r| r["slice"].as_str().expect("slice").to_string())
            .unwrap_or_default()
    };
    let prior_slice = slice_of(&file["functions"]["prior"], "addValues");
    let fresh_slice = slice_of(&file["functions"]["fresh"], "addValues");
    assert_eq!(
        prior_slice, fresh_slice,
        "identical source, identical slice"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn dump_is_deterministic_across_runs() {
    let dir = temp_dir("determinism");
    let a = serde_json::to_string(&dump_for(&dir)).expect("json");
    // A second run over freshly written fixtures (new prior formatting,
    // new input bytes) must produce the identical dump.
    let b = serde_json::to_string(&dump_for(&dir)).expect("json");
    assert_eq!(a, b, "the match verb's dump is byte-deterministic");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn statement_twins_and_close_tier_are_reported() {
    let dir = temp_dir("twins");
    let dump = dump_for(&dir);
    let file = one_file(&dump);
    let twins = &file["twins"];
    assert!(
        !twins["prior"]
            .as_array()
            .expect("prior statements")
            .is_empty(),
        "the prior statement inventory is reported"
    );
    assert!(
        !twins["gates"]["rows"]
            .as_array()
            .expect("the gates' dump rows")
            .is_empty(),
        "the shared statements are proposed and gated"
    );
    let close = &file["close"];
    assert!(
        close.is_object(),
        "the close tier ran (a prior version is present)"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_prior_file_is_a_loud_error() {
    let dir = temp_dir("missing-prior");
    let input = dir.join("input.js");
    std::fs::write(&input, NEW_VERSION).expect("write input fixture");
    let err = match_dump(&MatchVerbArgs {
        input: &input.display().to_string(),
        prior_version: &dir.join("nope.js").display().to_string(),
        sequential: false,
        bundler: None,
        minifier: None,
        webcrack_shim: None,
        work_dir: None,
        keep_work_dir: false,
    })
    .expect_err("a missing prior fails loudly");
    assert!(err.contains("nope.js"), "the error names the file: {err}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- a RAW bun-bundle work dir: the vendor factory files ---------------

/// The Bun CJS bundle head: the `createRequire` alias and the `x` factory
/// helper, the shape `detect` selects the Bun adapter on.
const BUN_BUNDLE_HEAD: &str = concat!(
    "import{createRequire as Glq}from\"node:module\";\n",
    "var m6=Glq(import.meta.url);\n",
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n"
);

/// A Bun CJS bundle whose one factory is an OLD-SHAPE `function (…) {…}`
/// expression — the outer shape Bun gives large factories, and the one
/// the raw unpack tree does NOT parse standalone (a bare anonymous
/// function expression at statement position). This is the walk-regime
/// input the exp092 single-file scope never hit: on the real
/// claude-code bundles, hundreds of vendor files are this shape.
const BUN_FACTORY_BUNDLE: &str = concat!(
    "import{createRequire as Glq}from\"node:module\";\n",
    "var m6=Glq(import.meta.url);\n",
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\n",
    "var mod_a=x(function(exports,module){\n",
    "  module.exports={pick:function(v){return v}};\n",
    "});\n",
    "var main=mod_a().pick(1);\n"
);

/// The prior release's vendor file for the same factory, as the finish
/// ships it — the split's `exports.f = __commonJS(F)` wrapping, the
/// shape every humanified prior tree carries for its vendor modules.
const PRIOR_VENDOR_FILE: &str = concat!(
    "const { __commonJS } = require(\"../.humanify/__bun-runtime.js\");\n",
    "exports.f = __commonJS(function (exports, module) {\n",
    "  module.exports = { pick: function (v) { return v; } };\n",
    "});\n"
);

/// Text formatted through the stage-6 formatter, written under `dir`.
fn formatted_file(dir: &std::path::Path, name: &str, code: &str) -> String {
    let text =
        humanify_core::format::format(code, &humanify_core::format::FormatOptions::default())
            .expect("the fixture formats");
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write fixture");
    path.display().to_string()
}

#[test]
fn raw_bundle_vendor_factories_are_wrapped_and_loudly_reported() {
    let dir = temp_dir("raw-bundle");
    let prior = formatted_file(&dir, "prior.js", PRIOR_VENDOR_FILE);
    let input_file = dir.join("input.js");
    std::fs::write(&input_file, BUN_FACTORY_BUNDLE).expect("write the raw bundle");
    let input = input_file.display().to_string();
    let dump = match_dump(&MatchVerbArgs {
        input: &input,
        prior_version: &prior,
        sequential: false,
        bundler: None,
        minifier: None,
        webcrack_shim: None,
        work_dir: Some(&dir.join("work").display().to_string()),
        keep_work_dir: true,
    })
    .expect("the verb runs on a RAW bun-bundle work dir (vendor files wrap)");

    // Two unpacked files (runtime plus the vendor file): the multi-file
    // shape — schema 2, the prior side hoisted, sections fresh-only.
    assert_eq!(
        dump["schemaVersion"], 2,
        "a raw bundle is a multi-file dump"
    );
    for section in dump["files"].as_array().expect("files is an array") {
        assert!(
            section["functions"].get("prior").is_none(),
            "no per-section prior inventory in the multi-file shape"
        );
    }
    assert!(
        !dump["prior"]["functions"]
            .as_array()
            .expect("the hoisted prior side")
            .is_empty(),
        "the prior side is hoisted once"
    );

    // The vendor file is in the dump, in the SPLIT'S wrapped shape — the
    // same matching surface the humanified prior carries.
    let files = dump["files"].as_array().expect("files is an array");
    let vendor = files
        .iter()
        .find(|f| f["path"].as_str().is_some_and(|p| p.starts_with("vendor/")))
        .expect("a wrapped vendor file section is in the dump");
    let fresh_text = vendor["freshText"].as_str().expect("the fresh text");
    assert!(
        fresh_text.contains("__commonJS("),
        "the dumped vendor text carries the split's wrapper: {fresh_text}"
    );
    // The factory's functions reached the matcher (the bodies are
    // anonymous: `pick` only names the property whose value is one).
    let fresh_rows = vendor["functions"]["fresh"].as_array().expect("rows");
    assert!(
        fresh_rows
            .iter()
            .any(|r| r["slice"].as_str().is_some_and(|s| s.contains("pick"))),
        "the factory body's functions are inventoried: {fresh_rows:?}"
    );
    // The wrapped factory matches its prior counterpart — the dump
    // describes the real matching surface.
    let pairs = vendor["functions"]["pairs"].as_array().expect("pairs");
    assert!(
        !pairs.is_empty(),
        "the wrapped vendor factory is matched against the prior"
    );
    // The runtime is still dumped too.
    assert!(
        files.iter().any(|f| f["path"] == "runtime.js"),
        "the runtime file is still in the dump"
    );
    // NEVER silently: meta says what was wrapped, by path and count.
    let wrapped: Vec<String> = dump["meta"]["wrappedFactoryFiles"]
        .as_array()
        .expect("meta lists the wrapped vendor files")
        .iter()
        .map(|p| p.as_str().expect("a path").to_string())
        .collect();
    assert_eq!(wrapped.len(), 1, "exactly the one vendor file: {wrapped:?}");
    assert!(
        wrapped[0].starts_with("vendor/"),
        "the wrapped list names the vendor path: {wrapped:?}"
    );
    let differences: Vec<String> = dump["meta"]["differences"]
        .as_array()
        .expect("differences")
        .iter()
        .map(|d| d.as_str().expect("a line").to_string())
        .collect();
    assert!(
        differences
            .iter()
            .any(|d| d.contains("1 vendor factory file(s) wrapped")),
        "meta.differences says the wrap loudly: {differences:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A bundle of `n` old-shape factories and the prior release's own vendor
/// files: ~2 prior functions per unit, so `n = 30` puts the prior side at
/// the same-program guard's production scale (>= 50 functions), where each
/// vendor file is a sliver of the whole prior tree and the per-call
/// guard's granularity would be wrong.
fn factory_fixture(n: usize) -> (String, String) {
    let mut bundle = String::from(BUN_BUNDLE_HEAD);
    let mut prior =
        String::from("const { __commonJS } = require(\"../.humanify/__bun-runtime.js\");\n");
    for i in 0..n {
        let i = i.to_string();
        bundle.push_str("var mod_");
        bundle.push_str(&i);
        bundle.push_str("=x(function(exports,module){module.exports={pick");
        bundle.push_str(&i);
        bundle.push_str(":function(v){return v+");
        bundle.push_str(&i);
        bundle.push_str("}};});\n");
        // One prior "vendor file" per factory, as the finish ships it (the
        // header deduped across the concatenated prior: one `const`).
        prior.push_str("exports.f = __commonJS(function (exports, module) {\n");
        prior.push_str("  module.exports = { pick");
        prior.push_str(&i);
        prior.push_str(": function (v) { return v + ");
        prior.push_str(&i);
        prior.push_str("; } };\n});\n");
    }
    bundle.push_str("var run=mod_0().pick0(1);\n");
    (bundle, prior)
}

/// Run the verb over a bundle/prior pair under `tag`.
fn raw_bundle_dump(dir: &std::path::Path, bundle: &str, prior_text: &str) -> Result<Value, String> {
    let prior = formatted_file(dir, "prior.js", prior_text);
    let input_file = dir.join("input.js");
    std::fs::write(&input_file, bundle).expect("write the raw bundle");
    match_dump(&MatchVerbArgs {
        input: &input_file.display().to_string(),
        prior_version: &prior,
        sequential: false,
        bundler: None,
        minifier: None,
        webcrack_shim: None,
        work_dir: Some(&dir.join("work").display().to_string()),
        keep_work_dir: true,
    })
}

#[test]
fn a_multi_file_dump_hoists_the_prior_side_and_matches_every_file() {
    let dir = temp_dir("multi-raw");
    let (bundle, prior_text) = factory_fixture(30);
    let dump = raw_bundle_dump(&dir, &bundle, &prior_text)
        .expect("the multi-file raw-bundle run matches every file");

    // The multi-file shape: schema 2, the prior side hoisted ONCE, the
    // sections carrying only their own fresh side.
    assert_eq!(
        dump["schemaVersion"], 2,
        "the multi-file shape is schemaVersion 2"
    );
    let files = dump["files"].as_array().expect("files");
    assert!(
        files.len() >= 31,
        "30 vendor sections plus the runtime: {files:?}"
    );
    for section in files {
        assert!(
            section["functions"].get("prior").is_none() && section["twins"].get("prior").is_none(),
            "sections carry no prior inventory: {}",
            section["path"]
        );
    }
    let prior_rows = dump["prior"]["functions"]
        .as_array()
        .expect("the hoisted prior");
    assert!(
        prior_rows.len() >= 50,
        "the prior at the guard's production scale: {}",
        prior_rows.len()
    );
    assert!(
        !dump["prior"]["statements"]
            .as_array()
            .expect("the hoisted prior statements")
            .is_empty()
    );
    // Pairs reference the SHARED inventory (indices valid there).
    let mut matched = std::collections::HashSet::new();
    for section in files {
        for pair in section["functions"]["pairs"].as_array().expect("pairs") {
            let idx = pair["prior"].as_u64().expect("a prior index") as usize;
            assert!(
                idx < prior_rows.len(),
                "pair indices reference the hoisted prior inventory"
            );
            matched.insert(idx);
        }
    }
    // The vendor factories ARE matched — the dump describes the real
    // matching surface — and the aggregate same-program fraction passes.
    assert!(
        !matched.is_empty() && matched.len() as f64 / prior_rows.len() as f64 >= 0.05,
        "matched {}/{} prior functions across the tree",
        matched.len(),
        prior_rows.len()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_multi_file_dump_with_a_wrong_prior_fails_the_same_program_check() {
    let dir = temp_dir("wrong-prior");
    let (bundle, _) = factory_fixture(30);
    // A prior of 55 functions sharing NOTHING with the bundle: the
    // same-program check must fail — at the DUMP's granularity (over the
    // union of every file's pairs), with the guard's own message.
    let mut wrong = String::new();
    for i in 0..55 {
        wrong.push_str(&format!(
            "function filler{i}(q) {{ return \"unrelated-thing-{i}\".length * q - {i}; }}\n"
        ));
    }
    let err = raw_bundle_dump(&dir, &bundle, &wrong)
        .expect_err("a wrong prior fails, never scores a wrong-program match");
    assert!(
        err.contains("does not appear to be the same program"),
        "the guard's message, at the dump's granularity: {err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
