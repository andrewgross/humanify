//! The pipeline program's flag surface (clap since config sweep step 3,
//! 2026-10-13; the commander-13 port it replaced is gone):
//!
//! 1. the rendered HELP text against the committed goldens
//!    (test/golden/help/humanify-ag.txt for `--help`, humanify-ag-short.txt for
//!    `-h`; regenerate with `humanify-ag --help > test/golden/help/humanify-ag.txt`
//!    and `humanify-ag -h > test/golden/help/humanify-ag-short.txt` after a
//!    deliberate change);
//! 2. exp179: flags and config keys are ONE-TO-ONE, both generated from
//!    the key registry (the bijection test);
//! 3. the commander ACCIDENTS, each now refused loudly (exit 1, the error
//!    names the option) instead of silently resolving;
//! 4. the retired and renamed flags, refused naming their replacement;
//! 5. the usage-error contract: exit 1, never clap's default 2.
//!
//! Launch parity with the commander port is `launch_parity_test.rs`.

use serde_json::Value;

use crate::config::file::{Kind, Val};
use crate::config::keys::KEYS;
use crate::surface::CommandOptions;
use crate::surface::{
    FLAGS, PipelineParse, RENAMED_FLAGS, RETIRED_FLAGS, help_text, package_version, parse_pipeline,
    short_help_text,
};

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

fn parse_opts(a: &[&str]) -> CommandOptions {
    match parse_pipeline(&argv(a)) {
        PipelineParse::Run { opts, .. } => *opts,
        PipelineParse::Exit { code, stderr, .. } => {
            panic!("{a:?} did not parse (exit {code}):\n{stderr}")
        }
    }
}

/// A refused argv: exit 1 (usage errors keep the pipeline's exit-1
/// contract) and the stderr, which must name `names` (the bad option).
fn refusal(a: &[&str], names: &str) -> String {
    match parse_pipeline(&argv(a)) {
        PipelineParse::Exit {
            code,
            stdout,
            stderr,
        } => {
            assert_eq!(code, 1, "{a:?}: usage errors exit 1\n{stderr}");
            assert!(stdout.is_empty(), "{a:?}: a refusal prints to stderr only");
            assert!(
                stderr.contains(names),
                "{a:?}: must name {names}:\n{stderr}"
            );
            stderr
        }
        PipelineParse::Run { .. } => panic!("{a:?} still parses"),
    }
}

// --- the commander accidents, now refused ---------------------------------

/// commander: a required option took the next argv entry even when it was
/// a flag (`--model --split` set the model to "--split" and dropped
/// `--split`). Now a value never starts with `-`; the option is refused.
#[test]
fn a_value_option_does_not_swallow_the_next_flag() {
    refusal(&["in.js", "--model", "--prior-version"], "--model");
    refusal(&["in.js", "-o", "--prior-version"], "--output-dir");
    refusal(&["in.js", "--endpoint", "-1"], "'-1'");
}

/// commander: a repeated option was last-wins (`-c 4 -c 8` ran at 8,
/// silently). The harness never repeats one; a repeat is now refused.
#[test]
fn a_repeated_option_is_refused() {
    refusal(&["in.js", "-o", "a", "-o", "b"], "--output-dir");
    refusal(&["in.js", "--model", "a", "--model", "b"], "--model");
    refusal(&["in.js", "--config", "a", "--config", "b"], "--config");
}

/// commander: a short value option took an ATTACHED value (`-c5`, `-ofoo`)
/// and a boolean could prefix a combo (`-vc 5`). The harness writes
/// `-c 472` and `-vv`; only the `-v` repeat (`-vv`) stays.
#[test]
fn an_attached_short_value_is_refused() {
    refusal(&["in.js", "-c5"], "-c5");
    refusal(&["in.js", "-ofoo"], "-ofoo");
    refusal(&["in.js", "-mgpt"], "-mgpt");
    refusal(&["in.js", "-o=out"], "-o=out");
    refusal(&["in.js", "-vc", "5"], "-vc");
}

