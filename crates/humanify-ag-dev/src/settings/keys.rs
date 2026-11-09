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
        default: none,
        unset: "per verb: /work for eval neutrality; /work/eval-work for eval score",
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
    path_key(
        "run",
        "out",
        Some("--out"),
        "the verb's own default: a rescore writes beside the committed results, a dev tool \
         into a fresh directory it prints",
        "Where a verb writes its output (a rescore's cards, summary.json and timing.json; \
         e2e's kept trees; ask-trace's dumps; match-truth's scorecard).",
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
    Key {
        section: "eval",
        name: "parallel_pairs",
        kind: Kind::Int { min: 1, max: ANY },
        flag: Some("--parallel-pairs"),
        default: || Some(Val::Int(5)),
        unset: "",
        doc: "How many pairs run at once, each in its own lane (rebase, scored leg, \
              scoring, boot; the self-hop in the last pair's lane). 1 is the serial run. \
              Every launch keeps pairs.json's llm.concurrency, so the LLM server sees up \
              to N times it in flight (exp207).",
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
        Some("--data-root"),
        "the corpus's root in data-roots.json (per host)",
        "Where the corpus's frozen bundles, truth and raw reports live on this host.",
    ),
    Key {
        section: "libscore",
        name: "units",
        kind: Kind::List(None),
        flag: Some("--units"),
        default: none,
        unset: "the committed layer manifest's units label(s)",
        doc: "The run label(s) whose unit spans a truth-layer builder reads (copies2, misattr13, mapdefect14).",
    },
    path_key(
        "libscore",
        "registry",
        Some("--npm-registry"),
        "/work/exp112-data/registry",
        "`library npm`: the npm registry-manifest cache to fill.",
    ),
    path_key(
        "libscore",
        "bundled",
        Some("--npm-bundled"),
        "/work/exp112-data/bundled",
        "`library npm`: the tarball license-collection cache to fill.",
    ),
    Key {
        section: "libscore",
        name: "packages",
        kind: Kind::List(None),
        flag: Some("--packages"),
        default: none,
        unset: "every package the corpus needs",
        doc: "`library npm`: only these packages.",
    },
    Key {
        section: "libscore",
        name: "apps",
        kind: Kind::List(None),
        flag: Some("--apps"),
        default: none,
        unset: "every app of the corpus's plan",
        doc: "`library corpus build`: only these apps (by slug).",
    },
    Key {
        section: "libscore",
        name: "layers",
        kind: Kind::List(None),
        flag: Some("--layers"),
        default: none,
        unset: "every layer",
        doc: "Only these truth layers (origins, origins7, ..., copies2, misattr13, mapdefect14).",
    },
    path_key(
        "libscore",
        "truth",
        Some("--truth"),
        "required by `eval vendor-precision check`",
        "The truth a vendor-precision check judges against: bun-dev:<entry id>, cc216 or none.",
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
    // [scorer]
    path_key(
        "scorer",
        "work",
        Some("--work"),
        "required by rescore",
        "The label's frozen trees (one directory per scored version, its rebased priors, \
         and each version's diag record): what `eval rescore` re-scores.",
    ),
    path_key(
        "scorer",
        "cards",
        Some("--cards"),
        "required by rescore",
        "The label's recorded cards and run records (read-only): which pairs to re-score, \
         and the records copied beside the new cards.",
    ),
    path_key(
        "scorer",
        "results",
        Some("--results"),
        "dev/eval/results (the committed labels)",
        "Where the eval's labels live: one directory per label.",
    ),
    // [index]
    path_key(
        "index",
        "spec",
        None,
        "required (the positional of `index build`)",
        "The committed index spec file.",
    ),
    path_key(
        "index",
        "registry",
        Some("--registry"),
        "required",
        "The npm registry snapshot directory (fetched once, by the dry run).",
    ),
    path_key(
        "index",
        "store",
        Some("--store"),
        "required",
        "The shard store.",
    ),
    path_key(
        "index",
        "work",
        Some("--index-work"),
        "required",
        "The build work directory (each version installs into its own).",
    ),
    path_key(
        "index",
        "out",
        Some("--index-out"),
        "required",
        "Where the merged index is written.",
    ),
    Key {
        section: "index",
        name: "chunks",
        kind: Kind::Int { min: 1, max: ANY },
        flag: Some("--chunks"),
        default: || Some(Val::Int(8)),
        unset: "",
        doc: "How many chunks the build is split into; all run at once.",
    },
    Key {
        section: "index",
        name: "jobs",
        kind: Kind::Int { min: 1, max: ANY },
        flag: Some("--index-jobs"),
        default: || Some(Val::Int(4)),
        unset: "",
        doc: "Concurrent package builds per chunk (each an npm install + bundle, ~2 GB).",
    },
    Key {
        section: "index",
        name: "merge_jobs",
        kind: Kind::Int { min: 1, max: ANY },
        flag: Some("--merge-jobs"),
        default: || Some(Val::Int(1)),
        unset: "",
        doc: "Concurrent shard merges in the final --merge-only.",
    },
    // [walk]
    path_key(
        "walk",
        "dir",
        Some("--walk-dir"),
        "required",
        "Where a walk keeps its manifest, frozen binary, trees, runs and cards.",
    ),
    switch(
        "walk",
        "resume",
        "--resume",
        off,
        "Continue the walk in --walk-dir: finished hops are skipped; another binary, model, \
         inputs, naming mode or index is refused.",
    ),
    switch(
        "walk",
        "reboot",
        "--reboot",
        off,
        "With --resume: re-run the boot gate on finished hops whose boot FAILED (re-recorded, marked).",
    ),
    switch(
        "walk",
        "report",
        "--report",
        on,
        "Score every hop and write <walk-dir>/report/ when the walk ends.",
    ),
    path_key(
        "walk",
        "report_dir",
        Some("--report-dir"),
        "<first walk>/report",
        "Where `eval walk-report` writes report.md and report.json.",
    ),
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
    Key {
        section: "dev",
        name: "files",
        kind: Kind::List(None),
        flag: Some("--files"),
        default: empty,
        unset: "",
        doc: "Census only these files (the pre-commit hook's staged files): only their \
              unlisted sites, their stale entries and their allow comments are reported; a \
              list with no `.rs` path is a full run.",
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
        "dev/census/concurrency-allowlist.json",
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
    (
        "--no-vendor",
        "removed: scorer v2 always scores vendor/ when both trees have one (rule 8)",
    ),
    (
        "--no-vendor-score",
        "removed: scorer v2 always scores vendor/ when both trees have one (rule 8)",
    ),
    (
        "--no-layout",
        "removed: scorer v2 always scores the layout block when both trees have src/",
    ),
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
    ("scorer", "Eval scorer"),
    ("index", "Index build"),
    ("dev", "Gate and dev tools"),
    ("walk", "Version walk"),
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
