# Experiment methodology in the Rust era

Status: PROPOSAL, same standing as the rest of `docs/rust-port/` (see its
`README.md`). This doc says how measurement and experimentation work during
the migration and after it: what is frozen, what gets faster, which new
instruments the port makes affordable, and what must be re-baselined when the
formatter swaps (`03-migration-plan.md`, phase 5b). The eleven rules of
`docs/measurement-pitfalls.md` bind everything below; where a section leans on
one, it names the rule.

The develop-quickly payoff at the methodology level is not "runs are faster."
It is that verification becomes cheap enough to buy more of: an exact
neutrality verdict per refactor PR instead of per scare, a 20-hop walk per
churn lever instead of per release, and phase timing held to the same band
discipline as churn. The loop speed changes how often we can afford to know.

## 1. What does not change

**The harness stays TypeScript and drives the pipeline as a subprocess over
file trees and JSON.** This is already the design (`02-rust-target-architecture.md`
section 8) and the safety rail: the instruments that judge the port are not
themselves being ported. The pipeline's side of that contract — CLI flags,
exit codes, tree layout, `-run.json` / `-run-status.json` / stats / ledger /
trail shapes — is the frozen surface (`03-migration-plan.md`, phase 0).

**The shared measurement library keeps every role it has.**
`experiments/lib/` is "the shared measurement library — everything a gate or a
ceiling needs, once" (experiments/lib/README.md:1-16); none of it cares what
language emitted the tree it reads:

| file                                  | owns                                                                                                                                                                                         |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `boot-gate.sh`                        | the two-half boot check (`--version` contains the release, live `boot-ok` prompt); fatal at source time if bun is missing (boot-gate.sh:29-59)                                               |
| `diff.ts`                             | the ONE changed-line counter; replaced ~12 disagreeing implementations (diff.ts:1-26)                                                                                                        |
| `neutrality.sh`                       | byte-identity for should-change-nothing edits; warm shared cache; baseline-leg writes must be 0 (neutrality.sh:1-27, 275-278)                                                                |
| `selfhop.sh`                          | draw-pinned self-hop idempotence for both legs of an A/B, over the split TREE (selfhop.sh:1-26)                                                                                              |
| `gate.sh`                             | draw-pinned A/B: leg ON populates the LLM cache, leg OFF replays; leg-OFF write count is the isolation diagnostic (gate.sh:1-22)                                                             |
| `run-pipeline.ts` / `run-manifest.ts` | one scored pipeline run → one `-run.json` manifest: exit code, wall, peak RSS by PPID tree walk, cache write counts, kill switches (run-pipeline.ts:100-131, 144-156; run-manifest.ts:32-91) |
| `invariants.ts`                       | run-status and boot/self-hop/preflight verdict files; whether the pipeline itself declared a run valid (invariants.ts:37-46, 195-205)                                                        |
| `matcher-preflight.sh`                | matcher outcome-set check against real npm packages; exit 2 = "could not run", not a finding (matcher-preflight.sh:36-91)                                                                    |
| `trees.ts`                            | reading emitted tree/ledger/bundle; input shape declared, wrong shape throws (trees.ts:1-22)                                                                                                 |
| `counterfactual.ts`                   | git-capped ceilings via the REAL splitter on a perturbed prior, never line attribution (counterfactual.ts:1-27)                                                                              |
| `read-tracking.ts`                    | field-level proof that a recorded fact is actually read (read-tracking.ts:1-27)                                                                                                              |
| `size-*.ts`                           | no-LLM sizing probes (the pattern step 1 of every lever copies)                                                                                                                              |

