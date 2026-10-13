# Concurrency in the Rust pipeline

How the pipeline (`crates/`) is allowed to do more than one thing at a time,
and how that is checked. Written 2026-10-12 from the concurrency audit
([`docs/audits/rust-concurrency-2026-10.md`](./audits/rust-concurrency-2026-10.md),
28 sites, findings W1–W2, R1–R6, O1–O20) and the blog post it was based on,
[_"I am sorry, but you still have to think"_](https://itsallaboutthebit.com/i-am-sorry-but-you-still-have-to-think/).

## Totals

- **8 rules**, below, one section each. Each says which finding it comes from.
- **10 patterns**. Every concurrency site in non-test pipeline code must
  follow one of them.
- **64 sites** on the allowlist (`scripts/concurrency-allowlist.json`),
  **2 of them known exceptions**: the two retry layers (R1). It started at
  61 sites and 5 exceptions; the other three — the `diff` subprocess (R3)
  and the webcrack and `git` subprocesses with no deadline — were fixed
  2026-10-12 by the one subprocess owner (`crate::subprocess`, see
  Subprocesses).
- **Three checks enforce it**:
  - clippy, for what a lint can see;
  - the `census:concurrency` gate stage;
  - the same census in the pre-commit hook, on staged `.rs` files.

| #   | Rule, in one line                                                                                                                                                              | From                   |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------- |
| 1   | CPU parallelism goes through `crate::par` only (rayon underneath), and results come back in input order.                                                                       | O13, O14, R4           |
| 2   | Async I/O runs on the ONE tokio runtime the LLM provider owns. No second runtime, no nested `block_on`, no blocking work in async, no sync lock across `.await`.               | O1–O3, R6, post        |
| 3   | Every network or subprocess wait has a deadline, and the deadline covers the WHOLE operation.                                                                                  | W2, W1 follow-up       |
| 4   | A subprocess drains all its pipes at the same time, has a deadline, and fails loudly.                                                                                          | W1, R3                 |
| 5   | Shared state: std locks for short sections only, an async lock only when the guard must cross `.await`, one lock order, bounded channels, `Relaxed` atomics only for counters. | O5, O6, O8, O17, post  |
| 6   | Completion order never reaches the emitted tree or the stats. Results are put back in canonical order before anything is recorded.                                             | O7, R5                 |
| 7   | A failed call is never silent. Retries live in exactly one layer, with jitter and a cap on total time.                                                                         | R2, R1 (open decision) |
| 8   | Side threads (observers) only watch. They never feed output, and they exit when their owner goes away.                                                                         | O18, O19, O20          |

## The thread and runtime model today

Everything runs on the main thread, one stage after another. Work leaves the
main thread in only four ways:

1. **`crate::par`** (`crates/humanify-core/src/par.rs`):
   - `map_ordered`, `produce_then_map` and `join` run pure work on rayon's
     global pool;
   - `beside` runs one pure job on a scoped thread next to the main thread.
     The oxc AST cannot leave the main thread.
2. **The LLM provider** (`crates/humanify-llm/src/provider.rs`): one
   `current_thread` tokio runtime.
   - `run_wave` is `block_on(join_all(chunk))`.
   - `run_pipelined` is `block_on(FuturesUnordered)`.
   - Each call passes through `AnswerMemo → RateLimited → DebugProvider →
OpenAiClient`.
3. **Subprocesses**, all through `humanify_core::subprocess::run`: the
   webcrack shim, `diff` for the normal-diff hunks, and `git rev-parse`.
4. **Observers**: the TTY redraw thread and the profiler's sampler.

The audit has the full diagram ("The thread and runtime model").

## The patterns

An allowlist entry names exactly one of these. The census rejects any other
name.

| pattern          | What it is                                                                   | Where today                                                                          |
| ---------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| `par-seam`       | the one place that touches rayon or a scoped thread for CPU work             | `par.rs`                                                                             |
| `llm-runtime`    | the one tokio runtime, its `block_on`s and its bounded fan-outs              | `provider.rs`                                                                        |
| `rate-limiter`   | the concurrency cap (semaphore) and the requests-per-minute window           | `rate.rs`                                                                            |
| `single-flight`  | one key, one answer: per-key async gate plus the answer maps                 | `cache.rs`                                                                           |
| `retry-envelope` | a retry loop's backoff                                                       | `rate.rs` `with_retry`, `client.rs` `post_with_sdk_retries` (R1)                     |
| `deadline`       | a timeout that bounds a whole operation, and the HTTP client it applies to   | `client.rs`                                                                          |
| `subprocess`     | a child process: drained pipes, deadline, loud failure                       | `subprocess.rs` (the owner); `webcrack.rs`, `hunks.rs`, `unified.rs` `git_short_sha` |
| `short-lock`     | a std `Mutex`/`RwLock` held for a few field updates, never across slow work  | `metrics.rs`, `llm_sim.rs`                                                           |
| `stats-counter`  | an atomic used as a counter or a unique-name sequence; nothing decides on it | `cache.rs` counters, `PRIOR_PARSES`, `hunks.rs` temp-dir `SEQ`                       |
| `observer`       | a side thread or lock that only watches (progress, profiling, logging)       | `progress.rs`, `profiling.rs`, `log.rs`, `unified.rs` log file                       |

## CPU parallelism

**Rule 1.** CPU work runs in parallel only through `crate::par`. Callers use
`par::map_ordered`, `par::produce_then_map`, `par::join` or `par::beside`,
never `par_iter` or `rayon::join` directly.

- **Results come back in input order.** Indexed `collect` writes each result
  into its input slot (O13). This is why a parallel stage cannot change the
  output.
- **Closures are `Fn + Sync` and pure.** They capture no lock and no atomic
  (O13). Anything a closure counts comes back in its result and is summed on
  the main thread.
- **Never call into tokio from rayon.** `dyn NameProvider` is not `Sync`, so
  no closure passed to `par` can capture the provider. The compiler enforces
  this (audit, "what cannot happen"). Do not add a `NameProvider + Sync`
  bound.
- **Pool size: an open decision (R4).** The global pool takes one worker
  per core, and nothing sets the size. Three heavy runs on one box means
  three times the cores in CPU-bound workers. That costs memory and time,
  not correctness. The pool is never built ad hoc: a sized pool is a
  `build_global` call in one owner, decided once.
- **`beside` joins on every path** and re-raises a panic from the background
  job (O14). A background panic surfaces only once the foreground job
  returns.

## Async I/O

**Rule 2.** All async I/O runs on the one `current_thread` tokio runtime that
`LlmClient` owns (O1). It is built once per run, and it runs only inside the
provider's `block_on`.

- **No second runtime.** `Runtime::new`, `Builder::new_*` and
  `#[tokio::main]` are disallowed outside `provider.rs`. Test harnesses are
  the exception, and each says so.
- **No nested `block_on`.** tokio panics on one. Today, nothing in `on_done`
  calls the provider. Keep it that way: `on_done` returns follow-up calls and
  never makes them.
- **No detached tasks** (`tokio::spawn`, `JoinSet`, `spawn_blocking`,
  `block_in_place`). The runtime drives every future to completion inside
  `run_wave` or `run_pipelined`, so nothing outlives the wave or escapes its
  deadline. On a `current_thread` runtime, `block_in_place` panics.
- **Nothing blocking in async.** This is the post's central hazard: "if a
  task doesn't yield, no other task can run". Here only ONE thread exists,
  so a blocking call stalls every in-flight request. While it runs, the
  per-attempt deadline keeps counting (R6).
  - Use `tokio::time::sleep`, never `std::thread::sleep`.
  - Do not add heavy CPU work or synchronous file I/O to `on_done`, the disk
    cache or the `-vv` sink. All three already run on the runtime thread
    (R6, a known cost, not measured).
  - If `on_done` grows, move preparation to the main thread and hand it over
    through a bounded channel.
- **No sync lock across `.await`.** A std `Mutex` guard is released before
  the next `.await`. The limiter's window lock closes its block before
  `sleep().await` (O5). When a guard really must cross an await, use
  `tokio::sync::Mutex`, as the single-flight gate does (O6). Clippy enforces
  this.
- **Fan-out is bounded.** `join_all` and `FuturesUnordered` take at most one
  prompt window of calls (O2, O3). The rate limiter's semaphore keeps load
  bounded too, so the client never sends more than the server can queue.

## Deadlines

**Rule 3.** Every wait on something outside the process has a deadline. The
deadline covers the whole operation, not the first half of it.

- **W2 is the worked example.** The LLM call's timeout wrapped `send()`, and
  `send()` resolves when the response headers arrive. When a body stalled
  after the headers, the call hung forever while holding a rate-limiter
  slot. Now one `timeout_at` deadline per attempt bounds connect, send and
  body together, and a body timeout retries like any other timeout.
- **Building an HTTP client is a confirmed site.** `reqwest::Client::new`
  sets no read timeout. That is fine only because every request runs under
  the per-attempt deadline (O12). A new client, or a request made outside
  `attempt`, needs its own deadline.
- **A sleep is not a deadline.** Wait for an event with a timeout, such as
  `recv_timeout` or `timeout_at`. Never poll in a loop with a sleep.
- **Every subprocess has one**, set by its caller and handed to
  `crate::subprocess::run` (values and why: Subprocesses). The W1
  follow-up, R3 and the `git` gap were closed 2026-10-12.

## Subprocesses

**Rule 4.** A child process goes through ONE owner,
`humanify_core::subprocess::run(command, stdin, deadline)`. Do not call
`Command::output`, `spawn` or `wait_with_output` yourself: the owner is
where all three parts of this rule live, and its tests pin them.

1. **Drains every pipe concurrently.** One plain thread each drains stdout,
   drains stderr, feeds stdin (when there is input) and waits for the exit.
   W1 deadlocked because the bundle was written in full before the output
   pipes were read: a child that wrote 64 KiB first blocked forever.
   `Child::wait_with_output` is disallowed by clippy.
2. **Has a deadline on the whole wait.** The deadline starts before the
   spawn and covers stdin, both outputs and the exit together: the caller's
   thread waits for the threads' events with `recv_timeout`, never a sleep.
   On expiry the child's whole **process group** is killed (the child is
   spawned as a group leader). The shim is `npx tsx …`, and its grandchild
   `node` holds the pipes, so killing `npx` alone would leave the drains
   blocked. Core forbids `unsafe`, so the kill is `kill -KILL -- -<pgid>`.
   The threads are not scoped: if something outside the group still holds
   a pipe, they end when it closes and the run does not wait.
