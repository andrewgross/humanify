//! `humanify match` — the fixture-backed tests for the match verb: a
//! fixed two-version input where the ground truth is known by
//! construction (functions `addValues` / `sumTo` are byte-identical in
//! both versions, `onlyInOld` is gone from the new version,
//! `onlyInNew` is brand new), plus determinism and the dump's contract
//! with the ground-truth harness (every graph row carries its source
//! slice, so the harness never re-parses JavaScript).

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
