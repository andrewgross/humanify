//! The pipeline program's command surface: `humanify <input> [options]`,
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
//! test/golden/help/humanify.txt (`--help`) and humanify-short.txt (`-h`).

use std::sync::LazyLock;

use clap::{Arg, ArgAction, ArgMatches};
use serde_json::Value;

use crate::config::file::{FlagValue, Key, Kind, Val};
use crate::config::keys::{COUNTED_FLAG, KEYS, SECTION_TITLES, SHORT_FLAGS, tuning_section};

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

/// One key's flag, as the command line spells it.
pub struct FlagSpec {
    pub key: &'static Key,
    /// The key, dotted (`llm.concurrency`).
    pub dotted: String,
    /// The long name without its dashes (`llm-concurrency`).
    pub long: String,
    /// A boolean's off switch (`no-split`).
    pub negated: Option<String>,
    pub short: Option<char>,
    /// Its `--help` heading (`LLM [llm]`).
    pub heading: String,
    /// A value flag's placeholder (`n`, `off|report|names`).
    pub value_name: String,
}

impl FlagSpec {
    fn of(key: &'static Key) -> FlagSpec {
        let long = key.flag_name().trim_start_matches("--").to_string();
        let dotted = key.dotted();
        FlagSpec {
            negated: matches!(key.kind, Kind::Bool).then(|| format!("no-{long}")),
            short: SHORT_FLAGS
                .iter()
                .find(|(d, _)| *d == dotted)
                .map(|(_, c)| *c),
            long,
            dotted,
            heading: heading(key.section),
            value_name: value_name(key),
            key,
        }
    }
}

/// Every key's flag, in registry order — built once.
pub static FLAGS: LazyLock<Vec<FlagSpec>> =
    LazyLock::new(|| KEYS.iter().map(FlagSpec::of).collect());

/// The flag `--config` is the one flag that is not a key: it names the file.
pub const CONFIG_FLAG: &str = "config";

/// What a value flag's placeholder says (`<n>`, `<off|report|names>`).
fn value_name(key: &Key) -> String {
    match (&key.kind, key.dotted().as_str()) {
        (_, "library.index") => "dir|id".to_string(),
        (_, "matching.prior_version") => "folder|file".to_string(),
        (_, "llm.model") => "model".to_string(),
        (Kind::Url, _) => "url".to_string(),
        (Kind::Int { .. }, _) => "n".to_string(),
        (Kind::Float { .. }, _) => "x".to_string(),
        (Kind::List(_), _) => "a,b,...".to_string(),
        (Kind::Choice(names), _) => {
            let joined = names().join("|");
            if joined.len() <= 40 {
                joined
            } else {
                "name".to_string()
            }
        }
        (_, d) if d.ends_with("_dir") || d.ends_with("index_store") => "dir".to_string(),
        (_, "diagnostics.dump_artifacts" | "diagnostics.rename_ledger") => "dir".to_string(),
        _ => "path".to_string(),
    }
}

/// The doc's first sentence, at most ~100 characters: the `-h` line.
fn summary(doc: &str) -> String {
    let first = doc.find(". ").map_or(doc, |i| &doc[..=i]);
    if first.chars().count() <= 100 {
        return first.trim_end_matches('.').to_string();
    }
    let cut: String = first.chars().take(97).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}...")
}

/// What `--help` adds after the doc: the default and the config key.
fn footnote(spec: &FlagSpec) -> String {
    let key = spec.key;
    let default = match ((key.default)(), &spec.negated) {
        (Some(Val::Bool(b)), Some(no)) => {
            format!(
                "default: {}; --{no} turns it off",
                if b { "on" } else { "off" }
            )
        }
        (Some(v), _) => format!("default: {}", v.render()),
        (None, _) => format!("unset: {}", key.unset),
    };
    format!("[{default}] [config: {}]", key.dotted())
}

fn heading(section: &str) -> String {
    let title = SECTION_TITLES
        .iter()
        .find(|(s, _)| *s == section)
        .map_or(section, |(_, t)| *t);
    format!("{title} [{section}]")
}

