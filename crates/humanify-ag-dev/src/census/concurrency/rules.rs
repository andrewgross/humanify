//! What the concurrency census looks for: the documented patterns an
//! allowlist entry may name, and the constructs that make a site — each
//! with its docs/concurrency.md section and the rule an author must have
//! checked. A construct is a list of alternatives matched over sanitized
//! source (comments, literals and `use` declarations blanked) the way the
//! TS census's regexes matched them: leftmost-first, non-overlapping, `\b`
//! word boundaries over ASCII `\w`.

/// The documented patterns (docs/concurrency.md, "The patterns"): name and
/// its doc section.
pub const PATTERNS: &[(&str, &str)] = &[
    ("par-seam", "cpu-parallelism"),
    ("order-mode", "the-ordered-mode-rule-9"),
    ("llm-runtime", "async-io"),
    ("rate-limiter", "async-io"),
    ("single-flight", "shared-state"),
    ("retry-envelope", "failures-and-retries"),
    ("deadline", "deadlines"),
    ("subprocess", "subprocesses"),
    ("short-lock", "shared-state"),
    ("stats-counter", "shared-state"),
    ("observer", "observers"),
    ("test-support", "test-support"),
];

/// The one crate whose sites may use the test-support pattern.
pub const TEST_SUPPORT_CRATE: &str = "crates/humanify-ag-testkit/";

/// Constructs whose entries must use one specific pattern.
pub const REQUIRED_PATTERN: &[(&str, &str)] = &[("atomic-relaxed", "stats-counter")];

/// What must hold just before an alternative's text.
#[derive(Clone, Copy, Debug)]
pub enum Pre {
    /// `\b`.
    Boundary,
    /// Nothing.
    Free,
    /// `(?<!tokio::sync::)\b` (a std lock, not the async one).
    BoundaryNotTokioSync,
    /// `(?<![:\w])` (a bare `Relaxed`, not `X::Relaxed`).
    NotColonOrWord,
}

/// What must hold (or is consumed) just after an alternative's text.
#[derive(Clone, Copy, Debug)]
pub enum Post {
    /// `\b`.
    Boundary,
    /// Nothing.
    Free,
    /// `\w*` (consumed; a `\b` after it always holds).
    WordTail,
    /// `\s*\(` (a call).
    Call,
    /// `\b(?!\s*[({])` (not an enum variant's payload).
    BoundaryNotPayload,
}

/// One alternative of a construct.
#[derive(Clone, Copy, Debug)]
pub struct Alt {
    pub lit: &'static str,
    pub pre: Pre,
    pub post: Post,
}

/// One construct: its id, its alternatives in regex order, its doc
/// section and its rule.
#[derive(Debug)]
pub struct Construct {
    pub id: &'static str,
    pub alts: &'static [Alt],
    pub section: &'static str,
    pub rule: &'static str,
}

const fn w(lit: &'static str) -> Alt {
    Alt {
        lit,
        pre: Pre::Boundary,
        post: Post::Boundary,
    }
}

const fn alt(lit: &'static str, pre: Pre, post: Post) -> Alt {
    Alt { lit, pre, post }
}

