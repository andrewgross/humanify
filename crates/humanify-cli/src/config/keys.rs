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
use humanify_core::libdetect::discovery::{
    BannerNames, DiscoveryOptions, TARGETED_MIN_NODES, TARGETED_SUPPORT, VetoSupport,
};
use humanify_core::libindex::lookup::LookupRules;

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
fn library_modes() -> Vec<String> {
    ["off", "report", "names"].map(String::from).to_vec()
}
fn unnamed_modes() -> Vec<String> {
    ["leave", "model"].map(String::from).to_vec()
}
fn veto_supports() -> Vec<String> {
    ["without-module", "all-rules"].map(String::from).to_vec()
}
/// The `library.discovery.veto_support` spelling of a rule.
pub fn veto_support_name(v: VetoSupport) -> &'static str {
    match v {
        VetoSupport::WithoutModule => "without-module",
        VetoSupport::AllRules => "all-rules",
    }
}
fn banner_names() -> Vec<String> {
    ["any", "known"].map(String::from).to_vec()
}
/// The `library.discovery.banner_names` spelling of a choice.
pub fn banner_names_name(v: BannerNames) -> &'static str {
    match v {
        BannerNames::Any => "any",
        BannerNames::Known => "known",
    }
}
fn rules() -> LookupRules {
    LookupRules::default()
}
fn discovery() -> DiscoveryOptions {
    DiscoveryOptions::default()
}
fn float(x: f64) -> Option<Val> {
    Some(Val::Float(x))
}

/// A library tuning integer (`[library.lookup]`, `[library.discovery]`).
const fn lib_int(
    section: &'static str,
    name: &'static str,
    min: u64,
    max: u64,
    default: fn() -> Option<Val>,
    doc: &'static str,
) -> Key {
    Key {
        section,
        name,
        kind: Kind::Int { min, max },
        flag: None,
        default,
        unset: "",
        doc,
    }
}

