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
        use humanify_ag_core::libindex::format::{INDEX_FORMAT, fingerprint_versions};
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

/// A scratch library-index STORE: empty, or holding one id-named index.
struct Store(std::path::PathBuf);

impl Store {
    fn new(tag: &str, with_index: bool) -> Store {
        let dir = std::env::temp_dir().join(format!("lib-store-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        if with_index {
            let ix = dir.join("cd".repeat(32));
            std::fs::create_dir_all(&ix).unwrap();
            std::fs::write(ix.join("manifest.json"), "{}").unwrap();
        }
        Store(dir)
    }
    fn toml(&self) -> String {
        format!(
            "[library]\nindex_store_dir = {:?}\n",
            self.0.display().to_string()
        )
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_default_is_names_searching_the_working_directory_store_and_leave() {
    // exp171 (Andrew, 2026-10-31): names mode is the default. exp174: with
    // no index passed, the run searches its store, which lives under the
    // WORKING directory, and never builds one unless asked.
    let c = build(&["in.js"], "").unwrap();
    assert_eq!(c.library.matching, LibraryMatching::Names);
    assert!(!c.library.index_from_bundle, "the run-time build is opt-in");
    assert!(c.library.runs_index_machinery());
    assert_eq!(
        c.library.index_store_root(),
        std::env::current_dir()
            .unwrap()
            .join(".humanify/library-index")
    );
    assert_eq!(c.library.unnamed, Unnamed::Leave);
    assert!(c.library.skips_libraries());
}

#[test]
fn an_absolute_store_dir_is_used_as_given() {
    let c = build(&["in.js", "--library-index-store", "/abs/store"], "").unwrap();
    assert_eq!(
        c.library.index_store_root(),
        std::path::PathBuf::from("/abs/store")
    );
}

#[test]
fn names_with_an_empty_store_and_no_index_is_no_problem_before_work() {
    // exp175 (Andrew, 2026-11-01): which index a run needs is a question of
    // the bundle's KIND, and the kind is detection's answer — so the empty
    // store is not refused at the parse point. The run decides after
    // detection: no index for its kind skips library name matching with one
    // line (or builds one, with --library-index-from-bundle).
    let empty = Store::new("empty", false);
    for argv in [
        &["in.js"][..],
        &["in.js", "--library-matching", "report"][..],
    ] {
        let c = build(argv, &empty.toml()).unwrap();
        assert_eq!(c.library_index_problem(), None, "{argv:?}");
    }
}

#[test]
fn a_store_holding_an_index_is_no_problem() {
    let full = Store::new("full", true);
    let c = build(&["in.js"], &full.toml()).unwrap();
    assert_eq!(c.library_index_problem(), None);
}

#[test]
fn the_from_bundle_flag_opts_into_the_run_time_build() {
    // The flag (or its key) is the ONLY way to the run-time build; the
    // build answers at run time, so an empty store is no problem with it.
    let empty = Store::new("from-bundle", false);
    let c = build(&["in.js", "--library-index-from-bundle"], &empty.toml()).unwrap();
    assert!(c.library.index_from_bundle);
    assert!(c.library.runs_index_machinery());
    assert_eq!(c.library_index_problem(), None);
    let c = build(&["in.js"], "[library]\nindex_from_bundle = true\n").unwrap();
    assert!(c.library.index_from_bundle);
}

#[test]
fn the_from_bundle_flag_is_refused_where_it_would_be_ignored() {
    refused(
        &[
            "in.js",
            "--library-index-from-bundle",
            "--library-matching",
            "off",
        ],
        "",
        "--library-index-from-bundle given with --library-matching off",
    );
    refused(
        &[
            "in.js",
            "--library-index-from-bundle",
            "--library-index",
            "ix",
        ],
        "",
        "--library-index-from-bundle given with --library-index",
    );
}

#[test]
fn the_retired_index_build_key_is_refused() {
    // library.index_build (off | auto) is replaced by the flag; no alias.
    assert!(
        ConfigFile::check("cfg.toml", "[library]\nindex_build = \"auto\"\n").is_err(),
        "an unknown key"
    );
}

#[test]
fn library_matching_off_is_the_opt_out_and_never_runs_the_machinery() {
    // The opt-out is the existing flag: it never searches the store, so
    // an empty one is no problem.
    let c = build(&["in.js", "--library-matching", "off"], "").unwrap();
    assert_eq!(c.library.matching, LibraryMatching::Off);
    assert!(!c.library.runs_index_machinery());
    assert_eq!(c.library_index_problem(), None, "off never opens an index");
    let c = build(&["in.js"], "[library]\nmatching = \"off\"\n").unwrap();
    assert_eq!(c.library.matching, LibraryMatching::Off);
    assert!(!c.library.runs_index_machinery());
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
fn names_needs_no_unlock() {
    let ix = Index::new("names", |_| {});
    let c = build(
        &[
            "in.js",
            "--library-matching",
            "names",
            "--library-index",
            ix.path(),
        ],
        "",
    )
    .expect("names is a mode like any other since exp171");
    assert_eq!(c.library.matching, LibraryMatching::Names);
    assert!(!c.library.runs_index_machinery(), "an explicit index wins");
    assert_eq!(c.library_index_problem(), None);
}

#[test]
fn the_retired_unlock_key_is_refused() {
    // library.names_unlocked lifted a refusal that no longer exists: the
    // key is deleted, and a file that still sets it fails loud.
    assert!(
        ConfigFile::check("cfg.toml", "[library]\nnames_unlocked = true\n").is_err(),
        "an unknown key"
    );
}

#[test]
fn an_index_with_matching_off_is_refused_as_a_conflict() {
    // With the default (names) an index is simply the index to use.
    let c = build(&["in.js", "--library-index", "ix"], "").unwrap();
    assert_eq!(c.library.matching, LibraryMatching::Names);
    assert!(!c.library.runs_index_machinery(), "an explicit index wins");
    refused(
        &[
            "in.js",
            "--library-index",
            "ix",
            "--library-matching",
            "off",
        ],
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
    let c = build(&["in.js"], "[library.lookup]\nagreement = true\nsupport = 0\n[library.lookup.targeted]\nmin_nodes = 25\n[library.discovery]\nveto_support = \"all-rules\"\n").unwrap();
    let r = c.library.rules();
    assert!(r.agreement);
    assert_eq!(r.support, 0);
    assert_eq!(c.library.lookup.targeted_min_nodes, 25);
    assert_eq!(
        c.library.discovery_options().veto_support,
        humanify_ag_core::libdetect::discovery::VetoSupport::AllRules
    );
    let spec = build(&["in.js"], "[library.lookup]\nspecificity = false\n").unwrap();
    assert!(spec.library.rules().specificity.is_none());
}

#[test]
fn the_exp102_precision_guards_are_keys() {
    let off = build(
        &["in.js"],
        "[library.discovery]\nevidence_conflict = false\nliteral_conflict = false\ncontested_cover = 0.0\n",
    )
    .unwrap();
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

/// exp109: the shipped DEFAULT is the campaign's measured recommendation
/// (experiments/106-matcher-hygiene, experiments/109-default-config): the
/// structural rule, exp102's three guards, and exp106's hygiene keys. Names
/// (the default since exp171) and report runs read these.
#[test]
fn the_default_library_tunables_are_the_measured_recommendation() {
    use humanify_ag_core::libdetect::discovery::BannerNames;
    let c = build(&["in.js"], "").unwrap();
    assert_eq!(c.library.matching, LibraryMatching::Names);
    let r = c.library.rules();
    assert!(!r.agreement, "the structural rule");
    assert!(r.literal_margin_by_family);
    let o = c.library.discovery_options();
    assert!(o.evidence_conflict);
    assert!(o.literal_conflict);
    assert_eq!(o.contested_cover, 0.05);
    assert!(o.specific_rivals);
    assert_eq!(o.banner_names, BannerNames::Known);
    assert!(o.derived_families);
    assert_eq!(o.payload_min_bytes, 4096);
    assert_eq!(o.helper_families, 0, "helper_families stays off (exp106)");
}

/// exp192: the isolation veto counts every rule's claims (MODULE fills
/// included) as a seed's support by default — exp189 measured it on the cb64
/// index: recall up on bun-dev and bun-held, precision held or up, app 0, cc
/// unchanged. "without-module" stays a choice.
#[test]
fn the_default_veto_support_is_all_rules() {
    use humanify_ag_core::libdetect::discovery::VetoSupport;
    let o = build(&["in.js"], "").unwrap().library.discovery_options();
    assert_eq!(o.veto_support, VetoSupport::AllRules);
    let old = build(
        &["in.js"],
        "[library.discovery]\nveto_support = \"without-module\"\n",
    )
    .unwrap()
    .library
    .discovery_options();
    assert_eq!(old.veto_support, VetoSupport::WithoutModule);
}

/// exp117: exp114's context-presence pair with its fence, and exp112's
/// declared cap, measured together on every corpus on k2-all
/// (experiments/117-default-flip). shadow_ties joined them 2026-10-15 on
/// Andrew's call: it removes fork claims (@redocly/ajv over ajv) on every
/// webpack/rollup set for 1,035 wrong bytes on test2-esbuild.
#[test]
fn the_default_presence_keys_and_declared_cap_are_on() {
    let o = build(&["in.js"], "").unwrap().library.discovery_options();
    assert_eq!(o.context_presence, 1.5);
    assert_eq!(o.context_presence_slack, 16384);
    assert!(o.presence_fence);
    assert_eq!(o.declared_max_bytes, 65536);
    assert!(o.shadow_ties, "shadow_ties on since 2026-10-15");
}

#[test]
fn the_exp106_hygiene_keys_turn_off() {
    use humanify_ag_core::libdetect::discovery::BannerNames;
    let off = build(
        &["in.js"],
        "[library.lookup]\nliteral_margin_by_family = false\n[library.discovery]\nbanner_names = \"any\"\npayload_min_bytes = 0\nspecific_rivals = false\nderived_families = false\n",
    )
    .unwrap();
    let o = off.library.discovery_options();
    assert_eq!(o.banner_names, BannerNames::Any);
    assert_eq!(o.helper_families, 0);
    assert_eq!(o.payload_min_bytes, 0);
    assert!(!o.specific_rivals);
    assert!(!o.derived_families);
    assert!(!off.library.rules().literal_margin_by_family);
    let on = build(
        &["in.js"],
        "[library.discovery]\nbanner_names = \"known\"\nhelper_families = 4\npayload_min_bytes = 4096\nspecific_rivals = true\nderived_families = true\n",
    )
    .unwrap();
    let o = on.library.discovery_options();
    assert_eq!(o.banner_names, BannerNames::Known);
    assert_eq!(o.helper_families, 4);
    assert_eq!(o.payload_min_bytes, 4096);
    assert!(o.specific_rivals);
    assert!(o.derived_families);
    let by = build(
        &["in.js"],
        "[library.lookup]\nliteral_margin_by_family = true\n",
    )
    .unwrap();
    assert!(by.library.rules().literal_margin_by_family);
    let err = ConfigFile::check(
        "cfg.toml",
        "[library.discovery]\nbanner_names = \"maybe\"\n",
    )
    .expect_err("an unknown choice is refused");
    assert!(err.iter().any(|e| e.contains("banner_names")), "{err:?}");
}

/// exp114's presence keys: context_presence + presence_fence on by default
/// since exp117, shadow_ties since 2026-10-15; each is set from the file.
#[test]
fn the_exp114_presence_keys_set_from_the_file() {
    let off = build(
        &["in.js"],
        "[library.discovery]\ncontext_presence = 0.0\ncontext_presence_slack = 0\npresence_fence = false\n",
    )
    .unwrap();
    let o = off.library.discovery_options();
    assert_eq!(o.context_presence, 0.0);
    assert_eq!(o.context_presence_slack, 0);
    assert!(!o.presence_fence);
    let on = build(&["in.js"], "[library.discovery]\nshadow_ties = false\n").unwrap();
    assert!(!on.library.discovery_options().shadow_ties);
}

/// exp121's copy presence: on by default since 2026-10-16 at its measured
/// "v2" setting (clean on all 19 sets on k2-all), turned off from the file.
#[test]
fn the_exp121_copy_presence_keys_default_to_v2_and_set_from_the_file() {
    let d = build(&["in.js"], "").unwrap().library.discovery_options();
    assert_eq!(d.copy_presence, 0.1);
    assert_eq!(d.copy_presence_factor, 4);
    assert!(d.copy_presence_origin);
    assert_eq!(d.copy_presence_own, 0.1);
    let off = build(
        &["in.js"],
        "[library.discovery]\ncopy_presence = 0.0\ncopy_presence_factor = 0\ncopy_presence_origin = false\ncopy_presence_own = 0.0\n",
    )
    .unwrap();
    let o = off.library.discovery_options();
    assert_eq!(o.copy_presence, 0.0);
    assert_eq!(o.copy_presence_factor, 0);
    assert!(!o.copy_presence_origin);
    assert_eq!(o.copy_presence_own, 0.0);
}

/// exp112's declared cap: 65536 by default since exp117, 0 turns it off.
#[test]
fn the_exp112_declared_cap_turns_off() {
    let off = build(&["in.js"], "[library.discovery]\ndeclared_max_bytes = 0\n").unwrap();
    assert_eq!(off.library.discovery_options().declared_max_bytes, 0);
}

/// exp127: `library.lookup.terser_slices` — off by default; union and
/// fallback read the index's terser-built slices beside the chosen one.
#[test]
fn the_terser_slices_key_is_off_by_default_and_takes_union_or_fallback() {
    use humanify_ag_core::libindex::recipe::TerserSlices;
    let d = build(&["in.js"], "").unwrap();
    assert_eq!(d.library.lookup.terser_slices, TerserSlices::Off);
    for (v, want) in [
        ("off", TerserSlices::Off),
        ("union", TerserSlices::Union),
        ("fallback", TerserSlices::Fallback),
    ] {
        let c = build(
            &["in.js"],
            &format!("[library.lookup]\nterser_slices = \"{v}\"\n"),
        )
        .unwrap();
        assert_eq!(c.library.lookup.terser_slices, want, "{v}");
    }
    let auto = build(
        &["in.js"],
        "[library.lookup]\nterser_slices = \"auto\"\nterser_share = 0.25\n",
    )
    .unwrap();
    assert_eq!(
        auto.library.lookup.terser_slices,
        TerserSlices::Auto { share: 0.25 }
    );
    let err = ConfigFile::check("cfg.toml", "[library.lookup]\nterser_slices = \"all\"\n")
        .expect_err("an unknown choice is refused");
    assert!(err.iter().any(|e| e.contains("terser_slices")), "{err:?}");
}

/// exp129: `helper_prints` (off | seed | credit): a unit whose whole print
/// a helper source's own build holds is no other package's index seed
/// (seed), or the helper package's (credit).
#[test]
fn the_helper_prints_key_sets_from_the_file() {
    use humanify_ag_core::libdetect::discovery::HelperPrints;
    for (text, want) in [
        ("off", HelperPrints::Off),
        ("seed", HelperPrints::Seed),
        ("credit", HelperPrints::Credit),
    ] {
        let o = build(
            &["in.js"],
            &format!("[library.discovery]\nhelper_prints = \"{text}\"\n"),
        )
        .unwrap()
        .library
        .discovery_options();
        assert_eq!(o.helper_prints, want);
    }
    let err = ConfigFile::check(
        "cfg.toml",
        "[library.discovery]\nhelper_prints = \"maybe\"\n",
    )
    .expect_err("an unknown choice is refused");
    assert!(err.iter().any(|e| e.contains("helper_prints")), "{err:?}");
}

/// exp138: `helper_prints = "seed"` is on by default since 2026-10-22
/// (precision at or above the previous default on all 21 sets, F0.5 at worst
/// -0.000006); it turns off from the file. `terser_slices` stays off: with
/// factory bodies it costs remin-terser 0.6 points of precision.
#[test]
fn the_exp138_helper_seed_defaults_on_and_turns_off() {
    use humanify_ag_core::libdetect::discovery::HelperPrints;
    use humanify_ag_core::libindex::recipe::TerserSlices;
    let d = build(&["in.js"], "").unwrap();
    assert_eq!(
        d.library.discovery_options().helper_prints,
        HelperPrints::Seed
    );
    assert_eq!(d.library.lookup.terser_slices, TerserSlices::Off);
    let off = build(&["in.js"], "[library.discovery]\nhelper_prints = \"off\"\n").unwrap();
    assert_eq!(
        off.library.discovery_options().helper_prints,
        HelperPrints::Off
    );
}

/// exp132: cutting bundled CommonJS modules into body statements, with its
/// three module-level precision keys, is on by default since 2026-10-21
/// (precision at or above the previous default on every set, F0.5 +29..62
/// over both scopes); each turns off from the file.
#[test]
fn the_exp132_factory_keys_default_on_and_turn_off() {
    let d = build(&["in.js"], "").unwrap().library.discovery_options();
    assert!(d.factory_bodies);
    assert!(d.factory_tie_fence);
    assert!(d.factory_banner_index);
    assert!(d.factory_whole_seed);
    let off = build(
        &["in.js"],
        "[library.discovery]\nfactory_bodies = false\nfactory_tie_fence = false\nfactory_banner_index = false\nfactory_whole_seed = false\n",
    )
    .unwrap();
    let o = off.library.discovery_options();
    assert!(
        !o.factory_bodies
            && !o.factory_tie_fence
            && !o.factory_banner_index
            && !o.factory_whole_seed
    );
}

/// exp176: the whole-seed rule's cover exception is one float key (a share
/// of the module's nodes), on by default at 0.95, read from the file.
#[test]
fn the_factory_whole_cover_key_reads_from_the_file() {
    // default since exp176: 0.95 of the module's nodes
    let d = build(&["in.js"], "").unwrap().library.discovery_options();
    assert_eq!(d.factory_whole_cover, 0.95);
    let set = build(
        &["in.js"],
        "[library.discovery]\nfactory_whole_cover = 0.75\n",
    )
    .unwrap();
    assert_eq!(set.library.discovery_options().factory_whole_cover, 0.75);
    assert!(
        ConfigFile::check(
            "cfg.toml",
            "[library.discovery]\nfactory_whole_cover = 1.5\n"
        )
        .is_err(),
        "a share over 1 is refused"
    );
}