3. **Fails loudly.** A spawn failure, a non-zero exit, unreadable output or
   an expired deadline is an error or a WARNING, never a silent fallback
   that changes the tree.

The three subprocesses and their deadlines (measured 2026-10-12, times a
generous factor — a deadline only a process that will never finish
reaches):

| child                                                     | deadline | measured                                                                                                                                                                         | on failure                                                                                                                                                                     |
| --------------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| webcrack shim (`WEBCRACK_DEADLINE`, `unpack/webcrack.rs`) | 30 min   | 12 s for a 2.9 MB / 3,000-module webpack bundle, `npx` start included; 0.85 s for 10 KB; about linear in size                                                                    | the unpack fails: the run stops with the error                                                                                                                                 |
| `diff` (`DIFF_DEADLINE`, `naming/reconcile/hunks.rs`)     | 10 min   | 4.5 s for the largest diff the pipeline runs, the pre-split reconcile over the whole Claude Code bundle (793k vs 805k lines, 2.1.215 -> 2.1.216); per-file diffs are far smaller | post-split: counted as `diff_failed` (apart from parse `discarded`) and a WARNING naming the first file; pre-split: a WARNING that the reconcile did not run (audit R3, fixed) |
| `git rev-parse` (`GIT_DEADLINE`, `unified.rs`)            | 1 min    | milliseconds                                                                                                                                                                     | not a checkout / no git: the decided quiet "unknown"; a timeout: "unknown" and a WARNING                                                                                       |

