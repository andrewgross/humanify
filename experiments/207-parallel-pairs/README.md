# exp207 — `eval score` runs its pairs in parallel

STATUS: DONE, NOT MERGED (2026-11-10), branch `exp207-parallel-pairs`. `--parallel-pairs 5
--parallel-rebases 1` is the default. Three valid parallel references
`ref-names-40921b9f-r1/r2/r3` plus a serial control `serial-40921b9f-s1`.
Bands NOT adopted (noise-bands.json and self-hop-reference.json
unchanged): see "Bands" below.

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

## The host during this lane (read every time number below with it)

- **LLM server: 4x gpt-oss-20b (half the GPUs; the other half runs
  glm53-flash)**, behind vllm-lb (replicas r4-r7). CLAUDE.md describes
  the references as 8x. The ~500-in-flight knee above was measured on
  these 4 replicas; R=2 may only pay off on 8. exp204-75ae9334 (the serial
  run this lane compares against) shows the same per-call latency and the
  same best-minute throughput as a single rebase here (table below), so it
  very likely ran on the same 4 replicas.
- **Host memory contention.** Docker sets no memory limit on the container
  (`memory.max` / `memory.high` = max), but the host has one NUMA node, ~49
  GB shmem held by the model servers, glm53-flash holds ~67 GB of host RAM,
  and swap is full. During the runs the container's `memory.pressure` went
  to 94-96% (full stall) for minutes at a time; a pipeline then spends its
  time in kernel reclaim (one 2.1.215 rebase: utime 30 s, stime 3,260 s,
  no log line for 29 min). The cgroup's page cache sat at ~85-89 GB with
  anon at 5-9 GB; `fincore` put only ~0.5 GB of it in the 43 GB library
  index and ~3.0 GB in every file over 200 MB under /work and the source
  tree, so the cache is not ours to explain and the stalls come from
  host-wide contention. A stall delays prompts; it does not change them.

## Step 3 — does running in parallel change what the eval measures?

Three cold repeats of `40921b9f7` (pipeline code = main 1c07bf20f; only
the harness differs) from the frozen worktree `/work/ref-names-40921b9f-frozen`,
default schedule (5 lanes, 1 rebase at a time, 472 per launch), names mode,
index 4736dcfa.

### r1 — `ref-names-40921b9f-r1` (2026-11-09 16:17-18:45)

Valid: 5 rebases, 5 scored legs, cold and warm self-hop all exit 0; 5
run-status files; 5 boots OK; warm self-hop byte-identical, 0 files, 0
cache writes; cold self-hop 2 ln (range 2-22); cache +0 on every leg; no
call failed, 0 timeouts, the only retries were recovered 5xx answers
(rebases 3/10/6/6/5, scored legs 0). novel 5,336 / realLn 556,875,
byte-equal to exp204-75ae9334.

| KPI        | exp204-75ae9334 (serial) | r1      | delta | band (byScorer.2 names) |
| ---------- | ------------------------ | ------- | ----- | ----------------------- |
| noise      | 3,038                    | 3,034   | −4    | ±9                      |
| noiseLn    | 68,225                   | 68,179  | −46   | ±989                    |
| reloc      | 1,671                    | 1,653   | −18   | ±24                     |
| mints      | 230                      | 239     | +9    | ±14                     |
| treeLn     | 176,009                  | 175,268 | −741  | ±745                    |
| vendorLn   | 10,015                   | 10,027  | +12   | ±64                     |
| vendorReal | 14,241                   | 14,305  | +64   | ±148                    |
| newName    | 4,066                    | 4,029   | −37   | ±26 (outside)           |

Server, rebase by rebase (each now runs alone at 472, as in serial):

| rebase | p50 serial / r1 | p90 serial / r1 | max serial / r1 | best minute serial / r1 |
| ------ | --------------- | --------------- | --------------- | ----------------------- |
| 2.1.85  | 3.0 / 3.1 s | 6.2 / 6.4 s | 39 / 70 s | 117 / 110 req/s |
| 2.1.118 | 3.3 / 3.3 s | 6.7 / 7.0 s | 46 / 60 s | — / 119 req/s   |
| 2.1.179 | 3.5 / 3.6 s | 7.6 / 7.7 s | 54 / 70 s | — / 113 req/s   |
| 2.1.197 | 3.6 / 3.7 s | 7.9 / 8.0 s | 64 / 77 s | — / 115 req/s   |
| 2.1.215 | 4.0 / 4.1 s | 8.3 / 8.4 s | 66 / 74 s | 103 / 108 req/s |