pub const CONSTRUCTS: &[Construct] = &[
    Construct {
        id: "thread-spawn",
        alts: &[w("thread::spawn"), w("thread::Builder")],
        section: "cpu-parallelism",
        rule: "a detached OS thread: only observers may run beside the pipeline, and they never touch output; CPU work goes through crate::par",
    },
    Construct {
        id: "thread-scope",
        alts: &[w("thread::scope")],
        section: "cpu-parallelism",
        rule: "a scoped thread: only inside crate::par (beside) or to drain a subprocess's pipes",
    },
    Construct {
        id: "tokio-spawn",
        alts: &[
            w("tokio::task::spawn"),
            w("tokio::spawn"),
            w("spawn_local"),
            w("JoinSet"),
        ],
        section: "async-io",
        rule: "no detached tasks: every future is driven to completion by the provider's run_wave/run_pipelined",
    },
    Construct {
        id: "spawn-blocking",
        alts: &[w("spawn_blocking"), w("block_in_place")],
        section: "async-io",
        rule: "no blocking pool: blocking or CPU work stays off the runtime thread entirely",
    },
    Construct {
        id: "block-on",
        alts: &[alt("block_on", Pre::Boundary, Post::Call)],
        section: "async-io",
        rule: "block_on only on the provider's one runtime, never nested, never from rayon",
    },
    Construct {
        id: "runtime-builder",
        alts: &[
            w("Runtime::new"),
            w("new_current_thread"),
            w("new_multi_thread"),
            alt("#[tokio::main", Pre::Free, Post::Boundary),
        ],
        section: "async-io",
        rule: "ONE runtime per process, owned by the LLM provider; no second runtime",
    },
    Construct {
        id: "rayon-par",
        alts: &[
            w("par_iter"),
            w("par_iter_mut"),
            w("into_par_iter"),
            w("par_bridge"),
            alt("par_chunks", Pre::Boundary, Post::WordTail),
            alt("par_sort", Pre::Boundary, Post::WordTail),
            w("par_extend"),
            w("par_drain"),
            w("par_windows"),
        ],
        section: "cpu-parallelism",
        rule: "raw rayon only inside crate::par; callers use par::map_ordered / produce_then_map / join (results in input order)",
    },
    Construct {
        id: "rayon-pool",
        alts: &[
            w("ThreadPoolBuilder"),
            w("ThreadPool"),
            w("build_global"),
            w("current_num_threads"),
        ],
        section: "cpu-parallelism",
        rule: "the rayon pool size is an open decision (audit R4); a pool is never built ad hoc",
    },
    Construct {
        id: "rayon-join",
        alts: &[w("rayon::join"), w("rayon::scope"), w("rayon::spawn")],
        section: "cpu-parallelism",
        rule: "raw rayon only inside crate::par",
    },
    Construct {
        id: "async-fanout",
        alts: &[
            w("join_all"),
            w("try_join_all"),
            w("FuturesUnordered"),
            w("FuturesOrdered"),
            alt("try_join!", Pre::Boundary, Post::Free),
            alt("join!", Pre::Boundary, Post::Free),
        ],
        section: "output-determinism",
        rule: "a fan-out is bounded by the prompt window, and completion order never reaches the tree or stats",
    },
    Construct {
        id: "select",
        alts: &[alt("select!", Pre::Boundary, Post::Free)],
        section: "async-io",
        rule: "select! cancels the losing branch: every branch must be cancel-safe",
    },
    Construct {
        id: "lock",
        alts: &[
            alt("Mutex", Pre::BoundaryNotTokioSync, Post::Boundary),
            alt("RwLock", Pre::BoundaryNotTokioSync, Post::Boundary),
        ],
        section: "shared-state",
        rule: "a std lock: short critical section, never held across .await, IO or a call that can take another lock",
    },
    Construct {
        id: "async-lock",
        alts: &[w("tokio::sync::Mutex"), w("tokio::sync::RwLock")],
        section: "shared-state",
        rule: "an async lock: only where the guard must be held across .await (the single-flight gate)",
    },
    Construct {
        id: "condvar",
        alts: &[w("Condvar"), w("Barrier")],
        section: "shared-state",
        rule: "a wait on another thread: needs a deadline or a proof it is always signalled",
    },
    Construct {
        id: "semaphore",
        alts: &[w("Semaphore")],
        section: "async-io",
        rule: "the rate limiter's concurrency cap; one cap, owned by rate.rs",
    },
    Construct {
        id: "channel",
        alts: &[
            w("mpsc"),
            alt("crossbeam", Pre::Boundary, Post::WordTail),
            w("oneshot"),
            w("sync_channel"),
            alt("broadcast::", Pre::Boundary, Post::Free),
            w("watch::channel"),
        ],
        section: "shared-state",
        rule: "a channel: bounded, and a full or closed channel is a decided failure, never a silent drop",
    },
    Construct {
        id: "atomic",
        alts: &[
            w("AtomicBool"),
            w("AtomicUsize"),
            w("AtomicIsize"),
            w("AtomicU8"),
            w("AtomicU16"),
            w("AtomicU32"),
            w("AtomicU64"),
            w("AtomicI8"),
            w("AtomicI16"),
            w("AtomicI32"),
            w("AtomicI64"),
            w("AtomicPtr"),
        ],
        section: "shared-state",
        rule: "an atomic: a counter or flag only; nothing that decides output may race on it",
    },
    Construct {
        id: "atomic-relaxed",
        alts: &[
            w("Ordering::Relaxed"),
            alt("Relaxed", Pre::NotColonOrWord, Post::BoundaryNotPayload),
        ],
        section: "shared-state",
        rule: "Ordering::Relaxed orders NOTHING: allowed only for stats counters and unique-name sequences (pattern stats-counter)",
    },
    Construct {
        id: "subprocess",
        alts: &[w("Command::new")],
        section: "subprocesses",
        rule: "a subprocess: drain every pipe concurrently, a deadline on the whole wait, and a failure is loud",
    },
    Construct {
        id: "piped-stdio",
        alts: &[w("Stdio::piped")],
        section: "subprocesses",
        rule: "a piped child: stdin written concurrently with draining stdout/stderr (audit W1)",
    },
    Construct {
        id: "http-client",
        alts: &[
            w("reqwest::Client"),
            w("reqwest::ClientBuilder"),
            w("Client::new"),
            w("Client::builder"),
        ],
        section: "deadlines",
        rule: "an HTTP client: every request runs under ONE deadline covering connect, send and the body (audit W2)",
    },
    Construct {
        id: "sleep",
        alts: &[
            alt("sleep_until", Pre::Boundary, Post::Call),
            alt("sleep", Pre::Boundary, Post::Call),
        ],
        section: "async-io",
        rule: "a sleep: never std::thread::sleep on the runtime thread; in async only tokio::time::sleep, for backoff or a rate window",
    },
    Construct {
        id: "timeout",
        alts: &[
            alt("timeout", Pre::Boundary, Post::Call),
            alt("timeout_at", Pre::Boundary, Post::Call),
            alt("recv_timeout", Pre::Boundary, Post::Call),
        ],
        section: "deadlines",
        rule: "a deadline must cover the WHOLE operation it guards (audit W2: send AND body)",
    },
];

