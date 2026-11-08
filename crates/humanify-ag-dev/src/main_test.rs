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
    let Some(v) = VERBS.iter().find(|v| v.lane.is_some()) else {
        return; // every verb is ported
    };
    let mut a: Vec<String> = v.words.iter().map(|w| w.to_string()).collect();
    a.push("x".into());
    let (code, out, err) = dispatch(&a);
    assert_ne!(code, 0);
    assert!(out.is_empty());
    assert!(
        err.contains(&format!("not yet ported (lane {})", v.lane.unwrap()))
            && err.contains(v.today),
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
    let (code, _, err) = dispatch(&args(&["build-bin", "--pairs", "x"]));
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

/// No host-wide lock (exp205): the coordinating agent decides what runs
/// together, from each verb's declared footprint. Every verb that runs the
/// pipeline, an LLM, a build or an index/corpus build declares one in full
/// (memory, LLM, time, what it writes), and `cargo dev` prints it.
#[test]
fn every_heavy_verb_declares_its_footprint() {
    let heavy = [
        "check",
        "eval score",
        "eval neutrality",
        "eval walk",
        "eval vendor-precision",
        "eval cjs-placement",
        "run stub",
        "library score",
        "library npm",
        "library corpus",
        "index build",
        "boot-gate",
        "build-bin",
        "e2e",
        "ask-trace",
        "match-truth",
    ];
    for v in VERBS {
        let n = name(v);
        let f = &v.footprint;
        assert_eq!(
            *f != LIGHT,
            heavy.contains(&n.as_str()),
            "{n}: footprint = {f:?}"
        );
        for (field, value) in [
            ("memory", f.memory),
            ("llm", f.llm),
            ("time", f.time),
            ("writes", f.writes),
        ] {
            assert!(!value.trim().is_empty(), "{n}: footprint {field} is empty");
        }
    }
    let t = table();
    assert!(t.contains("footprint:"), "the table prints each footprint");
    assert!(
        t.contains("17.4 GB"),
        "eval score's footprint carries the measured peak"
    );
}

/// The host-wide heavy lock and its held-by-the-caller escape hatch (env var) are gone:
/// nothing in the harness names either any more (a stale one in a
/// caller's environment silently matters nowhere).
#[test]
fn the_host_wide_lock_is_gone() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let retired = [
        ["/work/", "heavy.lock"].concat(),
        ["HELD", "_LOCK"].concat(),
        ["/work/", "index-build.lock"].concat(),
    ];
    let mut stack = vec![src];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                for r in &retired {
                    assert!(
                        !text.contains(r.as_str()),
                        "{} still names {r}",
                        p.display()
                    );
                }
            }
        }
    }
}