Scored legs now overlap a rebase: p50 3.3-4.7 s / p90 5.6-8.5 s (serial
2.5-3.4 / 4.5-5.6), no retries, no timeouts, no failures. Model answers
per pair (asked 3,752 / 9,615 / 4,983 / 7,282 / 5,814 vs serial 3,718 /
9,391 / 4,946 / 7,298 / 5,878) show no shift beyond cold-run variation.

Time and memory: raw wall **148 min**; memory stalls (pressure avg10 > 50%)
16:18-16:47 (~29 min, from the log gap; before the sampler started),
17:11-19, 17:30-31, 17:35-36, 17:45-50, 18:11-15, 18:36-39: **~55 min
in all**; net ~93 min (crude: a stall slows everything, it does not add
a fixed delay). The lanes overlapped as designed (waits for the base
3,188 / 4,420 / 5,212 / 6,754 / 8,292 s; the 2.1.216 self-hop finished at
17:39, while the 179 and 197 rebases still ran). Peak host used 127.9 GB;
summed pipeline RSS peak 30.8 GB. Peak RSS: rebases 5.9 / 6.8 / 8.9 / 9.6 /
10.6 GB (new: never recorded before), scored legs 9.8-17.7 GB, self-hops
18.5 / 18.7 GB.

### r2 — `ref-names-40921b9f-r2` (2026-11-09 18:45:47-20:29:45)

Valid on every check r1 passed (all legs exit 0, 5 run-status, 5 boots
OK, warm self-hop byte-identical with 0 writes, cold self-hop 2 ln,
cache +0, 0 failed calls, 0 timeouts). novel 5,336 / realLn 556,875.

- **Wall 103 min** (serial 141). One memory stall, 18:46:47-18:54:17 (7.8
  min), at the start; stall-free from 18:54 to the end. Rebases 673 / 764
  / 1,180 / 1,301 / 1,948 s (serial 679 / 690 / 1,128 / 1,241 / 1,458; the
  215 rebase carries the stall). Self-hops 154 + 152 s (serial 744 + 654).
  Peak host used 100.9 GB, summed pipeline RSS 26.6 GB.
- **Connection retries, new:** the 2.1.119 scored leg (started
  19:42:11 UTC, 443 s) retried 6 calls and the 2.1.181 scored leg (started
  20:01:51 UTC, 448 s) retried 27 calls on `connection` errors, all
  recovered. r1's scored legs and exp204's had none (exp204: one 5xx).
  Memory pressure was 0 then; a rebase at 472 ran beside each, as in r1.
  For the load balancer's logs: 2026-11-09 19:42-19:50 and 20:01-20:09
  UTC. A retry resends the same prompt; it cannot change a question.
  Rebase retries were 5xx only (10 / 16 / 8 / 5 / 8; r1 5 / 3 / 10 / 6 / 6).

### r3 — `ref-names-40921b9f-r3` (2026-11-09 20:30:39-22:05:17), STALL-FREE

Valid on every check (all legs exit 0, 5 run-status, 5 boots OK, warm
self-hop byte-identical with 0 writes, cold self-hop 2 ln, cache +0, 0
failed calls, 0 timeouts). novel 5,336 / realLn 556,875.

- **Wall 94.6 min, 0 stalled samples** (15 s sampling). Rebases 652 / 776 /
  1,165 / 1,282 / 1,447 s; scored legs 328-546 s; self-hops 157 + 150 s.
  Peak host used 59.9 GB, summed pipeline RSS 29.0 GB.
- Connection retries again in two scored legs: 2.1.216 (started
  20:54:46 UTC, 546 s) 9, 2.1.181 (started 21:37:59 UTC, 473 s) 9, all
  recovered. Rebase retries 5xx only: 5 / 10 / 9 / 10 / 6.

### Serial control — `serial-40921b9f-s1` (`--parallel-pairs 1`, 2026-11-09 22:05:17 - 11-10 00:03:57), STALL-FREE

A control, not a reference: same sha, same night, right after r3.
Valid on every check (cold self-hop 8 ln). novel 5,336 / realLn 556,875.

- **Wall 118.7 min, 0 stalled samples.** Rebases 659 / 708 / 1,173 / 1,242 /
  1,504 s; scored legs 188-367 s; self-hops 151 + 148 s.
