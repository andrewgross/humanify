//! The pipeline program's flag surface (clap since config sweep step 3,
//! 2026-10-13; the commander-13 port it replaced is gone):
//!
//! 1. the rendered HELP text against the committed golden
//!    (test/golden/help/humanify.txt; regenerate with
//!    `humanify --help > test/golden/help/humanify.txt` after a deliberate
//!    change);
//! 2. the commander ACCIDENTS, each now refused loudly (exit 1, the error
//!    names the option) instead of silently resolving;
//! 3. the retired flags, refused naming their replacement;
//! 4. the usage-error contract: exit 1, never clap's default 2.
//!
//! Launch parity with the commander port is `launch_parity_test.rs`.

use serde_json::Value;

use crate::surface::CommandOptions;
use crate::surface::{PipelineParse, RETIRED_FLAGS, help_text, package_version, parse_pipeline};

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
    assert_eq!(parse_opts(&["in.js"]).verbose, 0);
    assert_eq!(parse_opts(&["in.js", "-v"]).verbose, 1);
    assert_eq!(parse_opts(&["in.js", "-vv"]).verbose, 2);
}

/// commander: numbers were JavaScript `parseInt` (`-c 12abc` ran at 12).
/// No flag takes a number any more (config sweep step 5): every count is a
/// typed key in the `--config` file, refused there naming the key
/// (config_file_test.rs).
#[test]
fn a_count_is_no_longer_a_flag() {
    refusal(&["in.js", "-c", "12abc"], "llm.concurrency");
    refusal(&["in.js", "--batch-size", "7"], "naming.batch_size");
}

/// `--skip-libraries` (restates the default) and `--no-skip-libraries`
/// keep their effect until the library decision (sweep step 6) deletes
/// both; passing BOTH was last-wins and is now a conflict.
#[test]
fn the_two_skip_libraries_spellings_conflict() {
    let skip = |a: &[&str]| {
        crate::config::Config::build("in.js", &parse_opts(a), None, |_| Some("k".to_string()))
            .expect("valid")
            .library
            .skip_libraries
    };
    assert!(skip(&["in.js"]));
    assert!(skip(&["in.js", "--skip-libraries"]));
    assert!(!skip(&["in.js", "--no-skip-libraries"]));
    refusal(
        &["in.js", "--skip-libraries", "--no-skip-libraries"],
        "--skip-libraries",
    );
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
    assert!(line.contains("perf.perf_trace"), "{line}");
    let line = refusal(&["in.js", "--perf-trace", "p.json"], "--perf-trace");
    assert!(
        line.contains("moved: set perf.perf_trace in --config"),
        "{line}"
    );
}

// (The `--no-` lever spellings: config_test.rs.)

// --- schedule and Rust-only flags -----------------------------------------

/// The naming schedule (docs/rust-port/20-fast-mode.md, flipped
/// 2026-09-28): the DEFAULT is the relaxed tier, every lever on, no key.
#[test]
fn the_naming_schedule_defaults_to_relaxed() {
    use humanify_core::fast::{FastTier, Lever, Levers};
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

fn golden_help() -> String {
    let path = format!(
        "{}/../../test/golden/help/humanify.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn help_matches_the_golden_and_prints_to_stdout() {
    assert_eq!(
        help_text().trim_end(),
        golden_help().trim_end(),
        "help (regenerate the golden with --help after a deliberate change)"
    );
    for flag in ["--help", "-h"] {
        match parse_pipeline(&argv(&[flag])) {
            PipelineParse::Exit {
                code,
                stdout,
                stderr,
            } => {
                assert_eq!((code, stderr.as_str()), (0, ""), "{flag}");
                assert_eq!(stdout.trim_end(), golden_help().trim_end(), "{flag}");
            }
            PipelineParse::Run { .. } => panic!("{flag} ran the pipeline"),
        }
    }
}

/// What the help lists: the operational flags and `--config`; no moved
/// or retired flag, no deleted TS file.
#[test]
fn help_lists_the_operational_flags_only() {
    let help = help_text();
    for flag in [
        "--config",
        "--output-dir",
        "--prior-version",
        "--endpoint",
        "--model",
    ] {
        assert!(help.contains(flag), "{flag} missing:\n{help}");
    }
    for stale in [".ts", "parity-era", "07 §2", "env-reads"] {
        assert!(!help.contains(stale), "help mentions {stale:?}:\n{help}");
    }
    for (_, flag, _) in RETIRED_FLAGS.iter().filter(|r| r.0.is_none()) {
        assert!(!help.contains(&format!("{flag} ")), "{flag} is listed");
    }
    for (flag, _, _) in crate::surface::MOVED_FLAGS {
        assert!(
            !help.contains(&format!("{flag} ")) && !help.contains(&format!("{flag}\n")),
            "{flag} is listed:\n{help}"
        );
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
