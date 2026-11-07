use super::*;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn every_verb_declares_usage_proves_and_cannot_prove() {
    for v in VERBS {
        let n = name(v);
        assert!(
            v.usage.starts_with(&n),
            "{n}: the usage starts with the verb"
        );
        assert!(!v.proves.trim().is_empty(), "{n}: no proves");
        assert!(!v.cannot_prove.trim().is_empty(), "{n}: no cannotProve");
        if v.lane.is_some() {
            assert!(!v.today.is_empty(), "{n}: say what runs until it is ported");
        }
    }
}

/// The full subcommand map of docs/dev-harness.md §2.4 is in the table.
#[test]
fn the_table_holds_the_whole_subcommand_map() {
    let names: Vec<String> = VERBS.iter().map(name).collect();
    for want in [
        "check",
        "eval score",
        "eval rescore",
        "eval summarize",
        "eval leaderboard",
        "eval bands",
        "eval diff",
        "eval neutrality",
        "eval vendor-precision",
        "eval cjs-placement",
        "eval walk",
        "run stub",
        "library score",
        "library leaderboard",
        "library determinism",
        "library corpus",
        "library layers",
        "index build",
        "boot-gate",
        "build-bin",
        "e2e",
        "format-golden",
        "census clones",
        "census concurrency",
        "census slop",
        "ask-trace",
        "diff-asks",
        "match-truth",
        "env-check",
        "precommit",
        "stub-llm serve",
    ] {
        assert!(names.iter().any(|n| n == want), "missing verb {want}");
    }
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "a verb listed twice");
}

#[test]
fn every_declared_setting_is_a_harness_key_or_section() {
    for v in VERBS {
        for s in v.settings {
            let known = settings::HARNESS_KEYS
                .iter()
                .any(|k| k.dotted() == *s || k.section == *s);
            assert!(known, "{}: {s} is not a harness key or section", name(v));
        }
    }
}

#[test]
fn no_arguments_prints_the_table() {
    let (code, out, _) = dispatch(&[]);
    assert_eq!(code, 0);
    assert!(
        out.contains("eval neutrality") && out.contains("cannot prove:"),
        "{out}"
    );
}

#[test]
fn a_verb_not_yet_ported_says_so_and_fails() {
    let (code, out, err) = dispatch(&args(&["eval", "score", "x"]));
    assert_ne!(code, 0);
    assert!(out.is_empty());
    assert!(
        err.contains("not yet ported (lane L3)") && err.contains("npm run eval -- score"),
        "{err}"
    );
}

#[test]
fn an_unknown_verb_fails_with_the_table() {
    let (code, _, err) = dispatch(&args(&["eval", "scroe"]));
    assert_eq!(code, 2);
    assert!(err.starts_with("unknown verb: eval scroe"), "{err}");
}

#[test]
fn an_implemented_verb_refuses_an_unknown_flag_before_running() {
    let (code, _, err) = dispatch(&args(&["build-bin", "--workdr", "x"]));
    assert_eq!(code, 2);
    assert!(err.contains("did you mean --workdir"), "{err}");
    let (code, _, err) = dispatch(&args(&["build-bin", "--workdir", "x"]));
    assert_eq!(code, 2);
    assert!(err.contains("not a setting of `build-bin`"), "{err}");
}

#[test]
fn a_verbs_help_is_its_usage() {
    let (code, out, _) = dispatch(&args(&["library", "score", "--help"]));
    assert_eq!(code, 0);
    assert!(
        out.starts_with("usage: humanify-ag-dev library score <label>"),
        "{out}"
    );
}
