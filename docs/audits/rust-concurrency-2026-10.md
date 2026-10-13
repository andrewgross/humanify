# Rust pipeline concurrency audit — 2026-10

Scope: every concurrency site in `crates/` (tokio, rayon, std threads,
locks, channels, atomics, subprocess pipes), audited against the hazards in
_"I am sorry, but you still have to think"_ (itsallaboutthebit.com):
blocking work inside a cooperative async runtime, sync locks held across
slow work or `.await`, bounded channels that silently drop or lag,
capacity and timeout values nobody chose on purpose, closed-loop
benchmarks, and failure modes nobody decided. It also covers what the
post does not: nested `block_on`, rayon inside tokio (and the reverse),
thread oversubscription, lock ordering, poisoning, unbounded growth, retry
storms, missing timeouts, cancellation, and **completion order leaking
into output** (the output must be byte-deterministic for the same input
and the same LLM answers).

Branch `audit-rust-concurrency`, off main `e214db7a`.

## Summary — totals first

28 sites audited: **2 BUG** (both fixed, red then green), **6 RISK** (one
fixed, five listed as recommendations), **20 OK**.

| #   | Verdict                                 | Site                                                  | Failure                                                                                                            | Status                                                          |
| --- | --------------------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------- |
| W1  | **BUG**                                 | `unpack/webcrack.rs` `unpack_webcrack`                | Deadlock: stdin is written in full before stdout/stderr are read                                                   | **FIXED**, reproduced red                                       |
| W2  | **BUG**                                 | `humanify-llm/src/client.rs` `attempt`                | No timeout on the response body: a stall after the headers hangs the call and its slot forever                     | **FIXED**, reproduced red                                       |
| R2  | **RISK** (failure mode)                 | `humanify-cli/src/unified.rs` naming outcome          | A failed LLM call silently drops names. The run exits 0 and the count showed only under `-v`                       | **FIXED** (WARNING line), reproduced red                        |
| R1  | **RISK** (retry storm)                  | `rate.rs` × `client.rs` retry envelope                | 12 requests per failing call, retries in sync, slot held. A hung server holds a call about 1 h                     | verified by test. Recommendation (changes output under failure) |
| R3  | **RISK** (silent discard)               | `finish/reconcile.rs:788` → `hunks.rs:39`             | Up to `nproc` concurrent `diff` subprocesses. A spawn failure silently discards that file's reconcile              | read only, not reproduced. Recommendation                       |
| R4  | **RISK** (oversubscription)             | rayon global pool (never sized)                       | One pool per process sized to every core. Three heavy runs on 64 cores means 192 CPU-bound workers                 | read only. Recommendation                                       |
| R5  | RISK (low, determinism of a diagnostic) | `processor.rs:1692/1712` `GuardTally::note`           | `promptGuard.examples` in `--stats-json` is recorded in completion order on the pipelined path                     | read only (real stats show `retryUnshown: 3`). Recommendation   |
| R6  | RISK (low, blocking in async)           | the runtime thread: `on_done`, disk cache, `-vv` sink | Sync CPU or IO on the only runtime thread delays every in-flight poll. The per-attempt timeout clock keeps running | read only, size not measured                                    |

**Status update, 2026-10-12** (branch `fix/failed-calls-and-deadlines`):

- **R2 — every namer now counted.** One WARNING covers the waves and sweeps,
  the module namer, the vendor namer, the split namer and the tree reviser
  (which failed silently). `--stats-json` carries `providerErrors` (total +
  per namer), and the eval refuses a card whose scored run or rebuilt base
  lost any call (recommendation 1c). The exit code is unchanged (1a is
  still open).
- **R3 — fixed (recommendation 2a).** A failed `diff` is counted as
  `diff_failed`, apart from the parse discards, and WARNED; the pre-split
  reconcile's error, which was dropped too, is now a WARNING.
- **W1 follow-up + `git` — fixed.** All three subprocesses run through one
  owner, `humanify_core::subprocess::run`, with one deadline over the whole
  child and a process-group kill: webcrack 30 min, `diff` 10 min, `git`
  1 min (docs/concurrency.md, "Subprocesses").