/// The construct named `id`.
pub fn construct(id: &str) -> Option<&'static Construct> {
    CONSTRUCTS.iter().find(|c| c.id == id)
}

/// JS `\w` (ASCII).
pub fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// JS `\s` over ASCII.
pub fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// `\b` at `at` in `s`.
pub fn boundary(s: &[u8], at: usize) -> bool {
    let before = at > 0 && is_word(s[at - 1]);
    let after = at < s.len() && is_word(s[at]);
    before != after
}

fn pre_holds(s: &[u8], at: usize, pre: Pre) -> bool {
    match pre {
        Pre::Free => true,
        Pre::Boundary => boundary(s, at),
        Pre::BoundaryNotTokioSync => !s[..at].ends_with(b"tokio::sync::") && boundary(s, at),
        Pre::NotColonOrWord => at == 0 || !(s[at - 1] == b':' || is_word(s[at - 1])),
    }
}

fn skip_spaces(s: &[u8], mut at: usize) -> usize {
    while at < s.len() && is_space(s[at]) {
        at += 1;
    }
    at
}

/// Where the match ends when `post` holds at `at`.
fn post_end(s: &[u8], at: usize, post: Post) -> Option<usize> {
    match post {
        Post::Free => Some(at),
        Post::Boundary => boundary(s, at).then_some(at),
        Post::WordTail => {
            let mut e = at;
            while e < s.len() && is_word(s[e]) {
                e += 1;
            }
            Some(e)
        }
        Post::Call => {
            let e = skip_spaces(s, at);
            (e < s.len() && s[e] == b'(').then_some(e + 1)
        }
        Post::BoundaryNotPayload => {
            let e = skip_spaces(s, at);
            let payload = e < s.len() && matches!(s[e], b'(' | b'{');
            (boundary(s, at) && !payload).then_some(at)
        }
    }
}

/// The end of the first alternative of `c` that matches at `at`.
fn match_at(s: &[u8], at: usize, c: &Construct) -> Option<usize> {
    c.alts.iter().find_map(|a| {
        let lit = a.lit.as_bytes();
        if !s[at..].starts_with(lit) || !pre_holds(s, at, a.pre) {
            return None;
        }
        post_end(s, at + lit.len(), a.post)
    })
}

/// Start offsets of every match of `c` in `s` (leftmost-first,
/// non-overlapping, like a global regex).
pub fn matches(s: &[u8], c: &Construct) -> Vec<usize> {
    let mut first = [false; 256];
    for a in c.alts {
        first[usize::from(a.lit.as_bytes()[0])] = true;
    }
    let mut out = Vec::new();
    let mut at = 0;
    while at < s.len() {
        if !first[usize::from(s[at])] {
            at += 1;
            continue;
        }
        match match_at(s, at, c) {
            Some(end) => {
                out.push(at);
                at = end.max(at + 1);
            }
            None => at += 1,
        }
    }
    out
}
