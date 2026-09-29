//! The pipeline program's flag surface, natively (the TS-recorded option
//! table and argv corpus — test/parity/wpb4-cli-surface.json — were
//! retired 2026-09-28; the flags have been the Rust binary's own since
//! the cutover):
//!
//! 1. the rendered HELP text against the committed golden of the BINARY's
//!    help (test/golden/help/<command>.txt). The Rust binary is the product
//!    since the cutover, so its help is pinned to itself, not to the TS's
//!    wording (which named deleted TS files); regenerate a golden with
//!    `humanify --help > test/golden/help/humanify.txt` after a deliberate
//!    wording change;
//! 2. PARSE checks: the Rust-only flags parse, retired options are
//!    unknown, and the -V output equals package.json's version.

use serde_json::Value;

use crate::commander::ParseOutcome;
use crate::surface::{help_text, program};

/// `--sequential` is Rust-only: declared, parsed, and listed in the help.
/// `--relaxed-levers` is Rust-only too — and deliberately NOT in the help
/// (a sizing knob, hidden from the user-facing surface).
#[test]
fn the_sequential_flag_parses() {
    let root = program();
    let argv: Vec<String> = ["in.js", "--sequential"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    match root.parse(&argv) {
        ParseOutcome::Action { opts, .. } => {
            assert_eq!(opts.bool("sequential"), Some(true));
            assert!(root.help_information(&[]).contains("--sequential"));
            assert!(
                !root.help_information(&[]).contains("--relaxed-levers"),
                "the sizing knob stays hidden from the help"
            );
        }
        other => panic!("--sequential did not parse: {other:?}"),
    }
}

fn parse_opts(argv: &[&str]) -> crate::commander::OptionValues {
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    match program().parse(&argv) {
        ParseOutcome::Action { opts, .. } => opts,
        other => panic!("{argv:?} did not parse: {other:?}"),
    }
}

/// The naming schedule (docs/rust-port/20-fast-mode.md, flipped
/// 2026-09-28): the DEFAULT is the relaxed tier, every lever on, no flag.
/// `--sequential` is the conservative schedule (the relaxed levers OFF,
/// the exact tier's byte-identical parallelization kept); the rust-only
/// `--relaxed-levers` sizes a subset of them.
#[test]
fn the_naming_schedule_defaults_to_relaxed() {
    use crate::unified::{CommandOptions, FastTier};
    use humanify_core::fast::Levers;
    let tier = |argv: &[&str]| CommandOptions::from_values(&parse_opts(argv)).fast_tier();
    assert_eq!(tier(&["in.js"]), Ok(FastTier::Relaxed(Levers::all())));
    assert_eq!(tier(&["in.js", "--sequential"]), Ok(FastTier::Exact));
    let lanes = FastTier::Relaxed(Levers::parse("window-lanes").unwrap());
    assert_eq!(
        tier(&["in.js", "--relaxed-levers", "window-lanes"]),
        Ok(lanes)
    );
    assert_eq!(
        tier(&["in.js", "--relaxed-levers", "window-lanes,defer-shadowed"]),
        Ok(FastTier::Relaxed(Levers::all()))
    );
    assert!(tier(&["in.js", "--relaxed-levers", "bogus"]).is_err());
    assert!(
        tier(&["in.js", "--sequential", "--relaxed-levers", "window-lanes"]).is_err(),
        "--sequential has no relaxed levers to select"
    );
}

#[test]
fn the_llm_latency_simulation_takes_a_path() {
    let opts = parse_opts(&["in.js", "--simulate-llm-latency", "sim.json"]);
    assert_eq!(opts.str("simulateLlmLatency"), Some("sim.json"));
}

/// `--rename-retries <n>` (2026-09-29): Rust-only but USER-FACING —
/// declared, parsed, and listed in the help (unlike the hidden
/// `--relaxed-levers`), next to the `--max-retries` cap it composes with.
#[test]
fn the_rename_retries_flag_parses_and_lists() {
    let opts = parse_opts(&["in.js", "--rename-retries", "2"]);
    assert_eq!(opts.str("renameRetries"), Some("2"));
    let root = program();
    let help = root.help_information(&[]);
    assert!(
        help.contains("--rename-retries <n>"),
        "the budget flag is user-facing: {help}"
    );
    let max = help.find("--max-retries").expect("--max-retries is listed");
    let rename = help
        .find("--rename-retries")
        .expect("--rename-retries is listed");
    assert!(
        max < rename,
        "--rename-retries lists next to the cap it composes with"
    );
}

fn golden_help(command: &str) -> String {
    let path = format!(
        "{}/../../test/golden/help/{command}.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn help_text_matches_the_golden() {
    let root = program();
    let mut commands = vec![root.name.clone()];
    commands.extend(root.commands.iter().map(|c| c.name.clone()));
    for name in commands {
        assert_eq!(
            help_text(&name),
            golden_help(&name),
            "help for {name} (regenerate the golden with --help after a deliberate change)"
        );
    }
}

/// The help is the binary's user-facing text: it names no file the
/// cutover deleted (the TS pipeline's `.ts` files) and no migration-era
/// instrument label.
#[test]
fn help_names_no_deleted_ts_file() {
    let root = program();
    let mut helps = vec![help_text(&root.name)];
    helps.extend(root.commands.iter().map(|c| help_text(&c.name)));
    for help in helps {
        for stale in [".ts", "parity-era", "07 §2"] {
            assert!(!help.contains(stale), "help mentions {stale:?}:\n{help}");
        }
    }
}

/// Every option the help lists does something: --ambiguity-probe was
/// parsed and then never read by the Rust pipeline, so it is gone and is
/// an unknown option like any other.
#[test]
fn a_retired_option_is_unknown() {
    let argv: Vec<String> = ["in.js", "--ambiguity-probe", "p.json"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    match program().parse(&argv) {
        ParseOutcome::Exit {
            exit_code, code, ..
        } => {
            assert_eq!((exit_code, code), (1, "commander.unknownOption"));
        }
        ParseOutcome::Action { .. } => panic!("--ambiguity-probe still parses"),
    }
}

#[test]
fn version_is_package_json_version() {
    let path = format!("{}/../../package.json", env!("CARGO_MANIFEST_DIR"));
    let pkg: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("package.json")).unwrap();
    assert_eq!(
        program().version.as_deref(),
        pkg["version"].as_str(),
        "the -V output is package.json's version"
    );
}