/// `-vv` counts (the harness's spelling); `-v` is one level.
#[test]
fn the_verbosity_counter_counts() {
    let v = |a: &[&str]| parse_opts(a).flag("diagnostics.verbosity").cloned();
    assert_eq!(v(&["in.js"]), None);
    assert_eq!(v(&["in.js", "-v"]), Some(Val::Int(1)));
    assert_eq!(v(&["in.js", "-vv"]), Some(Val::Int(2)));
    assert_eq!(v(&["in.js", "--verbose", "--verbose"]), Some(Val::Int(2)));
}

/// commander: numbers were JavaScript `parseInt` (`-c 12abc` ran at 12).
/// A count flag (exp179: every key is a flag) takes a non-negative
/// integer or is refused, naming the flag.
#[test]
fn a_count_flag_is_typed() {
    refusal(
        &["in.js", "--llm-concurrency", "12abc"],
        "--llm-concurrency",
    );
    refusal(
        &["in.js", "--naming-batch-size", "1.5"],
        "--naming-batch-size",
    );
    refusal(
        &["in.js", "--library-lookup-cover", "lots"],
        "--library-lookup-cover",
    );
    assert_eq!(
        parse_opts(&["in.js", "--llm-concurrency", "64"]).flag("llm.concurrency"),
        Some(&Val::Int(64))
    );
}

/// The library decision (config sweep step 6): `--skip-libraries` and
/// `--no-skip-libraries` are ONE setting now, `[library] unnamed`; both
/// spellings (and `--library-discovery-units`) are refused naming its flag.
#[test]
fn the_skip_libraries_spellings_moved_to_one_setting() {
    let skip = refusal(&["in.js", "--skip-libraries"], "--skip-libraries");
    assert!(
        skip.contains("renamed: use --library-unnamed leave"),
        "{skip}"
    );
    let model = refusal(&["in.js", "--no-skip-libraries"], "--no-skip-libraries");
    assert!(
        model.contains("renamed: use --library-unnamed model"),
        "{model}"
    );
    let units = refusal(
        &["in.js", "--library-discovery-units"],
        "--library-discovery-units",
    );
    assert!(units.contains("--library-report-units"), "{units}");
}

/// An unknown option, a missing or extra input, a boolean given a value:
/// all exit 1, naming what was wrong.
#[test]
fn usage_errors_exit_1_naming_the_problem() {
    refusal(
        &["in.js", "--ambiguity-probe", "p.json"],
        "--ambiguity-probe",
    );
    refusal(&["in.js", "--skip-libraries=yes"], "--skip-libraries");
    refusal(&["-vv"], "<input>");
    refusal(&[], "<input>");
    refusal(&["a.js", "b.js"], "b.js");
    // `env-reads` was deleted with this step (Andrew, 2026-10-13).
    refusal(&["env-reads", "dir"], "dir");
}

// --- retired flags --------------------------------------------------------

/// Config sweep 2026-10-13, no backwards compatibility: a renamed or
/// deleted flag is refused, naming what replaced it — in both spellings.
#[test]
fn a_retired_flag_is_refused_naming_its_replacement() {
    let pipeline: Vec<_> = RETIRED_FLAGS.iter().filter(|r| r.0.is_none()).collect();
    assert!(!pipeline.is_empty());
    for (_, flag, note) in pipeline {
        let err = refusal(&["in.js", flag], flag);
        assert!(err.contains(note), "{flag}: {err}");
        let err = refusal(&["in.js", &format!("{flag}=x")], flag);
        assert!(err.contains(note), "{flag}=x: {err}");
    }
    let line = refusal(&["in.js", "--profile", "p.json"], "--profile");
    assert!(line.contains("renamed: use --perf-trace"), "{line}");
}

/// Every renamed spelling (the config sweep's moved flags, exp179's
/// one-to-one names) is refused naming a replacement that parses — and
/// is never itself a flag again.
#[test]
fn every_renamed_flag_is_refused_naming_a_flag_that_parses() {
    for (old, new) in RENAMED_FLAGS {
        let err = refusal(&["in.js", old, "x"], old);
        assert!(err.contains(&format!("renamed: use {new}")), "{old}: {err}");
        let flag = new.split_whitespace().next().unwrap();
        assert!(
            FLAGS.iter().any(|f| format!("--{}", f.long) == flag
                || f.negated.as_ref().is_some_and(|n| format!("--{n}") == flag)),
            "{old} -> {new}: not a flag"
        );
        assert!(
            FLAGS.iter().all(|f| format!("--{}", f.long) != *old),
            "{old} is renamed AND a flag"
        );
    }
}

