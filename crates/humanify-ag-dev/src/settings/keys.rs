//! `HARNESS_KEYS`: every harness setting, declared once (docs/dev-harness.md
//! §2.2) — the four TS flag tables (`SCORE_FLAGS`, `NEUTRALITY_FLAGS`,
//! `LIBRARY_FLAGS`, `walk.sh`'s header) and the env folklore they replaced
//! (`BOOT_GATE_MODEL`, `JOBS`, `MERGE_JOBS`), as ONE registry in the
//! pipeline's own format (`humanify_ag_cli::registry`). Every key is a
//! flag; the spellings people already type are kept as overrides.
//!
//! A default that lives in a committed file (`pairs.json`'s LLM settings
//! and paths) or a per-host file (`.humanify.local.json`'s endpoint) is
//! UNSET here, and the run layer fills it from the file: the registry says
//! where the value comes from in `unset`.

use humanify_ag_cli::registry::{Key, Kind, Registry, Val};

fn none() -> Option<Val> {
    None
}
fn off() -> Option<Val> {
    Some(Val::Bool(false))
}
fn on() -> Option<Val> {
    Some(Val::Bool(true))
}
fn text(s: &str) -> Option<Val> {
    Some(Val::Text(s.to_string()))
}
fn empty() -> Option<Val> {
    Some(Val::List(Vec::new()))
}

fn base_modes() -> Vec<String> {
    ["scratch", "seeded", "archive"].map(String::from).to_vec()
}
fn rules() -> Vec<String> {
    ["agreement", "structural"].map(String::from).to_vec()
}
fn efforts() -> Vec<String> {
    ["low", "medium", "high"].map(String::from).to_vec()
}
/// The exp096 corpora (lib096 `CORPORA`).
pub const CORPORA: &[&str] = &[
    "dev",
    "test",
    "test2",
    "bundlers",
    "apps",
    "pop",
    "remin",
    "remin2",
    "remin3",
    "remin4",
    "bun-dev",
    "bun-held",
    "kind-esbuild",
    "kind-webpack",
    "kind-rollup",
    "cc",
];
fn corpora() -> Vec<String> {
    CORPORA.iter().map(|c| c.to_string()).collect()
}

const ANY: u64 = u32::MAX as u64;

const fn path_key(
    section: &'static str,
    name: &'static str,
    flag: Option<&'static str>,
    unset: &'static str,
    doc: &'static str,
) -> Key {
    Key {
        section,
        name,
        kind: Kind::Text,
        flag,
        default: none,
        unset,
        doc,
    }
}

const fn count_key(
    section: &'static str,
    name: &'static str,
    flag: Option<&'static str>,
    unset: &'static str,
    doc: &'static str,
) -> Key {
    Key {
        section,
        name,
        kind: Kind::Int { min: 1, max: ANY },
        flag,
        default: none,
        unset,
        doc,
    }
}

const fn switch(
    section: &'static str,
    name: &'static str,
    flag: &'static str,
    default: fn() -> Option<Val>,
    doc: &'static str,
) -> Key {
    Key {
        section,
        name,
        kind: Kind::Bool,
        flag: Some(flag),
        default,
        unset: "",
        doc,
    }
}

