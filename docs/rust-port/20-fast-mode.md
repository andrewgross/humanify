# 20 — `--fast`: what parity cost, and how far past it the binary goes

**STATUS (2026-09-27): VERDICT BENCHMARKED on the idle box (binary built
from `perf/fast` cf783fa1, rebased onto rust-port 507163d9+c4784295; sha
7ec5910e…). Cold walls below are the verdict legs; the CPU-side numbers are
the warm legs.** Bottom line: `--fast` wins ~20% of the pipeline's own CPU
(byte-identical); cold end-to-end it LOSES — the server, not the client, is
the bottleneck — and the lever that does pay cold is bigger naming windows
(`--batch-size 25`, −9-11% cold).

## The question

How fast can the Rust binary go once it no longer has to reproduce the TS's
ordering and single-threading, while keeping the project's goals: correct
output (every tree boots, pure rename/relayout), DETERMINISM (same input +
same answers = same bytes), and clean cross-version diffs (novel/realLn
exact, reducible KPIs inside `noise-bands.json`)?

Two tiers answer it (`crates/humanify-core/src/fast.rs`):

- **`--fast` (= `--fast exact`)** only reorders or parallelizes work whose
  outcome cannot depend on the order. It SHIPS THE DEFAULT PATH'S BYTES; its
  quality verdict is warm byte-identity, and cold runs only time it.
- **`--fast relaxed`** adds levers that change decisions (which prompt sees
  which names, how work is batched). Still deterministic — every change
  merges in a canonical, input-derived order — but its quality is the
  EVAL's verdict. `--fast relaxed:<lever,...>` picks a subset (for sizing).

## Instruments

- `--profile <path>` records `phase` spans (category `phase`, tid 4) with the
  process CPU spent in each (`cpuMs`) and the average cores busy (`cores`):
  `profiling::phase()`, an RAII guard. Summarize with
  `/work/perf-fast/phases.py <profile.json>`.
- `--simulate-llm-latency <out.json>` (`humanify-cli/src/llm_sim.rs`) prices
  a WARM run's real answers on a virtual clock: every call gets a latency
  drawn deterministically from the last cold eval's measured distribution
  (10,283 calls, p50 1.15 s, p99 5.3 s), calls take one of `-c` slots as the
  rate limiter admits them, a turn barrier waits for its slowest call, a
  pipelined round advances each chain on its own. It separates the LLM
  schedule from the CPU without a model or an idle machine. Calibration: it
  prices the default path at 490 s where the last cold eval's (older)
  binary measured a 418 s LLM window, so read it RELATIVELY — it is high by
  roughly a sixth and assumes latency does not grow with load.
- `/work/perf-fast/stubnamer.py`: a deterministic stand-in endpoint for warm
  runs (answers any cache miss with `<id>Named`, instantly).
- `/work/perf-fast/levers.sh`: one lever per leg, each with both instruments.

## Where the time goes (215→216)

Warm (standing cache + the stub for misses), busy box, default path:

| phase                                      |  wall s |    cores | what it is                                |
| ------------------------------------------ | ------: | -------: | ----------------------------------------- |
| prior: parse + JSON, both graphs           |     ~15 |     ~1.8 | the AST is not `Send`: one side at a time |
| prior: match cascade (3 cascades)          |      22 |      1.0 | decision code: the alternation re-runs it |
| prior: close dump + twin inventories       |      12 |     4–26 | already parallel                          |
| era: transfer (settle 14 s)                |      16 |      1.0 | decision code                             |
| era: close contexts                        |    17.5 |      1.0 | a pure per-function map, run serially     |
| era: waves — setup (contexts, prompts)     |      16 |      1.0 | `build_context` 7 s, `context_view` 4 s   |
| era: waves — turn-barrier dispatch         |      30 |     0.01 | warm: stub latency; cold: the LLM         |
| naming: validate + reconcile               |      17 |     0.75 | two full re-parses, then a text diff      |
| naming: deferred sweep                     |       3 |      1.0 |                                           |
| naming: family permute                     |      16 |     1.25 | half of it indexes the PRIOR text         |
| split (TS-era prior: ledger re-derive 8 s) |      20 |      1.5 | a rebased prior skips the re-derive       |
| **total, warm**                            | **249** | **1.25** | on a 64-core box                          |

