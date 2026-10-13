//! The pipeline program's command surface: `humanify <input> [options]`,
//! declared for clap — the parser the stage verbs (main.rs) already use, so
//! the binary has ONE argument parser (config sweep step 3, 2026-10-13).
//!
//! It replaced a port of the npm `commander` 13 package, kept after the
//! cutover only because the TS harness used to spawn the TS pipeline with
//! commander's grammar. Its accidents are now refused, loudly, with exit 1
//! (surface_test.rs holds one test per accident):
//!
//! - a value option no longer swallows the next flag (`--model --split`
//!   set the model to "--split"); a value never starts with `-`;
//! - a repeated option is refused (commander: last wins);
//! - a short option's value must be a separate argument (`-c 472`, not
//!   `-c5`); the only combined short form is the `-v` counter (`-vv`);
//! - numbers are typed: a non-negative integer or a refusal (commander:
//!   JavaScript `parseInt`, so `-c 12abc` ran at 12);
//! - `--skip-libraries` with `--no-skip-libraries` was a conflict; both are
//!   gone since the library decision (config sweep step 6: `[library]
//!   unnamed`).
//!
//! Config sweep step 5 (2026-10-13): only the OPERATIONAL flags are flags
//! — the input, `-o`, `--prior-version`, `--endpoint`, `--model`,
//! `--config`, `-v`, and the ONE library decision (`--library-matching`,
//! `--library-index`, step 6). Every other
//! setting is a key in the `--config` TOML file; its old flag is refused
//! naming the key ([`MOVED_FLAGS`]).
//!
//! Every launch the harness makes parses to the settings the commander
//! port gave it (launch_parity_test.rs, captured from the port at
//! 587e211f). Usage errors exit 1, never clap's default 2: the pipeline
//! keeps exit 2 out of its contract. The rendered help is pinned by
//! test/golden/help/humanify.txt.

use clap::{CommandFactory, FromArgMatches, Parser};
use serde_json::Value;

use crate::config::{API_KEY_VAR, DEFAULT_ENDPOINT, DEFAULT_MODEL, DEFAULT_OUTPUT_DIR};

/// package.json's version — the single source `-V` prints.
pub fn package_version() -> String {
    let pkg: Value =
        serde_json::from_str(include_str!("../../../package.json")).expect("package.json is JSON");
    pkg["version"]
        .as_str()
        .expect("package.json has a version")
        .to_string()
}

fn endpoint_help() -> String {
    format!(
        "OpenAI-compatible API endpoint (default: {DEFAULT_ENDPOINT}). The API key is read \
         from {API_KEY_VAR} (the environment or a .env file), never from a flag"
    )
}

fn model_help() -> String {
    format!("Model identifier (default: {DEFAULT_MODEL})")
}

fn output_help() -> String {
    format!("Output directory (default: {DEFAULT_OUTPUT_DIR})")
}

/// The pipeline program: the input and its options.
#[derive(Parser, Debug)]
#[command(
    name = "humanify",
    about = "Unminify JavaScript using an OpenAI-compatible API"
)]
struct PipelineCli {
    #[arg(value_name = "input", help = "The input minified JavaScript file")]
    input: String,
    #[command(flatten)]
    opts: CommandOptions,
}

/// The parsed options, exactly as given: `None`/`false` is "not given".
/// Only the OPERATIONAL flags are flags (config sweep step 5, Andrew
/// 2026-10-13): what to read, where to write, which model, which config
/// file. Every other setting is a key in the `--config` file (each moved
/// flag is refused naming its key, [`MOVED_FLAGS`]). No default lives here
/// — `crate::config` owns every default, resolves and validates the
/// options ONCE into the run's `Config`, and nothing else reads these
/// fields.
#[derive(clap::Args, Clone, Debug, Default)]
pub struct CommandOptions {
    #[arg(
        long,
        value_name = "file",
        help = "TOML config file: every setting that is not a flag (see \
                humanify.example.toml for each key, its default and meaning). A \
                flag that disagrees with the file is refused"
    )]
    pub config: Option<String>,
    #[arg(short = 'o', long, value_name = "output", help = output_help())]
    pub output_dir: Option<String>,
    #[arg(
        long,
        value_name = "path",
        help = "Path to a prior humanified file for cross-version rename reuse"
    )]
    pub prior_version: Option<String>,
    #[arg(long, value_name = "url", help = endpoint_help())]
    pub endpoint: Option<String>,
    #[arg(short = 'm', long, value_name = "model", help = model_help())]
    pub model: Option<String>,
    #[arg(
        short = 'v',
        long,
        action = clap::ArgAction::Count,
        help = "Increase verbosity (-v for info, -vv for debug)"
    )]
    pub verbose: u8,
    #[arg(
        long,
        value_name = "mode",
        help = "Library matching: off (path/banner detection only, the default), report \
                (also the library index lookup + graph discovery, reported; output \
                unchanged), names (refused until an eval passes it)"
    )]
    pub library_matching: Option<String>,
    #[arg(
        long,
        value_name = "dir|id",
        help = "The library fingerprint index (a directory, or an id under \
                library.index_store); required unless --library-matching is off, \
                refused with off"
    )]
    pub library_index: Option<String>,
}

