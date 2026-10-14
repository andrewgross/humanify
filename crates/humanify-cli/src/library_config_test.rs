//! The ONE library decision (config sweep step 6;
//! docs/library-matching-config.md §2): `--library-matching off|report|
//! names` + `--library-index`, `[library] unnamed`, and the lookup's and
//! discovery's tunables as `[library]` keys. Every refusal before any work.

use crate::config::{API_KEY_VAR, Config, ConfigFile, LibraryMatching, Unnamed};
use crate::surface::{PipelineParse, parse_pipeline};

fn build(argv: &[&str], toml: &str) -> Result<Config, Vec<String>> {
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    let PipelineParse::Run { input, opts } = parse_pipeline(&argv) else {
        panic!("{argv:?} did not parse");
    };
    let f = ConfigFile::check("cfg.toml", toml).expect("valid");
    Config::build(&input, &opts, Some(&f), |n| {
        (n == API_KEY_VAR).then(|| "k".to_string())
    })
}

fn refused(argv: &[&str], toml: &str, want: &str) {
    let errs = build(argv, toml).expect_err("must be refused");
    assert!(
        errs.iter().any(|e| e.contains(want)),
        "want {want:?}: {errs:?}"
    );
}

/// A scratch index directory with a manifest (stamps from `edit`).
struct Index(std::path::PathBuf);

impl Index {
    fn new(tag: &str, edit: impl Fn(&mut serde_json::Value)) -> Index {
        use humanify_core::libindex::format::{INDEX_FORMAT, fingerprint_versions};
        let dir = std::env::temp_dir().join(format!("lib-cfg-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut m = serde_json::json!({
            "format": INDEX_FORMAT, "asOf": "2026-10-11",
            "fingerprints": fingerprint_versions(),
            "registry": {"earliestFetch": null, "latestFetch": null, "packuments": {}},
            "slices": [], "packages": [], "refused": [], "shards": {}, "tables": {}, "data": {}
        });
        edit(&mut m);
        std::fs::write(dir.join("manifest.json"), m.to_string()).unwrap();
        Index(dir)
    }
    fn path(&self) -> &str {
        self.0.to_str().unwrap()
    }
}