- Scored legs: 0 retries of any kind (the parallel runs' connection retries
  only ever hit a scored leg that ran beside a rebase).
- The 2.1.197 rebase (about 23:01-23:21 UTC) retried on 5xx 3,277 times
  (371 calls answered after a retry, 0 failed); every other rebase 3-11.
  Its latency (p50 3.5 s, p90 7.8 s) was normal: a server-side burst.

## Speed (the verdict)

| run | schedule | wall | stalled | note |
| --- | --- | --- | --- | --- |
| exp204-75ae9334 | serial | 141 min | unknown | self-hops 744 + 654 s |
| serial-40921b9f-s1 | serial | **118.7 min** | 0 | self-hops 151 + 148 s |
| ref-names-40921b9f-r3 | 5 lanes, 1 rebase | **94.6 min** | 0 | |
| ref-names-40921b9f-r2 | 5 lanes, 1 rebase | 103 min | 7.8 min | |
| ref-names-40921b9f-r1 | 5 lanes, 1 rebase | 148 min | ~55 min | |
| killed d751229f-r1 | 5 lanes, 5 rebases | (stopped at 28 min) | | server slower in total |

**Parallel lanes save ~24 min (−20%) against a serial run on the same host
the same night** (94.6 vs 118.7 min, both stall-free). That is less than
the census predicted from exp204's 141 min, because exp204's self-hops took
23 min and tonight's took 5: the self-hop was the biggest thing parallel
mode could hide, and it shrank on its own (likely host contention during
exp204; not established). What parallel mode hides now is the five scored
legs (~21-24 min serial) behind the rebases. The rebases themselves (~88
min) are the floor while one rebase saturates the server; on 8 replicas,
`--parallel-rebases 2` might lower it (unmeasured).

## LLM diagnostics, serial vs parallel

- Rebase calls, each alone at 472 in every run: p50 3.1-4.1 s, p90 6.4-8.4 s,
  best minute 108-119 req/s; equal to exp204's serial rebases within
  0.1-0.2 s. Rebase 5xx retries: parallel 3-16 per rebase, serial 3-11
  (plus the one 3,277 burst in serial). 0 timeouts, 0 failures anywhere.
- Scored legs: parallel p50 3.3-4.7 s / p90 5.6-8.5 s (a rebase runs beside
  them) vs serial 2.5-3.4 / 4.5-5.6. **Connection retries appear only in
  parallel scored legs** (r2: 6 + 27, r3: 9 + 9; r1: 0; both serial runs:
  0), all recovered, same prompt resent. Times for the load balancer's logs
  are listed under r2 and r3.
- Model answers per pair (asked / proposed / taken / dupInAns / unchanged)
  move by tens per pair between any two runs, serial or parallel; no
  direction separates the parallel runs from the serial ones.

## Bands

The spread of each KPI (max − min) over the three parallel runs and over
the two serial points on index 4736dcfa, against the current names band
(three serial repeats of 111401fe on the OLD index cb64):

| KPI        | exp204 | s1      | r1      | r2      | r3      | parallel | serial (2) | old band | parallel fits? |
| ---------- | ------ | ------- | ------- | ------- | ------- | -------- | ---------- | -------- | -------------- |
| noise      | 3,038  | 3,052   | 3,034   | 3,054   | 3,050   | 20       | 14         | 9        | no (marginal)  |
| noiseLn    | 68,225 | 68,065  | 68,179  | 67,829  | 67,879  | 350      | 160        | 989      | yes            |
| novel      | 5,336  | 5,336   | 5,336   | 5,336   | 5,336   | 0        | 0          | 0        | yes            |
| realLn     | 556,875| 556,875 | 556,875 | 556,875 | 556,875 | 0        | 0          | 0        | yes            |
| reloc      | 1,671  | 1,643   | 1,653   | 1,726   | 1,675   | 73       | 28         | 24       | **no** (r2)    |
| relocSt    | 437    | 436     | 436     | 438     | 437     | 2        | 1          | 1        | marginal       |
| newName    | 4,066  | 4,064   | 4,029   | 4,038   | 4,071   | 42       | 2          | 26       | **no** (r1)    |
| mints      | 230    | 231     | 239     | 231     | 230     | 9        | 1          | 14       | yes            |
| treeLn     | 176,009| 175,423 | 175,268 | 176,451 | 175,799 | 1,183    | 586        | 745      | **no** (r2)    |
| vendorLn   | 10,015 | 10,019  | 10,027  | 10,001  | 9,943   | 84       | 4          | 64       | **no** (r3)    |
| vendorReal | 14,241 | 14,279  | 14,305  | 14,277  | 14,283  | 28       | 38         | 148      | yes            |