**The eval harness contract stays whole.** `npm run eval` remains the one
dispatcher; the `VERBS` registry in scripts/eval.ts:145-339 (score,
neutrality, preflight, diff, summarize, bands, leaderboard) remains the only
list of supported instruments. Four pairs stay the corpus (pairs.json:1-35),
the KPI registry with declared directions stays authoritative
(kpis.ts:163-282), and the gate rule is unchanged: reducible KPIs (`noise`,
`noiseLn`, `relocSt`, `newName`, `mints`, `treeLn`, `reorderLn`, `vendorLn`)
go down; hold KPIs (`novel`, `realLn`, `vendorReal`) "must not move in EITHER
direction" (kpis.ts:103-118); every delta is judged against measured bands,
with the leaderboard printing `~0 (±band)` inside them (leaderboard.ts:202).
`experiments/034-eval-harness/VOCABULARY.md` stays the shared language.

**Corpus refresh policy** (the pairs are pinned, not permanent): during the
port the four pairs are deliberately frozen — adding one mid-port means an
oracle re-dump and a moving target for every gate, so it is not done. After
cutover, a fifth pair is ADDED (never substituted; the old pairs keep history
comparable) when either the newest pair falls roughly a hundred released
versions behind claude-code or the bundle format shifts (a new bundler era —
the signal that detection/unpack code paths are unexercised by the corpus).
Post-port the cost is small: one pipeline run per leg plus a band re-measure
for the new pair (three cold repeats, section 4). Whoever adds it re-points
the "current reference" label the same day — a label that says "current" ages
silently (CLAUDE.md's standing warning).

**Experiment filing conventions survive verbatim** (experiments/README.md:3-33,
144-157): a brief is a hypothesis, including its cautions; ceilings are
measured before builds; 118→119 stays the regression canary; tables lead with
the total; a finished README gets a STATUS block naming which of its own
claims died. Rule 9 stays the reading rule — every retraction lives in a newer
file than the claim it retracts, so a Rust-era reader of a TS-era experiment
checks the RESULTS file and the STATUS block before repeating a number.

**Ablation mechanics carry over** — kill-switch registry, `--disable`/`--probe`,
manifest recording. Section 7 covers them.

One harness-side change is required and is deliberately the ONLY one: today
every spawn site in the living instruments hardcodes `npx tsx .../src/index.ts`
— run.sh:198 (rebase leg), run.sh:369 (self-hop leg), run-pipeline.ts:187
(scored leg), selfhop.sh:77, gate.sh:109, neutrality.sh:170, and
experiments/076-statement-placement/walk.sh:76 (the mini-walk driver Policy B
promotes) — and no flag anywhere selects a different command. Archived
experiment scripts (038, 048-050, 056, 037) hardcode the same invocation but
are lab-notebook one-offs outside this change's scope. PROPOSAL: give "which
command is the pipeline" one owner in `experiments/lib` (a resolver all seven
living sites call, plus a `docs/responsibility.md` row), and give
`neutrality.sh` a per-leg override so one leg can be the TS pipeline and the
other the Rust binary. Phases 4-5a of `03-migration-plan.md` depend on exactly
that cross-implementation form, and phase 6's "point the harness at the
binary" becomes a one-line change instead of seven.

## 2. During the migration

Phases 0-5a have their own instruments — the parity gates of
`03-migration-plan.md`: decision-dump diffs (seconds, exact) for phases 1-3,
prompt byte-parity and warm-replay equivalence for phase 4, byte-identical
trees for 5a. The eval and neutrality instruments judge only at the
boundaries. Two standing rules for the period:

- **Port work and lever work never mix** (`02-rust-target-architecture.md`
  section 3). An experiment that lands on mainline TS during the port forces
  an oracle re-dump at the new commit (`03-migration-plan.md`, sequencing
  realities) — a re-run, not a re-design, but a real cost to schedule around.
- **Rule 10 governs the replay gates.** The warm cache is legitimate in
  phases 4-5a precisely because the verdict is about determinism with the
  model held fixed, and the write count proves the replay — the same proof
  obligation `neutrality.sh` already enforces (neutrality.sh:275-278). Any
  claim about naming QUALITY still goes cold.

