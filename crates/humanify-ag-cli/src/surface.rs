//! The pipeline program's command surface: `humanify-ag <input> [options]`,
//! built for clap — the parser the stage verbs (main.rs) already use, so
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
//! - a short option's value must be a separate argument (`-o out`, not
//!   `-oout`); the only combined short form is the `-v` counter (`-vv`);
//! - numbers are typed: a non-negative integer or a refusal (commander:
//!   JavaScript `parseInt`, so `-c 12abc` ran at 12).
//!
//! exp179 (Andrew, 2026-11-01): FLAGS AND CONFIG KEYS ARE ONE-TO-ONE.
//! Every key in the registry (`crate::config::keys::KEYS`) is a flag and
//! every flag but `--config` is a key, GENERATED here from the registry —
//! the name ([`crate::config::file::Key::flag_name`]: short for the
//! commonest settings, `--<section>-<name>` otherwise), the value it takes
//! (from the key's kind; a boolean is `--x` / `--no-x`, a list is
//! comma-separated), the help (the key's doc, grouped by section). The
//! config sweep's operational/config split is gone; its rule stays: a flag
//! that disagrees with the `--config` file is refused, naming both. A
//! spelling the sweep or exp179 renamed is refused naming its replacement
//! ([`RENAMED_FLAGS`]).
//!
//! Every launch the harness makes parses to the settings the commander
//! port gave it (launch_parity_test.rs, captured from the port at
//! 587e211f). Usage errors exit 1, never clap's default 2: the pipeline
//! keeps exit 2 out of its contract. The rendered help is pinned by
//! test/golden/help/humanify-ag.txt (`--help`) and humanify-ag-short.txt (`-h`).

use std::sync::LazyLock;

use clap::{Arg, ArgAction};
use serde_json::Value;

use crate::config::file::{FlagValue, Val};
use crate::config::keys::PIPELINE;
pub use crate::registry::flags::FlagSpec;
use crate::registry::flags::{add_flags, attached_short, flag_values, specs};

/// package.json's version — the single source `-V` prints.
pub fn package_version() -> String {
    let pkg: Value =
        serde_json::from_str(include_str!("../../../package.json")).expect("package.json is JSON");
    pkg["version"]
        .as_str()
        .expect("package.json has a version")
        .to_string()
}

/// The parsed options, exactly as given: the `--config` file, and the
/// value of every flag given (a flag not given is absent). No default lives
/// here — `crate::config` owns every default, resolves and validates the
/// options ONCE into the run's `Config`, and nothing else reads them.
#[derive(Clone, Debug, Default)]
pub struct CommandOptions {
    pub config: Option<String>,
    /// (dotted key, value), one per flag given, in registry order.
    pub flags: Vec<FlagValue>,
}

impl CommandOptions {
    /// The value the flag for `dotted` gave, if it was given.
    pub fn flag(&self, dotted: &str) -> Option<&Val> {
        self.flags
            .iter()
            .find(|(k, _)| *k == dotted)
            .map(|(_, v)| v)
    }
}

/// Every key's flag, in registry order — built once.
pub static FLAGS: LazyLock<Vec<FlagSpec>> = LazyLock::new(|| specs(&PIPELINE));

/// The flag `--config` is the one flag that is not a key: it names the file.
pub const CONFIG_FLAG: &str = "config";

fn pipeline_command() -> clap::Command {
    // The version is package.json's, read at run time (the crate's own is
    // 0.0.0); clap wants it 'static, and there is one per process.
    static VERSION: LazyLock<String> = LazyLock::new(package_version);
    // (`default().name`, not `new`: a clap command, not a subprocess — the
    // concurrency census reads `Command::new` as a spawn.)
    let cmd = clap::Command::default()
        .name("humanify-ag")
        .about("Unminify JavaScript using an OpenAI-compatible API")
        .version(VERSION.as_str())
        .after_help(
            "A boolean's --no-<flag> turns it off (--no-split). The library tuning flags \
             (--library-lookup-*, --library-discovery-*) are listed by --help. Every option is \
             also a key of the --config file (humanify.example.toml); a flag that disagrees \
             with the file is refused. The API key is read from HUMANIFY_API_KEY in the \
             environment.",
        )
        .after_long_help(
            "Every option is also a key of the --config file, shown as [config: section.key] \
             (humanify.example.toml lists them all); a flag that disagrees with the file is \
             refused, naming both. The API key is read from HUMANIFY_API_KEY in the \
             environment, never from a flag or the file.",
        )
        .arg(
            Arg::new("input")
                .value_name("input")
                .required(true)
                .help("The input minified JavaScript file"),
        )
        .arg(
            Arg::new(CONFIG_FLAG)
                .long(CONFIG_FLAG)
                .value_name("file")
                .action(ArgAction::Set)
                .help(
                    "TOML config file: any setting below as a key (see humanify.example.toml). \
                     A flag that disagrees with the file is refused",
                ),
        );
    add_flags(cmd, FLAGS.as_slice())
}