/// Every harness setting, by section.
pub static HARNESS_KEYS: &[Key] = &[
    // [run]
    Key {
        section: "run",
        name: "workdir",
        kind: Kind::Text,
        flag: Some("--workdir"),
        default: || text("/work"),
        unset: "",
        doc: "Where a run writes its trees, logs and run config. For neutrality it is also \
              the default prior root and cache root: pass --priors and --cache to move only \
              the workdir (a fresh cache is a COLD run, and a cold verdict is void).",
    },
    switch(
        "run",
        "force_mixed",
        "--force-mixed",
        off,
        "Override the comparison guard: proceed although the identities differ, printing \
         every mismatching field by name.",
    ),
    path_key(
        "run",
        "bin",
        Some("--bin"),
        "this repo's target/release/humanify-ag, built by the harness",
        "The pipeline binary to run (in some cargo workspace's target/, so its build \
         commit is known).",
    ),
    Key {
        section: "run",
        name: "pipeline_set",
        kind: Kind::List(None),
        flag: Some("--pipeline-set"),
        default: empty,
        unset: "",
        doc: "One PIPELINE setting, section.key=value, written into every launch's --config \
              file; repeatable, order kept. Checked against the pipeline's key registry \
              before anything runs.",
    },
    // [eval]
    Key {
        section: "eval",
        name: "base_mode",
        kind: Kind::Choice(base_modes),
        flag: Some("--base-mode"),
        default: || text("scratch"),
        unset: "",
        doc: "How each v-1 base is built: scratch (rebuilt by the current pipeline with no \
              prior; the default since 2026-10-03), seeded (rebuilt with the archive as its \
              prior; every reference before 2026-10-03), archive (no rebuild: reads ~3.7x \
              worse).",
    },
    Key {
        section: "eval",
        name: "pairs",
        kind: Kind::List(None),
        flag: Some("--pairs"),
        default: none,
        unset: "every pair in pairs.json",
        doc: "The pairs to score, as from->to versions (a PARTIAL label).",
    },
    path_key(
        "eval",
        "llm_cache",
        Some("--llm-cache"),
        "no cache: every prompt reaches the model (rule 10)",
        "An LLM cache directory. Iteration only: a cached run is never a verdict about \
         LLM-dependent behaviour.",
    ),
    switch(
        "eval",
        "layout",
        "--layout",
        on,
        "Score the tree layout block.",
    ),
    switch(
        "eval",
        "vendor_score",
        "--vendor-score",
        on,
        "Score vendor/ (rule 8: it went unscored for thirteen experiments).",
    ),
    switch(
        "eval",
        "boot_prompt",
        "--boot-prompt",
        on,
        "Run the boot gate's live prompt half (needs bun on PATH).",
    ),
    switch(
        "eval",
        "self_hop",
        "--self-hop",
        on,
        "Run the cold and warm self-hop gate.",
    ),
    path_key(
        "eval",
        "inputs_base",
        Some("--inputs-base"),
        "pairs.json inputsBase",
        "Where the walked versions' input bundles are.",
    ),
    path_key(
        "eval",
        "priors_base",
        Some("--priors-base"),
        "pairs.json priorsBase",
        "Where the archive priors are.",
    ),
    path_key(
        "eval",
        "library_index",
        Some("--library-index"),
        "pairs.json libraryIndex (names mode); none in off mode",
        "The library index a names-mode launch reads — never a run-time build, whose as-of \
         is the day it ran. Refused with --pipeline-set library.matching=off.",
    ),
    // [neutrality]
    path_key(
        "neutrality",
        "baseline",
        Some("--baseline"),
        "required (the first positional of `eval neutrality`)",
        "The committed ref the candidate (HEAD) is compared with.",
    ),
    Key {
        section: "neutrality",
        name: "pair",
        kind: Kind::Text,
        flag: Some("--pair"),
        default: || text("2.1.85:2.1.86"),
        unset: "",
        doc: "The pair, from:to (the second positional of `eval neutrality`).",
    },
    path_key(
        "neutrality",
        "cache",
        Some("--cache"),
        "<workdir>/neutrality-cache, the standing WARM cache",
        "The shared LLM cache both legs replay. Only a WARM run resolves zero.",
    ),
    path_key(
        "neutrality",
        "priors",
        Some("--priors"),
        "<workdir>/exp050-cold",
        "Where the pair's prior trees are.",
    ),
    // [libscore]
    path_key(
        "libscore",
        "index",
        Some("--index"),
        "required",
        "The library index directory discover reads.",
    ),
    Key {
        section: "libscore",
        name: "rule",
        kind: Kind::Choice(rules),
        flag: Some("--rule"),
        default: none,
        unset: "required",
        doc: "The lookup rule: agreement or structural (library.lookup.agreement).",
    },
    path_key(
        "libscore",
        "commit",
        Some("--commit"),
        "HEAD",
        "The commit whose binary is built and scored.",
    ),
    count_key(
        "libscore",
        "jobs",
        Some("--jobs"),
        "4",
        "Concurrent discover runs.",
    ),
    path_key(
        "libscore",
        "cpus",
        Some("--cpus"),
        "no CPU pinning",
        "The CPU list discover runs are pinned to (taskset).",
    ),
    Key {
        section: "libscore",
        name: "corpus",
        kind: Kind::Choice(corpora),
        flag: Some("--corpus"),
        default: || text("dev"),
        unset: "",
        doc: "The frozen corpus to score.",
    },
    Key {
        section: "libscore",
        name: "sets",
        kind: Kind::List(None),
        flag: Some("--sets"),
        default: none,
        unset: "every set of the corpus",
        doc: "A subset of the corpus's sets to run and score.",
    },
    path_key(
        "libscore",
        "data_root",
        None,
        "the corpus's own data root (per host)",
        "Where the corpus's frozen bundles, truth and raw reports live on this host.",
    ),
    // [llm]
    Key {
        section: "llm",
        name: "endpoint",
        kind: Kind::Url,
        flag: Some("--endpoint"),
        default: none,
        unset: "llm.endpoint in the git-ignored .humanify.local.json (this checkout's, else \
                the main checkout's)",
        doc: "The OpenAI-compatible endpoint. Never committed: the repo is public, and the \
              identity records only its sha256.",
    },
    path_key(
        "llm",
        "model",
        None,
        "pairs.json llm.model",
        "The model the pipeline asks.",
    ),
    Key {
        section: "llm",
        name: "reasoning_effort",
        kind: Kind::Choice(efforts),
        flag: None,
        default: none,
        unset: "pairs.json llm.reasoningEffort",
        doc: "The reasoning effort sent with every request.",
    },
    count_key(
        "llm",
        "concurrency",
        None,
        "pairs.json llm.concurrency",
        "Concurrent LLM requests.",
    ),
    // [boot]
    Key {
        section: "boot",
        name: "model",
        kind: Kind::Text,
        flag: None,
        default: || text("claude-opus-4-1"),
        unset: "",
        doc: "The model the boot gate pins for the walked CLI (the API refuses the \
              account default to CLIs older than 2.1.251).",
    },
    // [index]
    path_key(
        "index",
        "spec",
        None,
        "required",
        "The committed index spec file.",
    ),
    path_key(
        "index",
        "registry",
        None,
        "required",
        "The npm registry mirror.",
    ),
    path_key("index", "store", None, "required", "The shard store."),
    path_key(
        "index",
        "work",
        None,
        "required",
        "The build work directory.",
    ),
    path_key(
        "index",
        "out",
        None,
        "required",
        "Where the merged index is written.",
    ),
    count_key(
        "index",
        "chunks",
        None,
        "8",
        "How many chunks the build is split into.",
    ),
    count_key(
        "index",
        "jobs",
        None,
        "4",
        "Concurrent package builds per chunk.",
    ),
    count_key("index", "merge_jobs", None, "1", "Concurrent shard merges."),
    // [dev] — the gate, the censuses and the dev tools (lane L4).
    Key {
        section: "dev",
        name: "only",
        kind: Kind::List(None),
        flag: Some("--only"),
        default: empty,
        unset: "",
        doc: "Run only these named parts: `check`'s stage names (the run is then labelled \
              PARTIAL and is never a green gate), or `match-truth`'s package names.",
    },
    path_key(
        "dev",
        "out",
        Some("--out"),
        "a fresh directory, printed",
        "Where a dev tool writes its output.",
    ),
    Key {
        section: "dev",
        name: "files",
        kind: Kind::List(None),
        flag: Some("--files"),
        default: empty,
        unset: "",
        doc: "Census only these files (the pre-commit hook's staged files); stale-entry \
              checks are skipped, since a partial scan cannot see every site.",
    },
    switch(
        "dev",
        "loose",
        "--loose",
        off,
        "The clone census's review sweep: a lower cutoff plus literal-blurred matching; \
         always exits 0.",
    ),
    path_key(
        "dev",
        "root",
        Some("--root"),
        "this repository",
        "The tree a census scans (a planted test tree; default: this repository).",
    ),
    path_key(
        "dev",
        "allowlist",
        Some("--allowlist"),
        "scripts/concurrency-allowlist.json",
        "The concurrency census's allowlist file.",
    ),
    Key {
        section: "dev",
        name: "base",
        kind: Kind::Text,
        flag: Some("--base"),
        default: || text("main"),
        unset: "",
        doc: "The git ref the slop census measures its deltas against (at its merge-base \
              with HEAD).",
    },
    path_key(
        "dev",
        "prior",
        Some("--prior"),
        "a cold run",
        "ask-trace: run with --prior-version <path>.",
    ),
    path_key(
        "dev",
        "collide",
        Some("--collide"),
        "every identifier is answered <id>Renamed",
        "ask-trace: the stub names EVERY identifier this one name, forcing collisions.",
    ),
    switch(
        "dev",
        "sequential",
        "--sequential",
        off,
        "ask-trace: the conservative naming schedule.",
    ),
    count_key(
        "dev",
        "port",
        Some("--port"),
        "a free port, printed",
        "The port `stub-llm serve` listens on (127.0.0.1).",
    ),
    path_key(
        "dev",
        "corpus_dir",
        Some("--corpus-dir"),
        "/work/match-truth-corpus",
        "match-truth: where the pinned npm corpus is cached.",
    ),
];

