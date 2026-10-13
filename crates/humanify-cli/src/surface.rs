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
//! - `--skip-libraries` with `--no-skip-libraries` is a conflict (both
//!   still parse alone, until the library decision deletes them).
//!
//! Every launch the harness makes parses to the settings the commander
//! port gave it (launch_parity_test.rs, captured from the port at
//! 587e211f). Usage errors exit 1, never clap's default 2: the pipeline
//! keeps exit 2 out of its contract. The rendered help is pinned by
//! test/golden/help/humanify.txt.

use clap::{CommandFactory, FromArgMatches, Parser};
use serde_json::Value;

use crate::config::{
    API_KEY_VAR, DEFAULT_CONCURRENCY, DEFAULT_CONTEXT_TOKENS, DEFAULT_ENDPOINT, DEFAULT_MODEL,
    DEFAULT_MODULE_CONCURRENCY, DEFAULT_OUTPUT_DIR, ReasoningEffort, default_retries,
};
use humanify_model::detection::{SELECTABLE_BUNDLERS, SELECTABLE_MINIFIERS};
use humanify_model::llm::{DEFAULT_LLM_TIMEOUT_MS, DEFAULT_MAX_TOKENS};

/// package.json's version — the single source `-V` prints.
pub fn package_version() -> String {
    let pkg: Value =
        serde_json::from_str(include_str!("../../../package.json")).expect("package.json is JSON");
    pkg["version"]
        .as_str()
        .expect("package.json has a version")
        .to_string()
}