/// Flags MOVED into the config file (config sweep step 5, 2026-10-13; no
/// backwards compatibility): `(old spelling, the key, the value a
/// no-argument flag meant — empty for a flag that took a value)`. Each is
/// refused naming its key, never aliased.
pub const MOVED_FLAGS: &[(&str, &str, &str)] = &[
    ("-c", "llm.concurrency", ""),
    ("--concurrency", "llm.concurrency", ""),
    ("--module-concurrency", "llm.module_concurrency", ""),
    ("--max-tokens", "llm.max_tokens", ""),
    ("--context-tokens", "llm.context_tokens", ""),
    ("--retries", "llm.retries", ""),
    ("--timeout", "llm.timeout_ms", ""),
    ("--llm-cache", "llm.cache_dir", ""),
    ("--reasoning-effort", "llm.reasoning_effort", ""),
    ("--disable", "switches.disable", ""),
    ("--probe", "switches.probe", ""),
    ("--log-file", "diagnostics.log_file", ""),
    ("--diagnostics", "diagnostics.diagnostics_file", ""),
    ("--stats-json", "diagnostics.stats_json", ""),
    ("--dump-artifacts", "diagnostics.dump_artifacts", ""),
    ("--dump-asks", "diagnostics.dump_asks", ""),
    ("--rename-ledger", "diagnostics.rename_ledger", ""),
    ("--bundler", "unpack.bundler", ""),
    ("--minifier", "unpack.minifier", ""),
    ("--batch-size", "naming.batch_size", ""),
    ("--max-retries", "naming.max_retries", ""),
    ("--rename-retries", "naming.rename_retries", ""),
    ("--max-free-retries", "naming.max_free_retries", ""),
    ("--lane-threshold", "naming.lane_threshold", ""),
    ("--sequential", "naming.schedule", "\"sequential\""),
    ("--relaxed-levers", "naming.relaxed_levers", ""),
    ("--no-naming-floor", "naming.naming_floor", "false"),
    (
        "--no-naming-floor-sweep",
        "naming.naming_floor_sweep",
        "false",
    ),
    (
        "--no-reconcile-prior-diff",
        "matching.reconcile_prior_diff",
        "false",
    ),
    ("--split", "split.enabled", "true"),
    ("--split-pure", "split.pure", "true"),
    ("--split-ledger", "split.ledger", ""),
    ("--perf-trace", "perf.perf_trace", ""),
    ("--simulate-llm-latency", "perf.simulate_llm_latency", ""),
    // The library decision (config sweep step 6): one setting, not two
    // flags (docs/library-matching-config.md §2).
    ("--skip-libraries", "library.unnamed", "\"leave\""),
    ("--no-skip-libraries", "library.unnamed", "\"model\""),
    ("--library-discovery-units", "library.report_units", "true"),
];

/// The refusal for a moved flag.
fn moved_note(key: &str, example: &str) -> String {
    if example.is_empty() {
        format!("moved: set {key} in --config")
    } else {
        format!("moved: set {key} = {example} in --config")
    }
}