The offline index builder (`crates/humanify-index`, not the pipeline) runs
`npm install`, `npm view` and the bundlers through the same owner, under
`INSTALL_DEADLINE` (30 min), `VIEW_DEADLINE` (5 min), `BUILD_DEADLINE`
(30 min) and `VERSION_DEADLINE` (1 min); a failure fails that package's
task, which is reported and indexes nothing.

Up to one `diff` per core still runs at once on the rayon pool (R4, the
unsized pool). Resource exhaustion on the shared box can still fail one,
but it can no longer change the tree without a WARNING.

## Shared state

**Rule 5.**

- **std `Mutex`/`RwLock` only for short sections.** Take the lock, do a few
  field updates or one map lookup, and release it. Never hold one across
  `.await`, I/O, or a call that might take another lock (O5, O8). The
  post's line: holding a sync lock for 10 ms makes every task on that
  thread wait.
  - The observers are the exception, and it is deliberate: the log and
    progress locks are held across one write so that lines never
    interleave (O19, O20).
- **An async lock only when the guard must cross `.await`.** The
  single-flight gate (`cache.rs`, O6) is the one case.
- **One lock order.** Today it is key gate (outer), then semaphore slot
  (inner), always (O6). Waiters queue on the gate without a slot, so the
  slots cannot all end up held by waiters. A new lock taken while another
  is held needs its place in this order written in its reason.