Cold, the LLM dominates. The last cold Rust eval's `-vv` log
(`rust-5b-c3b272f-a`, 215→216, 453 s wall): 2,182 unique calls, 3,261
call-seconds, an LLM window of 418 s in which **136 s had zero calls in
flight and 170 s had fewer than 4** — effective concurrency 11.5 of the
configured 32 (85→86: 19.2; 118→119: 20.0; 197→198: 14.2). The simulator
prices the default schedule at effective concurrency 8.2.

## The parity constraints, their cost, and what each tier does

| constraint kept for TS parity                                                              | measured cost (215→216)                           | tier / result                                                                                                  |
| ------------------------------------------------------------------------------------------ | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| **turn barrier inside a round**: every lane's next call waits for the round's SLOWEST call | sim LLM 490 s; warm stub 30 s                     | **exact** — pipelined: sim 490 → 377 s; warm 30 → 7 s                                                          |
| per-function close contexts built one at a time                                            | 17.5 s at 1.0 core                                | **exact** — `par::map_ordered`: 2.0 s                                                                          |
| validate (two re-parses) serial, and the reconcile waits for it                            | 17 s                                              | **exact** — baseline beside the era; verdict beside a speculative reconcile: 7.2 s                             |
| the permute's prior index built after the waves, though it reads only the prior text       | 7.3 s                                             | **exact** — built beside the era: permute 16 → 10 s                                                            |
| fresh and prior graphs built in series (the AST is not `Send`; core forbids `unsafe`)      | ~12 s                                             | **exact** — the prior side's graph on its own thread from its OWN parse (a parse is deterministic): 15 → 9.6 s |
| a function's naming windows run in sequence in ONE lane (≤ 25 bindings) or 4–16 lanes      | lane chains of 4–7 calls                          | **relaxed `window-lanes`** — one lane per window: sim 377 → 310 s                                              |
| the shadowed-binding pass (round B) is a full round per wave                               | 30 extra rounds                                   | **relaxed `defer-shadowed`** — rides with the next wave's round A: sim 377 → 323 s                             |
| the wave barrier ACROSS waves (30 waves)                                                   | sim ~200 s even at 128 slots                      | kept (below)                                                                                                   |
| matching cascade × 3 and transfer settle, sequential                                       | 22 s + 14 s                                       | not changed (below)                                                                                            |
| `build_context` / `context_view` per function, serial                                      | 11 s                                              | not changed: reads the oxc `Semantic` (not `Sync`)                                                             |
| the same ~20 MB text re-parsed per post-generate pass                                      | each parse ~1–2 s; the passes' own work dominates | not changed: reconcile's cost is its text diff (4.4 s), the permute's its member hashing                       |
| split (input 5 s, finish 4 s), mostly single-threaded                                      | ~17 s                                             | not changed (other lanes' files: format/, finish/)                                                             |

Both relaxed levers together: **sim 377 → 257 s** (−32% over exact, −48%
over the default path), determinism holding (twice → 0 differing files).

## VERDICT (idle box, 2026-09-27)

Binary `perf/fast` cf783fa1 (rebased onto rust-port 507163d9 — the cold-run
memory fix #56 — plus c4784295, the in-memory dedup for uncached runs).
Cold legs went LIVE to gpt-oss-20b :8000; warm legs replayed each pair's own
cold cache (misses to the deterministic stub). Full log:
`/work/perf-fast/verdict/summary.txt`; phase tables per leg beside it.

### Cold wall (the eval-shaped run: live LLM, no cache)

| pair    | default | `--fast` | `--fast relaxed` | default + `--batch-size 25` |
| ------- | ------: | -------: | ---------------: | --------------------------: |
| 85→86   |   269 s |    369 s |            313 s |                   **239 s** |
| 118→119 |   354 s |    496 s |            451 s |                   **321 s** |
| 197→198 |   484 s |    660 s |            580 s |                             |
| 215→216 |   483 s |    570 s |            498 s |                             |

(batch-25 legs ran on 85→86 and 118→119 only.)

**Pipelining LOSES cold, on every pair.** The simulator said −23…−39%; the
real server took it back and more. Mechanism: vLLM's per-call latency grows
with load (continuous batching — a bigger running batch steps slower per
sequence), and the prompt is mostly PREFILL (86: ~3.2 M prompt tokens vs
~0.1 M generated), so the server is throughput-bound and its throughput is
the same either way. The turn barrier's bursty schedule feeds vLLM LARGE
prefill batches; pipelined dispatch feeds it a steady trickle of small ones.
Control: against a stub with load-INDEPENDENT latency (85→86, cold), default
344 s vs exact 290 s — the client schedule itself saves ~16%; the real
server's latency curve is what refunds it. Corollary: `-c` past the server's
admission width buys nothing (observed running+waiting plateaus ~16-18).

**The lever that pays cold is prompt-token amortization**: the naming
prompt re-sends a function's whole context once per window of ~10
identifiers; `--batch-size 25` cuts the number of windows ~2.5× and the
context re-sends with them. 85→86: cold 269 → 239 s (LLM window 163 → 134
s); 118→119: 354 → 321 s. batch 40 overshoots (more in-window collisions →
retries: 256 s). It is DETERMINISTIC (a plain config flag) but
decision-changing — it belongs in the relaxed tier's recipe, judged by the
eval.

### Warm wall (the pipeline's own CPU; LLM served instantly)

| pair    | default | `--fast` |    Δ | `--fast relaxed` |
| ------- | ------: | -------: | ---: | ---------------: |
| 85→86   |    78.8 |     61.9 | −21% |             63.4 |
| 118→119 |    88.5 |     73.7 | −17% |             79.4 |
| 197→198 |   144.7 |    112.8 | −22% |            115.4 |
| 215→216 |   155.0 |    121.5 | −22% |            124.7 |

The relaxed tier costs ~2-6 s over exact warm (its extra CPU: one lane per
window means more per-lane tails) while cutting the SIMULATED LLM wall
407→227 / 708→432 / 716→343 / 663→369 s — worth nothing on THIS server
(above), potentially real on one that keeps latency flat under load.

### The determinism / byte-identity checks (all four pairs)

- `warm-exact` vs `warm-exact2`: **0 files, 0 lines** ×4 — determinism.
- `warm-relaxed` vs `warm-relaxed2`: **0 files, 0 lines** ×4 — determinism.
- `warm-default` vs `warm-exact`: **0 files, 0 lines** ×4 — the exact tier
  ships the default path's bytes.
- `warm-default` vs `warm-relaxed`: differs, by design (2,124 / 3,514 /
  4,446 / 2,966 files) — the relaxed tier is the eval's to judge.
- Every warm default and exact leg: `cache+0`, exit 0. Warm relaxed legs
  wrote 172/595/385/423 entries: legitimate — the seed merges the DEFAULT
  cold leg's answers over relaxed's own (`cp -rn`), and the model re-rolls
  across cold legs (exp052), so some downstream prompts are new; both
  relaxed warm legs wrote the SAME count and shipped identical trees.

### Answer to the question

With parity off the table but determinism and quality kept:

1. **The binary's own CPU is no longer the story: −22% warm**, byte-identical
   (`--fast`), shipped behind the flag. The remaining serial CPU is decision
   code (matching 22 s, transfer 14 s) whose parallelization changes
   decisions — an eval-gated matching change, not a speed change.
2. **Cold wall is SERVER-bound.** ~75-85% of a cold run is vLLM prefill of
   naming prompts; the client schedule cannot beat the server's
   throughput/latency curve (measured both ways above). The client-side
   levers that remain:
   - **bigger naming windows** (`--batch-size 25`): −9-11% cold,
     deterministic, decision-changing → add to relaxed, eval it;
   - **fewer identifiers asked** (naming floor / transfer coverage) — the
     project's standing noise arc, which ALSO cuts cold wall linearly;
   - **server-side**: more capacity or a serving config that holds latency
     under load (ops, not code) — the one instrument that would make
     pipelining pay (the stub control shows ~16% is there).
3. **Peak RSS ~19 GB** on the biggest pair (down from the TS's 15-30 GB on a
   64 GB heap); the eval heap flag is inert.

Cold evals to judge quality (novel/realLn exact, reducible KPIs in band) —
run by the coordinator, cold per rule 10:

## Why each change keeps determinism

- **Pipelined dispatch** (`NameProvider::run_pipelined`; `LlmClient` runs it on
  `FuturesUnordered`; the trait default is generational = the old barrier). A
  lane reads only the round's FROZEN state plus its own claims (`batch.rs`),
  so its call sequence cannot depend on completion order; everything
  order-sensitive is put back into the turn driver's order before it is
  recorded (dispatch records by (turn, lane), retries first; contention
  events by (finishing turn, lane); retry entries in retry order). The apply
  order at the barrier is untouched.
- **Parallel maps / background work** (close contexts, baseline, permute
  prior index, prior-side graph): pure functions of their inputs, results in
  input order or consumed at a fixed point.
- **Speculative reconcile**: runs over a CLONE of the trail; an invalid
  output discards it.
- **window-lanes**: the lane split is a function of the binding list and the
  batch size; lanes apply at the barrier in the existing canonical
  (node, phase, binding, seq) order — the path every large function takes.
- **defer-shadowed**: the round-B lanes are built at the same point (after
  barrier A) and their requests read the same frozen state as in their own
  round; they apply at the next barrier in the canonical order; a node
  settles only when its deferred pass is done.

Proof: `naming/driver/fast_test.rs` — exact vs default under LIFO completion
(the worst order), with and without a prior (a close match exercised): same
shipped code, dispatch records and reconcile; every relaxed lever twice →
identical, LIFO vs generational completion → identical, output valid.
`scripts/e2e.ts` step 5: `--fast` twice → identical AND equal to the default
tree; `--fast relaxed` twice → identical; all outputs boot. On 215→216:
exact vs default 0 files; relaxed twice 0 files.

## What is left, and what it would take

- **The wave structure (~200 s of LLM wall at any concurrency).** 30 waves,
  each waiting for its members' callees; the early waves (912, 471, 244
  calls) are throughput-bound at 32 slots, the tail (~20 waves of 1–50
  calls) latency-bound. A dependency-driven scheduler (a caller dispatched
  when ITS callees are applied) would cut the tail, but a prompt also reads
  run-wide state (the module scope's used names, the program bindings), so
  to stay deterministic each node must read a snapshot defined by a
  canonical prefix, not by completion time — a redesign of the processor's
  state model, not a scheduling tweak. Ceiling from the sim: ~200 s → the
  pure-throughput floor of ~120 s at 32 slots.
- **Server concurrency (ops, not code).** The sim puts relaxed at 219 s with
  `-c 64` and 203 s with `-c 128`, IF the vLLM server holds latency under
  that load — measure on the server before believing it.
- **Matching (22 s) and transfer (16 s).** Sequential decision code; the
  alternation re-runs the whole function cascade 3×. Parallelizing it with a
  canonical tie-break changes which ambiguous pairs resolve — a matching
  change for the eval to judge, and a large one.
- **Per-function prompt setup (11 s).** Parallelizable only after extracting
  the callee views and scope chains to plain data (the oxc AST is not
  `Sync`), the "AST-bound stages" item in 16-findings-queue.

Rough ceiling with everything above: warm ~100 s, cold ~200 s on 215→216.

## The verdict procedure (as run)

```bash
/work/perf-fast/bench.sh /work/perf-fast/humanify-verdict /work/perf-fast/verdict
cat /work/perf-fast/verdict/summary.txt
python3 /work/perf-fast/phases.py /work/perf-fast/verdict/2.1.216-warm-relaxed.meta/profile.json
```

Per pair: cold default (seeds the cache), cold exact, cold relaxed (live
endpoint — the timing legs); then warm default / exact / exact / relaxed /
relaxed on a scratch copy of the seed (the CPU legs and the determinism
proof). Pass criteria, all met: `warm-exact vs warm-exact2` and
`warm-relaxed vs warm-relaxed2` 0 files ×4; `warm-default vs warm-exact`
0 files ×4; every warm default and exact leg `cache+0`; every leg exit 0.
`--warm-only` re-runs the warm legs. The batch-25 probe:
`/work/perf-fast/probe-batch.sh 86:25 86:40 119:25`.

The relaxed tier's QUALITY verdict is the cold eval (rule 10: the control
goes cold with it), judged against `noise-bands.json`:

```bash
npm run eval -- score perf-exact-cf783fa1
npm run eval -- score perf-relaxed-cf783fa1 --pipeline-arg --fast=relaxed
npm run eval -- score perf-batch25-cf783fa1 --pipeline-arg --batch-size 25
npx tsx experiments/034-eval-harness/leaderboard.ts main-2026-09-18 perf-exact-cf783fa1 perf-relaxed-cf783fa1 perf-batch25-cf783fa1
```

`--pipeline-arg` (scripts/eval.ts → run.sh) appends the argument to every
launch — rebase, scored leg and both self-hops — and records it in the
label's `pipeline.json`; without it the launches are byte-identical to the
golden (`run-launch.test.ts`). novel/realLn must be exact; noise, reloc and
mints must sit inside their bands. A lever that fails can be dropped from
`relaxed` on its own (`--fast relaxed:<the others>`); the batch-25 probe
composes as `--pipeline-arg --fast=relaxed --pipeline-arg --batch-size 25`.