## 3. What accelerates, and the policies it enables

Numbers from `04-performance-model.md` (projections labeled there; the
"today" column is measured):

| loop                                      | today     | post-port (projected) |
| ----------------------------------------- | --------- | --------------------- |
| warm neutrality pair (both legs)          | ~10 min   | ~1-2 min              |
| warm A/B probe (one candidate leg)        | ~5-10 min | ~1 min                |
| cold hop, biggest pair                    | ~17 min   | ~2.5-3.5 min          |
| cold 4-pair eval sweep (8 runs + scoring) | ~81 min   | ~25-35 min            |
| median walk hop                           | ~7.8 min  | ~2-3 min              |
| 20-hop mini-walk                          | ~2.5 h    | ~45-60 min            |

The mini-walk and warm-A/B-probe rows are derived: the probe's today figure
from 04's measured ~50 min / 8-run warm gate legs (exp048), its projection
from the ~10x warm factor; the mini-walk rows from 20 hops at the
walk-median 7.4-7.8 min/hop today and the projected ~2-3 min/hop after. Warm runs accelerate the most
(~10x) because they contain zero live LLM calls — pure CPU is Rust's best
case (`04-performance-model.md`, second-order effects).

Two policy proposals this enables:

**Policy A (proposal): a warm neutrality pair per should-change-nothing PR.**
Refactors, counters, type-level fixes — the class `neutrality.sh` exists for —
get an exact byte-identity verdict as a default step, because at ~1-2 min it
is cheaper than the review it accompanies. Mechanics unchanged:
`npm run eval -- neutrality <ref>` (scripts/eval.ts:197-225), warm standing
cache (`--cache`, neutrality.sh:87), dirty tree refused (neutrality.sh:132-136),
baseline-leg cache writes are the precondition read BEFORE the verdict line.
Today the same check is run when suspicion is high; the policy change is
running it when suspicion is low, which is where silent drift lives.

**Policy B (proposal): 20-hop mini-walks as the standard instrument for churn
levers.** Single-pair gates cannot see multi-hop feedback — the output becomes
the next hop's prior, which is exactly what rule 11's exp048 postmortem names
as the thing a pinned run cannot see. The driver already exists as a pattern:
experiments/076-statement-placement/walk.sh chains `PRIOR` hop to hop
(076/walk.sh:95), runs `boot_gate` every hop (076/walk.sh:94), and scores warm
pairs with the eval's own analyzer (076/walk.sh:107-116). At ~45-60 min
(projected) a mini-walk is affordable per lever rather than per arc.
PROPOSAL: promote the pattern to `npm run eval -- walk <label> --hops N` so it
enters the supported-verbs registry — an instrument not listed there is not
supported (house rule, CLAUDE.md).

One flag for later: post-port, the 4-pair sweep is dominated by the TS scoring
side (~27 min of analyze/report, `04-performance-model.md`), and 2-4 GB RSS
makes pairs parallelizable. The eval's bottleneck moves into the harness; see
open questions.

## 4. New instrument: per-stage wall times in every -run.json

Today the phase-anatomy table in `04-performance-model.md` was reconstructed
from log timestamps, because the span profiler that would have produced it
directly (`src/profiling/profiler.ts`, Chrome Trace Event output via
`--profile`, src/commands/unified.ts:1397-1413) has never been enabled by the
eval or the walk. The Rust pipeline makes stage timing a first-class output
instead of a flag:

- **Always on, no flag**: `tracing` spans (tokio-rs tracing +
  tracing-subscriber) at the twelve stage boundaries and the post passes —
  a dozen spans cost nothing. Written to `.humanify/phase-times.json`,
  unconditional on every run, exactly like `placement-stats.json`
  (src/split/layout.ts:56) and `stage-hashes.json` (src/split/layout.ts:73)
  are today.