- **Poisoning is loud or tolerated on purpose.** Use `.expect` where a
  panic inside the lock is impossible, and `PoisonError::into_inner` for
  observers (O18).
- **Channels are bounded**, and a full or closed channel is a failure
  someone decided on, never a silent drop. The post's example: a broadcast
  channel of capacity 256 dropped events as `Lagged`. Today the only channel
  is the profiler's stop signal.
- **Atomics are counters.** `Ordering::Relaxed` orders nothing else, so it
  is allowed only for stats counters and unique-name sequences (pattern
  `stats-counter`; the census rejects any other pattern for a `Relaxed`
  site). No decision may read a value that threads race on.
- **Growth is bounded or argued.** The answer memo grows with every
  distinct ask and is never trimmed (O9). That is argued: it is the
  single-flight's correctness store, and tens of MB on a full bundle.

## Output determinism

**Rule 6.** The output must be byte-identical for the same input and the
same LLM answers. Completion order must never leak into the emitted tree or
the stats.

- Parallel maps return results in input order (rule 1).
- `join_all` results are positional (O2).
- `run_pipelined` is the one loop that runs in completion order (O3/O7).
  Its decisions read only the round's frozen state plus the lane's own
  claims, and everything it records is re-sorted before it is kept:
  `records.sort_by_key((turn, lane))`, `finished.sort_by_key`, results
  indexed by id, and counts that do not depend on order. Three tests hold
  this:
  - `fast_test.rs` `relaxed_levers_are_deterministic_and_order_independent`;
  - `tests/replay_determinism.rs`;
  - the e2e stage, which runs every fixture twice.
- **Known residue (R5, diagnostic only):** `promptGuard.examples` in
  `--stats-json` is recorded in completion order on the pipelined path. The
  fix is to tally at `commit_prepared`. `peakLiveDispatches` and
  `peakLivePromptBytes` are timing gauges by nature.
- A new fan-out must say in its allowlist reason where its results are put
  back in order.

## Failures and retries

**Rule 7.**

