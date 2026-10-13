//! THE key registry: every setting the run reads, declared once — section,
//! name, type and range, compiled-in default, meaning, and the flag for
//! the few operational settings that are also flags. `Config::build` reads
//! every value through it; `humanify.example.toml` is rendered from it.

use humanify_core::fast::Lever;
use humanify_core::naming::waves::batch::WaveTunables;
use humanify_model::detection::{SELECTABLE_BUNDLERS, SELECTABLE_MINIFIERS};
use humanify_model::llm::DEFAULT_LLM_TIMEOUT_MS;

use super::file::{Key, Kind, Val};
use super::{
    DEFAULT_CONCURRENCY, DEFAULT_CONTEXT_TOKENS, DEFAULT_ENDPOINT, DEFAULT_MODEL,
    DEFAULT_MODULE_CONCURRENCY, DEFAULT_OUTPUT_DIR, default_retries,
};
use crate::pipeline_config::enum_name;

const ANY: u64 = u64::MAX;
/// The ceiling of a setting held as a u32.
const U32: u64 = u32::MAX as u64;

fn none() -> Option<Val> {
    None
}
fn off() -> Option<Val> {
    Some(Val::Bool(false))
}
fn on() -> Option<Val> {
    Some(Val::Bool(true))
}
fn empty_list() -> Option<Val> {
    Some(Val::List(Vec::new()))
}
fn text(s: &str) -> Option<Val> {
    Some(Val::Text(s.to_string()))
}
fn int(n: u64) -> Option<Val> {
    Some(Val::Int(n))
}

fn tunables() -> WaveTunables {
    WaveTunables::default()
}

fn efforts() -> Vec<String> {
    ["low", "medium", "high"].map(String::from).to_vec()
}
fn schedules() -> Vec<String> {
    ["relaxed", "sequential"].map(String::from).to_vec()
}
fn levers() -> Vec<String> {
    Lever::ALL.iter().map(|l| l.name().to_string()).collect()
}
fn bundlers() -> Vec<String> {
    SELECTABLE_BUNDLERS.iter().map(|b| enum_name(*b)).collect()
}
fn minifiers() -> Vec<String> {
    SELECTABLE_MINIFIERS.iter().map(|m| enum_name(*m)).collect()
}

/// A path (or other named value) with no default.
const fn unset_text(
    section: &'static str,
    name: &'static str,
    unset: &'static str,
    doc: &'static str,
) -> Key {
    Key {
        section,
        name,
        kind: Kind::Text,
        flag: None,
        default: none,
        unset,
        doc,
    }
}