- **Lifted into the manifest**: `run-pipeline.ts` already parses
  placement-stats into the `-run.json` manifest (run-pipeline.ts:222-233);
  phase-times joins it the same way. Every scored run then carries per-stage
  wall times with zero new flags, and `summarize.ts` can print a timing table
  next to the KPI table.
- **Deep traces stay opt-in**: the `--profile` equivalent emits a Chrome
  Trace via a `tracing-chrome` layer (per-function spans, concurrency
  snapshots — what the TS profiler's tid grouping does today).
- **Neutrality must exclude it**: add the basename to `DIAGNOSTIC_ONLY`
  (neutrality.sh:243, currently placement-stats.json and stage-hashes.json)
  or timing jitter fails every byte-identity verdict.

Phase timing then becomes a KPI with the same discipline as churn: direction
"lower", bands measured by the three-repeat protocol (section 8), and one
addition to band provenance — the HOST. Wall time is a machine artifact;
noise-bands.json provenance today records provisional/sources/commit/labels
and nothing about the host
(noise-bands.json:1-23), which is sufficient for churn and insufficient for
time. Rule 3 applies before trusting silence: plant a slowdown once (a
temporary probe switch that sleeps inside one stage) and watch the band alarm
fire, because a timing gate that has never flagged anything is not yet an
instrument.

TS-side down payment (proposal): `04-performance-model.md` already recommends
one profiled eval sweep as the cheapest pre-port measurement. Writing
`phase-times.json` unconditionally in TS is a small patch on the existing
profiler and gives the migration its today-columns for free.

## 5. New instrument: a criterion benchmark suite

Component-level regressions need a cheaper instrument than a full hop.
PROPOSAL: a `criterion` suite under `humanify-core` (`cargo bench`), with
committed baselines (`--save-baseline` per run, compared with `critcmp`),
sized by the measured scale of the machine being ported
(`01-current-architecture.md`, section 9):

| bench            | input                                        | scale anchor                            |
| ---------------- | -------------------------------------------- | --------------------------------------- |
| parse + semantic | the 215→216 fresh bundle (19.5 MB)           | two multi-MB ASTs live during matching  |
| hashing          | all three hash families over the same bundle | 64,493 functions, 35,903 statements     |
| cascade          | full oracle pair match                       | 200,425 bindings, 388,478 slot attempts |
| placement        | statements vs the prior ledger               | 35,903 statements placed                |
| emit             | codegen + slice + relink                     | ~1,500 src files + ~1,625 vendor files  |

Corpus: the phase-0 oracle corpus (`03-migration-plan.md`) — the eval inputs
live outside the repo (pairs.json `inputsBase`) and the oracle freeze already
commits or content-addresses them, so the full-scale tier pins to that. A
second, laptop-quick tier uses a small committed fixture, because the e2e
fixture builds are gitignored and built on demand (matcher-preflight.sh:56-91
exits 2 when they are absent — a bench must not inherit that failure mode).

Where it runs: on demand around perf-relevant changes, plus scheduled
(nightly or weekly), and NEVER in the commit gate — `npm run check` is ~25 s
(`01-current-architecture.md`, section 8) and stays that way.

How criterion's rigor relates to rule 11: criterion's within-run statistics
(many iterations, outlier handling, change detection) handle sampling noise
INSIDE one process on one occasion. They cannot see between-run environment
drift — thermal state, background load, a different machine. Rule 11 applies
at that level: run the full suite twice back to back on the blessed machine;
the largest per-bench disagreement is that bench's noise floor (the same
"largest pairwise disagreement" construction as `computeBands`,
noise-bands.ts:40-86); a delta inside the floor is `~0` regardless of the
p-value criterion prints. State the floor before reading any delta, commit it
next to the baselines, and set each bench's regression threshold above its
own floor — not at a universal percentage picked in advance. Rule 3's control
applies here too: a planted O(n^2) regression must fail the suite once before
its green is believed.

## 6. Profiling recipes per platform

