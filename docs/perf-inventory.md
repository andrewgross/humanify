# Perf inventory (2026-09-30)

**STATUS: measurement-only survey, branch `measure/perf-inventory` (from
main `e12c94d2`). No pipeline code was touched.** Binary: the release
build (`cargo build --release --locked`) of `e12c94d2`. All runs
LLM-free — the deterministic stub (`scripts/lib/stub-llm.ts`), `--profile`
captured, RSS sampled every 2s. Raw artifacts:
`/work/perf-inventory/` (`run-one.mts` the runner, `analyze.py` the
RSS-vs-phase aligner, `run{1,2a,2b,3,4}*/` per-run profile.json + rss.csv

- stderr.log).

The core lesson of the fast-mode study
([`docs/rust-port/20-fast-mode.md`](./rust-port/20-fast-mode.md)) frames
everything below: **a real cold run is SERVER-bound** (naming is ~97%
prefill; vLLM's throughput is the wall), so client-CPU wins only pay
where the client is the wall — warm/cache-replay runs, stub runs, faster
servers — and pure memory wins pay everywhere (every eval leg). Items
below say plainly which world they live in.

## What was measured

| run | shape                                                   |  wall | peak RSS | LLM calls (stub) |
| --- | ------------------------------------------------------- | ----: | -------: | ---------------: |
| 3   | fresh 2.1.182, `--split` (the eval's rebase-leg shape)  | 512 s | 57.74 GB |           43,795 |
| 4   | 2.1.213 ← 2.1.212 prior, `--split` (the walk-hop shape) | 192 s | 16.92 GB |            1,369 |
| 1   | fresh 2.1.182, NO `--split` (naming only)               | 505 s | 57.76 GB |           43,795 |
| 2b  | 213 ← prior, NO `--split`                               | 127 s | 16.87 GB |            1,369 |
| 2a  | control: 213 with a BROKEN prior (see incident 1)       | 700 s | 81.64 GB |           52,507 |

Runs 1–2b first ran without `--split` (inherited runner flags from the
prompt-memory lane — that lane's "trees" are naming-only trees; `--split`
is OFF by default and every standard launch — `run.sh`, `gate.sh`,
`selfhop.sh`, `neutrality.sh`, `scripts/e2e.ts` — passes it). Run 1/2b
phase tables still isolate the naming path cleanly, so they are kept as
evidence. Walls carry a few percent of noise: a ~2-core match-verb job
from the `fix/match-prior-cache` lane was co-running (its own lane, not
touched); no other heavy runs. `--profile` was on every run; the phase
vocabulary is `profiling::phase(...)` sites in `crates/` (see
`python3 /work/perf-fast/phases.py <profile.json>` for any future run).

## Fresh 2.1.182, full run (`--split`) — where 512 s goes

Single-core: 96% of the wall runs at **1.0 of the box's 64 cores**.

| phase                                           | wall s | cores | notes                                                                                                                                                                                                                            |
| ----------------------------------------------- | -----: | ----: | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| read+detect / unpack+vendor / libdetect / babel |    4.5 |    ~1 |                                                                                                                                                                                                                                  |
| era:waves                                       |  450.3 |  1.00 | **88% of the whole run**                                                                                                                                                                                                         |
| — waves:llm-pipelined                           |  302.2 |  0.99 | 43,795 stub asks; 6.9 ms/call of client CPU                                                                                                                                                                                      |
| — waves:setup                                   |  118.0 |  1.00 | of which build-context 40.7, owned-bindings 4.4, context-view 1.5, recrawl 0.7 — **70.7 s (60%) sits in NO sub-span** (binding-info collection/clones, lane assembly, ctx bookkeeping: ~1.4 ms per function pass, 53,636 passes) |
| — waves:gate-release                            |    7.9 |  1.00 |                                                                                                                                                                                                                                  |
| — waves:barrier                                 |    0.5 |       | stub-latency-free; server-bound in reality                                                                                                                                                                                       |
| era:naming-floor                                |   25.3 |  0.98 | fresh-only; full-AST pass, no sub-spans                                                                                                                                                                                          |
| naming:validate+reconcile                       |    4.7 |  1.00 | two re-parses + text diff — cheap now                                                                                                                                                                                            |
| naming:census / era:generate                    |    1.3 |       |                                                                                                                                                                                                                                  |
| split (whole stage)                             |   14.1 |  1.96 | compute 12.2 (input 3.3, assign 2.3, review 0.7, runnable-cjs 5.3) + write-tree 0.3 + **finish 1.6**                                                                                                                             |
| reports                                         |    0.0 |       |                                                                                                                                                                                                                                  |

The decomposition the brief asked for (fresh):

- **(a) AST parse/format: ~4.5 s (1%).** Trivial.
- **(b) serial decision code: ~25–35 s (5–7%)** — naming-floor 25.3,
  gate-release 7.9. Match/transfer do not exist fresh. (With a prior they
  are 35 s of a 192 s run — see below.)
- **(c) prompt build/render: ≥42 s measured** (build-context 40.7 +
  context-view 1.5), plus an unmeasured share of the 302 s dispatch span
  (rendering is inside `waves:llm-pipelined`; no sub-spans).
- **(d) waves-era working-set churn: the 57.7 GB story** — memory
  profile below; #65 proved this scales wall (−25%) when it shrinks.
- **(e) I/O: ~2–3 s.** Tree writes (33 MB, 1,586 files), ledger, carry.
  Never a bottleneck.

Per-ask wave-era client cost: (302 + 118 + 8.4) / 43,795 ≈ **9.8 ms per
ask**, all at one core. 165,131 identifiers (100%) go to the LLM fresh.

## With-prior 2.1.213 ← 2.1.212, full run (`--split`) — where 192 s goes

The walk-hop shape: 91.3% of functions carry from the prior (58,154
cached, 945 LLM-asked); only 1,369 stub calls. Uses the box a little
(2.3 cores inside naming; 50 cores only inside twin-inventories).

| phase                                                         |   wall s | cores | notes                                                                                                                                    |
| ------------------------------------------------------------- | -------: | ----: | ---------------------------------------------------------------------------------------------------------------------------------------- |
| unpack+vendor                                                 |      2.7 |   3.1 |                                                                                                                                          |
| prior:sides-parallel                                          |      8.3 |   4.5 | both parses + both graph builds; the 33 MB prior is parsed TWICE in the fast path (its own thread + again for the AST later stages walk) |
| prior:index                                                   |      1.2 |   2.0 |                                                                                                                                          |
| prior:match-cascade                                           |     21.7 |  1.08 | 3 function cascades (16.5) + 2 binding rounds (4.7) — the serial decision code, verified                                                 |
| prior:close-dump                                              |      9.1 |   6.5 |                                                                                                                                          |
| prior:twin-inventories                                        |     0.35 |  49.7 | already parallel                                                                                                                         |
| era:transfer                                                  |     13.4 |  1.00 | settle 9.8 — serial decision code, verified                                                                                              |
| era:close-contexts                                            |      1.7 |  39.9 | already parallel (`par::map_ordered`; prior-path only, by design)                                                                        |
| era:waves                                                     |     36.5 |  1.00 | setup 12.2 (build-context 3.3), dispatch 18.3 (13.5 ms/call), gate-release 0.7                                                           |
| validate+reconcile / deferred-sweep / family-permute / census |     20.6 |  ~1.2 | sweep 4.3, permute 9.8, reconcile 5.5                                                                                                    |
| **split (whole stage)**                                       | **66.7** |  1.13 | **35% of the whole run**                                                                                                                 |
| — split:compute                                               |     25.9 |  1.56 | assign 13.7 (the prior-inherit layout path), input 3.9, runnable-cjs 6.4, review 0.9                                                     |
| — split:finish                                                | **40.2** |  0.86 | **relink + post-split reconcile + carry: restored 2,489 prior names across 1,264 of 4,772 files — serial**                               |
| — split:write-tree                                            |      0.6 |       |                                                                                                                                          |

Highest-signal surprises vs the fast-mode study's 215→216 table:

1. **`split:finish` is 40 s serial with a prior** (fresh: 1.6 s). The
   post-split reconcile + carry (reading/renaming the prior's file set)
   is now the single biggest non-waves item in a walk hop, and the walk
   pays it 33 times. NOT in the fast-mode study's table (its 20 s "split"
   line was pre-cutover TS-era-prior shape). No sub-spans exist inside
   it — relink vs reconcile-read vs reconcile-apply vs carry is not yet
   measurable.
2. The fast-mode study's serial-decision numbers **verify on current
   main**: match cascade 22 s → 21.7 s; transfer 16 s → 13.4 s (settle,
   1.0 core).
3. The study's parallelize-exact levers all landed as defaults and show:
   close-contexts 39–50 cores, twin-inventories 41, validate+reconcile
   17 s → 4.7–5.5 s, family-permute 16 s → 9.8 s.

## The memory story (finding #66's phenotype, re-measured)

Fresh 182 (`--profile` + RSS): RSS starts ~0.5 GB at the unpack, grows
**steadily and linearly through the waves era** (~1.27 MB per ask) to a
57.7 GB peak at era end (~465 s), then **collapses to ~6 GB the moment
the wave `Run` is dropped**, and the tail (naming-floor, reconcile,
split) runs at ~6 GB. With-prior 213 peaks 16.9 GB, concentrated in the
FIRST minute (the prior side: two prior parses + graphs + matching), and
the waves add little (few asks).

What this pins down:

- The ~50 GB over baseline is **live, retained wave-`Run`-internal state,
  held until the era ends** — consistent with the lane's mimalloc
  finding (committed == RSS, purge-proof). #65's window gauge reads peak
  105 prompts / 2 MB — the in-flight prompt set is NOT the owner.
- Growth is per-ask/ per-function-pass, NOT per-identifier-asked-window.
- **The live-owner split is still unmeasured, and the named instrument
  (`context_set_names`) is test-only today** — it is a `WaveOutcome`
  field computed at run end (`processor.rs:624`) and read only by a unit
  test; `unified.rs` prints #65's two gauges but not this one, and it is
  not in `--stats-json`. External heap profilers cannot help:
  heaptrack/valgrind are not installed and have no sudo, and neither
  could see mimalloc anyway (it mmaps directly as the Rust global
  allocator; `LD_PRELOAD` interception sees nothing).
- Code reading CONTRADICTS the obvious suspects, which is exactly #65's
  lesson (attribute, don't trust the hypothesis): the #56 sharing fix
  means used-identifier layers are shared `Arc<NameLayer>`s; the stored
  `FnContext` per strategy pass (`strategies` grows one entry per
  function pass, never freed until the era ends) holds callee
  signature _snippets_ (3 lines), capped context vars (30 × 120 chars)
  and a `taken` set — likely tens of KB, not 1.4 MB. The full-body
  `CalleeView`s are transient. So the per-ask ~1.27 MB is going
  somewhere not yet named. Candidate owners worth gauging first, in
  order: the `strategies` Vec total bytes (53,636 entries, full contents
  summed), the `ctxs` NodeCtx maps, `sets` + `renamed_layers`
  accumulation (post-#56 residue — the #56 observable, `context_set_names`,
  counts exactly these), `state` (RenameState) growth from per-pass
  recrawls, and the `names`/`Prepared` record Vecs.

Practical cost of #66 today: the box's heavy-run rule permits ~3 fresh
legs at once (247 GB) — every eval leg with a fresh rebase spends 58 GB
of it, and (per #65) the churn costs wall.

## The ranked inventory

Ranked by (measured size) × (confidence it is behavior-preserving). BP =
byte-identical-output, provable via warm-cache/neutrality tools. DC =
decision-changing, needs the cold eval.

### 1. #66: the fresh run's ~58 GB live working set — instrument, then fix

**RESOLVED 2026-10-02 (branch `fix/taken-set-retention`).** The
instrumentation lane's gauges split the fresh-182 peak's stored-strategy
owner at 31.1 GB (measured 2026-10-02); the sub-gauge added on the fix
branch re-pinned that owner at **28.15 GB** (the lane's 31.1 GB
double-counted the module strategies' shared taken `Arc`s, once per
strategy) and split it: **taken-name sets 28.11 GB (99.9%)** — 843.9M
name strings, one private clone of the scope chain's already-renamed
names per function pass — against bindings 5 MB, callee snippets 14 MB,
callsites 3 MB, context vars 5 MB, module lists 11 MB. The #56
phenotype on the renamed-name field, as the lane suspected.

The fix is #56's pattern on that field: `naming::waves::taken::TakenNames`
holds the `renamed_layers` map's immutable per-scope snapshots (one
`Arc` per chain scope) instead of cloning the union — membership over
the layers is membership over the union, and the `Arc`s freeze the
build-time state exactly as the private clone did (the map replaces its
`Arc` on a table-version bump, never mutates it). Before/after, same
input, same flags, stub LLM (the instrumentation lane's runner; artifacts
in `/work/taken-set-retention/`):

- fresh 2.1.182: peak RSS **57.81 GB → 8.35 GB** (−49.5 GB, −86%; the
  clone traffic was mimalloc-amplified far beyond its own 28 GB — with
  843.9M small string allocations gone, the wave era now peaks below the
  tail's old 58→6 GB collapse line); the taken sub-gauge 28,112 MB →
  78 MB (843.9M names → 2.37M — the residue is stale-generation
  snapshots a context froze before its scope's table bumped); wall
  527 s → 446 s (−15%).
- with-prior 213←212: peak 16.99 GB → 16.88 GB (the hop's peak is the
  prior side's parse/graph/matching, not the wave-era retention — the
  strategies gauge still fell 1,935 MB → 38 MB, taken 1,932 MB → 35 MB);
  wall 199 s → 191 s.
- byte-identity: trees, asks.jsonl (44,351 rows fresh / 1,446 prior),
  prompts.jsonl and cache-keys.jsonl (11.5 GB fresh) all IDENTICAL
  before vs after on both shapes (`compare.sh` in the records dir,
  output in `compare.txt`); exit codes 0 everywhere; the gate 12/12
  (`npm run check` on the branch).

(Everything below is the item's pre-fix state, kept as the record of
how the number was chased.)

Memory is perf (see above). The fix class is #65's: a retention fix with
byte-identity proof; #65 incidentally made runs ~25% faster, so a wall
dividend is plausible but unmeasured. First step is instrumentation a
5-line print-only change away: surface `context_set_names` (the #56
observable) in `unified.rs` beside the #65 window gauge, plus
accumulated-bytes gauges over the candidate owners above, then one fresh
182 run splits them. **BP-high confidence for the instrumentation and
most likely for the fix; size: 58 GB → unknown; pays in every eval leg
(memory headroom → more parallel legs) and plausibly in wall. Do this
first.**

### 2. Wave round setup is single-core and 60% un-attributed — extract plain data, then parallelize

Fresh: 118 s of `waves:setup` + 8 s gate-release (~25% of the run);
only 40% of setup has sub-spans. The proven pattern is in-repo —
`par::map_ordered` on plain-data extraction turned close-contexts into
39-core work with byte-identical results (fast-mode "exact" lever, now
default; e2e-pinned). The blockers are the known ones: `build_context` /
owned-binding collection read the oxc `Semantic` (not `Sync`) — the
fast-mode study's "extract plain data first, then parallelize". First
step: sub-spans for the unattributed 60% (binding-info collection, lane
assembly, ctx maps) so the extractable share is measurable. **BP; size
−40…−100 s fresh (−8…−20%), −15 s with-prior; client-side only — worth
it only where the client, not the server, is the wall.**

**RESOLVED 2026-10-02 (branch `perf/module-lanes`).** The sub-spans
isolated the whole story to ONE seam: the module-lane groups'
`module_strategy`, which for EVERY group of a wave step re-cloned the
~25k-name used list (`JsSet::to_vec`), re-counted the target scope's
bindings (`bindings_in(..).len()` — a full clone + sort), re-read the
taken-name set, and re-windowed via `get_proximate_used_names`
(per-name binding lookup + per-ref line mapping) — over inputs that are
IDENTICAL for every group of a step, because no ask is driven, so no
rename applies, until `drive_round` runs after every group's strategy is
built. The function-side constituents everyone suspected were already
sub-2 s (select-bindings 0.75, register 0.56, lane assembly 0.55).

The fix is the extract-then-parallel pattern verbatim: `ProximityWindow`
(`rename::votes::proximity`) extracts the used list, binding count,
droppability and per-name proximity lines ONCE per wave step on the
calling thread (the oxc-side state behind the extraction is not
`Sync`); each group's batch windows on the rayon pool
(`par::map_ordered`) and rejoins in group order. Module retry seeds
re-ask through the same step snapshot (see
`docs/responsibility.md`'s proximity-window row; byte identity to the
serial `get_proximate_used_names` — still the function lanes' per-request
path — is pinned in `proximity_test` against the live serial function, a
hardcoded serial-plan fixture, and the parallel rejoin order).

Numbers (clean main `a977f4e7` vs the branch, stub LLM, back-to-back
legs, box otherwise idle; artifacts `/work/module-lanes/`):

|                                          | before |  after |
| ---------------------------------------- | -----: | -----: |
| fresh 182: `setup:module-lanes`          | 78.7 s |  1.8 s |
| fresh 182: `waves:setup` (whole)         | 92.3 s | 15.0 s |
| fresh 182: total wall                    |  449 s |  368 s |
| with-prior 213←212: `setup:module-lanes` | 10.6 s | 0.29 s |
| with-prior 213←212: total wall           |  173 s |  165 s |

(The 73.9 s figure quoted around this item was measured on the PRE-#66
binary; on current main the span was 78.7 s — after #66 shrank
`setup:build-context` 40.7→1.5 s, module-lanes dominated round setup
even more.) The remaining 1.8 s is the once-per-step extraction span
(`setup:module-window`, 1.68 s serial) plus ~0.14 s of pool wall
(~40 cores). Peak RSS unchanged: fresh legs byte-equal (8.35 GB plain /
9.36 GB dump), prior legs within 2 s-sample noise (±0.1 GB).
Byte-identity proof: all four before/after trees `diff -r`-identical
(5,762 fresh / 6,455 prior files), `asks.jsonl` identical (44,351 /
1,494 rows), `prompts.jsonl` identical (154.8 MB / 13.1 MB),
`cache-keys.jsonl` identical (11.5 GB / 1.08 GB) — asks, prompts, apply
order and output all preserved.

### 3. With-prior `split:finish`: 40 s serial per hop — sub-spans first

New measurement (not in any prior study). Post-split reconcile + relink

- carry costs 40 s × every walk hop and every eval prior leg, at 0.9
  cores. Unknown what it decomposes into (no sub-spans). If it is
  per-file work over the prior tree, canonical-order parallelization is
  the same BP pattern as #2. First step: sub-spans (relink, reconcile
  read, reconcile apply, carry). **Likely BP (deterministic stage; the
  repo's own doc notes reconcile is why draw-pinned A/B is licensed to
  measure it); size −30 s on a 192 s hop (−16%); client-side only.**

### 4. Incident: a useless prior silently degrades to a full-ask fresh run

Control run 2a: passing the wrong file as `--prior-version` (a 285 KB
entry stub instead of the tree's `.humanify/humanified.js`) produced
exit 0, a "loaded" banner, vendor entries re-keyed 1607/1607 — and ZERO
carry: 196,945 identifiers asked (52,507 calls), 700 s, **81.6 GB**. No
message louder than the coverage table said so. The cost multiplier of a
broken prior is real: 3.7× wall, 4.8× memory. Suggested guard: a loud
WARNING line when the match cascade binds ~0 functions (the
`--profile`'s 0-second `prior:match-cascade` was the only tell).
**BP (print-only); cheap; protects eval/walk wall and the heavy-run
memory budget.**

### 5. The client's per-call floor: 6.9 ms/ask (render + HTTP + parse, un-split)

Fresh dispatch is 302 s of client CPU for 43,795 asks — the floor for
warm replays and any faster server. #65's −25% shows allocator/dispatch
churn here is worth real wall. But no evidence yet says where the 6.9 ms
goes (prompt render vs HTTP vs response parse vs record bookkeeping).
First step: sub-spans inside `waves:llm-pipelined`; second: reuse of
rendered context across windows of the same function (batch-25 amortizes
the LLM-side re-send; the client still re-renders per window).
**Probably BP for render-caching (same bytes, same order) but it must be
proven; size bounded by 302 s fresh; worthless on today's server-bound
cold runs.**

### 6. Fresh naming-floor: 25.3 s single-core, fresh-only

`era:naming-floor` (taint collection + expression-name derivation +
retry decoration over the whole AST) is 5% of a fresh run and absent
cheap with a prior (0.4 s). No sub-spans to say which of the three
passes owns it. **BP after sub-spans; candidate for the same
extract-then-parallelize pattern; −25 s fresh upper bound;
client-side only.**

### 7. Serial decision code: match cascade 21.7 s + transfer settle 13.4 s (per hop)

Verified current main. The fast-mode study's honest ceiling statement
stands: parallelizing with a canonical tie-break changes which ambiguous
pairs resolve — **an eval-gated matching change, not a speed change**
(DC, large). Not recommended as perf work. Related but owned elsewhere:
the match VERB's per-call prior reparse (`fix/match-prior-cache`, row
#68's cost lever) — this survey did not touch `match_verb.rs` or
`prior.rs`.

### 8. Wave-barrier redesign (~200 s of LLM wall at any concurrency)

Standing B-item. Invisible in stub runs (barrier span = 0.5 s here);
lives entirely in the server era. The deterministic-snapshot DESIGN work
(prefix-defined snapshots so prompts read run-wide state canonically) is
LLM-free and shippable as a proposal; the payoff ceiling is the sim's
~200 → ~120 s at 32 slots. **Worth it only when the LLM server is no
longer the bottleneck, or on a server that holds latency under load
(where the sim says relaxed-tier is worth −40%).**

### Explicitly NOT worth it (measured fine)

- Split fresh: 14.1 s total, already ~2 cores. emit/finish fresh: 1.6 s.
- I/O (writes): ~2–3 s/run, 33 MB trees.
- validate+reconcile: 4.7–5.5 s (the fast-mode 17 s is already fixed).
- Prior side foundations: sides-parallel 8.3 s at 4.5 cores; twin
  inventories 0.35 s at 50 cores. The prior double-parse (33 MB × 2 in
  the fast path) is real but overlaps the fresh side — ≤3 s; fold into
  the match-prior-cache lane's duplicate-parse work if it touches the
  pipeline too.

## What I would do first

1. **Instrument #66** (print-only): surface `context_set_names` + owner
   byte-gauges, one fresh 182 run, split the 58 GB. Everything about the
   biggest memory item is downstream of a ~day of instrumentation.
   #65's fix bought 36 GB and 25% wall by doing exactly this first.
2. **Sub-spans for the three un-attributed serial spans** — wave setup's
   60%, `split:finish`, `waves:llm-pipelined` internals — one small
   instrumentation PR, then this table fills in and items 2/3/5 size
   honestly.
3. **The broken-prior WARNING** (item 4) in the same PR: it is
   print-only, and it just cost this survey a 700 s / 82 GB control run
   by accident.
