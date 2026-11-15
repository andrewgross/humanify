use super::*;

const SCORE: &[&str] = &["run", "eval", "llm", "boot"];
const NEUTRALITY: &[&str] = &[
    "run.workdir",
    "run.pipeline_set",
    "neutrality",
    "eval.inputs_base",
    "eval.library_index",
    "llm.endpoint",
];

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn refusals(accepted: &[&str], a: &[&str]) -> Vec<String> {
    parse("eval score", accepted, &[], &args(a)).expect_err("refused")
}

#[test]
fn every_key_is_a_flag_and_flags_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for k in HARNESS_KEYS {
        assert!(
            seen.insert(k.flag_name()),
            "two keys share {}",
            k.flag_name()
        );
        assert!(!k.doc.is_empty(), "{} has no doc", k.dotted());
        if (k.default)().is_none() {
            assert!(
                !k.unset.is_empty(),
                "{} has no default and no unset note",
                k.dotted()
            );
        }
    }
}

#[test]
fn positionals_flags_and_defaults_resolve() {
    let p = parse(
        "eval score",
        SCORE,
        &[],
        &args(&["mylabel", "--no-self-hop", "--pairs", "2.1.85->2.1.86"]),
    )
    .expect("parses");
    assert_eq!(p.positionals, vec!["mylabel"]);
    assert!(!p.settings.flag_on("eval.self_hop"));
    assert!(p.settings.flag_on("eval.boot_prompt"), "default on");
    // the work dir's default is per verb (eval score: /work/eval-work)
    assert_eq!(p.settings.text("run.workdir"), None);
    assert_eq!(
        p.settings.text("boot.model").as_deref(),
        Some("claude-opus-4-1")
    );
    assert_eq!(
        p.settings.list("eval.pairs"),
        Some(vec!["2.1.85->2.1.86".to_string()])
    );
}

#[test]
fn an_unknown_flag_fails_upfront_with_a_suggestion() {
    let e = refusals(SCORE, &["x", "--workdr", "/w"]);
    assert_eq!(e, vec!["unknown flag --workdr (did you mean --workdir?)"]);
}

#[test]
fn a_flag_of_another_verb_is_refused_by_name() {
    let e = refusals(NEUTRALITY, &["ref", "--pairs", "85->86"]);
    assert_eq!(
        e,
        vec!["--pairs is not a setting of `eval score` (it sets eval.pairs)"]
    );
}

#[test]
fn type_and_range_errors_are_collected_together() {
    let e = refusals(
        &["libscore", "llm"],
        &["x", "--rule", "fresh", "--endpoint", "localhost:8000"],
    );
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(
        e.iter()
            .any(|m| m.contains("--rule") && m.contains("agreement, structural"))
    );
    assert!(
        e.iter()
            .any(|m| m.contains("--endpoint") && m.contains("http"))
    );
}

#[test]
fn pipeline_sets_are_checked_against_the_pipelines_registry() {
    let ok = parse(
        "eval score",
        SCORE,
        &["library.index"],
        &args(&[
            "x",
            "--pipeline-set",
            "naming.schedule=sequential",
            "--pipeline-set",
            "library.matching=off",
        ]),
    )
    .expect("valid sets");
    assert_eq!(
        ok.pipeline_sets,
        vec![
            (
                "naming.schedule".to_string(),
                Val::Text("sequential".into())
            ),
            ("library.matching".to_string(), Val::Text("off".into())),
        ],
        "typed, order kept"
    );
    let e = parse(
        "eval score",
        SCORE,
        &["library.index"],
        &args(&[
            "x",
            "--pipeline-set",
            "naming.schedul=sequential",
            "--pipeline-set",
            "library.matching=maybe",
            "--pipeline-set",
            "library.index=/x",
            "--pipeline-set",
            "perf.profile=true",
            "--pipeline-set",
            "noequals",
        ]),
    )
    .expect_err("all refused");
    assert_eq!(e.len(), 5, "{e:#?}");
    assert!(e[0].contains("did you mean naming.schedule"), "{}", e[0]);
    assert!(e[1].contains("off, report, names"), "{}", e[1]);
    assert!(e[2].contains("set by the harness"), "{}", e[2]);
    assert!(e[3].contains("retired"), "{}", e[3]);
    assert!(e[4].contains("section.key=value"), "{}", e[4]);
}

#[test]
fn heads_default_comes_from_the_linked_registry() {
    assert_eq!(
        pipeline_default("library.matching"),
        Some(Val::Text("names".into()))
    );
}

#[test]
fn the_run_config_records_every_accepted_setting_once() {
    let p = parse(
        "eval neutrality",
        NEUTRALITY,
        &[],
        &args(&["abc123", "--cache", "/c"]),
    )
    .unwrap();
    let text = crate::json::js_text(&run_config("eval neutrality", NEUTRALITY, &p), true);
    assert!(text.contains("\"neutrality.cache\": \"/c\""), "{text}");
    assert!(
        text.contains("\"neutrality.priors\": null"),
        "unset is null: {text}"
    );
    assert!(
        !text.contains("eval.pairs"),
        "not a neutrality setting: {text}"
    );
    assert!(
        text.contains("\"positionals\": [\n    \"abc123\"\n  ]"),
        "{text}"
    );
}

/// `--bin` and `--force-mixed` are settings parsed upfront (flags, never
/// env vars); a retired switch is an unknown flag, refused before anything
/// runs.
#[test]
fn the_bin_flag_family_is_flags_and_retired_switches_fail_upfront() {
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let p = parse(
        "eval score",
        &["run"],
        &[],
        &args(&["label", "--bin", "/x/humanify-ag", "--force-mixed"]),
    )
    .unwrap();
    assert_eq!(
        p.settings.text("run.bin").as_deref(),
        Some("/x/humanify-ag")
    );
    for (flag, why) in [
        ("--skip-preflight", "unknown flag --skip-preflight"),
        ("--warm-self-hop", "unknown flag --warm-self-hop"),
    ] {
        let refused = parse("eval score", &["run"], &[], &args(&["label", flag]))
            .expect_err(flag)
            .join("\n");
        assert!(refused.contains(why), "{flag}: {refused}");
    }
}