When a phase-time band alarm or a bench regression fires, the next tool is a
profiler. Recipes, by platform and question:

| question                                | macOS (laptop)                                                     | Linux (devcontainer)                  |
| --------------------------------------- | ------------------------------------------------------------------ | ------------------------------------- |
| where does this stage spend CPU         | `samply record` (Firefox Profiler UI) or Instruments Time Profiler | `perf record -g` + `cargo flamegraph` |
| did allocation VOLUME regress (exact)   | `dhat` (dhat-rs, feature-gated allocator)                          | `dhat`                                |
| heap timeline / retention across stages | Instruments Allocations                                            | `heaptrack`                           |

When to reach for each:

1. **The phase-times table first** — it is free and already in every
   `-run.json`. Most regressions localize to one stage before any profiler
   runs.
2. **A sampling profiler second** (samply first — no instrumentation, no
   build change) when the stage regressed without an obvious workload change.
   Build with `[profile.release] debug = true` so frames have symbols; in the
   devcontainer, `perf` needs perf_event access (kernel.perf_event_paranoid
   or CAP_PERFMON).
3. **dhat when the suspicion is allocation volume.** Its counts are exact,
   and the pipeline's decision path is deterministic, so two runs diff to
   zero when nothing changed — an instrument with a zero noise floor, rule
   11's best case. An allocation-count delta is attributable in a way a wall
   time never is.
4. **heaptrack / Instruments Allocations when RSS grows across stage
   boundaries.** In the arena model (`02-rust-target-architecture.md`,
   section 6) memory that survives a stage boundary is a lifetime decision;
   growth there is a design bug, not tuning.

## 7. Ablation: kill switches and mandatory trails

The mechanism carries over from TS intact. Today: one registry
(`KILL_SWITCHES`, src/kill-switches.ts:49-120 — 13 `disable` passes, 1
`probe`), CLI entry via `--disable <passes>` / `--probe <probes>`
(src/commands/unified.ts:1451-1458), unknown or wrong-kind names fatal with
the valid set listed (kill-switches.ts:140-160), reads only through
`switchOn` with a census test against unregistered literals
(kill-switches.ts:19-21, 178), and every scored run's switches recorded into
its manifest from the child argv (run-pipeline.ts:144-156). The Rust CLI
keeps all of it — registry in `humanify-cli`, clap-validated, same argv
contract so the manifest recording keeps working.

The fail-loud property is worth naming because it already caught rot:
the header of experiments/076-statement-placement/walk.sh (line 28) still
documents `--disable fossil-settled-anchor`, a name no longer in the registry
— running its documented ablation leg today dies at the pipeline's arg parse
instead of silently ablating nothing. A
switch that is not in the registry is not a switch; keep it that way.

What the port adds is that **trails stop being optional**. Today three of the
fourteen cascade ladders have per-item trails; in the target design the tier
runner records attempt/settle/abstain itself, so a pass physically cannot run
untrailed (`02-rust-target-architecture.md`, section 5). That makes rule 11's
mechanics enforceable as protocol. For every A/B, in order, before believing
any metric:

1. **Read the manifest**: cache mode and write counts. Live-call evidence is
   cache WRITES, never a request counter (rule 10).
2. **Read the mechanism trail**: a pass with an empty trail on a hop cannot
   have moved that hop's KPIs — whatever the leaderboard prints there is draw
   noise. exp048 is the standing example: a hop credited with −2,864 lines
   had shipped zero renames; the real effect was −335 (rule 11).
3. **Compare against the mechanism ceiling** computed before the run (the
   lines the touched names occupy). A delta of the wrong order of magnitude
   is not yours.
4. **Only then read KPI deltas, against bands.**

## 8. Re-baselining after phase 5b