fn names<T: serde::Serialize>(values: &[T]) -> String {
    values
        .iter()
        .map(|v| {
            serde_json::to_value(v)
                .ok()
                .and_then(|j| j.as_str().map(str::to_string))
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(", ")
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

fn concurrency_help() -> String {
    format!(
        "Max concurrent function-lane LLM requests (default: {DEFAULT_CONCURRENCY}). \
         Module-lane size is set separately via --module-concurrency; the global in-flight \
         cap is their sum."
    )
}

fn module_concurrency_help() -> String {
    format!("Max concurrent module-lane LLM requests (default: {DEFAULT_MODULE_CONCURRENCY})")
}

fn max_tokens_help() -> String {
    format!("Per-request completion token budget (default: {DEFAULT_MAX_TOKENS})")
}

fn context_tokens_help() -> String {
    format!(
        "Model context window in tokens; sizes the batched split-naming prompts (default: \
         {DEFAULT_CONTEXT_TOKENS}, the local measurement model's window)"
    )
}

fn retries_help() -> String {
    format!(
        "Number of retry attempts for failed API calls (default: {})",
        default_retries()
    )
}

fn timeout_help() -> String {
    format!("LLM request timeout in milliseconds (default: {DEFAULT_LLM_TIMEOUT_MS})")
}

fn bundler_help() -> String {
    format!("Force bundler type ({})", names(&SELECTABLE_BUNDLERS))
}

fn minifier_help() -> String {
    format!("Force minifier type ({})", names(&SELECTABLE_MINIFIERS))
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
/// No default lives here — `crate::config` owns every default, resolves
/// and validates the options ONCE into the run's `Config`, and nothing
/// else reads these fields.
#[derive(clap::Args, Clone, Debug, Default)]
pub struct CommandOptions {
    #[arg(long, value_name = "url", help = endpoint_help())]
    pub endpoint: Option<String>,
    #[arg(short = 'm', long, value_name = "model", help = model_help())]
    pub model: Option<String>,
    #[arg(short = 'o', long, value_name = "output", help = output_help())]
    pub output_dir: Option<String>,
    #[arg(
        short = 'v',
        long,
        action = clap::ArgAction::Count,
        help = "Increase verbosity (-v for info, -vv for debug)"
    )]
    pub verbose: u8,
    #[arg(short = 'c', long, value_name = "n", help = concurrency_help())]
    pub concurrency: Option<u32>,
    #[arg(long, value_name = "n", help = module_concurrency_help())]
    pub module_concurrency: Option<u32>,
    #[arg(long, value_name = "n", help = max_tokens_help())]
    pub max_tokens: Option<u64>,
    #[arg(long, value_name = "n", help = context_tokens_help())]
    pub context_tokens: Option<u64>,
    #[arg(
        long,
        value_name = "passes",
        help = "Comma-separated pass switches to turn OFF for ablation (unknown \
                names are fatal and list the valid set)"
    )]
    pub disable: Option<String>,
    #[arg(
        long,
        value_name = "probes",
        help = "Comma-separated instrumentation probes to turn ON (unknown names \
                are fatal and list the valid set)"
    )]
    pub probe: Option<String>,
    #[arg(long, value_name = "n", help = retries_help())]
    pub retries: Option<u32>,
    #[arg(long, value_name = "ms", help = timeout_help())]
    pub timeout: Option<u64>,
    #[arg(
        long,
        value_name = "dir",
        help = "Cache LLM responses on disk keyed by request content. Repeated \
                prompts become deterministic across sessions and reruns are nearly \
                free — it also pins answers against a server whose replies drift \
                between runs."
    )]
    pub llm_cache: Option<String>,
    #[arg(
        long,
        value_name = "level",
        help = "Reasoning effort for reasoning models: low, medium, or high (default: \
                server-side default). 'low' is ~8x faster on \
                gpt-oss at equal name quality; only set it for reasoning models — \
                non-reasoning models (e.g. gpt-4o-mini) reject it."
    )]
    pub reasoning_effort: Option<ReasoningEffort>,
    // The two library spellings stay until the library decision (config
    // sweep step 6) replaces both; together they are a conflict.
    #[arg(
        long,
        conflicts_with = "no_skip_libraries",
        help = "Skip library code instead of processing it with the LLM (the default)"
    )]
    pub skip_libraries: bool,
    #[arg(long, help = "Process library code with the LLM")]
    pub no_skip_libraries: bool,
    #[arg(
        long,
        value_name = "path",
        help = "Write debug logs to file (implies -vv)"
    )]
    pub log_file: Option<String>,
    #[arg(
        long,
        value_name = "path",
        help = "Write detailed rename diagnostics to JSON file"
    )]
    pub diagnostics: Option<String>,
    /// The library fingerprint index discovery looks statement units up in
    /// (report-only, with `--diagnostics`).
    #[arg(
        long,
        value_name = "dir",
        help = "Library fingerprint index for --diagnostics' libraryDiscovery \
                (report-only: names, moves and renames nothing)"
    )]
    pub library_index: Option<String>,
    #[arg(
        long,
        help = "List every claimed, vetoed, library-class and queued unit in \
                --diagnostics' libraryDiscovery (default: totals and packages)"
    )]
    pub library_discovery_units: bool,
    #[arg(
        long,
        value_name = "path",
        help = "Write the deterministic match/rename breakdown as compact JSON \
                (coverage + transfer stats + prior-match counts) for the eval harness"
    )]
    pub stats_json: Option<String>,
    #[arg(
        long,
        value_name = "dir",
        help = "Write the span-keyed decision-record dump to this directory \
                (instrumentation). Inert by construction: the run's decisions are \
                unchanged (proven by neutrality)."
    )]
    pub dump_artifacts: Option<String>,
    #[arg(
        long,
        value_name = "path",
        help = "Write the reason-labeled ask log (one JSONL row per LLM ask: scope, \
                identifiers, wave/round, WHY it was asked) to this path. Recording \
                only — no decision reads it; off unless passed."
    )]
    pub dump_asks: Option<String>,
    #[arg(long, value_name = "type", help = bundler_help())]
    pub bundler: Option<String>,
    #[arg(long, value_name = "type", help = minifier_help())]
    pub minifier: Option<String>,
    #[arg(
        long,
        value_name = "n",
        help = "Identifiers per LLM batch (default: 25)"
    )]
    pub batch_size: Option<usize>,
    #[arg(
        long,
        value_name = "n",
        help = "Per-identifier LLM call limit, initial + retries (default: 2; \
                further conflicts resolve by suffixing)"
    )]
    pub max_retries: Option<u32>,
    #[arg(
        long,
        value_name = "n",
        help = "Name-conflict re-ask limit: how many times a rejected rename \
                suggestion may be re-asked, each retry disclosing every failed \
                suggestion (default: 2). Independent of --max-retries — a \
                barrier/sweep collision re-ask consumes this budget, not the \
                in-window call limit"
    )]
    pub rename_retries: Option<usize>,
    #[arg(
        long,
        value_name = "n",
        help = "Cross-lane collision retry limit (default: 100)"
    )]
    pub max_free_retries: Option<u32>,
    #[arg(
        long,
        value_name = "n",
        help = "Min bindings to enable parallel lanes (default: 25)"
    )]
    pub lane_threshold: Option<usize>,
    #[arg(
        long,
        help = "Split output into a multi-file tree (src/ + vendor/ + run scaffold), \
                emitted as a runnable CommonJS module graph by default"
    )]
    pub split: bool,
    #[arg(
        long,
        value_name = "path",
        help = "Path to a prior humanified file for cross-version rename reuse"
    )]
    pub prior_version: Option<String>,
    #[arg(
        long,
        help = "Disable the prior-diff reconcile pass (on by default with \
                --prior-version: after generation, snap rename-noise diff hunks back \
                to the prior version's names)"
    )]
    pub no_reconcile_prior_diff: bool,
    #[arg(
        long,
        help = "Disable the deterministic naming floor (on by default: closes \
                minted-token coverage gaps by class/function-expression inner-id \
                derivation + decoration retry)"
    )]
    pub no_naming_floor: bool,
    #[arg(
        long,
        help = "Disable the LLM sweep of minted survivors (on by default: LLM-names \
                the params/decls/vars the naming floor cannot derive; prior names \
                transfer first)"
    )]
    pub no_naming_floor_sweep: bool,
    #[arg(
        long,
        value_name = "path",
        help = "Prior split ledger for cross-release file-assignment inheritance \
                (default: auto-discovered next to --prior-version)"
    )]
    pub split_ledger: Option<String>,
    #[arg(
        long,
        help = "Emit the byte-exact review tree instead of the runnable CommonJS \
                module graph (the --split default). Requires --split"
    )]
    pub split_pure: bool,
    #[arg(
        long,
        value_name = "dir",
        help = "Write a replayable rename ledger (every rename keyed by byte \
                position) + source snapshot + a standalone apply.mjs, so the \
                LLM-rename output can be reproduced without re-running the model"
    )]
    pub rename_ledger: Option<String>,
    #[arg(
        long,
        help = "The conservative naming schedule: the relaxed levers OFF (each naming \
                window waits for the previous one and for its own shadowed-binding \
                round), the byte-identical parallelization kept. The DEFAULT is the \
                relaxed schedule (both levers on); --sequential ships the \
                pre-2026-09-28 default's bytes at the same --batch-size — for \
                debugging and comparison"
    )]
    pub sequential: bool,
    // NOT in the help: a lever-sizing knob, not a user knob. None = every
    // lever.
    #[arg(
        long,
        value_name = "levers",
        hide = true,
        help = "Comma-separated subset of the relaxed levers to turn on (default: \
                every lever)"
    )]
    pub relaxed_levers: Option<String>,
    #[arg(
        long,
        value_name = "path",
        help = "Instrument: price this run's LLM schedule with cold per-call \
                latencies on a virtual clock and write the simulated LLM wall time \
                to <path> (JSON)"
    )]
    pub simulate_llm_latency: Option<String>,
    #[arg(
        long,
        value_name = "path",
        help = "Write a performance trace to JSON file (Chrome Trace Event format, \
                viewable at chrome://tracing or ui.perfetto.dev)"
    )]
    pub perf_trace: Option<String>,
}