fn flag_arg(spec: &'static FlagSpec) -> Arg {
    let key = spec.key;
    let mut arg = Arg::new(spec.long.as_str())
        .long(spec.long.as_str())
        .help(summary(key.doc))
        .long_help(format!("{}\n{}", key.doc, footnote(spec)))
        .help_heading(spec.heading.as_str())
        .hide_short_help(tuning_section(key.section));
    if let Some(c) = spec.short {
        arg = arg.short(c);
    }
    match key.kind {
        Kind::Bool => arg.action(ArgAction::SetTrue).conflicts_with(
            spec.negated
                .as_deref()
                .expect("a boolean has an off switch"),
        ),
        _ if key.dotted() == COUNTED_FLAG => arg.action(ArgAction::Count),
        _ => arg
            .action(ArgAction::Set)
            .value_name(spec.value_name.as_str()),
    }
}

/// A boolean's `--no-x`: listed by its `--x` line, not on its own.
fn negated_arg(spec: &'static FlagSpec) -> Option<Arg> {
    let no = spec.negated.as_deref()?;
    Some(Arg::new(no).long(no).action(ArgAction::SetTrue).hide(true))
}

fn pipeline_command() -> clap::Command {
    // The version is package.json's, read at run time (the crate's own is
    // 0.0.0); clap wants it 'static, and there is one per process.
    static VERSION: LazyLock<String> = LazyLock::new(package_version);
    // (`default().name`, not `new`: a clap command, not a subprocess — the
    // concurrency census reads `Command::new` as a spawn.)
    let mut cmd = clap::Command::default()
        .name("humanify")
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
    for spec in FLAGS.iter() {
        cmd = cmd.arg(flag_arg(spec));
        if let Some(no) = negated_arg(spec) {
            cmd = cmd.arg(no);
        }
    }
    cmd
}

/// A flag's text as the key's value: typed by the key's kind (a range is
/// checked later, by the resolver, under the flag's name).
fn typed_flag(spec: &FlagSpec, raw: &str) -> Result<Val, String> {
    let bad = |what: &str| {
        format!(
            "error: invalid value '{raw}' for '--{} <{}>': {what}\n",
            spec.long, spec.value_name
        )
    };
    match spec.key.kind {
        Kind::Int { .. } => raw
            .parse::<u64>()
            .map(Val::Int)
            .map_err(|_| bad("a non-negative integer is expected")),
        Kind::Float { .. } => raw
            .parse::<f64>()
            .map(Val::Float)
            .map_err(|_| bad("a number is expected")),
        Kind::List(_) => Ok(Val::List(
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        _ => Ok(Val::Text(raw.to_string())),
    }
}

/// Every flag given, as (key, value).
fn flag_values(m: &ArgMatches) -> Result<Vec<FlagValue>, String> {
    let mut out = Vec::new();
    for spec in FLAGS.iter() {
        let dotted: &'static str = spec.dotted.as_str();
        let value = match &spec.negated {
            Some(no) if m.get_flag(no) => Some(Val::Bool(false)),
            Some(_) => m.get_flag(&spec.long).then_some(Val::Bool(true)),
            None if spec.dotted == COUNTED_FLAG => {
                let n = m.get_count(&spec.long);
                (n > 0).then(|| Val::Int(u64::from(n)))
            }
            None => match m.get_one::<String>(&spec.long) {
                Some(raw) => Some(typed_flag(spec, raw)?),
                None => None,
            },
        };
        if let Some(v) = value {
            out.push((dotted, v));
        }
    }
    Ok(out)
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
    args.iter().take_while(|a| *a != "--").find_map(|arg| {
        let name = arg.split_once('=').map_or(arg.as_str(), |(n, _)| n);
        let retired = RETIRED_FLAGS
            .iter()
            .find(|(v, flag, _)| *v == verb && *flag == name)
            .map(|(_, flag, note)| (*flag, note.to_string()));
        let renamed = || {
            RENAMED_FLAGS
                .iter()
                .find(|(flag, _)| verb.is_none() && *flag == name)
                .map(|(flag, new)| (*flag, format!("renamed: use {new}")))
        };
        retired
            .or_else(renamed)
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
                 argument (`-o out`, not `-oout`), and only -v repeats (`-vv`)\n"
            )
        })
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
    let full = std::iter::once("humanify".to_string()).chain(argv.iter().cloned());
    match pipeline_command().try_get_matches_from(full) {
        Ok(m) => match flag_values(&m) {
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