The committed noise bands are formatter-era artifacts. Every line-unit KPI is
counted over trees emitted by the current formatter, and phase 5b changes
every emitted byte at once (`03-migration-plan.md`), so noise-bands.json
expires wholesale — it is already commit-scoped, and the leaderboard already
warns when bands were measured at a different commit than the labels
(leaderboard.ts:66-70). The re-baseline, at the cutover commit:

- **Three cold `score` repeats, then `npm run eval -- bands <a> <b> <c>`**
  (the verb refuses cross-commit labels, scripts/eval.ts:277-323). Note the
  current file was computed from TWO repeats (noise-bands.json:1-23, labels
  exp076-head-a/b) while CLAUDE.md says three; standardize on three and
  correct whichever document is wrong at that point. Cost today would be
  3 × ~81 min ≈ 4 h; post-port 3 × 25-35 min, less if pairs run concurrently
  at 2-4 GB RSS (projected, `04-performance-model.md`).
- **Self-hop re-baselined the same way, with its two regimes kept distinct.**
  Warm/pinned self-hop must return to exactly 0 — it is a deterministic
  surface and 5b's own exit gate. The COLD self-hop floor is a measured
  quantity, not an invariant (78 lines on a control run,
  docs/measurement-pitfalls.md rule 10), and the three repeats re-measure it,
  since self-hop runs inside every `score` on the last pair (run.sh:354-413).
- **A fresh reference label** scored at the cutover commit becomes the
  comparison point; `main-2026-08-20` and its predecessors stay historical.
  The standing rule applies: a label that says "current" ages silently —
  re-score and re-point rather than trusting the name (CLAUDE.md).
- **Timing bands** (section 4) are measured in the same three-repeat session,
  with host provenance recorded.

Until this re-baseline exists, no Rust-era churn number is comparable to a
TS-era one; the first post-5b eval establishes the new floor rather than
checking against the old one — the same posture rule 10 forced when the
cache-pinned baselines went cold.

## 9. LLM-side experiments: the concurrency sweep (runnable today)

The post-port floor is the LLM wall — service time ÷ concurrency, ~113 s on
the biggest pair at c=32 (`04-performance-model.md`) — and the one lever on
it is server-side concurrency. The sweep:

- One pair, 215→216 (largest service time, 3,615 call-seconds), COLD — cache
  off is required both by rule 10 and because a cache hit never touches the
  server, so warm timing is fiction.
- c ∈ {32, 64, 96, 128, 32} via the existing `-c` flag (run.sh builds `-c N`
  into the child argv, run.sh:229-234; default 32 from pairs.json). The
  repeated c=32 endpoint, run last in the same session, measures serving-state
  drift — the sweep's own noise floor, stated before reading deltas (rule 11).
- Read: the naming-window wall (today `coverage.elapsedMs` from
  `--stats-json`; phase-times.json once section 4 lands), mean call latency,
  and the vLLM server's own throughput/queue metrics.

Interpretation is a discrimination, not a number: today's TS client reaches
only 7.3x effective parallelism at c=32 (`04-performance-model.md`), so if the
naming wall does not drop from 32→64, the CLIENT is confirmed as the binding
constraint and the server question stays open — answer it with an offline
replay probe that drives captured prompts from the request-content-keyed disk
cache at controlled concurrency, with no pipeline CPU in the way. If the wall
does drop with flat latency, the post-port floor halves per doubling:
~113 s → ~57 s at c=64 (projected). Either outcome informs
`04-performance-model.md`'s floor, costs nothing to obtain, and pays in any
language — it is one of the two items the plan's README lists as worth doing
regardless of the port decision.

## 10. Worked example: one lever, end to end

A hypothetical post-port lever: a new matching tier (say, two-hop caller-shape
corroboration to crack residue buckets). TOTAL loop, all steps serialized:
today ~4.5 h of machine time; post-port ~1.5-2 h (projected). The steps and
what each is allowed to claim:

