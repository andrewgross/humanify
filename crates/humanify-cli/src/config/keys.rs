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
    ["without-module", "all-rules", "shown"]
        .map(String::from)
        .to_vec()
}
/// The `library.discovery.veto_support` spelling of a rule.
pub fn veto_support_name(v: VetoSupport) -> &'static str {
    match v {
        VetoSupport::WithoutModule => "without-module",
        VetoSupport::AllRules => "all-rules",
        VetoSupport::Shown => "shown",
    }
}
/// `library.lookup.terser_share`'s default (exp127).
pub const TERSER_SHARE: f64 = 0.2;
fn terser_slices_choices() -> Vec<String> {
    humanify_core::libindex::recipe::TerserSlices::NAMES
        .map(String::from)
        .to_vec()
}
fn helper_prints_choices() -> Vec<String> {
    humanify_core::libdetect::discovery::HelperPrints::NAMES
        .map(String::from)
        .to_vec()
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
/// The shipped library tunables (exp109): the measured recommendation, not
/// the engine's all-off baseline.
fn rules() -> LookupRules {
    LookupRules::recommended()
}
fn discovery() -> DiscoveryOptions {
    DiscoveryOptions::recommended()
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
    Key {
        section: "library.lookup",
        name: "literal_margin_by_family",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(rules().literal_margin_by_family)),
        unset: "",
        doc: "Read the literal margin against the best file of another package family, not \
              another package: a sibling holding the same literals is no rival (exp106).",
    },
    Key {
        section: "library.lookup",
        name: "max_packages_by_family",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(rules().max_packages_by_family)),
        unset: "",
        doc: "Count a print's owners by package family, not package, against max_packages: \
              a scope's members or a fork and its origin are one identity, and a bigger \
              index's carriers of the same code no longer make it too common (exp125).",
    },
    Key {
        section: "library.lookup",
        name: "terser_slices",
        kind: Kind::Choice(terser_slices_choices),
        flag: None,
        default: || text(humanify_core::libindex::recipe::TerserSlices::default().name()),
        unset: "",
        doc: "Also read the index's terser-built slices (webpack, rollup) beside the bundle's \
              own: \"off\", \"union\" (every print) or \"fallback\" (a print the bundle's \
              slice does not hold) or \"auto\" (union for a bundle the terser slices alone hold \
              `terser_share` of) — terser-minified code matches terser rows (exp127).",
    },
    lib_float(
        "terser_share",
        1.0,
        || float(TERSER_SHARE),
        "terser_slices = \"auto\": the share of the bundle's looked-up whole-unit nodes the \
         terser-built slices alone must hold for the bundle to read them (exp127).",
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
        doc: "What may support a seed against the isolation veto: \"without-module\", \
              \"all-rules\" (the lab prototype's), or \"shown\" (exp144: without-module's \
              support, or all-rules' for a family the bundle shows without module fill).",
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
        name: "helper_prints",
        kind: Kind::Choice(helper_prints_choices),
        flag: None,
        default: || text(discovery().helper_prints.name()),
        unset: "",
        doc: "A unit whose whole print the index's helper table holds (a helper source's own \
              code: tslib, @babel/runtime, @swc/helpers) is no other package's index seed: \
              \"off\", \"seed\" (dropped; propagation may still reach it) or \"credit\" (the \
              helper package's seed when one helper family holds it; exp129).",
    },
    lib_int(
        "library.discovery",
        "payload_min_bytes",
        0,
        U32,
        || int(u64::from(discovery().payload_min_bytes)),
        "A propagated unit of at least this many bytes of its own (a module wrapper is not its \
         body), nine tenths string literal (a wasm blob, a data table), needs the claimed \
         family's own index evidence or a base64 payload naming it (exp106, exp113; 0 = off).",
    ),
    Key {
        section: "library.discovery",
        name: "specific_rivals",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().specific_rivals)),
        unset: "",
        doc: "Read another family's prints in evidence_conflict / contested_cover under the \
              lookup's specificity rule: a sponge's unspecific print is no rival (exp106).",
    },
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
    Key {
        section: "library.discovery",
        name: "context_presence",
        kind: Kind::Float {
            min: 0.0,
            max: 100.0,
        },
        flag: None,
        default: || float(discovery().context_presence),
        unset: "",
        doc: "A package every index seed of which sits in a module group its index holds that \
              code only in modules at least this many times the group's bytes (plus \
              context_presence_slack) is no seed: its copy lives in a dist that inlines the \
              code's origin (exp114; 0 = off).",
    },
    lib_int(
        "library.discovery",
        "context_presence_slack",
        0,
        u32::MAX as u64,
        || int(discovery().context_presence_slack),
        "The context-presence slack in bytes (exp114).",
    ),
    Key {
        section: "library.discovery",
        name: "shadow_ties",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().shadow_ties)),
        unset: "",
        doc: "A package whose every index seed is a tie with another family that has a seed of \
              its own in the bundle is that family's shadow: its tied seeds become the rival's, \
              the rest are dropped (exp114).",
    },
    Key {
        section: "library.discovery",
        name: "presence_fence",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().presence_fence)),
        unset: "",
        doc: "What context_presence / shadow_ties drop is claimed by no one, the dropped package \
              nor a rival propagating into it: the code is ambiguous between a shipper and its \
              origin, so name neither (exp114).",
    },
    lib_int(
        "library.discovery",
        "declared_max_bytes",
        0,
        U32,
        || int(u64::from(discovery().declared_max_bytes)),
        "A declared seed (an inlined package.json, a scoped error prefix) whose statement is \
         bigger than this many bytes is no seed: a manifest inside a huge statement names one of \
         its packages, not the statement (exp112; 0 = off).",
    ),
    Key {
        section: "library.discovery",
        name: "copy_presence",
        kind: Kind::Float { min: 0.0, max: 1.0 },
        flag: None,
        default: || float(discovery().copy_presence),
        unset: "",
        doc: "A package whose every index seed is a copy seed (another package holds at least this \
              share of the nodes its prints cover there, in index modules copy_presence_factor \
              times smaller: the origin its dist inlined) is no seed: a sponge, not shown in the \
              bundle by code of its own (exp121; 0 = off).",
    },
    lib_int(
        "library.discovery",
        "copy_presence_factor",
        0,
        U32,
        || int(discovery().copy_presence_factor),
        "How many times smaller the origin's index module must be than the sponge's for \
         copy_presence (exp121).",
    ),
    Key {
        section: "library.discovery",
        name: "copy_presence_origin",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().copy_presence_origin)),
        unset: "",
        doc: "What copy_presence drops becomes the seed of its origin (the package co-holding the \
              most of it in the smaller module) instead of no one's (exp121).",
    },
    Key {
        section: "library.discovery",
        name: "copy_presence_own",
        kind: Kind::Float { min: 0.0, max: 1.0 },
        flag: None,
        default: || float(discovery().copy_presence_own),
        unset: "",
        doc: "For copy_presence, a package shows in the bundle only when its non-copy index seeds \
              hold at least this share of its index seed bytes; otherwise its copy seeds go \
              (exp121; 0 = any one non-copy seed vouches).",
    },
    Key {
        section: "library.discovery",
        name: "factory_bodies",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().factory_bodies)),
        unset: "",
        doc: "Cut each bundled CommonJS module into its body statements, each judged and \
              propagated as a lazy module's body is (the index's rows already hold factory \
              bodies); off, a factory is one external unit no library claim reaches (exp128).",
    },
    Key {
        section: "library.discovery",
        name: "factory_tie_fence",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().factory_tie_fence)),
        unset: "",
        doc: "With factory_bodies: a bundled CommonJS module whose every index seed is a tie \
              the structural rule accepts for two or more families (a fork, a sponge carrying \
              it verbatim, generated code) and which has no untied, declared or banner seed is \
              claimed by no one (exp132).",
    },
    Key {
        section: "library.discovery",
        name: "factory_whole_seed",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().factory_whole_seed)),
        unset: "",
        doc: "With factory_bodies: a package names a bundled CommonJS module's statements only \
              when one of its index seeds there is a whole statement its family's index holds \
              (or a declared or banner seed); function-print seeds alone are dropped, and a \
              module left with no seed is claimed by no one (exp132).",
    },
    Key {
        section: "library.discovery",
        name: "factory_banner_index",
        kind: Kind::Bool,
        flag: None,
        default: || Some(Val::Bool(discovery().factory_banner_index)),
        unset: "",
        doc: "With factory_bodies: a /*! banner seed on a bundled CommonJS module's statement is \
              refused when another family's index prints cover more of that statement than \
              the banner family's (exp132).",
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
