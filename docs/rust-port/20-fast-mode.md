# 20 — `--fast`: what parity cost, and the first levers past it

**STATUS (2026-09-26): code on branch `perf/fast`, default OFF. No verdict
benchmark yet** — every number below was taken on a busy machine (other
heavy runs sharing it) and is an ESTIMATE, not a verdict. The verdict
commands are at the end.

## The question

How fast can the Rust binary go once it no longer has to reproduce the TS's
ordering and single-threading, while keeping the project's goals: correct
output (every tree boots, pure rename/relayout), DETERMINISM (same input +
same answers = same bytes), and clean cross-version diffs (KPIs inside their
bands, novel/realLn unmoved)?

## Where the time goes (215→216, profiled)

`--profile` now records `phase` spans (category `phase`, tid 4) with the
process CPU time spent in each (`cpuMs`) and the average cores busy
(`cores`) — `profiling::phase()`, an RAII guard. Summarize with
`/work/perf-fast/phases.py <profile.json>`.

Warm-ish replay, parity path (`p2-parity`, busy box). The standing cache
(`/work/neutrality-cache`) no longer matches the binary's prompts (2,014 of
the naming calls missed), so misses were answered by an instant-400 stub;
the CPU phases are real, the LLM phases are not.

| phase (parity path)                        |   wall s |    cores | note                                         |
| ------------------------------------------ | -------: | -------: | -------------------------------------------- |
| unpack + vendor                            |      3.0 |      2.5 |                                              |
| prior: parse + JSON (both sides)           |      4.0 |      1.6 |                                              |
| prior: graph fresh, then prior             |     12.0 |      1.7 | sequential: the oxc AST is not `Send`/`Sync` |
| prior: match cascade                       |     22.9 |      1.0 | decision code, sequential by design          |
| prior: close dump                          |      9.2 |      7.0 |                                              |
| era: transfer                              |     13.8 |      1.0 | decision code                                |
| era: close contexts                        |     17.5 |      1.0 | **pure per-function map, run serially**      |
| era: waves (CPU part)                      |      ~21 |      0.4 | prompt build, validate, apply                |
| naming: validate (2 full re-parses)        |     10.1 |      1.0 | **serial, reads only the texts**             |
| naming: prior-diff reconcile               |      6.9 |      0.4 | waits on validate                            |
| naming: deferred sweep                     |      8.1 |      0.7 |                                              |
| naming: family permute                     |     14.7 |      1.3 |                                              |
| split (input, assign, review, cjs, finish) |     17.5 |      1.7 |                                              |
| **CPU-bound total (excl. LLM)**            | **~165** | **~1.3** | on a 64-core box                             |

Cold, the LLM dominates. From the last cold Rust eval's `-vv` log
(`rust-5b-c3b272f-a`, 215→216, 453 s wall): 2,182 unique calls, 3,261
call-seconds (p50 1.26 s, p90 2.7 s, max 20 s), an LLM window of 418 s in
which **136 s had zero calls in flight and 170 s had fewer than 4** —
effective concurrency 11.5 of the configured 32. Same picture on every pair
(85→86: 19.2; 118→119: 20.0; 197→198: 14.2).

## The parity constraints that cost time

