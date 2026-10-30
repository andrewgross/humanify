# Ordering audit (exp163) — 2026-10-30

Andrew's directive: the determinism/ordering rules were born for TS-parity
comparability; the TS pipeline was deleted at the cutover (tag `m4`,
2026-09-26), and speedups matter more than order for its own sake —
_"we will definitely see blocking if we force a particular order upon
concurrency instead of working through items as fast as possible."_

This audit re-justifies every determinism/ordering requirement against
TODAY's gates, classifies every ordered-collect site by whether ordering
actually blocks, and lands the mode split: ordering becomes a TEST-MODE
property (`run.order = "ordered"`), never a production property.
Production's guarantee is canonical OUTPUT, not ordered WORK.

Read with [`docs/concurrency.md`](../concurrency.md) (rewritten in the same
commit) and the audit it came from
([`rust-concurrency-2026-10.md`](./rust-concurrency-2026-10.md)).

## Totals

- **15 requirements inventoried** (§1): **11 keep** (all but two keep with a
  rewritten justification), **2 re-justify into the test mode**, **2 cut as
  framing only** (the rule survives; the TS-parity reason dies). **No rule is
  deleted** — every one backs a live gate or a correctness property; what
  aged is WHY the docs said it existed.
- **The gates that back determinism are all alive and all byte-identity
  gates**: e2e's run-twice, the eval's warm self-hop, and neutrality. That is
  the whole justification today; TS parity appears nowhere.
- **Ordered-collect census (§2)**: 30+ fan-out sites classify as
  **reorder-buffer (free)** — including the entire parallel-era work
  (close-contexts, twin inventories, reconcile, module lanes, the pipelined
  wave driver). The **blocking collects** are the chunked `run_wave` loops
  (5 sites, measured §5) and the two opt-in sequential drivers.
- **The one completion-order leak left in production is R5** (a stats
  diagnostic), unchanged from the 2026-10 concurrency audit.
- **The mode split (§4)**: `run.order = "throughput" (default) | "ordered"`.
  Ordered forces `FastTier::Off` (the turn driver — the old
  "parity-faithful" path, now repurposed), sequential `par`, and one-call
  dispatch windows. Default runs never pay for it. Proven at integration
  level: an ordered replay of a conservative-schedule cache has zero
  misses, zero writes and a byte-identical tree (the pin in
  `tests/replay_determinism.rs`).

## 1. The inventory

For each requirement: the live gate that breaks if it is dropped, the
TS-parity-era origin, and the verdict.

