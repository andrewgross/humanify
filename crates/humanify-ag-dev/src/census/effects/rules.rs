//! What the effects census looks for: which crates are the pure core and
//! which the shell, the effect constructs (matched over sanitized source
//! with the concurrency census's matcher), the roles an allowlist entry may
//! carry, and the dependencies a pure crate may have.

use crate::census::concurrency::rules::{Alt, Construct, Post, Pre, alt, w};

/// The product crates (linked into the release binary or the index
/// builder): every effect site in them is on the allowlist.
pub const PRODUCT_CRATES: &[&str] = &[
    "crates/humanify-ag-cli/",
    "crates/humanify-ag-core/",
    "crates/humanify-ag-index/",
    "crates/humanify-ag-llm/",
    "crates/humanify-ag-model/",
];

/// The pure crates, each with the normal dependencies it may have: the
/// functional core. An effect site in one is a `legacy` entry (to migrate
/// to the shell) or an `observation` (the profiling clock).
pub const PURE_CRATES: &[(&str, &[&str])] = &[
    (
        "crates/humanify-ag-core/",
        &[
            "humanify-ag-model",
            "oxc_allocator",
            "oxc_ast",
            "oxc_estree",
            "oxc_parser",
            "oxc_semantic",
            "oxc_span",
            "oxc_syntax",
            "oxc_traverse",
            "rayon",
            "serde",
            "serde_json",
            "sha1",
            "sha2",
        ],
    ),
    (
        "crates/humanify-ag-model/",
        &["dragonbox_ecma", "serde", "serde_json", "sha2"],
    ),
];

/// The files whose effect sites may be `observation`s: the profiling
/// clock, its sampler thread and its global recorder, which only feed the
/// `--perf-trace` report.
pub const OBSERVATION_FILES: &[&str] = &[
    "crates/humanify-ag-core/src/profiling.rs",
    "crates/humanify-ag-core/src/profiling/summary.rs",
];

/// The line each pure crate's `src/lib.rs` keeps: the swallowed-error
/// lints that live in source because the crate's `[lints]` is the
/// workspace's.
pub const PURE_LIB_DENY: &str = "#![deny(clippy::map_err_ignore, clippy::let_underscore_must_use)]";

/// The roles an entry may carry.
pub const ROLES: &[(&str, &str)] = &[
    (
        "legacy",
        "an effect in a pure crate, or a `Result<_, String>` signature, that a migration step removes",
    ),
    (
        "observation",
        "the profiling clock, sampler and recorder (only in the OBSERVATION_FILES)",
    ),
    (
        "shell",
        "an effect in a shell crate (cli, index, llm): inventory",
    ),
];

/// The construct id of a non-private fn returning `Result<_, String>`.
pub const RESULT_STRING: &str = "result-string";

/// The construct id of a global mutable static.
pub const STATIC_STATE: &str = "static-state";

/// Words (with any `Atomic*`) that make a static's type mutable state. A
/// `OnceLock` is set at run time, so it can carry run state; a `LazyLock`
/// is computed by its own initializer, a constant (an effect inside that
/// initializer is still an effect site).
pub const STATE_TYPES: &[&str] = &[
    "Mutex",
    "RwLock",
    "OnceLock",
    "OnceCell",
    "Cell",
    "RefCell",
    "UnsafeCell",
];

const fn call(lit: &'static str) -> Alt {
    alt(lit, Pre::Free, Post::Call)
}

const fn bare_call(lit: &'static str) -> Alt {
    alt(lit, Pre::NotMember, Post::Call)
}

/// The effect constructs. Matched over source whose comments, literals
/// and `use` declarations are blanked, so each alternative names the
/// spelling a call site keeps after its import (`fs::write`, `File::open`,
/// `Instant::now`).
pub const EFFECTS: &[Construct] = &[
    Construct {
        id: "fs",
        alts: &[
            alt("fs::", Pre::Boundary, Post::WordTail),
            w("File::open"),
            w("File::create"),
            w("File::create_new"),
            w("OpenOptions"),
            call(".exists"),
            call(".try_exists"),
            call(".is_file"),
            call(".is_dir"),
            call(".is_symlink"),
            call(".read_dir"),
            call(".canonicalize"),
            call(".metadata"),
            call(".symlink_metadata"),
            call(".read_link"),
        ],
        section: "disk",
        rule: "disk I/O: the shell reads and writes; core decides over the values it was handed",
    },
    Construct {
        id: "process",
        alts: &[
            w("Command::new"),
            w("process::exit"),
            w("process::abort"),
            w("process::id"),
        ],
        section: "processes",
        rule: "a process (or the exit code): the shell runs it; core takes an injected trait",
    },
    Construct {
        id: "env",
        alts: &[
            w("env::var"),
            w("env::var_os"),
            w("env::vars"),
            w("env::vars_os"),
            w("env::temp_dir"),
            w("env::current_dir"),
            w("env::set_current_dir"),
            w("env::current_exe"),
            w("env::args"),
            w("env::args_os"),
            w("env::set_var"),
            w("env::remove_var"),
            w("env::home_dir"),
            w("path::absolute"),
        ],
        section: "environment",
        rule: "the environment (variables, working directory, temp dir, argv): read once by the shell",
    },
    Construct {
        id: "clock",
        alts: &[w("Instant::now"), w("SystemTime::now")],
        section: "clock",
        rule: "a clock read: only the profiling module observes time",
    },
    Construct {
        id: "thread",
        alts: &[w("thread::spawn"), w("thread::Builder")],
        section: "threads",
        rule: "a detached thread: CPU work goes through crate::par; only observers run beside the pipeline",
    },
    Construct {
        id: "stdio",
        alts: &[
            alt("println!", Pre::Boundary, Post::Free),
            alt("print!", Pre::Boundary, Post::Free),
            alt("eprintln!", Pre::Boundary, Post::Free),
            alt("eprint!", Pre::Boundary, Post::Free),
            alt("dbg!", Pre::Boundary, Post::Free),
            w("io::stdout"),
            w("io::stderr"),
            w("io::stdin"),
            bare_call("stdout"),
            bare_call("stderr"),
            bare_call("stdin"),
        ],
        section: "output",
        rule: "terminal output: the shell renders; core returns values and warnings",
    },
    Construct {
        id: "net",
        alts: &[
            w("TcpStream"),
            w("TcpListener"),
            w("UdpSocket"),
            alt("reqwest::", Pre::Boundary, Post::WordTail),
        ],
        section: "network",
        rule: "the network: only the LLM provider's client talks to it",
    },
];

/// Every construct id an entry may name (with its rule).
pub fn rule_of(id: &str) -> Option<&'static str> {
    match id {
        RESULT_STRING => Some(
            "an error as a String: the type says nothing about what failed; a typed error per crate, formatted once by the shell",
        ),
        STATIC_STATE => Some(
            "global mutable state: run state passes as a value; only the profiling recorder is global",
        ),
        _ => EFFECTS.iter().find(|c| c.id == id).map(|c| c.rule),
    }
}

/// The pure crate holding `file`, if any.
pub fn pure_crate_of(file: &str) -> Option<&'static str> {
    PURE_CRATES
        .iter()
        .map(|c| c.0)
        .find(|c| file.starts_with(c))
}

/// Whether `file` is in a product crate.
pub fn is_product(file: &str) -> bool {
    PRODUCT_CRATES.iter().any(|c| file.starts_with(c))
}