// --- flags and keys: one to one (exp179) ----------------------------------

/// A value that the key accepts, as the flag spells it.
fn sample(kind: &Kind) -> Option<(&'static str, Val)> {
    Some(match kind {
        Kind::Bool => return None,
        Kind::Text => ("t", Val::Text("t".into())),
        Kind::Url => ("http://h/v1", Val::Text("http://h/v1".into())),
        Kind::Int { .. } => ("2", Val::Int(2)),
        Kind::Float { .. } => ("0.5", Val::Float(0.5)),
        Kind::Choice(_) => ("c", Val::Text("c".into())),
        Kind::List(_) => ("a,b", Val::List(vec!["a".into(), "b".into()])),
    })
}

/// THE bijection: every registry key has exactly one flag (a boolean also
/// its `--no-` form), every flag the command line accepts — but `--config`,
/// `--help` and `--version` — is a key's, no two keys share a spelling, and
/// each key's flag sets that key and nothing else.
#[test]
fn every_key_is_a_flag_and_every_flag_is_a_key() {
    assert_eq!(FLAGS.len(), KEYS.len(), "one flag per key");
    let mut seen = std::collections::BTreeSet::new();
    for f in FLAGS.iter() {
        for spelling in std::iter::once(&f.long).chain(f.negated.as_ref()) {
            assert!(seen.insert(spelling.clone()), "--{spelling} is two flags");
            assert!(
                !["config", "help", "version"].contains(&spelling.as_str()),
                "--{spelling} is reserved"
            );
        }
    }
    // Every long the command accepts is one of those (no hand-written flag).
    let cmd_longs: std::collections::BTreeSet<String> = crate::surface::accepted_longs()
        .into_iter()
        .filter(|l| !["config", "help", "version"].contains(&l.as_str()))
        .collect();
    assert_eq!(
        cmd_longs, seen,
        "the command's flags are exactly the registry's"
    );
    for f in FLAGS.iter() {
        let long = format!("--{}", f.long);
        let only = |argv: &[&str], want: Val| {
            let o = parse_opts(argv);
            assert_eq!(o.flags, vec![(f.dotted.as_str(), want)], "{argv:?}");
        };
        match sample(&f.key.kind) {
            None => {
                only(&["in.js", &long], Val::Bool(true));
                let no = format!("--{}", f.negated.as_ref().unwrap());
                only(&["in.js", &no], Val::Bool(false));
                refusal(&["in.js", &long, &no], &long);
            }
            Some(_) if f.dotted == crate::config::keys::COUNTED_FLAG => {
                only(&["in.js", &long], Val::Int(1));
            }
            Some((raw, want)) => only(&["in.js", &long, raw], want),
        }
    }
}

/// A short letter is an alias of a key's flag: `-o`, `-m`, `-v`.
#[test]
fn the_short_flags_alias_their_keys() {
    assert_eq!(
        parse_opts(&["in.js", "-o", "d"]).flag("io.output_dir"),
        Some(&Val::Text("d".into()))
    );
    assert_eq!(
        parse_opts(&["in.js", "-m", "x"]).flag("llm.model"),
        Some(&Val::Text("x".into()))
    );
}

// (The `--no-` lever spellings: config_test.rs.)

// --- schedule and Rust-only flags -----------------------------------------

