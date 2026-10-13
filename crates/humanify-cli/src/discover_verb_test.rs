//! `humanify discover` — the library-matching score's instrument, on a
//! committed Bun fixture looked up in an index built from that fixture's
//! own unit prints (so every statement the lookup can reach is "indexed"):
//! the rule flag reaches the lookup, the unit listing is parallel to the
//! graph, two runs are byte-identical, and an unreadable index fails loud.

use std::path::{Path, PathBuf};

use humanify_core::libindex::format::{Manifest, UnitRow, fingerprint_versions};
use serde_json::Value;

use crate::config::{ConfigFile, Library, library_from_file};
use crate::discover_verb::{DiscoverArgs, discover};

/// The `[library]` settings a config text gives (empty: the defaults).
fn library(toml: &str) -> Library {
    let f = ConfigFile::check("cfg.toml", toml).expect("a valid config");
    library_from_file(Some(&f)).expect("valid")
}

fn rule(name: &str) -> Library {
    match name {
        "agreement" => library(""),
        "structural" => library("[library.lookup]\nagreement = false\n"),
        other => panic!("{other}"),
    }
}

fn fixture() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/e2e/fixtures/bun-mixed/build/v1.0.0/build/index.js");
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// An index holding every unit print of `code`, owned by `fixture-lib`.
fn self_index(code: &str, name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("humanify-discover-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse_unambiguous(&allocator, code);
    let tc = humanify_core::toolchain::resolve_toolchain(
        &humanify_core::detect::detect_bundle(code),
        None,
        None,
    );
    let prints = humanify_core::libindex::units::bundle_unit_prints(
        code,
        &ingest,
        tc.layout.piece,
        tc.module_wrappers.piece,
        tc.interop.piece,
    );
    let rows: String = prints
        .iter()
        .map(|(_, p)| {
            serde_json::to_string(&UnitRow {
                hash: p.whole.hash.clone(),
                owner: "fixture-lib".into(),
                file: "index.js".into(),
                nodes: p.whole.nodes,
                content: p.whole.content,
                module_bytes: 100,
                versions: vec![("1.0.0".into(), "1.0.0".into())],
            })
            .unwrap()
                + "\n"
        })
        .collect();
    std::fs::write(dir.join("units.bun.jsonl"), rows).unwrap();
    std::fs::write(dir.join("literals.jsonl"), "").unwrap();
    let manifest = Manifest {
        format: "humanify-library-index/1".into(),
        as_of: "lab".into(),
        fingerprints: fingerprint_versions(),
        registry: Default::default(),
        slices: vec![],
        packages: vec![],
        refused: vec![],
        shards: Default::default(),
        tables: Default::default(),
        data: Default::default(),
    };
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();
    dir
}

fn index_claims(doc: &Value) -> usize {
    doc["libraryDiscovery"]["claims"]
        .as_array()
        .map(|cs| {
            cs.iter()
                .filter(|c| c["evidence"]["kind"] == "index")
                .count()
        })
        .unwrap_or(0)
}

#[test]
fn the_rule_reaches_the_lookup_and_the_units_are_listed() {
    let code = fixture();
    let ix = self_index(&code, "rule");
    let run = |name: &str, unit_prints: bool| {
        discover(
            &code,
            &DiscoverArgs {
                input: "fixture",
                library_index: ix.to_str(),
                library: &rule(name),
                bundler: None,
                unit_prints,
                work_dir: None,
            },
        )
        .expect("runs")
    };
    let structural = run("structural", true);
    assert_eq!(structural["toolchain"]["slice"], "bun");
    assert_eq!(
        structural["detected"]["bundler"], "bun",
        "the detector's verdict"
    );
    assert!(index_claims(&structural) > 0, "the fixture indexes itself");
    assert_eq!(
        index_claims(&run("agreement", false)),
        0,
        "no literal table: the agreement rule never agrees"
    );
    let d = &structural["libraryDiscovery"];
    assert_eq!(
        d["unitRows"].as_array().map(Vec::len),
        d["graph"]["units"].as_u64().map(|n| n as usize),
        "one row per graph unit"
    );
    assert_eq!(d["indexId"].as_str().map(str::len), Some(64));
    assert_eq!(structural, run("structural", true), "deterministic");
    std::fs::remove_dir_all(&ix).ok();
}

#[test]
fn a_bad_setting_or_an_unreadable_index_fails_the_verb() {
    let code = fixture();
    let agreement = rule("agreement");
    let args = |library_index| DiscoverArgs {
        input: "fixture",
        library_index,
        library: &agreement,
        bundler: None,
        unit_prints: false,
        work_dir: None,
    };
    assert!(discover(&code, &args(Some("/nonexistent/index"))).is_err());
    assert!(
        discover(&code, &args(None)).is_ok(),
        "no index: evidence seeds only"
    );
    // The lab knobs are config keys, refused like any other.
    let bad = ConfigFile::check("cfg.toml", "[library.lookup]\ncover = 1.5\n").expect_err("x");
    assert_eq!(
        bad,
        ["cfg.toml: library.lookup.cover = 1.5 is outside 0..1"]
    );
    assert!(crate::discover_verb::library_settings(Some("/nonexistent/c.toml")).is_err());
}

#[test]
fn the_default_settings_are_the_pipelines_compiled_in_rules() {
    use humanify_core::libdetect::discovery::{DiscoveryOptions, DiscoveryRequest};
    use humanify_core::libindex::lookup::{LibraryIndex, LookupRules};
    let ix = LibraryIndex::from_rows(vec![], vec![]);
    let l = library("");
    assert_eq!(
        format!("{:?}", l.rules()),
        format!("{:?}", LookupRules::default())
    );
    assert_eq!(
        format!("{:?}", l.discovery_options()),
        format!("{:?}", DiscoveryOptions::default())
    );
    let ours = l.discovery_request(Some(&ix)).lookup().expect("an index");
    let today = DiscoveryRequest {
        index: Some(&ix),
        ..DiscoveryRequest::default()
    }
    .lookup()
    .expect("an index");
    assert_eq!(format!("{:?}", ours.rules), format!("{:?}", today.rules));
    assert_eq!(
        format!("{:?}", ours.targeted),
        format!("{:?}", today.targeted)
    );
}

#[test]
fn the_bundler_override_resolves_the_slice_and_the_verdict_is_kept() {
    // The fixture without its `// @bun` pragma (Bun writes none for
    // --target node: the exp096 held-out apps) reads as Bun by its module
    // helper's shape (finding #96; before it, esbuild by helper names).
    // The pipeline's `--bundler` picks the toolchain against that verdict,
    // and the detector's own verdict is kept beside it.
    let full = fixture();
    let code = full.split_once('\n').expect("a pragma line").1;
    let run = |bundler| {
        discover(
            code,
            &DiscoverArgs {
                input: "plain",
                library_index: None,
                library: &rule("agreement"),
                bundler,
                unit_prints: false,
                work_dir: None,
            },
        )
    };
    let doc = run(Some("esbuild")).expect("runs with the override");
    assert_eq!(doc["detected"]["bundler"], "bun");
    assert_eq!(doc["toolchain"]["slice"], "esbuild");
    assert_eq!(doc["bundlerOverride"], "esbuild");
    assert!(
        run(Some("rollup-ish")).is_err(),
        "an unknown name fails loud"
    );
}

#[test]
fn a_bundle_the_adapter_floors_reports_that_no_discovery_ran() {
    // No module the unpacker recognises: the one-file floor, where the
    // pipeline runs no discovery either. Said, never an error or a zero.
    let doc = discover(
        "var a = function (e) { return e + 1; };\nvar b = a(2);\n",
        &DiscoverArgs {
            input: "plain",
            library_index: None,
            library: &rule("agreement"),
            bundler: Some("bun"),
            unit_prints: true,
            work_dir: None,
        },
    )
    .expect("runs");
    assert!(doc["libraryDiscovery"].is_null());
    assert!(
        doc["noDiscovery"]
            .as_str()
            .is_some_and(|r| r.contains("no discovery ran")),
        "{doc}"
    );
}