Everything that decides the **emitted tree** came out OK. The one place
where completion order meets decisions (the pipelined lanes) puts
everything back in canonical order before it is recorded, and three tests
already hold it there (see O7).

## The thread and runtime model

```
 main thread ───────────────────────────────────────────────────────────────────┐
  │  the whole pipeline runs here, sequentially, stage by stage                  │
  │                                                                              │
  ├─ par::map_ordered / produce_then_map / join ──► rayon GLOBAL pool            │
  │      (pure per-item maps, results in INPUT order; nproc workers, unsized)    │
  │      └─ finish::reconcile per file ──► spawns `diff` subprocesses (R3)       │
  │                                                                              │
  ├─ par::beside(bg, here) ──► 1 scoped std thread runs `bg` (pure work, may     │
  │      itself use rayon) while `here` stays on main (the oxc AST is !Sync)     │
  │      sites: prior.rs:172, naming/driver.rs:253 + :366, statement_align:1349  │
  │                                                                              │
  ├─ unpack_webcrack ──► `npx tsx webcrack-shim.ts` child, 3 pipes               │
  │      └─ scoped feeder thread writes stdin (W1 fix)                           │
  │                                                                              │
  └─ NameProvider::run_wave / run_pipelined (LlmClient)                          │
         runtime.block_on(...)  ── ONE current_thread tokio runtime, owned by    │
         │                         LlmClient, polled only inside block_on        │
         │  join_all(chunk ≤ window)  or  FuturesUnordered(≤ window)             │
         │  each call: AnswerMemo ─► RateLimited ─► DebugProvider ─► OpenAiClient│
         │              (per-key    (Semaphore =     (log sink)     (reqwest,    │
         │               async gate) max_concurrent,                timeout,     │
         │                           retries hold slot)             SDK retries) │
         │  on_done(id, result) runs ON the runtime thread between polls (R6)    │
         └─ reqwest/hyper connection tasks: spawned on the same runtime;         │
            DNS on tokio's blocking pool (only if the endpoint is a hostname)    │
                                                                                 │
 side threads that only OBSERVE:                                                 │
   TtyRenderer redraw (progress.rs:383, 250 ms, TTY only)                        │
   profiler concurrency sampler (profiling.rs:170, never started in prod)        │
```

Who calls whom, and what cannot happen:

- **tokio is never entered from rayon.** The pipeline holds the provider
  as `&dyn NameProvider`, and `dyn NameProvider` is not `Sync`. A closure
  that captures it cannot be handed to `par::map_ordered`, which needs
  `Sync`. This is a compile-time guarantee. No `NameProvider + Sync`
  bound exists anywhere.
- **rayon inside tokio is harmless here.** An `on_done` callback that runs
  a rayon map blocks the only runtime thread, and no tokio worker is
  starved because there is only one. The cost is latency (R6).
- **There are no nested `block_on` calls today, but nothing prevents one.**
  `on_done` returns follow-up calls and never calls the provider. It does
  have `this.provider` in reach through `&Self`, though. A future edit
  that called `run_wave` from inside `on_done` would panic: tokio refuses
  `block_on` inside a runtime. The panic is loud, not silent.
- **There is only one runtime.** `build_provider` runs once per run. The
  stub server in `stub_server.rs` builds its own runtime on its own
  thread, and it is test-only.

## Findings, most severe first

### W1 — BUG (fixed): the webcrack shim can deadlock on its pipes

`crates/humanify-core/src/unpack/webcrack.rs`, `unpack_webcrack`. Before
the fix, the bundle (multi-MB) was written to the child's stdin with
`write_all` **before** `wait_with_output` started draining stdout and
stderr. Suppose the child writes more than a pipe buffer (64 KiB on
Linux) before it has read all of its input: `npx` notices, a dependency
warning storm, a future shim that streams progress. The child then blocks
on its write and the parent blocks on its own. **The run hangs forever.**
No timeout surrounds it.

- **Reproduced.** In `unpack/webcrack/webcrack_test.rs`,
  `a_shim_that_writes_before_it_reads_does_not_deadlock`, a `sh` shim
  writes 256 KiB to stderr and then reads a 4 MiB input. Before the fix:
  `unpack_webcrack deadlocked on the shim's pipes: Timeout` at 30 s.
  After: ok in 0.01 s.