| step                        | instrument                                                                                                        | validity condition                                                                                               | today                                                      | post-port (projected)             |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------- | --------------------------------- |
| 1. ceiling                  | no-LLM sizing script over match dumps/diagnostics (the `experiments/lib/size-*.ts` pattern)                       | probe shown to fire on a known-positive fixture before any zero is believed (rule 3)                             | minutes, plus ~5-10 min warm run if fresh dumps are needed | minutes, plus ~1-2 min            |
| 2. build with trails        | tier registered in the cascade; the runner trails it by construction (`02-rust-target-architecture.md` section 5) | commit gate green                                                                                                | ~25 s (`npm run check`)                                    | cargo test + the TS harness tests |
| 3. warm probe, draws pinned | one warm candidate leg against the reference cache; byte-diff vs the baseline tree                                | candidate-leg cache writes = 0                                                                                   | ~5-10 min                                                  | ~1 min                            |
| 4. cold gate                | `npm run eval -- score <label>`, four pairs vs a cold reference                                                   | trail non-empty on hops that moved; delta inside the mechanism ceiling; `novel`/`realLn` inside their zero bands | ~81 min                                                    | ~25-35 min                        |
| 5. mini-walk                | 20 hops, boot gate per hop, warm pairs scored with analyze.ts                                                     | boot gates green; churn trend vs the reference walk                                                              | ~2.5 h                                                     | ~45-60 min                        |

Step 3 deserves its fine print, because it is the step that is exact. A
matching tier that only CONVERTS prompted functions into mechanically-named
ones shrinks the prompt set, so the candidate's prompts are a subset of the
cache and the warm replay pins every draw — the byte diff against the baseline
tree is then the tier's exact mechanical effect, two orders of magnitude below
the cold noise floor if need be. The write count is the tripwire, not a
formality: if the tier changes downstream prompt CONTEXT (mechanically named
callees now appear in later prompts), those prompts miss the cache, writes go
nonzero, and the probe voids itself — the same leg-OFF diagnostic `gate.sh`
uses (gate.sh:1-22). A voided probe is not a failure; it means step 4 is the
first valid instrument for this lever.

The contrast with today is not which steps exist — all five exist now — but
which are affordable to repeat. Re-running step 4 after a fix costs another
81 minutes today, so fixes get batched and levers share gate runs; at 25-35
minutes they do not. Step 5 today costs an afternoon and is therefore run for
arcs, not levers; at ~45-60 min it becomes the default final gate for
anything that touches naming or placement.

What REMAINS outside even the full five-step loop: cross-session LLM
serving-state drift (bounded only by repeats, rule 10), long-walk behavior
beyond 20 hops (the mini-walk samples it; the 124-hop walk remains the
occasional full-scale check), and any emitted surface the harness does not
score — rule 8's audit ("enumerate what the harness does not look at") re-runs
whenever the pipeline grows a new output, exactly as it failed to for
`vendor/` through thirteen experiments.

## Open questions

1. Post-port, the eval sweep's dominant term is TS-side scoring (~27 min,
   `04-performance-model.md`). Parallelizing analyze per pair is harness-side
   TS work — when does it pay, and does anything in analyze assume serial
   execution?
2. Which machines are blessed for timing bands and the bench suite (laptop
   vs the Linux devcontainer), and where does host provenance live in
   `noise-bands.json` without breaking `loadNoiseBands` consumers?
3. `eval -- walk`: fixed standard 20-hop segment for cross-lever
   comparability, or rolling latest for relevance? A fixed segment ages like
   a reference label; a rolling one breaks lever-to-lever comparison.
4. If the concurrency sweep shows the server sustains c > 32, does
   concurrency stay a `pairs.json` constant or become per-machine config?
5. Do `phase-times.json` and its manifest lift land in TS now (small patch on
   the existing profiler, gives migration-era today-columns) or first in the
   Rust binary?
6. Criterion regression thresholds: the per-bench floor protocol needs an
   idle machine — is the shared devcontainer quiet enough, or does scheduled
   benching need a reserved window?