| #   | Requirement                                                                               | Live gate it backs today                                                                                                                                                                                                                         | TS-parity-era origin                                                                                                                                         | Verdict                                                                                                                                                                                                                                |
| --- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| a   | Parallel results come back in input order (concurrency rule 1, O13)                       | neutrality (a parallel stage must be indistinguishable from its sequential loop or the refactor gate is meaningless); e2e run-twice                                                                                                              | "porting lesson 4: iteration order is a decision input" (`par.rs` header) — the TS pipeline's deterministic-iteration port rule                              | **KEEP**, rewrite the justification                                                                                                                                                                                                    |
| b   | Completion order never reaches the emitted tree or the stats (concurrency rule 6, O7/R5)  | e2e run-twice; the eval's warm self-hop (byte-identity + 0 cache writes); neutrality                                                                                                                                                             | O7 was found while porting the TS pipelined lanes' behaviour; the "byte-identical for the same input AND the same LLM answers" phrasing predates the cutover | **KEEP** — but the rule is now about canonical OUTPUT; ordered WORK moves to the test mode (§4)                                                                                                                                        |
| c   | e2e runs every fixture twice and requires byte-identity                                   | `npm run check`'s `e2e` stage — alive, runs on every gate                                                                                                                                                                                        | born to prove the Rust pipeline matched the TS (the legacy goldens)                                                                                          | **KEEP** — today it proves run-to-run determinism of the Rust binary alone                                                                                                                                                             |
| d   | Warm self-hop byte-identity + 0 cache writes                                              | the eval's self-hop half (`00-control` §3) — alive on every scored run                                                                                                                                                                           | 00-control (2026-09-25), post-parity                                                                                                                         | **KEEP**                                                                                                                                                                                                                               |
| e   | Neutrality: byte-identical trees for should-change-nothing edits (WARM only)              | the refactor gate — the cheapest and strongest "changed nothing" proof                                                                                                                                                                           | born 2026-08-03 in the diagnostics arc, post-parity                                                                                                          | **KEEP** — this is the gate the determinism rules actually serve                                                                                                                                                                       |
| f   | `novel`/`realLn` have a measured zero band                                                | the leaderboard's judgement of eval deltas                                                                                                                                                                                                       | a MEASUREMENT property (byte-equal across three same-commit cold repeats), never a rule anyone enforces                                                      | **KEEP as a measurement fact; not a rule** — nothing to enforce or drop                                                                                                                                                                |
| g   | The index id is `manifest.json`'s sha256 (`humanify-index/src/cache.rs`)                  | the index loader's refusal of a mismatched id; exp161/162's identity proofs                                                                                                                                                                      | 2026-10 (index arc), post-parity                                                                                                                             | **KEEP trivially** — content-deterministic, no ordering rule involved. No ordering requirement may cite it as its justification                                                                                                        |
| h   | Hash-map iteration order never reaches a decision                                         | e2e run-twice (std `HashMap` seeds per process, so an unordered leak differs across RUNS — the gate catches it, not a reviewer)                                                                                                                  | "iteration order is a decision input" (the port rule)                                                                                                        | **KEEP** as a determinism rule. It is not correctness-in-one-run (a single run's output is not _wrong_), but every gate this repo has is a byte-identity gate, so a non-reproducible output is wrong in every way the repo can measure |
| i   | The match list's insertion order drives rename order (`docs/matching-architecture.md` §8) | none — this is OUTPUT-affecting behaviour: a different order yields different NAMES with the same matches. Changing it is an eval-gated matching change (perf-inventory item 7's canonical-tie-break class), NOT a refactor neutrality can clear | the order is inherited from the TS `MatchMap` (JS `Map` insertion order)                                                                                     | **KEEP the order** (it is shipped behaviour). **Re-justify the doc line**: not a parity requirement — a deliberate, eval-gated property. Speed work must not touch it without an eval                                                  |
| j   | `FastTier::Off` — the turn driver ("the parity-faithful driver", `processor.rs`)          | the Off ≡ Exact byte-identity tests (`fast_test.rs`) and e2e's legacy goldens                                                                                                                                                                    | kept at the cutover because it reproduced the TS turn loop's bytes                                                                                           | **REPURPOSE (§4)** — it becomes the ordered test mode's driver. The parity framing is cut; the code is load-bearing again                                                                                                              |
| k   | The `par` seam: closures pure, `Fn + Sync`, no tokio from rayon (O13)                     | correctness (compile-time separation of the LLM runtime from rayon; no lock/atomic captured into a worker) + rule a's order                                                                                                                      | O13 (2026-10 audit)                                                                                                                                          | **KEEP** — correctness, not parity                                                                                                                                                                                                     |
| l   | `beside` joins on every path and re-raises a background panic (O14)                       | correctness (a background panic must not be swallowed)                                                                                                                                                                                           | O14                                                                                                                                                          | **KEEP**                                                                                                                                                                                                                               |
| m   | The rayon pool is unsized (R4)                                                            | nothing (a throughput/memory risk, not an ordering rule)                                                                                                                                                                                         | R4                                                                                                                                                           | **KEEP as an open perf decision** — out of scope here; see the 2026-10 audit's recommendation 3                                                                                                                                        |
| n   | `promptGuard.examples` lands in completion order on the pipelined path (R5)               | nothing (a `--stats-json` diagnostic; the counts are commutative and exact)                                                                                                                                                                      | R5 (2026-10 audit) — found on the ported pipelined lanes                                                                                                     | **KEEP documented** — the one known rule-6 residue; diagnostic-only. Fix (tally at `commit_prepared`) stays available if the examples are ever compared across runs                                                                    |
| o   | Wave rounds run one after another (the round barrier)                                     | correctness/data dependency: a round's lanes read the state the previous round's renames froze — collapsing rounds changes which call sees which name                                                                                            | partly order (the barrier also carries the relaxed tier's cross-window contention resolution)                                                                | **KEEP the dependency; keep it out of "ordering" accounting** — the real lever is the standing wave-barrier redesign (perf-inventory item 8, ~200 s of LLM wall), which is design work with its own gate, not a cut                    |
| p   | The chunked `run_wave` dispatch windows (sweep, split namer, retries, the turn driver)    | finding #65: the rendered-prompt window — a MEMORY bound; a run renders at most `window` prompts at once                                                                                                                                         | #65 (2026-09), post-parity                                                                                                                                   | **KEEP the memory bound; classify the barrier side effect** (§2, measured §5). These are NOT order-for-its-own-sake                                                                                                                    |
| q   | e2e/eval legs run serially                                                                | the heavy-run rule (memory), not ordering                                                                                                                                                                                                        | —                                                                                                                                                            | **KEEP** — inherently serial by resource budget                                                                                                                                                                                        |

### The two framing cuts

1. **`par.rs`'s header** cited "porting lesson 4: iteration order is a
   decision input" — the TS-parity reason. Rewritten: input-order results are
   what make a parallel stage byte-indistinguishable from the sequential loop
   it replaced, which is what neutrality (e) proves and what lets a reviewer
   read every parallel map as its `.iter().map().collect()`.
2. **`drive_round_turns`'s "parity-faithful driver"** comment. The driver is
   no longer kept for parity: it is the ordered test mode's driver (§4).

## 2. Where ordering could actually block — the classification

Andrew's concern, made concrete. For every ordered-collect implementation:
does any stage wait for item _i_ before starting/finishing item _i+1_'s
downstream work (a barrier dressed as ordering), or does it collect all
results and reorder at the end (free)?

**Classes.**

- **reorder-buffer (free)** — work proceeds as fast as the pool/slots allow;
  results land in canonical order at the end. Dropping "order" costs nothing
  because the order is applied where the results are consumed, not while the
  work runs.
- **blocking-collect (costs)** — the shape of the collect itself makes
  something wait. The cost is in the code's shape: what waits on what.
- **inherently-serial** — the work is a dependency chain; no ordering rule
  creates the serialism.
- **test-mode (opt-in)** — ordered operation available behind `run.order`,
  never paid by a default run.

| Site                                                                                                                                                                                                                                                | Class                        | What waits on what, and the cost shape                                                                                                                                                                                                                                                                                                                                                                        |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `par::map_ordered` — all ~25 call sites (twins, graph rows, vendor pairing/content/identity, statement hashes/align, bun unpack, family-permute, libindex units, index merge/slim, era items, library_names, reconcile per-file, proximity windows) | **reorder-buffer**           | rayon's indexed collect writes each result into its input slot while the pool works every item it can reach. Nothing waits for item _i_ to start item _i+1_. Free by construction. This is where the 39–50-core spans (close-contexts 1.7 s at 39.9 cores, twin-inventories 0.35 s at 49.7 cores, reconcile 10.8 s → 1.8 s at 7.4 cores) all live — the parallelism the ordering rule does NOT cost           |
| `par::produce_then_map` (graph.rs binding fingerprints)                                                                                                                                                                                             | **blocking-collect (small)** | the producer runs on the calling thread chunk by chunk; chunk _n+1_'s produce cannot start until chunk _n_'s map finishes, so the pool idles during produce and the producer idles during map. One chunk boundary per `BINDING_HASH_CHUNK` items. Not reworked: the produce half (serializing AST content) and the map half (hashing) are the same order of work and the site is seconds per hop, not minutes |
| `par::beside` / `par::join` (prior sides, statement_align, driver spec-trail)                                                                                                                                                                       | **reorder-buffer**           | two independent computations, joined at the end. Free. (O14's join guarantee is correctness, not ordering.)                                                                                                                                                                                                                                                                                                   |
| `drive_round_pipelined` (the waves' default driver)                                                                                                                                                                                                 | **reorder-buffer**           | bounded window + backlog backfill: a slot freed by ANY completion starts the next backlog item; a lane's follow-up starts the moment ITS answer lands. Everything recorded is re-sorted to canonical `(turn, lane)` order after the round. This is the work-through-as-fast-as-possible design already in production                                                                                          |
| wave rounds (one round's lanes built after the previous round's renames commit)                                                                                                                                                                     | **inherently-serial**        | a data dependency (the round's reads are frozen), not an ordering rule. The redesign (prefix-defined snapshots) is perf-inventory item 8, a standing B-item with its own gate                                                                                                                                                                                                                                 |
| `drive_round_turns` (Off tier)                                                                                                                                                                                                                      | **test-mode**                | turn barrier: every active lane's next call is dispatched together and the turn waits for its slowest. Reachable only under `run.order = "ordered"` (§4)                                                                                                                                                                                                                                                      |
| `run_retries` chunked dispatch (`processor.rs` ~2624)                                                                                                                                                                                               | **blocking-collect**         | fresh runs issue thousands of retry seeds in chunks of `prompt_window` (64, or the concurrency bound + headroom); each chunk's `join_all` waits for its slowest call before the next chunk dispatches, so in-flight drains to zero at every chunk edge. Measured §5                                                                                                                                           |
| `sweep_minted_names` groups (`sweep.rs` ~893)                                                                                                                                                                                                       | **blocking-collect**         | same shape over the sweep's groups; the rows are recorded at request-build time, so dispatch chunking never affects record order — only the LLM wall. Measured §5                                                                                                                                                                                                                                             |
| sweep retry rounds (`sweep.rs` ~1164)                                                                                                                                                                                                               | **blocking-collect**         | same shape over the reask rounds                                                                                                                                                                                                                                                                                                                                                                              |
| `ProviderSplitNamer::name` (`place/assign/namer.rs` ~563)                                                                                                                                                                                           | **blocking-collect**         | same shape over the split namer's batches (module naming included)                                                                                                                                                                                                                                                                                                                                            |
| vendor namer (`vendor_names.rs`)                                                                                                                                                                                                                    | **reorder-buffer**           | one call at a time on the main thread — no barrier exists to remove (it is serial because it is one call, not because ordering forbids more)                                                                                                                                                                                                                                                                  |
| the turn driver's `dispatch` chunks (`processor.rs` ~2360)                                                                                                                                                                                          | **test-mode**                | the Off tier's turn dispatch; ordered mode only                                                                                                                                                                                                                                                                                                                                                               |
| index builder lanes / `--merge-jobs` partitions                                                                                                                                                                                                     | **reorder-buffer**           | exp161/162's lanes: partitioned key ranges, byte-identical rejoin (jobs16/jobs30 ≡ serial). Owned by those lanes; not audited here                                                                                                                                                                                                                                                                            |

**The chunked `run_wave` barrier's cost shape.** A chunk of W calls on C
semaphore slots runs `ceil(W/C)` rows of near-parallel calls, and between
chunks the in-flight count drains to zero before refilling. Two prices:

- the tail: each row waits for its slowest call, and the cold latency
  distribution is heavy-tailed (the sim's quantile table: median ≈ 0.65 s,
  p97 ≈ 4.4 s, p100 = 20 s) — a row of 32 calls waits for roughly the max of
  32 draws;
- the drain: slots idle while the last row of a chunk finishes, then refill
  from zero.

The pipelined driver pays neither: its backlog backfill keeps the slots full
from the round's first dispatch to its last. The measured gap between the
chunked sites and that bound is §5.

## 3. What the 2026-10 concurrency audit actually found

Re-read of `rust-concurrency-2026-10.md`'s ordering-relevant findings:

- **O13** (the `par` seam): a CORRECTNESS finding — closures must be pure and
  `Fn + Sync`, no tokio from rayon, panic safety. Its input-order clause is
  rule (a). Nothing here was a parity convenience.
- **O14** (`beside` joins): CORRECTNESS — a background panic must surface.
  Kept.
- **O7** (the pipelined lanes' re-sort discipline): CORRECTNESS of output —
  this discipline is exactly rule (b), and three tests hold it
  (`fast_test.rs`, `tests/replay_determinism.rs`, e2e's double run). Kept.
- **R4** (unsized pool): throughput/memory, not ordering. Untouched.
- **R5** (completion order in a stats diagnostic): the ONE real leak, and it
  is diagnostic-only. Kept documented (inventory #n).

None of the findings was a parity convenience. What aged is the FRAMING
around them — the rule texts that justified ordering by TS comparability
rather than by the gates that enforce it today.

## 4. The mode split: `run.order = "throughput" | "ordered"`

**Rule (new, in docs/concurrency.md): ordering is a TEST-MODE property, not a
production property. Production's guarantee is canonical OUTPUT, not ordered
WORK.**

- **`throughput` (the default, every regular run and the shipped default):**
  work proceeds as fast as the pool and the semaphore allow; results land in
  canonical order where they are consumed. No stage waits for order.
- **`ordered` (what determinism-sensitive tests and debugging opt into):**

  - the waves' turn driver (`FastTier::Off` — no pipelining, every turn waits
    for its slowest call);
  - `par::map_ordered` / `produce_then_map` / `join` / `beside` run
    sequentially (the sequential branch inside the same helpers — one
    implementation per helper, never two);
  - every chunked dispatch window is forced to 1 (one call at a time,
    in canonical order), through the ONE `prompt_window` construction point.

  Ordered forces the conservative naming schedule (`FastTier::Off`); the
  combination with `naming.relaxed_levers` is refused in config validation
  (the levers have nothing to select once the schedule is forced), mirroring
  the existing `schedule()` refusal. `naming.schedule = "sequential"` +
  `run.order = "ordered"` is allowed and redundant.

- **Why Off and not the tier alone:** the Off ≡ Exact byte-identity is proven
  (`fast_test.rs`, e2e's legacy goldens); relaxed-levers + turn-driver is an
  unproven pairing. The mode admits only proven territory.
- **What ordered changes in the artifacts (measured, replay_determinism's
  pin, 2026-10-30):** the TREES are byte-identical to the conservative
  schedule's (`naming.schedule = "sequential"` — proven territory, and the
  pin replays a conservative cache under ordered mode with zero misses,
  zero writes and a byte-identical tree). The cache-KEY SET is identical
  too — every key the ordered run computes is a key the conservative run
  also computed. Two things are NOT identical, both measured and both fine:
  the ask STREAM (the turn driver dispatches one extra duplicate-key call
  on the audit's fixture — 7 asks vs 6, same 3 distinct keys; the ask count
  is dispatch detail, like the timing gauges), and the prompts under the
  RELAXED default, which differ because the levers change decisions — that
  is exactly why ordered mode forces the conservative schedule and refuses
  `naming.relaxed_levers`. Ordered-vs-ordered is the determinism contract;
  the trees are the cross-mode contract.
- **Single parse point:** the key is `run.order` in the key registry
  (`config/keys.rs`), reachable through the config file (no flag — the
  key registry is the one parse point and the no-flag-keys guard records
  it), config validation refuses the conflicting combination, and the CLI
  sets the `par` seam's mode once at startup before any stage runs.
- **Who opts in today:** the tests that pin ordering already opt in
  explicitly and keep doing so — `fast_test.rs` builds `FastTier::Off`
  configs directly; e2e's sequential leg sets `naming.schedule =
"sequential"` in its config TOML (it pins the CONSERVATIVE SCHEDULE as a
  shipped product, deliberately — the ordered mode is layered on the same
  driver); the replay-determinism and order-independence tests need NO mode
  — they prove order-independence, the opposite of needing order. The mode
  itself is gate-covered by the replay pin (`tests/replay_determinism.rs`,
  runs in `rust:unit`). The audit found no test that relies on ordered
  operation as an ACCIDENTAL default.

## 5. Measurement

Run under `flock /work/heavy.lock` after exp161's serial index merge
released the box (§6 for the runs' records; instrument: `--simulate-llm-latency`
over a warm cache — no LLM endpoint involved — and the deterministic-stub
run for the client-CPU side).

**The workload:** the corpus bundle (claude-code 2.1.216's
`entrypoints/index.js`, 6,509 emitted files), answers from a deterministic
stub LLM (every asked identifier gets `<id>Renamed`; the same stub filled
the cache). One cold fill leg (the conservative schedule) wrote **70,887
cache entries**; every priced leg replayed it. **All three priced legs
wrote the byte-identical tree** (sha256 over the whole tree
`007b27b5…`) — the schedule comparisons below hold the WORK fixed, and
the ordered mode's zero-miss / zero-write / same-tree contract is proven
on the real 6,509-file bundle, not only the unit fixture.

**The simulated LLM wall** (the virtual clock, 50 slots, the measured
cold latency distribution; 71,984 calls, 103,812 call-s of drawn latency
— identical to the digit across the Exact and ordered legs):

| Leg | Schedule                                                                                                                                   | Simulated LLM wall     | Effective concurrency | Dispatches               |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------- | --------------------- | ------------------------ |
| A   | Exact (`naming.schedule = "sequential"` — the pipelined driver, window 66)                                                                 | **3,736 s (62.3 min)** | **27.8 of 50**        | 288                      |
| B   | `run.order = "ordered"` (turn driver, window 1, sequential `par`)                                                                          | **103,812 s (28.8 h)** | **1.0**               | 71,984 (all single-call) |
| C   | the shipped default (Relaxed) — context only: 2,556 asks missed the cache (the levers change decisions) and were answered by the stub live | 3,446 s (57.4 min)     | 29.7                  | 234                      |

**Andrew's question, quantified.** Fully ordering the concurrency — one
call at a time, in canonical order — costs **27.8× the conservative
schedule's LLM wall** (28.8 h vs 62 min): effective concurrency falls from
27.8 to 1.0, exactly the "blocking" he predicted. The ordered mode stays
what the audit made it: a test-mode property, never a default.

**The shipped schedules are already near the streaming bound.** The
50-slot ideal for this call mass is 103,812 s / 50 ≈ 2,076 s. Exact
achieves 55.6% of that packing, the Relaxed default 60.2%. Where the gap
lives (the dispatches table): the FIVE largest batches (12,136 / 10,010 /
8,052 / 6,767 / 5,604 calls — the retry dispatches and the big sweep
groups) each run within a few percent of their own streaming bound (a
12k-call batch prices at 357 s vs its ~350 s ideal — the per-batch tail is
amortized to noise at that size). The remaining 263 dispatches carry 49% of
the calls but 65% of the wall: **the cost is the SMALL batches' tails** —
per-batch round barriers, the heavy-tailed latency distribution's max
draw, and the wave rounds' inherent serialization (§2 row o). A
work-through rework of the small-batch sites (backfilling run_retries /
sweep groups / the split namer the way the lane driver already does) is
the standing lever this measurement sizes: worth at most the ~1.7×
between 3,736 s and 2,076 s, most of it in batches under ~1,000 calls.

**The client-CPU side** (warm replays without the sim, real wall, one run
each): Exact 839 s, ordered 649 s. NOT a verdict — four warm legs of
identical-or-equivalent work landed between 634 s and 839 s (the candidate
Exact leg re-run in the neutrality proof: 634 s; MAIN's binary on the same
cache: 752 s), so the ordered leg's 649 s sits INSIDE the run-to-run
spread and the sequential `par` cost (~68 s for close-contexts at full
parallelism, per the 2026-10 perf inventory) is not resolvable in a
single wall-time comparison on this box. No rework follows from either
number: the pipelined bookkeeping is what buys the 27.8× concurrency, the
fan-out sites are reorder-buffers (§2), and the ordered mode exists for
tests only.

**The refactor's own neutrality proof.** The live neutrality instrument
needs the LLM endpoint; none was available this session, so the proof ran
against the deterministic stub — where it is prompt-byte-exact, because
the cache key IS the prompt bytes' hash: MAIN's binary (223446104, built
in a detached worktree) replayed the candidate's warm cache and asked
**nothing new (0 stub hits, 0 cache writes)** and wrote the
**byte-identical tree** (`007b27b5…`); the candidate's re-run matched on
both counts too. What a live-model neutrality adds beyond this: answers
from the real model's distribution — impossible to differ silently here,
since any prompt change forces a cache miss and would have shown up as a
stub hit. Records: `neutral-proof.json`.

## 6. Artifacts

- Audit + doc rewrite + mode switch: branch `exp163-ordering-audit`.
- Benchmark runs: `/work/exp163-data/` — `bench-ordering.mts` (the
  harness), `bench-results.json` + `bench.log` (the legs), `exact-sim.json`
  / `ordered-sim.json` / `default-sim.json` (the virtual-clock reports),
  `sim-cache/` (the warm cache the legs replayed), `neutral-proof.mts` +
  `neutral-proof.json` (the neutrality-equivalent proof), and the
  baseline binary's detached worktree `/work/exp163-baseline` (removable).
