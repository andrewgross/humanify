//! The pipeline program's command surface — TS: `src/index.ts` (program
//! name, description, version, positional options), `src/cli.ts`
//! (help-after-error), `configureUnifiedCommand` (unified.ts:1469-1628) and
//! `configureEnvReadsCommand` (env-reads.ts:26-59).
//!
//! Every flag string and default below is the TS literal, and the surface
//! gate (`surface_test.rs`) compares them and a parse corpus against the
//! real commander program the TS recorded. The DESCRIPTIONS are the
//! binary's own since the cutover: the rendered help (which every usage
//! error also prints) is pinned by a golden of this program's help,
//! test/golden/help/<command>.txt, not by the TS's wording.

use serde_json::Value;

use crate::commander::CliCommand;
use crate::util::{DEFAULT_CONCURRENCY, DEFAULT_LLM_TIMEOUT_MS};
use humanify_model::detection::{SELECTABLE_BUNDLERS, SELECTABLE_MINIFIERS};

/// Rust-only options: migration scaffolding the TS program does not have.
/// Hidden from help, and named here so the surface gate reports them
/// instead of passing over them.
///
/// NONE are left: the program surface is exactly the TS's. The three TS
/// inputs are gone — `--beautified-input` and `--ts-library-functions`
/// (WP5.6d: the binary formats natively and carries the library
/// classification itself) and `--inject-ts-hashes` (WP5.6e, 2026-09-25:
/// the structuralSignature exemption ended; the Rust hashes are the only
/// hashes, and a TS-era prior is re-keyed or refused loudly).
pub const RUST_ONLY_OPTIONS: &[&str] = &["--fast"];

/// package.json's version — the single source commander's `-V` prints.
pub fn package_version() -> String {
    let pkg: Value =
        serde_json::from_str(include_str!("../../../package.json")).expect("package.json is JSON");
    pkg["version"]
        .as_str()
        .expect("package.json has a version")
        .to_string()
}