- **Fix.** A `std::thread::scope` feeder thread writes stdin while
  `wait_with_output` drains the other two pipes. Dropping the feeder's
  `ChildStdin` closes it, so EOF is unchanged.
  `a_shim_that_exits_without_reading_is_an_error_not_a_hang` pins the
  failing-shim path, which is still an error.
- **Output.** Unchanged: same bytes in, same bytes out. The real shim
  reads all of stdin before it writes, so this has not been seen in a
  real run. It is a latent hang.
- **Not fixed.** There is still no wall-clock timeout on the shim. A
  webcrack that loops forever hangs the run. It is listed under
  recommendations.

### W2 — BUG (fixed): the LLM call's timeout stopped at the response headers

`crates/humanify-llm/src/client.rs`, `attempt` and `post_with_sdk_retries`.
`tokio::time::timeout` wrapped only `send()`, which resolves at the
response headers. `response.text()` (the body) had **no timeout**, and
`reqwest::Client::new()` sets no read timeout and no TCP keepalive. This
was faithful to the openai SDK, which clears its AbortController once
`fetch` resolves.

Failure scenario: a proxy (nginx sits in front of the eval server) or a
half-open TCP link delivers the headers, and then the body never
finishes. The call's future never resolves, and it holds one rate-limiter
slot forever. The wave cannot finish, because `join_all` and
`FuturesUnordered` wait for every call, so **the run hangs**. It produces
no error and no output.

- **Reproduced.** In `client_test.rs`,
  `a_body_that_stalls_after_the_headers_times_out`, a raw TCP server sends
  `200` headers with `content-length: 1000` and 6 body bytes, then holds
  the socket. With `timeout_ms = 300`, the call was still pending when the
  test's 10 s guard fired: `the call hung on a stalled body: Elapsed(())`.
  After the fix it ends in `LlmErrorKind::Timeout` / "Request timed out."
- **Fix.** One deadline per attempt (`timeout_at`) now bounds the send and
  the body read together. A body timeout is a `Timeout`, and it retries
  exactly like a header timeout. The status, headers and body come back
  as one `Exchange`, so the non-2xx path reads its error body under the
  same deadline.
- **Output.** Unchanged unless a body takes longer than the per-attempt
  budget (300 s by default). A healthy server sends the body together
  with the headers.

### R2 — RISK (fixed: made loud): a failed LLM call is a silent hole in the output

`crates/humanify-cli/src/unified.rs`, after the naming stage. This is the
post's "decide the failure mode on purpose" point. When a call fails
after every retry (the endpoint is down, or keeps answering 5xx or 429),
the waves and sweeps take **no renames** for that function (`Tally::map`
→ `Err(())`, `sweep.rs` → `skipped += targets`). The run continues and
**exits 0**. The count (`outcome.errors`) was printed only under `-v`, and
it is not in `--stats-json`. The default report even reads "Asked, kept:
100%" for identifiers whose ask never reached a model.

A one-minute endpoint outage in the middle of an eval leg therefore ships
a degraded tree that looks normal. Worse, rule 11 says the eval cannot
tell a lost name from noise.

- **Reproduced.** `crates/humanify-cli/tests/provider_errors.rs` runs the
  release-shape binary against a dead endpoint. Before the fix: exit 0, 7
  calls failed, and no line at the default verbosity said so. After:
  `WARNING: 7 LLM call(s) failed after every retry (provider errors) —
their names are missing from this output; re-run when the endpoint is
healthy`.
- **What was changed.** A print-only WARNING through the renderer. No
  decision reads it, and the exit code and the fail-soft behaviour are
  unchanged. Changing the exit code is a policy decision for Andrew (see
  recommendations).
- **Still not covered.** The vendor namer's `batches_failed` (stats only)
  and the split namer's `failed_batches` have their own counters. The
  module namer's `failed_calls` is printed in the "Module names:" line but
  not as a WARNING.

### R1 — RISK (verified, recommendation): the retry envelope multiplies into a storm

`rate.rs` `with_retry` (limiter: `1 + retry_attempts` = 4 attempts,
backoff `retry_delay_ms · 2^n` = 1, 2, 4 s, **no jitter**) wraps
`client.rs` `post_with_sdk_retries` (SDK: `1 + sdk_max_retries` = 3
attempts, 0.5 s and 1 s with up to 25 % jitter). Both are on by default.

