# exp207 — `eval score` runs its pairs in parallel

STATUS: IN PROGRESS (2026-11-09), branch `exp207-parallel-pairs`. Not merged.

Andrew, 2026-11-09: "ok lets run in parallel" (earlier: "taking extremely
long to do these tests right now").

## Step 1 — census: where the serial 2 h 21 min goes

From `exp204-75ae9334` (serial, index 4736dcfa, names mode, scratch bases;
results `/work/exp204-75ae9334-frozen/dev/eval/results/exp204-75ae9334`,
trees `/work/eval-work/exp204-75ae9334`). Wall times from file times and
run manifests; LLM load from the `-vv` logs (each call is logged twice,
once by the client and once by the debug wrapper; counted once).

| phase                 | 85→86 | 118→119 | 179→181 | 197→198 | 215→216 | total     | share |
| --------------------- | ----- | ------- | ------- | ------- | ------- | --------- | ----- |
| rebase (cold, no prior) | 11.3 min | 11.5 | 18.8 | 20.7 | 24.3 | 86.6 min | 61%   |
| scored leg            | 205 s | 263 s   | 464 s   | 447 s   | 381 s   | 29.3 min  | 21%   |
| scoring + page + boot | 13 s  | 13 s    | 17 s    | 17 s    | 17 s    | 1.3 min   | 1%    |
| self-hop (2.1.216)    |       |         |         |         | cold 12.4 + warm 10.9 | 23.3 min | 16% |
| **whole eval**        |       |         |         |         |         | **141 min** |     |

What bounds each phase:

- **Rebases are LLM-bound**: 42k / 47k / 62k / 68k / 76k requests (295k, 97%
  of the eval's calls). Average 223-266 in flight against the 472 cap;
  ~55% of their seconds at >= 300 in flight, ~1/3 at < 50 (CPU phases and
  wave tails). Latency rises with ONE run's own load (p50 3.0-4.0 s, p90
  6.2-8.3 s, max 39-66 s; ~2.5 s on a lightly loaded server); the best
  minute is 103-117 req/s. One run at 472 already about saturates the server
  inside its waves; parallel runs mostly fill the gaps between them.
- **Scored legs are CPU-bound**: 1.2k-3.1k requests, 10-35 in flight on
  average, > 85% of seconds at < 50 in flight.
- **The self-hop is CPU-only**: 9 requests cold; the warm leg replays.
- **Memory**: scored legs 9.8 / 11.4 / 12.9 / 16.1 / 15.4 GB peak RSS. The
  rebases' was never recorded (this lane adds it).
- **Critical path in parallel**: the 215→216 chain, 24.3 + 6.4 + 23.3 ≈ 54
  min unloaded. The LLM floor is ~305k requests at ~100-117 req/s ≈ 45-50
  min of a saturated server. Expected wall: ~75-90 min.

## Step 2 — the build

- **`--parallel-pairs N`** (harness key `eval.parallel_pairs`, default 5).
  Each pair (rebase → scored leg → scoring → boot) runs in its own lane
  through the harness's one subprocess fan-out (`runners::each`); the
  self-hop runs in the lane of the LAST attempted pair right after its
  scored leg. With fewer lanes than pairs (but more than one), that lane
  starts first: its chain is the critical path. **1 is the serial run**,
  launch for launch in the goldens' order.
- **Every launch keeps pairs.json `llm.concurrency` (472).** Splitting it
  (472/N) would change each run's dispatch window, and so which names are
  in context when a prompt is built: the parallel references would measure
  a different schedule, not just a different load. The server sees up to
  N × 472 in flight; `pipeline.json` records `parallelPairs` and
  `llmConcurrencyPerRun`, and every run manifest its `config.concurrency`.
- **Completion order reaches nothing.** Lanes say into an ordered transcript
  (`runners::lanes`: one slot per pair in pairs.json order, then the
  self-hop's). A slot's lines print at once while every earlier slot has
  finished, else are held and printed in order: with one lane it prints as
  it is said. Each lane writes only its own pair's files; `pipeline.json`
  and `commit.txt` are written before the lanes, `summary.json` and the
  identity files after them, by `score::run_inner` alone.
- **A failing leg fails its pair, not the others** (a lane's failure is a
  return value; the summary says which pair). Every launch keeps its
  `PIPELINE_DEADLINE` (12 h, over the whole run).
- **The rebase and self-hop launches now sample peak RSS** (one console line
  each), and **every rebase writes its LLM call ledger**
  (`diagnostics.stats_json=<results>/<from>-rebase.stats.json`): the
  rebases carry 97% of the calls and parallel load hits them hardest, and
  their retries / timeouts were recorded nowhere. This is the ONE launch
  change: one config line per rebase, 5 lines in each of
  `run-launch.golden.txt` and `run-launch.seeded-base.golden.txt`,
  re-recorded by the recorder test. `humanify-ag-cli`'s launch parity test
  compares a rebase with that key taken out after asserting it exactly
  (as exp171 did for the library index).

Tests (red first; the ordering tests were also seen red with the ordering
disabled):

- `score_test::parallel_lanes_launch_exactly_the_serial_launches` — the
  first lane made to finish LAST; the launch blocks equal a serial run's.
- `score_test::completion_order_never_reaches_the_transcript_or_the_label` —
  the same run: the transcript equals the serial transcript line for line
  (timings aside), the self-hop is on the last pair, the results file
  list is equal.
- `score_test::a_failing_lane_fails_its_pair_and_not_the_others`.
- `score_test::the_label_records_how_many_lanes_ran_and_each_launchs_llm_concurrency`.
- `score_test::every_rebase_records_its_llm_call_ledger`.
- `lanes_test` (three): slot order whatever the finish order; one lane prints
  as it is said; a late line is never lost.
- The two launch goldens are held by `--parallel-pairs 1` runs, in order.

## The first schedule (5 rebases at once) was refused by measurement

The first parallel reference attempt (`ref-names-d751229f-r1`: 5 lanes,
every launch at 472, so up to 2,360 rebase requests in flight) was STOPPED
after 28 minutes with all five rebases still running. Nothing had failed
after retries and every `-vv` call was a SUCCESS, but the LLM server did
LESS work in total under the load (the `-vv` logs; evidence kept in
`/work/exp207-evidence/killed-r1-d751229f/`, its results and work folder
deleted so the label cannot pass for a reference):

|                                   | serial (exp204-75ae9334)                  | 5 rebases at once (first 25 min)          |
| --------------------------------- | ----------------------------------------- | ----------------------------------------- |
| requests / s, all runs together   | ~57 average over the rebases (100-117 peak minute) | **48** (73k calls in 1,523 s)    |
| rebase call latency p50           | 3.0-4.0 s                                 | 8.6-14.3 s                                |
| rebase call latency p90           | 6.2-8.3 s                                 | 19.6-22.6 s                               |
| rebase call latency max           | 39-66 s                                   | 89-242 s (the per-attempt timeout is 300 s) |
| average in flight                 | ~250 (one run)                            | ~638                                      |
| host memory used (peak)           |                                           | 117.8 GB (5 pipelines 25 GB summed RSS)   |

The host was not the limit (load average ~7); the server was. Above
roughly 500 in flight it slows down (most likely the vLLM KV cache
swapping under the queue). At 48 req/s the ~295k rebase calls would
have taken ~100+ min, so this schedule was SLOWER than serial, and with
latency maxima approaching the 300 s attempt timeout it would also have
measured the eval under a load the serial bands never saw (rule 11).

**The schedule adopted: `--parallel-rebases R` (default 1).** Lanes still
run five at once, but only R rebases run at a time, in a fixed order with
the self-hop's pair first (215, then 85, 118, 179, 197), on a driver beside
the lanes; each lane waits for its own base (one `base: waited Ns` line in
its block) and its CPU-bound scored leg, scoring and boot overlap the next
rebase. The 215 pair's scored leg and its 23-min self-hop overlap the other
four rebases. The server sees one rebase at 472 plus the scored legs'
10-35 in flight: about the serial load. Expected wall: ~87 min of rebases +
the last scored leg + scoring ≈ 95-100 min.

R=2 might fit under the ~500 knee (one rebase averages ~250 in flight and
is under 50 for a third of its time); that is a later, MEASURED option,
not a default.

## Step 3 — does running in parallel change what the eval measures?

(in progress)