/// A library tuning number.
const fn lib_float(
    name: &'static str,
    max: f64,
    default: fn() -> Option<Val>,
    doc: &'static str,
) -> Key {
    Key {
        section: "library.lookup",
        name,
        kind: Kind::Float { min: 0.0, max },
        flag: None,
        default,
        unset: "",
        doc,
    }
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
        name: "code_window_lines",
        kind: Kind::Int { min: 1, max: ANY },
        flag: None,
        default: || int(tunables().code_window_lines as u64),
        unset: "",
        doc: "Most lines of a function the naming prompt shows (longer functions are \
              windowed around the names asked); lower it for a model with a small context \
              window.",
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
    // [library] (config sweep step 6, docs/library-matching-config.md §2)
    Key {
        section: "library",
        name: "matching",
        kind: Kind::Choice(library_modes),
        flag: Some("--library-matching"),
        default: || text("off"),
        unset: "",
        doc: "What library matching does: \"off\" (path/banner detection only), \"report\" \
              (also the index lookup + graph discovery, written to \
              .humanify/library-report.json; output bytes unchanged), \"names\" (refused \
              until an eval passes it).",
    },
    Key {
        section: "library",
        name: "index",
        kind: Kind::Text,
        flag: Some("--library-index"),
        default: none,
        unset: "no index; required unless matching is \"off\", refused with \"off\"",
        doc: "The library fingerprint index: its directory, or its id (resolved under \
              index_store).",
    },
    unset_text(
        "library",
        "index_store",
        "an index id cannot be resolved: pass the index directory",
        "Where index ids live (<index_store>/<id>/manifest.json).",
    ),
    Key {
        section: "library",
        name: "unnamed",
        kind: Kind::Choice(unnamed_modes),
        flag: None,
        default: || text("leave"),
        unset: "",
        doc: "Library code with no index name: \"leave\" (library files not model-named, \
              library functions in mixed files frozen with the <library>_<name> prefix) or \
              \"model\" (the model names everything).",
    },
    Key {
        section: "library",
        name: "report_units",
        kind: Kind::Bool,
        flag: None,
        default: off,
        unset: "",
        doc: "List every claimed, vetoed, library-class and queued unit in the report \
              (default: totals and packages).",
    },
    // [library.lookup]: the index lookup's rules (libindex::lookup::LookupRules).
    lib_int(
        "library.lookup",
        "min_nodes",
        1,
        U32,
        || int(u64::from(rules().min_nodes)),
        "AST nodes a statement needs to be looked up.",
    ),
    lib_int(
        "library.lookup",
        "min_content",
        0,
        U32,
        || int(u64::from(rules().min_content)),
        "Content tokens a whole statement needs to be looked up.",
    ),
    lib_int(
        "library.lookup",
        "min_function_content",
        0,
        U32,
        || int(u64::from(rules().min_function_content)),
        "Content tokens a function needs to be looked up.",
    ),
    lib_float(
        "cover",
        1.0,
        || float(rules().cover),
        "Fraction of a statement's units the index must cover.",
    ),
    lib_int(
        "library.lookup",
        "max_packages",
        1,
        U32,
        || int(rules().max_packages as u64),
        "Most packages a unit may match and still claim.",
    ),
    lib_int(
        "library.lookup",
        "support",
        0,
        U32,
        || int(rules().support as u64),
        "A claim under `strong` nodes needs another claim of its package within this many \
         units (0: none).",
    ),
    lib_int(
        "library.lookup",
        "strong",
        0,
        U32,
        || int(u64::from(rules().strong)),
        "Node count at which a claim needs no support.",
    ),
    Key {
        section: "library.lookup",
        name: "specificity",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(rules().specificity.is_some())),
        unset: "",
        doc: "Refuse an index module much bigger than the query's module (factor x + slack).",
    },
    lib_int(
        "library.lookup",
        "specificity_factor",
        1,
        U32,
        || int(rules().specificity.map_or(4, |s| s.0)),
        "Specificity: the index module may be at most this many times the query's.",
    ),
    lib_int(
        "library.lookup",
        "specificity_slack",
        0,
        ANY,
        || int(rules().specificity.map_or(4096, |s| s.1)),
        "Specificity: plus this many bytes.",
    ),
    Key {
        section: "library.lookup",
        name: "agreement",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(rules().agreement)),
        unset: "",
        doc: "Require the literal rule to agree (same package family); false = the \
              structural rule.",
    },
    lib_int(
        "library.lookup",
        "min_literals",
        0,
        U32,
        || int(rules().min_literals as u64),
        "Literals a module needs for the literal rule.",
    ),
    lib_float(
        "literal_score",
        f64::MAX,
        || float(rules().literal_score),
        "The literal rule's score floor (bits).",
    ),
    lib_float(
        "literal_cover",
        1.0,
        || float(rules().literal_cover),
        "The literal rule's cover floor.",
    ),
    lib_float(
        "literal_margin",
        f64::MAX,
        || float(rules().literal_margin),
        "The literal rule's margin over the runner-up (bits).",
    ),
    // [library.lookup.targeted]: the second round, on queued regions.
    lib_int(
        "library.lookup.targeted",
        "min_nodes",
        1,
        U32,
        || int(u64::from(TARGETED_MIN_NODES)),
        "The targeted round's node floor (the other rules are the lookup's).",
    ),
    lib_int(
        "library.lookup.targeted",
        "support",
        0,
        U32,
        || int(TARGETED_SUPPORT as u64),
        "The targeted round's neighbour support (0: the region's own calls are the context).",
    ),
    // [library.discovery]: the graph fixpoint (libdetect::discovery::DiscoveryOptions).
    lib_int(
        "library.discovery",
        "closure_floor",
        0,
        U32,
        || int(u64::from(discovery().closure_floor)),
        "Content floor a down-class start needs.",
    ),
    lib_int(
        "library.discovery",
        "queue_cap_bytes",
        0,
        U32,
        || int(u64::from(discovery().queue_cap)),
        "Largest region queued for a targeted index lookup, bytes.",
    ),
    lib_int(
        "library.discovery",
        "max_rounds",
        1,
        U32,
        || int(u64::from(discovery().max_rounds)),
        "Fixpoint round limit.",
    ),
    Key {
        section: "library.discovery",
        name: "veto_support",
        kind: Kind::Choice(veto_supports),
        flag: None,
        default: || text(veto_support_name(discovery().veto_support)),
        unset: "",
        doc: "What may support a seed against the isolation veto: \"without-module\" or \
              \"all-rules\" (the lab prototype's).",
    },
    Key {
        section: "library.discovery",
        name: "evidence_conflict",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().evidence_conflict)),
        unset: "",
        doc: "Refuse an index seed or a propagated claim when another package family's index \
              prints cover more of the unit than the claimed family's (exp102).",
    },
    Key {
        section: "library.discovery",
        name: "literal_conflict",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().literal_conflict)),
        unset: "",
        doc: "Refuse a propagated claim when the unit carries a long literal the index ties \
              only to other package families (exp102).",
    },
    Key {
        section: "library.discovery",
        name: "contested_cover",
        kind: Kind::Float { min: 0.0, max: 1.0 },
        flag: None,
        default: || float(discovery().contested_cover),
        unset: "",
        doc: "Refuse a propagated claim on a unit another package family's prints also cover \
              when the claimed family's prints cover less than this share of it (exp102; \
              0 = off).",
    },
    Key {
        section: "library.discovery",
        name: "banner_names",
        kind: Kind::Choice(banner_names),
        flag: None,
        default: || text(banner_names_name(discovery().banner_names)),
        unset: "",
        doc: "Which names a kept /*! banner may seed: \"any\" (the banner reader's) or \"known\" \
              (with an index, only a package the index knows, in npm's spelling; exp106).",
    },
    lib_int(
        "library.discovery",
        "helper_families",
        0,
        U32,
        || int(u64::from(discovery().helper_families)),
        "An index print at least this many package families hold is emitted helper code: a unit \
         such prints cover half of is no seed and no propagated claim (exp106; 0 = off).",
    ),
    Key {
        section: "library.discovery",
        name: "derived_families",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().derived_families)),
        unset: "",
        doc: "Join package families by the npm-metadata links (libindex/families-derived.json, \
              among the index's packages) wherever discovery asks \"one family?\" (exp106).",
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