- **A failed call is never silent.** After every retry, a failed LLM call
  leaves a hole in the names. R2 made that hole loud: the run prints
  `WARNING: N LLM call(s) failed after every retry`.
  - Since 2026-10-12 the WARNING covers EVERY namer (the waves and sweeps,
    the module namer, the vendor namer, the split namer, the tree reviser:
    `humanify-cli/src/provider_failures.rs`) and is printed once, after the
    split. `--stats-json` carries the same counts as `providerErrors` (a
    total plus each namer), and the eval refuses to score a card whose
    scored run or rebuilt base lost any (`experiments/lib/invariants.ts`).
  - Whether too many failures should make the run exit non-zero is Andrew's
    decision (audit recommendation 1a).
- **The target: retries in exactly one layer**, with full jitter and a cap
  on a call's total wall time, not only on each attempt.
- **Today there are two layers (R1, an open decision, marked as an
  exception on both sites):**

  - the limiter's `with_retry` makes 4 attempts with un-jittered 1/2/4 s
    backoff, holding its slot;
  - inside it, the SDK layer's `post_with_sdk_retries` makes 3 attempts
    with jittered 0.5/1 s backoff.

  That is 12 requests for one failing call (`rate_test.rs`
  `one_call_against_a_failing_server_sends_twelve_requests`), and about an
  hour per call against a server that accepts and then hangs. Collapsing
  the layers changes which calls succeed under failure, so it needs the
  fault-injection measurement in the audit before it can land.

- **A panic ends the run.** There is no `catch_unwind`, which is the right
  failure mode for a batch tool.

## Observers

**Rule 8.** A side thread or lock that exists to watch the pipeline:

