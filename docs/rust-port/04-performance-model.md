# Performance model: what a Rust/oxc port buys, from measured data

**Status: projections current as of 2026-08-27, none yet replaced by a
measurement.** When the phase-1 criterion bench lands
(`11-risk-register.md` R12), the superseded numbers here get edited in place
with a dated note — this file must stay the newest document about its own
claims (measurement-pitfalls rule 9; upkeep rule in `10-work-breakdown.md`
§4).

Every number in the "today" columns below is measured, with its source named.
The "projected" columns are estimates and are labelled as such; the section at
the end says how to firm them up cheaply before believing them (this repo's own
measurement rules apply to this document too).

## The headline

**A cross-version hop is ~85% single-threaded CPU.** The LLM is not the
bottleneck: true LLM wall time at concurrency 32 is 75–135 seconds per hop,
inside runs of 9–17 minutes. The intuition "we are mostly waiting on the
model" is wrong by roughly a factor of six, and the CPU share is exactly the
part a Rust port with real parallelism attacks. Rust does not shrink the LLM
minute-or-two; it deletes most of everything else.

## Where a hop's time goes today

Wall time per pair, cold eval runs (`results/baseline-2026-08-03/*-run.json`,
confirmed within ±4% by `session-2026-08-05`):

| pair    | wall    | LLM calls | avg call | LLM service time | ÷32 concurrency ≈ LLM wall | CPU share |
| ------- | ------- | --------- | -------- | ---------------- | -------------------------- | --------- |
| 85→86   | 532 s   | 1,598     | 1.50 s   | 2,389 s          | ~75 s                      | ~86%      |
| 118→119 | 726 s   | 3,541     | 1.20 s   | 4,239 s          | ~132 s                     | ~82%      |
| 197→198 | 945 s   | 2,618     | 1.65 s   | 4,307 s          | ~135 s                     | ~86%      |
| 215→216 | 1,013 s | 2,046     | 1.77 s   | 3,615 s          | ~113 s                     | ~89%      |

Walk-scale corroboration (124 hops, `~/Development/unpacked-claude-code/logs/`,
RUN 4): median **7.8 min/hop**, p90 18.8, and after the ephemeron fixes the
largest-bundle era (2.1.176–216) runs **median 7.4 / max 10.9 min**. The
20–30 minute experience matches the pre-fix big-bundle era (28–40 min spikes,
`docs/issue-ephemeron-cache-thrash.md`) and eval-with-rebase sessions (each
leg is 8 pipeline runs, ~50 min, exp048); a single current hop is 8–17 min.

Phase anatomy of a real 9m34s 215→216 hop
(`results/baseline-main/2.1.216.log`, timestamps between phase log lines):

| phase                                | duration | LLM busy? |
| ------------------------------------ | -------- | --------- |
| parse + graph, fresh bundle (19.5MB) | 53 s     | idle      |
| parse + graph, prior bundle (31.9MB) | 137 s    | idle      |
| prior-version matching               | 91 s     | idle      |
| LLM naming window                    | 173 s    | busy      |
| floor + reconcile + deferred sweep   | 81 s     | idle      |
| split (10 phase stamps)              | 15 s     | idle      |
| bun relink (3,146 files)             | 12 s     | idle      |

`RUNNER.md` quotes the same shape for an ~11 min hop: ~3 min parse+graph,
~2 min matching, ~6 min naming, ~40 s split/relink — "an idle LLM does NOT
mean a stalled walk."

**Two measured facts sharpen the picture:**

1. **The naming pass is itself mostly CPU.** On 215→216 cold, the naming pass
   took 495 s (`coverage.elapsedMs`) while its calls sum to 3,615 call-seconds
   — effective parallelism **7.3× against a configured ceiling of 32**. The
   single-threaded process cannot build prompts, validate answers, and apply
   renames fast enough to keep 32 requests in flight. A port that keeps the
   server saturated compresses this phase toward service/32 ≈ 113 s even
   before any per-call CPU speedup.
2. **The GC tax is a measured pathology class, not a suspicion.** Peak RSS is
   15–30 GB; the eval needs a 64 GB heap because 215→216 OOMs at 14 GB cold;
   three separate incidents (ephemeron/WeakMap rehash, retained prior AST,
   scope-tree refill at 4.4× the fresh-build cost) each burned tens of minutes
   per hop until manually fixed. In an arena model these are non-events: drop
   the allocator at the stage boundary.