| constraint (kept for TS parity)                                                                                               | measured cost (215→216)                                                                            | `--fast`                                                                                            |
| ----------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| **turn barrier inside a round**: every active lane's next call is dispatched together and the turn waits for its SLOWEST call | cold: the idle/low-concurrency seconds above (~40% of the 418 s window); warm stub: 30.4 s → 7.0 s | **pipelined**: a lane's follow-up starts when ITS answer lands                                      |
| per-function close contexts built one at a time                                                                               | 17.5 s at 1.0 core                                                                                 | `par::map_ordered`: 2.0 s                                                                           |
| validate = two full re-parse + serialize passes, serial, before the reconcile may start                                       | 10.1 s + the reconcile's 6.9 s in series                                                           | baseline beside the era; verdict beside a speculative reconcile: 16.9 s → 7.2 s                     |
| the wave barrier ACROSS waves (60 on this pair)                                                                               | the pipelined model's remaining LLM wall (below)                                                   | kept — a real data dependency (a caller's prompt shows its callees' new names)                      |
| sequential matching cascade / transfer (ambiguous-map insertion order, propagation, demote)                                   | 22.9 s + 13.8 s                                                                                    | not changed (decision code; parallelizing it changes decisions — an eval question, not a speed one) |
| fresh and prior graph builds in series                                                                                        | ~6 s recoverable                                                                                   | not changed: needs each side parsed and built on its own thread (the AST is not `Send`)             |
| the same ~20 MB text re-parsed by every post-generate pass (validate ×2, reconcile, sweep, permute ×2, census, split, relink) | ~2–3 s per parse, ~20–30 s total                                                                   | not changed: spans five other lanes' files                                                          |
| family permute, split: mostly single-threaded                                                                                 | 14.7 s, 17.5 s                                                                                     | not changed                                                                                         |

## What `--fast` changes, and why each is deterministic

1. **Pipelined LLM dispatch within a round**
   (`NameProvider::run_pipelined`; `LlmClient` runs it on `FuturesUnordered`;
   the default impl is generational, i.e. the old turn barrier). A lane reads
   only the round's FROZEN state plus its own claims (`batch.rs`), so its call
   sequence cannot depend on completion order; everything order-sensitive is
   put back into the turn driver's canonical order before it is recorded —
   dispatch records by (turn, lane) with retries first, contention events by
   (finishing turn, lane), retry entries in retry order. The apply order at the
   barrier is untouched.
2. **Close contexts on the pool** — a pure map, input order, first error
   first.
3. **Validate overlapped** — the fresh baseline on its own thread during the
   era; the verdict beside a speculative reconcile over a CLONE of the trail,
   discarded when the output is invalid (the parity outcome).

All three are decision-neutral: `--fast` ships the parity path's bytes. Proven
by `naming/driver/fast_test.rs` (LIFO completion — the worst order — with and
without a prior, a close match exercised: same shipped code, same dispatch
records, same reconcile), by `scripts/e2e.ts` step 5 (`--fast` twice →
byte-identical, output boots), and on the real 215→216 pair twice (busy box):
parity vs fast **0 differing files**.

Because `--fast` is byte-identical to the default given the same answers, a
cold eval of it measures only the LLM's re-roll band; its quality verdict is
the warm byte-identity on all four pairs (below).

## Estimates (not verdicts)

- Warm 215→216 on a busy box: 195 s → 148 s wall (−24%), CPU-bound phases
  only (the stub LLM hides the pipelining gain).
- Cold 215→216: modelling each round as max(longest lane chain × latency,
  calls × latency / 32) over the 60 recorded rounds puts the naming LLM
  wall at roughly half of the turn-barrier window — about 453 s → ~300 s
  per run. To be measured by the cold legs below.

## The verdict run (on "machine idle")

```bash
cd /Users/andrewgross/Development/humanify/.claude/worktrees/<this lane>
cargo build --release --locked -p humanify-cli
cp target/release/humanify /work/perf-fast/humanify-verdict
# per pair: cold parity (seeds the cache) + cold fast, then warm parity / warm
# fast / warm fast again against a scratch copy of the seeded cache
/work/perf-fast/bench.sh /work/perf-fast/humanify-verdict /work/perf-fast/verdict
cat /work/perf-fast/verdict/summary.txt
python3 /work/perf-fast/phases.py /work/perf-fast/verdict/2.1.216-warm-fast.meta/profile.json
```

Pass criteria: `warm-fast vs warm-fast2` 0 files on every pair (determinism);
`warm-parity vs warm-fast` 0 files (decision-neutral); every warm leg
`cache+0`; exit codes equal. `--warm-only` re-runs just the warm legs.