- never feeds a value back into a decision or the output;
- exits when its owner goes away (the TTY redraw thread holds a `Weak`,
  O19), or is stopped and joined (the profiler's sampler, O18);
- tolerates poisoning, or cannot be poisoned in practice.

Today the observers are:

- the TTY progress renderer and its 250 ms redraw thread;
- the profiler (its global span recorder and its sampler, which nothing
  starts in production);
- the verbose and debug log redirects;
- the `--log-file` writer.

## What clippy enforces

`Cargo.toml` `[workspace.lints.clippy]` and `clippy.toml`. These run at
`rust:clippy` over every target, tests included.

| lint                                                                                 | catches                                                                                                                |
| ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| `await_holding_lock` (deny)                                                          | a std `Mutex`/`RwLock` guard held across `.await` (rule 2)                                                             |
| `await_holding_refcell_ref` (deny)                                                   | a `RefCell` borrow held across `.await`                                                                                |
| `await_holding_invalid_type` (deny)                                                  | any type listed in `await-holding-invalid-types`. The list is empty today; the lint is on so adding a type is one line |
| `disallowed_methods`: `std::thread::sleep`                                           | a blocking sleep (rule 2)                                                                                              |
| … `tokio::runtime::Builder::new_current_thread` / `new_multi_thread`, `Runtime::new` | a second runtime (rule 2)                                                                                              |
| … `tokio::spawn`, `tokio::task::spawn_blocking`, `tokio::task::block_in_place`       | detached tasks or the blocking pool (rule 2)                                                                           |
| … `reqwest::Client::new`                                                             | an HTTP client whose deadline nobody confirmed (rule 3, W2)                                                            |
| … `std::process::Child::wait_with_output`                                            | the W1 pipe deadlock (rule 4)                                                                                          |

**An allowed exception carries a reason.** Write
`#[allow(clippy::disallowed_methods)]` on the smallest item that needs it,
with a `// concurrency: <reason>` comment on the same line or within the
three lines above. The census fails an allow that has no such comment.
`// env: <reason>` is accepted for the environment-reader ban, which shares
the lint.

Today there are 18 allows: 3 in production code and 15 in tests. The
production three:

- `provider.rs` `runtime`: THE runtime;
- `client.rs` `OpenAiClient::new`: the deadline is per attempt;
- `progress.rs` `TtyRenderer::start`: the observer redraw thread's tick.

(`webcrack.rs` `unpack_webcrack`'s allow for `wait_with_output` went with
the move to `crate::subprocess`, 2026-10-12.)

The test allows cover private runtimes, `#[tokio::test]`, deliberate sleeps,
and the stub server.

**What clippy cannot see:**

- whether a timeout covers the whole operation;
- whether a lock is held across slow synchronous work;
- whether completion order reaches a record;
- whether a subprocess has a deadline;
- whether a counter is read by a decision.

These questions are what the census makes someone answer, once per site.

## The concurrency census

`npm run census:concurrency` (`scripts/concurrency-census.ts`) is gate stage
`census:concurrency`, and it is pass/fail.

**What it scans:**

- every `crates/**/*.rs` that is not test code;
- **test code is excluded:** `*_test.rs`, `tests/` directories, modules
  declared `#[cfg(test)] mod x;`, and inline `#[cfg(test)]` items. Tests
  build their own runtimes and sleep on purpose; the rules are for the code
  that makes the output. Clippy and the allow-comment check still cover
  tests.

**What it looks for:**

- `thread::spawn`/`scope`, `tokio::spawn`, `spawn_blocking`, `block_on`
  and runtime builders;
- rayon's `par_*`, pool and `join`/`scope`;
- `join_all` and `FuturesUnordered`, and `select!`;
- `Mutex`/`RwLock` (std and tokio), `Condvar`/`Barrier`, `Semaphore`, and
  channels;
- atomics, with `Relaxed` flagged separately;
- `Command::new`, `Stdio::piped`, and HTTP clients;
- sleeps and timeouts.

Comments, string literals and `use` declarations are blanked first, so a doc
comment that mentions `block_on` does not count.

**Site key:** a site is file + scope + construct.

- The scope is the enclosing function as `Type::fn`, or the item (for
  example `struct X` or `static X`).
- Several uses in one scope count as one site.
- Line numbers are not part of the key, so moving code within a function
  does not churn the allowlist. Moving it to another function makes it a
  new site to judge.

**The allowlist** (`scripts/concurrency-allowlist.json`) gives every site a
`pattern` from the table above and a `reason`. A known deviation from these
rules gets an `exception` field that names the open finding.

**It fails on:**

- an unlisted site;
- a stale entry, whose site is gone or has moved to another function;
- an entry that has no reason, or uses an unknown pattern or construct;
- a `Relaxed` entry whose pattern is not `stats-counter`;
- a duplicate entry;
- a `disallowed_*` allow with no reason comment.

For an unlisted site, the message names the construct, links the section of
this doc with its rule, and prints the exact entry to add:

```
  crates/demo/src/lib.rs:7  Pool::launch  —  tokio-spawn
    Rule (docs/concurrency.md#async-io): no detached tasks: every future is driven to completion by the provider's run_wave/run_pipelined
    If this is intended, confirm it: add to scripts/concurrency-allowlist.json:
      {
        "file": "crates/demo/src/lib.rs",
        "scope": "Pool::launch",
        "construct": "tokio-spawn",
        "pattern": "<one of: par-seam, llm-runtime, rate-limiter, single-flight, retry-envelope, deadline, subprocess, short-lock, stats-counter, observer>",
        "reason": "<why this site needs it, and how it meets the rule>"
      }
    If not, use the sanctioned primitive instead (docs/concurrency.md).
```

**This failure is a question, not a verdict.** It is the "did you really
mean this?" flag for whoever added the site, usually an agent. Either
confirm the site, by writing down which pattern it follows and why it meets
that pattern's rules, or switch to the sanctioned primitive.

**Pre-commit:** lint-staged runs the census on the staged `.rs` files
(`--files`) and reports only on those. A staged
`concurrency-allowlist.json` runs it in full. The full scan takes about a
second.