/// Flags renamed or deleted by the config sweep (2026-10-13; no backwards
/// compatibility): `(verb, old spelling, what to say instead)`, `None` for
/// the pipeline program, `Some(verb)` for a clap stage verb (main.rs).
/// The old spelling is refused, naming its replacement, never aliased.
pub const RETIRED_FLAGS: &[(Option<&str>, &str, &str)] = &[
    (
        None,
        "--profile",
        "renamed and moved: set perf.perf_trace in --config",
    ),
    (
        None,
        "--api-key",
        "removed: set HUMANIFY_API_KEY in the environment — a secret never goes on the \
         command line",
    ),
    (
        None,
        "--naming-floor",
        "removed: it restated the default (on); naming.naming_floor = false in --config \
         turns it off",
    ),
    (
        None,
        "--naming-floor-sweep",
        "removed: it restated the default (on); naming.naming_floor_sweep = false in \
         --config turns it off",
    ),
    (
        None,
        "--reconcile-prior-diff",
        "removed: it restated the default (on with --prior-version); \
         matching.reconcile_prior_diff = false in --config turns it off",
    ),
    (Some("detect"), "--profile", "renamed: use --perf-trace"),
    (
        Some("discover"),
        "--rule",
        "moved: set library.lookup.agreement in a --config file (false = the structural \
         rule)",
    ),
    (Some("unpack"), "--index", "renamed: use --bundle-order"),
    (
        Some("match"),
        "--sequential",
        "removed: it was inert and never changed the dump",
    ),
];

/// A retired flag in `args` (`verb`'s own arguments; `None` = the
/// pipeline's): the refusal naming what replaced it, or None. `--flag=value`
/// counts too; nothing after `--` is a flag.
pub fn retired_flag(verb: Option<&str>, args: &[String]) -> Option<String> {
    args.iter().take_while(|a| *a != "--").find_map(|arg| {
        let name = arg.split_once('=').map_or(arg.as_str(), |(n, _)| n);
        let retired = RETIRED_FLAGS
            .iter()
            .find(|(v, flag, _)| *v == verb && *flag == name)
            .map(|(_, flag, note)| (*flag, note.to_string()));
        let moved = || {
            MOVED_FLAGS
                .iter()
                .find(|(flag, _, _)| verb.is_none() && *flag == name)
                .map(|(flag, key, example)| (*flag, moved_note(key, example)))
        };
        retired
            .or_else(moved)
            .map(|(flag, note)| format!("error: unexpected argument '{flag}' ({note})\n"))
    })
}

/// A combined single-dash argument other than the `-v` counter (`-vv`):
/// clap would read `-c5` as `-c 5` and `-vc 5` as `-v -c 5`; the harness
/// writes neither, so both are refused rather than guessed at.
fn attached_short(args: &[String]) -> Option<String> {
    args.iter()
        .take_while(|a| *a != "--")
        .find(|a| {
            a.len() > 2
                && a.starts_with('-')
                && !a.starts_with("--")
                && !a[1..].bytes().all(|b| b == b'v')
        })
        .map(|a| {
            format!(
                "error: unexpected argument '{a}': a short option's value is a separate \
                 argument (`-c 472`, not `-c472`), and only -v repeats (`-vv`)\n"
            )
        })
}

fn pipeline_command() -> clap::Command {
    // The version is package.json's, read at run time (the crate's own is
    // 0.0.0); clap wants it 'static, and there is one per process.
    let version: &'static str = Box::leak(package_version().into_boxed_str());
    PipelineCli::command().version(version)
}

/// The pipeline program's help, as `--help` prints it.
pub fn help_text() -> String {
    pipeline_command().render_help().to_string()
}

/// What the pipeline's argv parsed to: a run, or an exit (help, version,
/// a usage error) with what to print where.
#[derive(Debug)]
pub enum PipelineParse {
    Run {
        input: String,
        opts: Box<CommandOptions>,
    },
    Exit {
        code: i32,
        stdout: String,
        stderr: String,
    },
}

fn usage_error(stderr: String) -> PipelineParse {
    PipelineParse::Exit {
        code: 1,
        stdout: String::new(),
        stderr,
    }
}

/// Parse the pipeline program's argv (no program name). Help and version
/// go to stdout with exit 0; every usage error goes to stderr with exit 1.
pub fn parse_pipeline(argv: &[String]) -> PipelineParse {
    if let Some(refusal) = retired_flag(None, argv).or_else(|| attached_short(argv)) {
        return usage_error(refusal);
    }
    let full = std::iter::once("humanify".to_string()).chain(argv.iter().cloned());
    let parsed = pipeline_command()
        .try_get_matches_from(full)
        .and_then(|m| PipelineCli::from_arg_matches(&m));
    match parsed {
        Ok(cli) => PipelineParse::Run {
            input: cli.input,
            opts: Box::new(cli.opts),
        },
        Err(e) => {
            use clap::error::ErrorKind;
            let text = e.render().to_string();
            match e.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => PipelineParse::Exit {
                    code: 0,
                    stdout: text,
                    stderr: String::new(),
                },
                _ => usage_error(text),
            }
        }
    }
}