- **Verified.** In `rate_test.rs`,
  `one_call_against_a_failing_server_sends_twelve_requests`: a server
  that always answers 503 receives **12 requests for one call**. This is
  a characterization test that pins today's behaviour, not a fix.
- **Scenario A: the server is overloaded (429/5xx).** Every one of up to
  `max_concurrent` calls (concurrency plus module concurrency, e.g.
  32+16) retries ×12, and the limiter's un-jittered backoffs line up
  across calls. The extra load lands when the server is least able to
  take it. Each retry holds its rate-limiter slot through the sleep, by
  design (`rate.rs` header, step 3). That keeps the offered load bounded,
  which is good, but it also stalls the whole round behind the slowest
  failing call.
- **Scenario B: the server accepts and then hangs.** Each attempt runs
  the full 300 s, so one call takes about 12 × 300 s + 7.5 s ≈ **1 h
  before it fails**, and then R2 applies. Every slot does the same. The
  run sits idle for about an hour per wave and then ships a hole.
- **Scenario C: a client timeout under server-side queueing.** When the
  server queues requests (vLLM behind nginx at `-c 472`), queue time
  counts against the 300 s. The client gives up, vLLM aborts the request
  on disconnect, and the retry rejoins the back of the queue: work thrown
  away under sustained overload. The semaphore keeps the client
  closed-loop (offered load ≤ slots), so this cannot run away. It does
  waste server time.

These are recommendations, not fixes: any change here alters **which
calls succeed under failure**, and so the output on real bundles. See the
list at the end.

### R3 — RISK (not reproduced, recommendation): concurrent `diff` subprocesses with a silent discard

`finish/reconcile.rs:788` runs `reconcile_one_file` for every split file
on the rayon pool. Each call runs `naming/reconcile/hunks.rs:30`
`compute_normal_diff`, which writes two temp files and spawns `diff`. With
`nproc` = 64 workers, up to 64 `diff` processes and 128 temp files exist
at once. Any `Err` (spawn failure from EAGAIN, EMFILE or ENOMEM, a full
`/tmp`, or `diff` exiting 2) returns `FileWork::failed()`, which is a
**silent discard** of that file's reconcile. It is counted only in
`stats.discarded`, mixed with parse failures. The tree then differs from a
run where the spawn succeeded.

Resource exhaustion is the one way concurrency can reach the emitted
tree. The box is shared, with up to three heavy runs at about 19 GB each.

- **Not reproduced.** I did not force a spawn failure: it needs
  process-global `ulimit` or `PATH` changes. The discard path was
  verified by reading. The pre-split reconcile (`reconcile/step.rs:82`)
  runs one diff on the main thread, and there the `Err` becomes a skipped
  step with an error string.

### R4 — RISK (recommendation): the rayon pool is never sized

No `ThreadPoolBuilder` or `num_threads` call exists anywhere. The global
pool takes `nproc` workers, 64 on bahadur. `/work/HEAVY-RUN-RULE.md`
allows three heavy runs at once, which makes 192 CPU-bound rayon workers
on 64 cores, plus 64 concurrent `diff` children per run (R3). This costs
throughput and peak memory (each worker holds one file's oxc AST during
the per-file maps), not correctness: every map is `map_ordered`, which
returns results in input order. Rayon still honours `RAYON_NUM_THREADS`
on its own. No harness sets it.

### R5 — RISK (low; recommendation): completion order in a stats diagnostic

`processor.rs` `guard_note` / `guard_note_module` (lines 1692 and 1712)
tally the prompt guard while request builders run. On the pipelined path,
those builders run inside `on_done`, in **completion order**.
`GuardTally::note` keeps the first 20 `examples` as they arrive, so the
order of `promptGuard.examples` in `--stats-json` (and the selection, past 20) depends on completion order. The counts are commutative and fine.

The order matters only when unshown is non-zero. A real run did show it
non-zero (`/work/naming-collisions-2026-10-06/r5/stats.json`:
`retryUnshown 3`). `peakLiveDispatches` and `peakLivePromptBytes`
(`note_live`, `processor.rs:2481`) are completion-order gauges by nature.
Both are in `--stats-json` and on stdout, and neither is in the tree.