"Fits" = the parallel spread is within max(serial spread, old band).

**Not adopted.** `cargo dev eval bands` writes the WHOLE names slot (every
KPI) and the self-hop range from the labels it is given; it has no
per-KPI form, and the slot is generator-only (never hand-edited). Five
KPIs do not fit: noise (20 vs 14, marginal), reloc (73 vs 28: r2's 1,726),
newName (42 vs 26: r1's 4,029), treeLn (1,183 vs 745: r2) and vendorLn
(84 vs 64: r3's 9,943). The generator would also set the names self-hop
range to 2-2 ln (all three parallel cold self-hops read 2), while the
serial control read 8 the same night: too narrow to judge anything. So
noise-bands.json and self-hop-reference.json are unchanged; the dry run's
output is kept in the lane's scratch notes, not committed.

**Index or parallel mode?** Both serial points on the new index already
spread wider than the old band on noise (14 vs 9) and reloc (28 vs 24): the
new index looks somewhat wider than the old one on those two. The parallel
runs go further on reloc, newName, treeLn and vendorLn, but each excess
comes from ONE run, a different run each time (r2 reloc and treeLn, r1
newName, r3 vendorLn), and no KPI's parallel mean sits outside the serial
values by more than the old band (reloc 1,685 vs 1,657; newName 4,046 vs
4,065; treeLn 175,839 vs 175,716; vendorLn 9,990 vs 10,017). Three runs
against two also favour the wider range by construction. **Not proven
either way**: there is no systematic shift, but the evidence cannot show
the parallel spread is no wider than serial. Settling it needs a third
serial repeat on this index (3 vs 3), or more.

## What this lane could NOT prove

- That parallel mode leaves the KPI spread unchanged (above).
- The speed gain on an 8-replica server, or with `--parallel-rebases 2`.
- Why exp204's self-hops took 23 min when tonight's took 5.
- What the connection retries are (the load balancer's logs would tell).
- What holds the container's ~89 GB page cache (not the index: ~0.5 GB of it).

## Gate

| check | 7a1ee466c | 40512942f | d751229f1 | 40921b9f7 | 0f184aeb7 | b9dcee289 |
| --- | --- | --- | --- | --- | --- | --- |
| `cargo dev check` (bare, worktree) | stopped (superseded) | FAIL rust:unit (launch_parity: rebase stats_json uncaptured) | PASS all 11 | PASS all 11 | PASS all 11 | PASS all 11 |
| `cargo dev check` (bare, clean checkout) | | | | | FAIL rust:unit: `parallel_rebases_lets_that_many_overlap` raced (an instant recorder rebase ended before the last started); fixed by making every rebase slow in that test | PASS all 11 |

b9dcee289 is the last code commit; the commit after it changes only this
table. census:clones prints REVIEW (advisory) with the same 51 groups as main
throughout; none is in code this branch touched.

## Proposed CLAUDE.md text (not edited here)

- Eval-time line, replace "(~2.3 hr names mode)" with: "(~95 min names mode
  with the default five lanes and one rebase at a time; ~119 min with
  `--parallel-pairs 1`, measured stall-free the same night, exp207)".
- "Run an eval ALONE": keep, and add: "`eval score` already runs its five
  pairs in parallel lanes (`--parallel-pairs`, default 5) with ONE rebase
  at a time (`--parallel-rebases`, default 1): a rebase at 472 saturates
  the 4-replica LLM server, and five at once made it slower in total
  (exp207). Peak memory is the lanes' sum, ~27-31 GB."
- Server line: "the LLM server is 4x gpt-oss-20b behind vllm-lb (half the
  GPUs; the other half runs glm53-flash), not 8x".
- References: `ref-names-40921b9f-r1/r2/r3` are valid five-pair names-mode
  runs on index 4736dcfa (parallel lanes; novel 5,336 / realLn 556,875
  byte-equal to exp204-75ae9334), but their spread does not fit the serial
  evidence on noise, reloc, newName, treeLn and vendorLn, so they did NOT
  re-set the bands; the bands remain 111401fe's (old index) until a serial
  repeat settles it.