impl Drop for Index {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_default_is_off_and_leave() {
    let c = build(&["in.js"], "").unwrap();
    assert_eq!(c.library.matching, LibraryMatching::Off);
    assert_eq!(c.library.unnamed, Unnamed::Leave);
    assert!(c.library.skips_libraries());
    assert_eq!(c.library_index_problem(), None, "off never opens an index");
}

#[test]
fn unnamed_model_is_the_old_no_skip_libraries() {
    let c = build(&["in.js"], "[library]\nunnamed = \"model\"\n").unwrap();
    assert!(!c.library.skips_libraries());
    let c = build(&["in.js"], "[library]\nunnamed = \"leave\"\n").unwrap();
    assert!(c.library.skips_libraries());
    assert!(
        ConfigFile::check("cfg.toml", "[library]\nunnamed = \"skip\"\n").is_err(),
        "only leave | model"
    );
}

#[test]
fn names_is_refused_until_an_eval_passes_it() {
    refused(
        &[
            "in.js",
            "--library-matching",
            "names",
            "--library-index",
            "ix",
        ],
        "",
        "--library-matching names is refused until an eval passes it",
    );
}

#[test]
fn report_without_an_index_is_refused() {
    refused(
        &["in.js", "--library-matching", "report"],
        "",
        "--library-matching report needs --library-index",
    );
}

#[test]
fn an_index_with_matching_off_is_refused_as_a_conflict() {
    refused(
        &["in.js", "--library-index", "ix"],
        "",
        "--library-index given with --library-matching off",
    );
    refused(
        &[
            "in.js",
            "--library-index",
            "ix",
            "--library-matching",
            "off",
        ],
        "",
        "the index would be silently ignored",
    );
}

#[test]
fn an_unknown_mode_is_refused() {
    refused(
        &["in.js", "--library-matching", "on"],
        "",
        "--library-matching must be one of: off, report, names (got \"on\")",
    );
}

#[test]
fn the_flag_and_the_file_must_agree() {
    refused(
        &[
            "in.js",
            "--library-matching",
            "report",
            "--library-index",
            "a",
        ],
        "[library]\nmatching = \"off\"\n",
        "config conflict: library.matching = \"report\" (--library-matching) vs \"off\"",
    );
}

#[test]
fn a_missing_unreadable_or_stale_index_is_refused_before_work() {
    let report = |ix: &str| {
        build(
            &[
                "in.js",
                "--library-matching",
                "report",
                "--library-index",
                ix,
            ],
            "",
        )
        .expect("the parse accepts it")
        .library_index_problem()
    };
    let missing = report("/nonexistent/index").expect("refused");
    assert!(missing.contains("no such directory"), "{missing}");
    let unreadable = Index::new("unreadable", |_| {});
    std::fs::write(unreadable.0.join("manifest.json"), "{not json").unwrap();
    let e = report(unreadable.path()).expect("refused");
    assert!(e.contains("is not an index manifest"), "{e}");
    let fine = Index::new("fine", |_| {});
    assert_eq!(report(fine.path()), None);
    for (field, stale) in [
        ("format", "humanify-library-index/0"),
        ("module", "vendor-content-key/1"),
        ("units", "units/1"),
        ("literals", "literals/0"),
    ] {
        let ix = Index::new(field, |m| {
            if field == "format" {
                m["format"] = stale.into();
            } else {
                m["fingerprints"][field] = stale.into();
            }
        });
        let e = report(ix.path()).expect("stale is refused");
        assert!(
            e.contains(stale) && e.contains("this binary reads"),
            "{field}: {e}"
        );
        assert!(e.contains("rebuild"), "{e}");
    }
}

#[test]
fn an_index_id_resolves_under_the_store_or_is_refused() {
    let store = Index::new("store", |_| {});
    let id = "ab".repeat(32);
    let inner = store.0.join(&id);
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::copy(store.0.join("manifest.json"), inner.join("manifest.json")).unwrap();
    let argv = [
        "in.js",
        "--library-matching",
        "report",
        "--library-index",
        &id,
    ];
    let no_store = build(&argv, "")
        .unwrap()
        .library_index_problem()
        .expect("refused");
    assert!(no_store.contains("needs library.index_store"), "{no_store}");
    let toml = format!("[library]\nindex_store = {:?}\n", store.path());
    assert_eq!(build(&argv, &toml).unwrap().library_index_problem(), None);
}

#[test]
fn the_lab_knobs_are_keys_with_todays_values() {
    let c = build(&["in.js"], "[library.lookup]\nagreement = false\nsupport = 0\n[library.lookup.targeted]\nmin_nodes = 25\n[library.discovery]\nveto_support = \"all-rules\"\n").unwrap();
    let r = c.library.rules();
    assert!(!r.agreement);
    assert_eq!(r.support, 0);
    assert_eq!(c.library.lookup.targeted_min_nodes, 25);
    assert_eq!(
        c.library.discovery_options().veto_support,
        humanify_core::libdetect::discovery::VetoSupport::AllRules
    );
    let spec = build(&["in.js"], "[library.lookup]\nspecificity = false\n").unwrap();
    assert!(spec.library.rules().specificity.is_none());
}

#[test]
fn the_exp102_precision_guards_are_keys_default_off() {
    let off = build(&["in.js"], "").unwrap();
    assert!(!off.library.discovery_options().evidence_conflict);
    assert!(!off.library.discovery_options().literal_conflict);
    assert_eq!(off.library.discovery_options().contested_cover, 0.0);
    let on = build(
        &["in.js"],
        "[library.discovery]\nevidence_conflict = true\nliteral_conflict = true\ncontested_cover = 0.05\n",
    )
    .unwrap();
    assert!(on.library.discovery_options().evidence_conflict);
    assert!(on.library.discovery_options().literal_conflict);
    assert_eq!(on.library.discovery_options().contested_cover, 0.05);
    let err = ConfigFile::check("cfg.toml", "[library.discovery]\ncontested_cover = 1.5\n")
        .expect_err("a share over 1 is refused");
    assert!(err.iter().any(|e| e.contains("contested_cover")), "{err:?}");
}