Suggested fix: tally at the canonical commit (`commit_prepared`) rather
than at dispatch. Not applied: it is diagnostic-only, and the replay
machinery (`replaying`) would need care.

### R6 — RISK (low; not measured): synchronous work on the only runtime thread

Every LLM future, hyper's connection tasks and all timers share the one
`current_thread` runtime, and the runtime only runs inside `block_on`. On
that thread, three kinds of synchronous work run between polls:

- `on_done` (`processor.rs:2432`): `feed_lane`, `step_lane`, request
  building and prompt rendering for the follow-up and backfill calls;
- `AnswerMemo::read_disk` / `write_through` (`cache.rs:143/160`): sync
  `std::fs` on every miss and write with `--llm-cache`;
- the `-vv` log sink: synchronous stdout writes of whole prompts, and
  every call is logged twice.

While one of these runs, no other response is read. tokio's `Timeout`
polls its inner future first, but that future is waiting on a hyper
connection task that has not been polled yet. So a response that arrived
before the deadline can still be reported as `Timeout` if the thread was
blocked past it. Retries then cost a second ask.

With a 300 s budget and millisecond-scale callbacks this needs a
pathological stall, and I did not measure `on_done`'s cost. This is the
post's central hazard. Here it costs latency, not correctness, because
nothing else shares the runtime. The fix, if `on_done` ever grows, is to
move preparation off the runtime thread (render on the main thread and
hand over through a bounded channel). That is not worth doing today.

## Sites judged OK, and why