/// Retired spellings, refused naming what replaced them (eval.ts
/// `RETIRED_FLAGS`, run.sh, and the base-mode flags this registry folded
/// into one key).
pub const RETIRED_FLAGS: &[(&str, &str)] = &[
    ("--no-vendor", "renamed: use --no-vendor-score"),
    (
        "--pipeline-arg",
        "retired: the pipeline's settings are --config keys now (config sweep step 5) — use \
         --pipeline-set <section.key=value>",
    ),
    (
        "--heap-mb",
        "removed: it was inert (the pipeline is a Rust binary, not a Node process)",
    ),
    ("--seeded-base", "renamed: use --base-mode seeded"),
    ("--archive-prior", "renamed: use --base-mode archive"),
    (
        "--skip-preflight",
        "removed: the matcher preflight retired with the TS matcher",
    ),
];

const SECTION_TITLES: &[(&str, &str)] = &[
    ("run", "Run"),
    ("eval", "Eval"),
    ("neutrality", "Neutrality"),
    ("libscore", "Library score"),
    ("llm", "LLM"),
    ("boot", "Boot gate"),
    ("index", "Index build"),
    ("dev", "Gate and dev tools"),
];

/// The harness's registry.
pub static HARNESS: Registry = Registry {
    keys: HARNESS_KEYS,
    retired_keys: &[],
    retired_flags: RETIRED_FLAGS,
    short_flags: &[],
    counted_flag: None,
    repeatable: &["run.pipeline_set"],
    section_titles: SECTION_TITLES,
    short_help_hides: |_| false,
    value_names: &[
        ("run.pipeline_set", "section.key=value"),
        ("eval.pairs", "from->to,..."),
        ("neutrality.pair", "from:to"),
        ("neutrality.baseline", "ref"),
        ("libscore.commit", "ref"),
        ("libscore.cpus", "list"),
        ("llm.model", "model"),
        ("boot.model", "model"),
    ],
};
