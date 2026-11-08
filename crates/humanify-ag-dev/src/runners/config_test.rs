//! The runners' settings, resolved once from the parsed command line and
//! pairs.json (the cases eval-score-args.test.ts held for eval.ts): the
//! naming mode and its index, the neutrality defaults the workdir carries,
//! the refusals that happen before anything builds, and the label guard.

use std::path::{Path, PathBuf};

use super::Ctx;
use super::score::{config_from as score_config, naming_mode_of};
use crate::committed::eval::Pairs;
use crate::settings::parse;

fn pairs() -> Pairs {
    crate::committed::eval::pairs(&crate::paths::repo_root())
        .unwrap()
        .value
}

fn a(x: &[&str]) -> Vec<String> {
    x.iter().map(|s| s.to_string()).collect()
}

fn ctx() -> Ctx {
    let mut c = Ctx::new(
        crate::paths::repo_root(),
        Default::default(),
        PathBuf::from("/cwd"),
    );
    c.echo = false;
    c
}

const SCORE: &[&str] = &["run", "eval", "llm", "boot"];
const NEUT: &[&str] = &[
    "run.workdir",
    "run.pipeline_set",
    "neutrality",
    "eval.inputs_base",
    "eval.library_index",
    "llm.endpoint",
];

#[test]
fn names_mode_reads_the_eval_index_unless_library_index_names_another() {
    let p = pairs();
    let (mode, ix) = naming_mode_of(&[], None, &p.library_index).unwrap();
    assert_eq!(
        (mode.as_str(), ix.as_deref()),
        ("names", Some(p.library_index.as_str()))
    );
    let (_, ix) = naming_mode_of(&[], Some(Path::new("/ix")), &p.library_index).unwrap();
    assert_eq!(ix.as_deref(), Some("/ix"));
    // a relative --library-index is read against the working directory
    let parsed = parse(
        "eval score",
        SCORE,
        &[],
        &a(&["l", "--library-index", "rel/ix"]),
    )
    .unwrap();
    assert_eq!(
        score_config(&parsed, &p, &ctx())
            .unwrap()
            .library_index
            .as_deref(),
        Some("/cwd/rel/ix")
    );
}

#[test]
fn off_mode_hands_no_index_and_report_mode_is_the_off_regime_with_an_index() {
    let p = pairs();
    let off = a(&["library.matching=off"]);
    assert_eq!(
        naming_mode_of(&off, None, &p.library_index).unwrap(),
        ("off".into(), None)
    );
    assert!(
        naming_mode_of(&off, Some(Path::new("/ix")), &p.library_index)
            .unwrap_err()
            .contains("silently ignored")
    );
    let report = a(&["library.matching=report"]);
    let (mode, ix) = naming_mode_of(&report, None, &p.library_index).unwrap();
    assert_eq!(mode, "off");
    assert!(ix.is_some());
    let e = naming_mode_of(&a(&["library.index=/x"]), None, &p.library_index).unwrap_err();
    assert!(e.contains("--library-index"), "{e}");
}

#[test]
fn every_score_default_is_resolved_once() {
    let p = pairs();
    let parsed = parse("eval score", SCORE, &[], &a(&["lbl"])).unwrap();
    let c = score_config(&parsed, &p, &ctx()).unwrap();
    assert_eq!(c.base_mode, "scratch");
    assert_eq!(c.workdir, PathBuf::from(super::score::DEFAULT_WORKDIR));
    assert_eq!(c.inputs_base, p.inputs_base);
    assert_eq!(c.priors_base, p.priors_base);
    assert!(c.boot_prompt && c.self_hop);
    assert_eq!(c.llm_cache, None, "no cache by default (rule 10)");
    assert_eq!(c.pairs, None);
    assert!(c.bin.ends_with("target/release/humanify-ag"));
    let parsed = parse(
        "eval score",
        SCORE,
        &[],
        &a(&[
            "lbl",
            "--base-mode",
            "archive",
            "--no-self-hop",
            "--pairs",
            "85->86,215->216",
            "--pipeline-set",
            "naming.batch_size=10",
            "--pipeline-set",
            "naming.schedule=sequential",
        ]),
    )
    .unwrap();
    let c = score_config(&parsed, &p, &ctx()).unwrap();
    assert_eq!(c.base_mode, "archive");
    assert!(!c.self_hop);
    assert_eq!(c.pairs, Some(a(&["85->86", "215->216"])));
    assert_eq!(
        c.pipeline_sets,
        a(&["naming.batch_size=10", "naming.schedule=sequential"]),
        "order kept"
    );
}

#[test]
fn score_refuses_a_harness_key_a_missing_label_and_two_labels() {
    let p = pairs();
    for args in [
        &["l", "--pipeline-set", "split.enabled=false"][..],
        &["l", "--pipeline-set", "diagnostics.stats_json=x"],
    ] {
        let parsed = parse("eval score", SCORE, &[], &a(args)).unwrap();
        assert!(score_config(&parsed, &p, &ctx()).is_err(), "{args:?}");
    }
    for args in [&[][..], &["a", "b"]] {
        let parsed = parse("eval score", SCORE, &[], &a(args)).unwrap();
        assert!(
            score_config(&parsed, &p, &ctx())
                .unwrap_err()
                .contains("usage")
        );
    }
}

#[test]
fn neutrality_defaults_ride_on_the_workdir_and_explicit_paths_survive_it() {
    let p = pairs();
    let parsed = parse("eval neutrality", NEUT, &[], &a(&["main"])).unwrap();
    let c = super::neutrality::config_from(&parsed, &p).unwrap();
    assert_eq!((c.from.as_str(), c.to.as_str()), ("2.1.85", "2.1.86"));
    assert_eq!(c.cache, PathBuf::from("/work/neutrality-cache"));
    assert_eq!(c.priors, PathBuf::from("/work/exp050-cold"));
    let parsed = parse(
        "eval neutrality",
        NEUT,
        &[],
        &a(&["main", "2.1.215:2.1.216", "--workdir", "/w"]),
    )
    .unwrap();
    let c = super::neutrality::config_from(&parsed, &p).unwrap();
    assert_eq!(
        c.cache,
        PathBuf::from("/w/neutrality-cache"),
        "the documented trap, kept visible"
    );
    let parsed = parse(
        "eval neutrality",
        NEUT,
        &[],
        &a(&["main", "--workdir", "/w", "--cache", "/c", "--priors", "/p"]),
    )
    .unwrap();
    let c = super::neutrality::config_from(&parsed, &p).unwrap();
    assert_eq!(
        (c.cache, c.priors),
        (PathBuf::from("/c"), PathBuf::from("/p"))
    );
}

#[test]
fn neutrality_refuses_a_missing_baseline_a_malformed_pair_and_an_extra_positional() {
    let p = pairs();
    for args in [
        &[][..],
        &["main", "2.1.85-2.1.86"],
        &["main", "a:b:c"],
        &["main", "2.1.85:2.1.86", "extra"],
    ] {
        let parsed = parse("eval neutrality", NEUT, &[], &a(args)).unwrap();
        assert!(
            super::neutrality::config_from(&parsed, &p).is_err(),
            "{args:?}"
        );
    }
    let parsed = parse(
        "eval neutrality",
        NEUT,
        &[],
        &a(&["main", "--pipeline-set", "llm.cache_dir=/x"]),
    )
    .unwrap();
    assert!(super::neutrality::config_from(&parsed, &p).is_err());
}