## Component projections (estimates)

Basis: oxc parses real-world JS at >100 MB/s/core with semantic analysis at a
comparable cost (Babel measures in the tens of seconds on these bundles);
`oxc_codegen` prints a 34 MB tree in ~1–2 s versus a full `@babel/generator`
pass; our own algorithms (hashing, cascade, placement) are allocation-heavy
tight loops — the class where native code with no GC typically runs 5–20×
per core, and they parallelize per-function/per-statement with rayon across
8–12 cores.

| component (215→216 cold)            | today       | projected        | basis                                                        |
| ----------------------------------- | ----------- | ---------------- | ------------------------------------------------------------ |
| parse + semantic + graph, both ASTs | ~190 s      | ~5–10 s          | oxc parse+semantic on 51 MB total; graph walk rayon-parallel |
| matching (64k fns, 200k bindings)   | ~91–120 s   | ~5–15 s          | 388k slot attempts, embarrassingly parallel, no GC           |
| naming-pass CPU orchestration       | ~380 s      | ~10–20 s         | prompt build + validation + 175k validated renames           |
| LLM wall (service ÷ 32)             | ~113 s      | ~110–135 s       | **unchanged — not a language problem**                       |
| floor / reconcile / sweep           | ~81 s       | ~5–10 s          | re-parse+re-generate cycles become ~1 s each                 |
| generate + split + relink + gates   | ~60–80 s    | ~5–10 s          | oxc_codegen + text slicing + parallel relink                 |
| **hop total**                       | **~17 min** | **~2.5–3.5 min** | LLM-bound at last                                            |

## Bottom line across the workflows (projected)

| workflow                             | today                    | projected    | note                                                        |
| ------------------------------------ | ------------------------ | ------------ | ----------------------------------------------------------- |
| Cold hop, biggest pair               | ~17 min                  | ~2.5–3.5 min | ~5–6×; floor is the LLM wall                                |
| Median walk hop                      | ~7.8 min                 | ~2–3 min     | ~3×; walk hops are smaller and already post-fix             |
| Full 124-hop walk                    | ~35 h                    | ~5–7 h       | becomes LLM-bound end to end                                |
| Neutrality pair (warm, both legs)    | ~10 min                  | ~1–2 min     | warm = zero live calls = pure CPU = the best case for Rust  |
| Eval 4-pair sweep (8 runs + scoring) | ~81 min                  | ~25–35 min   | pipeline ~15–25 min; **scoring/analyze (~27 min) stays TS** |
| Peak memory                          | 15–30 GB RSS, 64 GB heap | ~2–4 GB      | enables laptop runs and parallel pair evals                 |

Two second-order effects worth as much as the raw speed:

- **The verification loop compounds.** Neutrality and warm A/B probes are the
  runs done most often while iterating, they involve zero live LLM calls, and
  they are therefore the runs Rust accelerates the most (~10×). The cost of
  _proving_ a change safe drops more than the cost of running it.
- **The remaining LLM wall has its own, language-independent lever.** 3,615
  call-seconds at c=32 is 113 s; the observed 7.3× effective parallelism means
  today's client never reaches that. Once orchestration is free, the next
  question is server-side: whether the local vLLM instance sustains more than
  32 concurrent at acceptable latency. That experiment costs nothing and pays
  in any language.

## What this projection is NOT

Per this repo's measurement rules (`docs/measurement-pitfalls.md`):

- These are **mechanism-derived estimates, not measurements** (rule 11 in
  spirit: a ceiling computed before the run). The soft numbers are the
  5–20×/core native factor for our own loops — V8 JITs hot paths well — and
  the rayon scaling factor. The hard numbers are the LLM wall (measured) and
  parse/codegen (well-benchmarked upstream).
- The projection assumes the port keeps the algorithms; a port that also
  changes decisions is measuring two things at once (rule 10's spirit:
  hold one variable fixed — the migration plan's parity gates exist for this).
- **Cheapest way to firm this up before writing any Rust:** the span profiler
  already exists (`src/profiling/profiler.ts`, spans for parse, graph-build,
  prior-version, rename, generate, split) and is **never enabled** — neither
  the eval nor the walk passes `--profile`. One profiled run per pair turns
  the phase-anatomy table above from log-timestamp reconstruction into a
  Chrome trace, per phase, for free.