fn s(v: &str) -> Option<Value> {
    Some(Value::String(v.to_string()))
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

/// The whole program: the pipeline command at the root, `env-reads` below.
pub fn program() -> CliCommand {
    CliCommand::new("humanify")
        .description("Unminify JavaScript using an OpenAI-compatible API")
        .version(&package_version())
        .enable_positional_options()
        .argument("<input>", "The input minified JavaScript file")
        .option(
            "--endpoint <url>",
            "OpenAI-compatible API endpoint",
            s("https://api.openai.com/v1"),
        )
        .option(
            "--api-key <key>",
            "API key (flag > HUMANIFY_API_KEY > OPENAI_API_KEY env vars)",
            None,
        )
        .option("-m, --model <model>", "Model identifier", s("gpt-4o-mini"))
        .option("-o, --output-dir <output>", "Output directory", s("output"))
        .counter_option(
            "-v, --verbose",
            "Increase verbosity (-v for info, -vv for debug)",
            0,
        )
        .option(
            "-c, --concurrency <n>",
            "Max concurrent function-lane LLM requests. Module-lane size is set \
             separately via --module-concurrency; the global in-flight cap is their sum.",
            s(&DEFAULT_CONCURRENCY.to_string()),
        )
        .option(
            "--module-concurrency <n>",
            "Max concurrent module-lane LLM requests (default derived from -c)",
            None,
        )
        .option("--max-tokens <n>", "Per-request completion token budget", None)
        .option(
            "--context-tokens <n>",
            "Model context window in tokens; sizes the batched split-naming \
             prompts (default: 32768)",
            None,
        )
        .option(
            "--disable <passes>",
            "Comma-separated pass switches to turn OFF for ablation (unknown \
             names are fatal and list the valid set)",
            None,
        )
        .option(
            "--probe <probes>",
            "Comma-separated instrumentation probes to turn ON (unknown names \
             are fatal and list the valid set)",
            None,
        )
        .option(
            "--retries <n>",
            "Number of retry attempts for failed API calls",
            s("3"),
        )
        .option(
            "--timeout <ms>",
            "LLM request timeout in milliseconds",
            s(&DEFAULT_LLM_TIMEOUT_MS.to_string()),
        )
        .option(
            "--llm-cache <dir>",
            "Cache LLM responses on disk keyed by request content. \
             Repeated prompts become deterministic \
             across sessions and reruns are nearly free — the serving-drift \
             countermeasure the 034 eval README describes.",
            None,
        )
        .option(
            "--reasoning-effort <level>",
            "Reasoning effort for reasoning models: low, medium, or high \
             (no env fallback; default: server-side default). \
             'low' is ~8x faster on gpt-oss at equal name quality; only set it for \
             reasoning models — non-reasoning models (e.g. gpt-4o-mini) reject it.",
            None,
        )
        // Two declarations, as the TS now has (16-findings-queue #19: the
        // old one-option pair made both spellings set false).
        .option(
            "--skip-libraries",
            "Skip library code instead of processing it with the LLM (default: true)",
            None,
        )
        .option("--no-skip-libraries", "Process library code with the LLM", None)
        .option(
            "--log-file <path>",
            "Write debug logs to file (implies -vv)",
            None,
        )
        .option(
            "--diagnostics <path>",
            "Write detailed rename diagnostics to JSON file",
            None,
        )
        .option(
            "--stats-json <path>",
            "Write the deterministic match/rename breakdown as compact JSON \
             (coverage + transfer stats + prior-match counts) for the eval harness",
            None,
        )
        .option(
            "--dump-artifacts <dir>",
            "Write the span-keyed decision-record dump to this directory \
             (instrumentation). Inert by construction: the run's decisions are \
             unchanged (proven by neutrality).",
            None,
        )
        .option(
            "--bundler <type>",
            &format!("Force bundler type ({})", names(&SELECTABLE_BUNDLERS)),
            None,
        )
        .option(
            "--minifier <type>",
            &format!("Force minifier type ({})", names(&SELECTABLE_MINIFIERS)),
            None,
        )
        .option(
            "--batch-size <n>",
            "Identifiers per LLM batch (default: 10)",
            None,
        )
        .option(
            "--max-retries <n>",
            "Per-identifier LLM call limit, initial + retries (default: 2; further conflicts resolve by suffixing)",
            None,
        )
        .option(
            "--max-free-retries <n>",
            "Cross-lane collision retry limit (default: 100)",
            None,
        )
        .option(
            "--lane-threshold <n>",
            "Min bindings to enable parallel lanes (default: 25)",
            None,
        )
        .option(
            "--split",
            "Split output into a multi-file tree (src/ + vendor/ + run scaffold), \
             emitted as a runnable CommonJS module graph by default",
            None,
        )
        .option(
            "--prior-version <path>",
            "Path to a prior humanified file for cross-version rename reuse",
            None,
        )
        .option(
            "--reconcile-prior-diff",
            "After generation, snap rename-noise diff hunks back to the prior version's names (default with --prior-version)",
            None,
        )
        .option(
            "--no-reconcile-prior-diff",
            "Disable the prior-diff reconcile pass",
            None,
        )
        .option(
            "--naming-floor",
            "Close minted-token coverage gaps deterministically (class/function-expression inner-id derivation + decoration retry; default on)",
            None,
        )
        .option(
            "--no-naming-floor",
            "Disable the deterministic naming floor",
            None,
        )
        .option(
            "--naming-floor-sweep",
            "LLM-name the minted survivors the naming floor cannot derive (params/decls/vars; default on). \
             Prior-aware with a prior version: prior names transfer deterministically and the LLM names only the residue",
            None,
        )
        .option(
            "--no-naming-floor-sweep",
            "Disable the LLM sweep of minted survivors",
            None,
        )
        .option(
            "--split-ledger <path>",
            "Prior split ledger for cross-release file-assignment inheritance \
             (default: auto-discovered next to --prior-version)",
            None,
        )
        .option(
            "--split-pure",
            "Emit the byte-exact review tree instead of the runnable CommonJS \
             module graph (the --split default). Requires --split",
            None,
        )
        .option(
            "--rename-ledger <dir>",
            "Write a replayable rename ledger (every rename keyed by byte position) \
             + source snapshot + a standalone apply.mjs, so the LLM-rename output \
             can be reproduced without re-running the model",
            None,
        )
        .option(
            "--fast",
            "Post-parity performance mode: pipelined LLM dispatch and parallel stages. \
             Deterministic (same input + same answers = same bytes), but not \
             byte-equal to the default parity-faithful path",
            None,
        )
        .option(
            "--profile <path>",
            "Write performance profile to JSON file (Chrome Trace Event format, viewable at chrome://tracing or ui.perfetto.dev)",
            None,
        )
        .subcommand(env_reads_command())
}

/// The help `command` prints for `--help` and after a usage error:
/// `"humanify"` names the pipeline program, anything else a subcommand.
pub fn help_text(command: &str) -> String {
    let root = program();
    if command == root.name {
        return root.help_information(&[]);
    }
    root.commands
        .iter()
        .find(|c| c.name == command)
        .unwrap_or_else(|| panic!("the program has no command {command}"))
        .help_information(&[root.name.as_str()])
}

/// A recorded usage-error envelope with each `{{help:<command>}}`
/// placeholder replaced by that command's current help: the recorded
/// corpora (test/parity/wpb4-cli-surface.json, wpb4-scenarios.json) keep
/// commander's error lines, while the help's wording is the binary's own.
pub fn expand_help_placeholders(text: &str) -> String {
    let root = program();
    let mut out = text.to_string();
    for name in std::iter::once(&root.name).chain(root.commands.iter().map(|c| &c.name)) {
        let placeholder = format!("{{{{help:{name}}}}}");
        if out.contains(&placeholder) {
            out = out.replace(&placeholder, &help_text(name));
        }
    }
    out
}

/// `configureEnvReadsCommand`.
fn env_reads_command() -> CliCommand {
    CliCommand::new("env-reads")
        .description("Inventory process.env / Bun.env / import.meta.env reads in a file or tree")
        .argument("<path>", "A JS file, or a directory of JS files to scan")
        .option("--markdown", "Emit the report as Markdown", None)
        .option(
            "-o, --output <file>",
            "Write the report to a file instead of stdout",
            None,
        )
}