| #   | Site                                                                            | What it does                                            | Why OK                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --- | ------------------------------------------------------------------------------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| O1  | `provider.rs:80` `new_current_thread` runtime                                   | One runtime per `LlmClient`, built once per run         | Single-threaded by choice: the futures are IO-bound, and one thread keeps every async/sync interaction on one thread (no `Send` hazards in `on_done`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| O2  | `provider.rs:149` `run_wave` = `block_on(join_all)`                             | One chunk of calls, results in call order               | `join_all` output is positional. Every caller chunks at the prompt window (`sweep.rs:863`, `:1153`, `namer.rs:559`, `processor.rs:2350`, `:2624`), so the number of live futures is bounded. A chunk is a closed-loop batch (it waits for its slowest call): throughput, not correctness                                                                                                                                                                                                                                                                                                                                                                  |
| O3  | `provider.rs:168` `run_pipelined` = `block_on(FuturesUnordered)`                | Completion-driven follow-ups                            | The only completion-ordered loop. See O7 for why its order never reaches output. `in_flight` is bounded by the window (`processor.rs:2408/2451`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| O4  | `rate.rs:82/171` `Semaphore`                                                    | The `max_concurrent` cap                                | tokio's semaphore is FIFO-fair. The permit is held across `.await` (it is an async primitive) and released on drop, so it cannot leak on cancellation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| O5  | `rate.rs:101` std `Mutex<VecDeque>` (rpm window)                                | Check and record one start stamp                        | Held for a deque scan in a block scope that ends **before** `sleep(...).await`. A short sync lock, the post's acceptable case. `requests_per_minute` is 0 in production                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| O6  | `cache.rs:186/366` per-key `tokio::sync::Mutex` (single-flight)                 | One key, one answer (finding #57)                       | An **async** lock held across the inner call's `.await`, exactly as the post prescribes. **Lock ordering:** key gate (outer) → semaphore slot (inner), always. Waiters queue on the gate WITHOUT a slot, so slots cannot all be held by waiters (no deadlock). Which copy asks first depends on completion order, which is harmless: the prompt is a pure function of the request (`processor.rs` `rebuild_record`: `render_system_prompt(&request)` / `render_user_prompt(&request, profile)`), so every copy sends the same bytes                                                                                                                       |
| O7  | `processor.rs:2390–2500` `drive_round_pipelined`                                | Lanes fed in completion order                           | Decisions read only the round's frozen state plus the lane's own claims. Everything recorded is re-sorted: `records.sort_by_key((turn, lane))` (2477), `finished.sort_by_key` (2482), `retry_results[id]` indexed, `Tally` = commutative counts. **Tested:** `naming/driver/fast_test.rs` `relaxed_levers_are_deterministic_and_order_independent` (LIFO completion vs generational → identical fingerprint), `crates/humanify-cli/tests/replay_determinism.rs` (stub answers out of order with varying delay → replay byte-identical, zero misses), and the e2e stage (every fixture twice). Residual completion-order effects are diagnostics only (R5) |
| O8  | `cache.rs:201/202` std `Mutex<HashMap>` (in-flight gates, answers)              | Map get or insert                                       | Never held across `.await` or IO. Never nested (no lock-order cycle). Poisoning is impossible in practice (no panicking code inside), and if it happened `.expect` would fail loud                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| O9  | `cache.rs` `answers` growth                                                     | Every answer kept for the run                           | Unbounded but small: about one entry per distinct ask (tens of thousands × about 1 KB = tens of MB on a full bundle). It is the single-flight's correctness store, so bounding it would break "one key, one answer"                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| O10 | `cache.rs:276` `release` and cancellation                                       | Drop the gate when unshared                             | `strong_count <= 2` under the map lock. A late copy that finds no gate re-checks `recorded` first, and the answer is remembered BEFORE release, so it is served the same answer. If a future were dropped mid-call (no production path drops one: `block_on` runs every future to completion, and a panic ends the process, which has no `catch_unwind`), the gate entry would leak harmlessly and the next copy reuses it                                                                                                                                                                                                                                |
| O11 | `cache.rs:148` atomic disk write                                                | `<target>.<pid>.tmp` + rename                           | Single-flight means one writer per key per process. Different pids give different temp names, and rename is atomic                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| O12 | `client.rs:65` `reqwest::Client::new()`                                         | One HTTP client, pooled connections                     | Built outside the runtime, which reqwest allows; the runtime is entered at request time. Pool size is unbounded but follows the semaphore's in-flight count. No connect timeout of its own: the per-attempt deadline covers connect                                                                                                                                                                                                                                                                                                                                                                                                                       |
| O13 | `par.rs:14` `map_ordered`, `:26` `produce_then_map`, `:48` `join`               | The one rayon seam                                      | Indexed `collect` writes each result into its input slot: order by construction. Closures must be `Fn + Sync`, and no core closure captures a lock or an atomic (grep: no `Mutex` or `Atomic` in core except profiling, the `PRIOR_PARSES` counter and the temp-dir sequence)                                                                                                                                                                                                                                                                                                                                                                             |
| O14 | `par.rs:59` `beside` (scoped std thread)                                        | Pure background work beside the AST-holding main thread | `std::thread::scope` joins on every path, and a background panic is re-raised. A caveat in the post's "fail fast" sense: a panic in `bg` surfaces only after `here` returns. At `naming/driver.rs:253` `here` is the whole naming era, so minutes of LLM calls run before the run dies. Correctness is unaffected                                                                                                                                                                                                                                                                                                                                         |
| O15 | `prior.rs:172` prior side on its own thread                                     | Two independent parses                                  | Each thread has its own allocator and parse. Plain data crosses the join. The code comments that both parses are deterministic, and the e2e prior leg (twice) holds it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| O16 | `finish/reconcile.rs:788` per-file map                                          | Pure per-file reconcile                                 | Rejoined in ledger order (`for (file, work) in files.iter().zip(works)`). The `diff` subprocess hazard is R3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| O17 | `hunks.rs:58` `SEQ` atomic + pid + nanos temp dir                               | Unique temp dir per diff                                | Names only. `create_dir` refuses an existing directory and the loop retries                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| O18 | `profiling.rs:227` `GLOBAL` `RwLock` + `Mutex<Recorded>`, sampler thread `:170` | `--profile` spans from any thread                       | Every lock is poison-tolerant (`unwrap_or_else(PoisonError::into_inner)`). Observation only. Span order depends on thread timing, and the profile is not output. The sampler is never started in production (no callers of `start_concurrency_sampling`), and it is stopped through a channel and joined                                                                                                                                                                                                                                                                                                                                                  |
| O19 | `progress.rs:383` TTY redraw thread                                             | 250 ms redraw                                           | Holds a `Weak` and exits when the renderer drops or finishes. The lock is held across a terminal write (a blocking write on a stalled TTY would stall `message()` too). `.lock().unwrap()` would cascade a panic if the sink ever panicked mid-redraw (`print!` on EPIPE). `Drop` is already poison-tolerant. TTY only; the gate and eval use the scripted renderer                                                                                                                                                                                                                                                                                       |
| O20 | `log.rs:13–106` `VerboseLogger` `RwLock`/`Mutex`, `DEBUG_OUTPUT`                | Process-wide log redirect                               | Every call site runs on the main/runtime thread, so the locks are uncontended. They are held across a write by design (whole lines never interleave). Poisoning would need a panicking writer                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

Also checked and clean: no `spawn_blocking`, `select!`, broadcast or mpsc
channels in production. The only mpsc is the profiler's stop signal and
the test stub's address hand-off. There are no global id counters that a
parallel stage could interleave: the only process-global atomics are
`PRIOR_PARSES`, a diagnostic count, and the temp-dir sequence. There is
no `catch_unwind`: a panic anywhere ends the run, which is the right
failure mode for a batch tool. The test stub server (`stub_server.rs`)
calls a sync handler inside its async task, and `rate_test`
`a_timed_out_request_is_retried` deliberately `thread::sleep`s in it to
block the stub. That is test-only and intentional.

## Recommendations that need a decision or a measurement

None of these is applied. Each changes behaviour under failure or load,
so each can change output on real bundles.

1. **R1 + R2: decide the failure policy, then build it.** Options, in
   order of preference:

   - (a) A run-level error budget. If more than N calls (or X %) fail
     after retries, exit non-zero with the WARNING. A degraded tree is
     worse than no tree for the eval, where rule 11 cannot see it.
   - (b) Collapse the two retry layers into one, keep the attempt count
     deliberate (e.g. 4), add full jitter to the limiter's backoff, and
     cap a call's TOTAL wall time, not just per attempt.
   - (c) Add `providerErrors` to `--stats-json`, so the eval can refuse a
     card from a run that lost answers. That is a schema change:
     `test/parity/wpb4-stats-schema.json`.

   Measurement: a fault-injection run (a stub that 503s or hangs for a
   window) for the policy, plus `experiments/lib/neutrality.sh <base>`
   WARM with `--cache /work/neutrality-cache` to show a healthy run is
   byte-identical. A warm cache never reaches the retry path, so this
   proves only the healthy-path half.

2. **R3: make a failed `diff` loud or impossible.**
   - (a) Count spawn and diff failures apart from parse discards, and
     WARN. This is print-only and could be applied without a
     measurement.
   - (b) Compute the normal diff in-process. That removes 64 concurrent
     children and their temp files, and it needs the
     `format-goldens`-style byte check against `diff`'s output on a real
     tree, then a WARM neutrality run.
3. **R4: size the rayon pool deliberately.** Have `heavy-run.sh` export
   `RAYON_NUM_THREADS=$(( nproc / slots ))`, or add a `--threads` flag
   that calls `ThreadPoolBuilder::build_global`. Output is unaffected
   (O13). Measure wall time and peak RSS of one full-bundle run at 64 vs
   21 threads, alone and with three runs in parallel.
4. **W1 follow-up: put a wall-clock timeout on the webcrack shim.** Kill
   the child after it expires. Pick the value from the slowest observed
   webpack fixture × 10, and it should fail the run loudly.
5. **R5: tally the prompt guard at the canonical commit** if
   `promptGuard.examples` is ever compared across runs. Diagnostic only.
   Verify with the existing fast-mode LIFO-vs-generational test extended
   to compare `processor.prompt_guard`.
6. **Benchmarking (the post's open-loop point).** `--simulate-llm-latency`
   (`llm_sim.rs`) and the eval are closed-loop by construction. That is
   the right model for a batch client, since our offered load is the
   semaphore. The open question is the server side: at `-c 472` across
   one 8-GPU vLLM, how long do requests queue, and how often do 300 s
   timeouts fire (scenario R1-C)? The `-vv` roundtrip `duration_ms`
   already records it. A histogram from one cold run would answer it.