/// Spellings renamed by the config sweep (2026-10-13) or by exp179's
/// one-to-one flags (no backwards compatibility): `(old spelling, what to
/// write instead)`. Each is refused naming its replacement, never aliased.
/// A replacement is today's flag (and, for an old switch, its value).
pub const RENAMED_FLAGS: &[(&str, &str)] = &[
    ("-c", "--llm-concurrency"),
    ("--concurrency", "--llm-concurrency"),
    ("--module-concurrency", "--llm-module-concurrency"),
    ("--max-tokens", "--llm-max-tokens"),
    ("--context-tokens", "--llm-context-tokens"),
    ("--retries", "--llm-retries"),
    ("--timeout", "--llm-timeout-ms"),
    ("--llm-cache", "--llm-cache-dir"),
    ("--reasoning-effort", "--llm-reasoning-effort"),
    ("--disable", "--switches-disable"),
    ("--probe", "--switches-probe"),
    ("--log-file", "--diagnostics-log-file"),
    ("--diagnostics", "--diagnostics-file"),
    ("--stats-json", "--diagnostics-stats-json"),
    ("--dump-artifacts", "--diagnostics-dump-artifacts"),
    ("--dump-asks", "--diagnostics-dump-asks"),
    ("--rename-ledger", "--diagnostics-rename-ledger"),
    ("--batch-size", "--naming-batch-size"),
    ("--max-retries", "--naming-max-retries"),
    ("--rename-retries", "--naming-rename-retries"),
    ("--max-free-retries", "--naming-max-free-retries"),
    ("--lane-threshold", "--naming-lane-threshold"),
    ("--sequential", "--naming-schedule sequential"),
    ("--relaxed-levers", "--naming-relaxed-levers"),
    ("--reconcile-prior-diff", "--matching-reconcile-prior-diff"),
    (
        "--no-reconcile-prior-diff",
        "--no-matching-reconcile-prior-diff",
    ),
    ("--simulate-llm-latency", "--perf-simulate-llm-latency"),
    ("--profile", "--perf-trace"),
    // The library decision (config sweep step 6): one setting, not two
    // flags (docs/library-matching-config.md §2).
    ("--skip-libraries", "--library-unnamed leave"),
    ("--no-skip-libraries", "--library-unnamed model"),
    ("--library-discovery-units", "--library-report-units"),
];

/// Flags deleted by the config sweep (2026-10-13; no backwards
/// compatibility), and the stage verbs' renamed flags: `(verb, old
/// spelling, what to say instead)`, `None` for the pipeline program,
/// `Some(verb)` for a clap stage verb (main.rs). The old spelling is
/// refused, naming its replacement, never aliased. (The pipeline's renamed
/// spellings: [`RENAMED_FLAGS`].)
pub const RETIRED_FLAGS: &[(Option<&str>, &str, &str)] = &[
    (
        None,
        "--api-key",
        "removed: set HUMANIFY_API_KEY in the environment — a secret never goes on the \
         command line",
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
    let renamed: Vec<(&str, String)> = RENAMED_FLAGS
        .iter()
        .filter(|_| verb.is_none())
        .map(|(flag, new)| (*flag, format!("renamed: use {new}")))
        .collect();
    let table: Vec<(&str, &str)> = RETIRED_FLAGS
        .iter()
        .filter(|(v, _, _)| *v == verb)
        .map(|(_, flag, note)| (*flag, *note))
        .chain(renamed.iter().map(|(flag, note)| (*flag, note.as_str())))
        .collect();
    crate::registry::retired_in(&table, args)
}

/// Every long flag the command accepts (`--x` and `--no-x` alike), as
/// clap holds it — for the bijection test against the registry.
pub fn accepted_longs() -> Vec<String> {
    pipeline_command()
        .get_arguments()
        .filter_map(|a| a.get_long().map(str::to_string))
        .collect()
}

/// The pipeline program's help, as `--help` prints it: every flag, its
/// whole doc, default and config key.
pub fn help_text() -> String {
    pipeline_command().render_long_help().to_string()
}

/// The pipeline program's help, as `-h` prints it: one line per flag, the
/// library tuning sections left out.
pub fn short_help_text() -> String {
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
    let full = std::iter::once("humanify-ag".to_string()).chain(argv.iter().cloned());
    match pipeline_command().try_get_matches_from(full) {
        Ok(m) => match flag_values(FLAGS.as_slice(), &m) {
            Ok(flags) => PipelineParse::Run {
                input: m.get_one::<String>("input").expect("required").clone(),
                opts: Box::new(CommandOptions {
                    config: m.get_one::<String>(CONFIG_FLAG).cloned(),
                    flags,
                }),
            },
            Err(e) => usage_error(e),
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