/// The naming schedule (docs/rust-port/20-fast-mode.md, flipped
/// 2026-09-28): the DEFAULT is the relaxed tier, every lever on, no key.
#[test]
fn the_naming_schedule_defaults_to_relaxed() {
    use humanify_ag_core::fast::{FastTier, Lever, Levers};
    let tier = |toml: &str| {
        let f = crate::config_test::file(toml);
        crate::config::Config::build("in.js", &parse_opts(&["in.js"]), Some(&f), |_| {
            Some("k".to_string())
        })
        .map(|c| c.naming.schedule)
    };
    assert_eq!(tier(""), Ok(FastTier::Relaxed(Levers::all())));
    assert_eq!(
        tier("[naming]\nschedule = \"sequential\"\n"),
        Ok(FastTier::Exact)
    );
    let lanes = FastTier::Relaxed(Levers::default().with(Lever::WindowLanes));
    assert_eq!(
        tier("[naming]\nrelaxed_levers = [\"window-lanes\"]\n"),
        Ok(lanes)
    );
    assert!(
        tier("[naming]\nschedule = \"sequential\"\nrelaxed_levers = [\"window-lanes\"]\n").is_err()
    );
}

// --- help and version -----------------------------------------------------

fn golden(name: &str) -> String {
    let path = format!(
        "{}/../../test/golden/help/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn help_matches_the_golden_and_prints_to_stdout() {
    assert_eq!(
        help_text().trim_end(),
        golden("humanify-ag.txt").trim_end(),
        "help (regenerate the golden with --help after a deliberate change)"
    );
    assert_eq!(
        short_help_text().trim_end(),
        golden("humanify-ag-short.txt").trim_end(),
        "short help (regenerate the golden with -h after a deliberate change)"
    );
    for (flag, want) in [
        ("--help", "humanify-ag.txt"),
        ("-h", "humanify-ag-short.txt"),
    ] {
        match parse_pipeline(&argv(&[flag])) {
            PipelineParse::Exit {
                code,
                stdout,
                stderr,
            } => {
                assert_eq!((code, stderr.as_str()), (0, ""), "{flag}");
                assert_eq!(stdout.trim_end(), golden(want).trim_end(), "{flag}");
            }
            PipelineParse::Run { .. } => panic!("{flag} ran the pipeline"),
        }
    }
}

/// What the help lists: every key's flag (`--help`; `-h` leaves out the
/// library tuning sections) with its config key; no renamed or retired
/// flag, no deleted TS file, no `.env`.
#[test]
fn help_lists_every_flag_and_its_key() {
    let help = help_text();
    let short = short_help_text();
    assert!(help.contains("--config <file>"), "the harness greps for it");
    for f in FLAGS.iter() {
        // (`--verbose...`: clap marks the counted flag.)
        let long = format!("--{} ", f.long).replace("--verbose ", "--verbose...");
        let long_eol = format!("--{}\n", f.long).replace("--verbose\n", "--verbose...");
        assert!(
            help.contains(&long) || help.contains(&long_eol),
            "{long} missing"
        );
        assert!(
            help.contains(&format!("[config: {}]", f.dotted)),
            "{} missing",
            f.dotted
        );
        let in_short = short.contains(&long) || short.contains(&long_eol);
        assert_eq!(
            in_short,
            !crate::config::keys::tuning_section(f.key.section),
            "{long} in -h"
        );
    }
    assert!(!help.contains(".env"), "no .env is read");
    for (flag, _) in RENAMED_FLAGS {
        assert!(
            !help.contains(&format!("{flag} ")) && !help.contains(&format!("{flag}\n")),
            "{flag} is listed"
        );
    }
    for stale in [".ts", "parity-era", "07 §2", "env-reads"] {
        assert!(!help.contains(stale), "help mentions {stale:?}:\n{help}");
    }
    for (_, flag, _) in RETIRED_FLAGS.iter().filter(|r| r.0.is_none()) {
        assert!(!help.contains(&format!("{flag} ")), "{flag} is listed");
    }
}

#[test]
fn version_is_package_json_version() {
    let path = format!("{}/../../package.json", env!("CARGO_MANIFEST_DIR"));
    let pkg: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("package.json")).unwrap();
    assert_eq!(Some(package_version().as_str()), pkg["version"].as_str());
    match parse_pipeline(&argv(&["-V"])) {
        PipelineParse::Exit { code, stdout, .. } => {
            assert_eq!(code, 0);
            assert!(stdout.contains(&package_version()), "{stdout}");
        }
        PipelineParse::Run { .. } => panic!("-V ran the pipeline"),
    }
}