/// Flags renamed or deleted by the config sweep (2026-10-13; no backwards
/// compatibility): `(verb, old spelling, what to say instead)`, `None` for
/// the pipeline program, `Some(verb)` for a clap stage verb (main.rs).
/// The old spelling is refused, naming its replacement, never aliased.
pub const RETIRED_FLAGS: &[(Option<&str>, &str, &str)] = &[
    (None, "--profile", "renamed: use --perf-trace"),
    (
        None,
        "--api-key",
        "removed: set HUMANIFY_API_KEY in the environment — a secret never goes on the \
         command line",
    ),
    (
        None,
        "--naming-floor",
        "removed: it restated the default (on); --no-naming-floor turns it off",
    ),
    (
        None,
        "--naming-floor-sweep",
        "removed: it restated the default (on); --no-naming-floor-sweep turns it off",
    ),
    (
        None,
        "--reconcile-prior-diff",
        "removed: it restated the default (on with --prior-version); \
         --no-reconcile-prior-diff turns it off",
    ),
    (Some("detect"), "--profile", "renamed: use --perf-trace"),
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
        RETIRED_FLAGS
            .iter()
            .find(|(v, flag, _)| *v == verb && *flag == name)
            .map(|(_, flag, note)| format!("error: unexpected argument '{flag}' ({note})\n"))
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