/// Every setting, in the order the example file lists them.
pub static KEYS: &[Key] = &[
    // [io]
    Key {
        section: "io",
        name: "output_dir",
        kind: Kind::Text,
        flag: Some("--output-dir"),
        default: || text(DEFAULT_OUTPUT_DIR),
        unset: "",
        doc: "Output directory.",
    },
    // [llm]
    Key {
        section: "llm",
        name: "endpoint",
        kind: Kind::Url,
        flag: Some("--endpoint"),
        default: || text(DEFAULT_ENDPOINT),
        unset: "",
        doc: "OpenAI-compatible API endpoint (the key: HUMANIFY_API_KEY).",
    },
    Key {
        section: "llm",
        name: "model",
        kind: Kind::Text,
        flag: Some("--model"),
        default: || text(DEFAULT_MODEL),
        unset: "",
        doc: "Model identifier.",
    },
    Key {
        section: "llm",
        name: "timeout_ms",
        kind: Kind::Int { min: 1, max: ANY },
        flag: None,
        default: || int(DEFAULT_LLM_TIMEOUT_MS),
        unset: "",
        doc: "LLM request timeout, milliseconds.",
    },
    Key {
        section: "llm",
        name: "retries",
        kind: Kind::Int { min: 0, max: U32 },
        flag: None,
        default: || int(u64::from(default_retries())),
        unset: "",
        doc: "Retry attempts for a failed API call.",
    },
    Key {
        section: "llm",
        name: "concurrency",
        kind: Kind::Int { min: 1, max: U32 },
        flag: None,
        default: || int(u64::from(DEFAULT_CONCURRENCY)),
        unset: "",
        doc: "Max concurrent function-lane LLM requests (the in-flight cap is this plus \
              module_concurrency).",
    },
    Key {
        section: "llm",
        name: "module_concurrency",
        kind: Kind::Int { min: 1, max: U32 },
        flag: None,
        default: || int(u64::from(DEFAULT_MODULE_CONCURRENCY)),
        unset: "",
        doc: "Max concurrent module-lane LLM requests.",
    },
    Key {
        section: "llm",
        name: "max_tokens",
        kind: Kind::Int { min: 1, max: ANY },
        flag: None,
        default: none,
        unset: "requests carry 6000, and the LLM cache key records \"unset\" — a different \
                key from an explicit 6000",
        doc: "Per-request completion token budget.",
    },
    Key {
        section: "llm",
        name: "context_tokens",
        kind: Kind::Int { min: 1, max: ANY },
        flag: None,
        default: || int(DEFAULT_CONTEXT_TOKENS),
        unset: "",
        doc: "Model context window, tokens; sizes the batched split-naming prompts.",
    },
    Key {
        section: "llm",
        name: "reasoning_effort",
        kind: Kind::Choice(efforts),
        flag: None,
        default: none,
        unset: "the server's default; only set it for reasoning models",
        doc: "Reasoning effort: low, medium or high ('low' is ~8x faster on gpt-oss).",
    },
    unset_text(
        "llm",
        "cache_dir",
        "no cache: every prompt is live",
        "Cache LLM responses on disk, keyed by request content (iteration only, never a \
         gate run).",
    ),
    // [matching]
    Key {
        section: "matching",
        name: "prior_version",
        kind: Kind::Text,
        flag: Some("--prior-version"),
        default: none,
        unset: "a fresh run, nothing to reuse",
        doc: "A prior humanified bundle, for cross-version rename reuse.",
    },
    Key {
        section: "matching",
        name: "reconcile_prior_diff",
        kind: Kind::Bool,
        flag: None,
        default: on,
        unset: "",
        doc: "With a prior: snap rename-noise diff hunks back to the prior's names.",
    },
    // [naming]
    Key {
        section: "naming",
        name: "schedule",
        kind: Kind::Choice(schedules),
        flag: None,
        default: || text("relaxed"),
        unset: "",
        doc: "\"relaxed\" (both relaxed levers on) or \"sequential\" (the pre-2026-09-28 \
              default's bytes at the same batch_size).",
    },
    Key {
        section: "naming",
        name: "relaxed_levers",
        kind: Kind::List(Some(levers)),
        flag: None,
        default: none,
        unset: "every lever",
        doc: "Lever-sizing knob: the relaxed levers to turn on (relaxed schedule only).",
    },
    Key {
        section: "naming",
        name: "batch_size",
        kind: Kind::Int { min: 1, max: ANY },
        flag: None,
        default: || int(tunables().batch_size as u64),
        unset: "",
        doc: "Identifiers per LLM batch.",
    },
    Key {
        section: "naming",
        name: "max_retries",
        kind: Kind::Int { min: 1, max: U32 },
        flag: None,
        default: || int(u64::from(tunables().max_retries)),
        unset: "",
        doc: "Per-identifier LLM call limit, initial + retries.",
    },
    Key {
        section: "naming",
        name: "rename_retries",
        kind: Kind::Int { min: 0, max: ANY },
        flag: None,
        default: || int(tunables().reask_limit as u64),
        unset: "",
        doc: "Name-conflict re-asks per rejected suggestion, each disclosing every failed \
              one.",
    },
    Key {
        section: "naming",
        name: "max_free_retries",
        kind: Kind::Int { min: 0, max: U32 },
        flag: None,
        default: || tunables().max_free_retries.map(|n| Val::Int(u64::from(n))),
        unset: "the processor's own rule (100)",
        doc: "Cross-lane collision retry limit.",
    },
    Key {
        section: "naming",
        name: "lane_threshold",
        kind: Kind::Int { min: 0, max: ANY },
        flag: None,
        default: || int(tunables().lane_threshold as u64),
        unset: "",
        doc: "Min bindings to enable parallel lanes.",
    },
    Key {
        section: "naming",
        name: "naming_floor",
        kind: Kind::Bool,
        flag: None,
        default: on,
        unset: "",
        doc: "The deterministic naming floor (closes minted-token gaps).",
    },
    Key {
        section: "naming",
        name: "naming_floor_sweep",
        kind: Kind::Bool,
        flag: None,
        default: on,
        unset: "",
        doc: "The LLM sweep of minted survivors (needs naming_floor).",
    },
    // [unpack]
    Key {
        section: "unpack",
        name: "bundler",
        kind: Kind::Choice(bundlers),
        flag: None,
        default: none,
        unset: "detected",
        doc: "Force the bundler verdict.",
    },
    Key {
        section: "unpack",
        name: "minifier",
        kind: Kind::Choice(minifiers),
        flag: None,
        default: none,
        unset: "detected",
        doc: "Force the minifier verdict.",
    },
    // [split]
    Key {
        section: "split",
        name: "enabled",
        kind: Kind::Bool,
        flag: None,
        default: off,
        unset: "",
        doc: "Split the output into a multi-file tree (src/ + vendor/ + run scaffold).",
    },
    Key {
        section: "split",
        name: "pure",
        kind: Kind::Bool,
        flag: None,
        default: off,
        unset: "",
        doc: "Emit the byte-exact review tree instead of the runnable module graph (needs \
              enabled).",
    },
    unset_text(
        "split",
        "ledger",
        "auto-discovered next to the prior",
        "A prior split ledger, for cross-release file-assignment inheritance (needs \
         enabled).",
    ),
    // [switches]
    Key {
        section: "switches",
        name: "disable",
        kind: Kind::List(None),
        flag: None,
        default: empty_list,
        unset: "",
        doc: "Ablation: pass switches to turn OFF (unknown names are refused, listing the \
              valid set).",
    },
    Key {
        section: "switches",
        name: "probe",
        kind: Kind::List(None),
        flag: None,
        default: empty_list,
        unset: "",
        doc: "Instrumentation probes to turn ON.",
    },
    // [diagnostics]
    Key {
        section: "diagnostics",
        name: "verbosity",
        kind: Kind::Int { min: 0, max: 2 },
        flag: Some("-v"),
        default: || int(0),
        unset: "",
        doc: "0, 1 (info) or 2 (debug); -v / -vv on the command line.",
    },
    unset_text(
        "diagnostics",
        "log_file",
        "no log file",
        "Write debug logs to this file (implies verbosity 2).",
    ),
    unset_text(
        "diagnostics",
        "diagnostics_file",
        "not written",
        "Write detailed rename diagnostics to this JSON file.",
    ),
    unset_text(
        "diagnostics",
        "stats_json",
        "not written",
        "Write the eval harness's match/rename breakdown (and this config) as JSON.",
    ),
    unset_text(
        "diagnostics",
        "dump_artifacts",
        "not written",
        "Write the span-keyed decision-record dump to this directory.",
    ),
    unset_text(
        "diagnostics",
        "dump_asks",
        "not written",
        "Write the reason-labelled ask log (one JSONL row per LLM ask) to this path.",
    ),
    unset_text(
        "diagnostics",
        "rename_ledger",
        "not written",
        "Write a replayable rename ledger + source snapshot + apply.mjs to this directory.",
    ),
    // [perf]
    unset_text(
        "perf",
        "perf_trace",
        "not written",
        "Write a Chrome Trace Event performance trace to this JSON file.",
    ),
    unset_text(
        "perf",
        "simulate_llm_latency",
        "not written",
        "Price this run's LLM schedule on a virtual clock; write the simulated wall time \
         here.",
    ),
];
