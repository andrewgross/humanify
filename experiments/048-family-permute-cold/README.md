# 048 — The family-permute pass, re-decided cold

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
totals-first; every hop judged **on its own**.

**Read [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md) first
— all ten rules.** This brief is a HYPOTHESIS, including its cautions. exp046's
brief was wrong about its central proposal; exp047's was wrong about its central
proposal AND its "no third unscored surface" negative result. Assume the same
rate here.

## The one-sentence version

A post-render pass that permutes names within provably-interchangeable statement
buckets was built in exp036, measured at **−2,103 noiseLn (−35%) on one hop**,
and shelved because it cost a **6-line self-hop regression** against references
that held a strict-0 self-hop. **exp047 proved that strict-0 was an artifact of
the LLM cache** — cold, main's own control violates self-hop by **78 lines**. The
criterion that blocked the largest identified `src/` lever was measuring the
cache, not the code. Re-decide it cold.

## Background you need, because the source docs disagree with themselves

`experiments/036-interchangeable-assignment/README.md` is the primary source, but
its STATUS block at the top says the axis is **CLOSED** and calls the residue
"irreducible", while §8b further down explicitly retracts that:

> **(Superseded) earlier over-conclusion — the residual as a floor.** … The
> post-render diff DOES carry usage-context evidence v1 threw away (hence v2's
> win) — so this is not a hard floor, but a determinism tradeoff pending the
> user's call.

**The retraction is the current state** (rule 9: the correction lives in the
newer text, and here it is even in the same file). Do not read the STATUS block
and stop.

### What a family bucket is, and what rotates

Several top-level statements share one [statement hash](../034-eval-harness/VOCABULARY.md#family-bucket)
— 16 identical identity functions, 536 `var a, b, c;` lists. The hash proves
there are N members on each side of a release; it says **nothing about which
corresponds to which**. The matcher picks a pairing; next release it picks a
different one; names permute around the bucket. The classic tell is a cycle:
`commandExtractor → commandRunner → commandValidator → commandExtractor`. No
name entered or left. The diff charges every usage site of every member, because
a name is tree-wide.

**The reframe that licenses the fix:** for provably-interchangeable members, any
consistent assignment is equally correct — they are semantically
indistinguishable, so a "wrong" pairing is unobservable in the output. The goal
is **stability, not identification**. A permutation within a certified bucket is
a bijection, safe by construction; no liveness gates are needed and the
pure-rename validation is the backstop.

### What was built (branch `exp036-8b-diff-objective`, UNMERGED)

Post-render pass: re-parse the shipped output and the prior, group top-level
statements by statement hash (same hash ⇒ differ ONLY in bound names ⇒ the
interchangeability certificate, recomputed post-render with no cross-boundary
identity bridge), solve each equal-count bucket as a min-diff bipartite
assignment against the prior's names, apply as an atomic name permutation.

Files: `src/rename/family-permute.ts` (pure core, 185 ln),
`family-permute-step.ts` (wrapper, 316 ln), wired via `finalizeWithFamilyPermute`
in `src/rename/plugin.ts:530`. Toggle: **`HUMANIFY_NO_FAMILY_PERMUTE=1`**.

| version | approach                                        | noiseLn (ctrl 5,981) | self-hop |
| ------- | ----------------------------------------------- | -------------------: | -------- |
| v1      | naive pool byte-match                           |   **16,523 (+177%)** | —        |
| **v2**  | lock round-trips + masked usage context, greedy |   **3,878 (−2,103)** | 6 ln     |
| v3      | + MIN_SUPPORT≥2                                 |           5,984 (+3) | —        |
| v4      | + mutual-unique-best                            |          5,936 (−45) | 14 ln    |

Two facts to carry:

- **v1 was a real bug, caught by reading**: it renamed CORRECT names
  (`getClaudeCodeOAuthToken → deviceActionMap`). The rebuild locks any name that
  round-trips (present on both sides → never touched) and moves only orphans, by
  MASKED USAGE CONTEXT (reference lines compared with each own name blanked),
  plus a no-mint-target gate.
- **−2,103 far exceeds the 297-line declaration ceiling** because each restored
  name also cleans its call-site echoes. The ceiling under-predicted; do not use
  it as the expected value.

### Why it stalled, and why that reason is now void

Recall and determinism are COUPLED through the same ambiguity: v2's greedy
resolution of ambiguous pairings buys the −2,103 and leaves 6 lines of self-hop;
v4's strict mutual-unique-best makes the pairings clean but collapses recall to
−45 and makes self-hop WORSE (14 ln). The self-hop violators are chronically
draw-unstable bindings (`p2cValue`, loop-local echoes) whose restored names
become the next hop's prior.

The decision was framed as "6-line regression vs references holding strict-0".
**exp047 measured self-hop cold: the pre-exp047 CONTROL violates it by 78 lines**
(`experiments/047-vendor-residual/RESULTS.md`). Strict-0 was cache luck. The
comparison was never 6-vs-0.

**Caveat, and it cuts both ways: every 8b number above is cache-pinned too.** The
−2,103 and the 6-line self-hop must both be re-measured cold before either is
banked. Cold could make the recovery smaller, larger, or the determinism cost
irrelevant.

## Tasks, in order

### Task 1 — rebase and re-establish the pass (no measurement yet)

The branch is **8 commits ahead, 59 behind main**, and conflicts in ONE file:
`src/rename/plugin.ts` (where the pass is wired). Everything else applies clean.

`main` has moved under it: exp041–043 (content anchor, anchor-preempt,
corroborated-content), exp044–045 (naming/reorder floors), exp046 (vendor scored,
`factoryVar` dropped, body inheritance), exp047 (manifest prior-order,
`hashOrdinal`). None of those touch `family-permute*`, but `plugin.ts` and the
rename pipeline have changed around it.

The branch also carries a stale `run.sh` change (`CFG="${EVAL_PAIRS:-…}"`) —
**main already has EVAL_PAIRS**, implemented differently. Drop the branch's
version, keep main's.

Gate for this task: `npm run check` green, and the pass OFF
(`HUMANIFY_NO_FAMILY_PERMUTE=1`) produces output **byte-identical to main**.
Prove that before measuring anything — if OFF is not byte-identical, the A/B
measures the rebase, not the pass.

### Task 2 — the cold A/B, four pairs

This is the whole experiment. Same-session A/B, pass ON vs pass OFF, both legs
COLD.

`experiments/037-noise-source-decomposition/ab-pair.sh` on the branch does the
single-pair version, but it is stale in three ways and must be rewritten or
replaced: it hardcodes `REPO=/Users/andrewgross/Development/humanify-lever1v2`,
it uses `/tmp/eval-work` (the real workdir is **`/work`**), and it passes a
`CACHE` (forbidden for a verdict — see below).

Prefer the standard harness with the kill switch as the control leg, which is
exactly how exp047 ran its cold control:

```bash
export PATH="$HOME/.bun/bin:$PATH"          # bun is NOT on PATH; without it
                                            # run.sh SILENTLY skips the boot gate
# control leg (pass OFF)
env HUMANIFY_NO_FAMILY_PERMUTE=1 REBASE_PRIOR=1 \
  experiments/034-eval-harness/run.sh exp048-cold-control /work
# candidate leg (pass ON)
env REBASE_PRIOR=1 \
  experiments/034-eval-harness/run.sh exp048-cold /work

npx tsx experiments/034-eval-harness/leaderboard.ts exp048-cold-control exp048-cold
experiments/041-content-anchor/gate-verdict.sh exp048-cold-control exp048-cold
```

Each leg is 8 pipeline runs (4 rebase + 4 hop) and took ~50 min in exp047.

### Task 3 — if determinism is still the blocker, the placement-provenance gate

Only if Task 2 shows the recall/determinism coupling survives cold.

Using output FILE PLACEMENT as the pairing signal is the obvious next
disambiguator and it has failed three times — read `036/README.md` before
proposing it. But the failures share a mechanism that a gate can avoid:

- Output placement is inherited from the prior split ledger, and a large minority
  of assignments come from **NAME VOTES** — for those, where a statement lands is
  downstream of what it is called, so using placement to decide names is
  circular. Measured on the exp047 cold candidate:

  | hop     |       inherited | via hashes | via name votes | via ordinals | locality |
  | ------- | --------------: | ---------: | -------------: | -----------: | -------: |
  | 85→86   | 19,239 / 19,966 |     10,954 |          7,586 |          472 |      727 |
  | 118→119 | 22,140 / 23,442 |     12,945 |          8,573 |          582 |    1,302 |
  | 197→198 | 30,644 / 31,839 |     18,412 |         11,323 |          781 |    1,195 |
  | 215→216 | 34,828 / 35,903 |     21,223 |         12,553 |          907 |    1,075 |

- The **HASH tier is name-independent** and is the majority (~55-59%), and
  `src/split/placement-trail.ts` already records which tier placed each statement
  (`--diagnostics`). Re-derive these counts for your own run rather than reusing
  the table — they shift with the pass and with LLM draws.

So the non-circular variant: use placement as a signal ONLY for members whose
placement was decided name-independently, and abstain otherwise. **Expected value
is tens of lines, not thousands** — masked usage context is a strictly richer
signal than "same file" — so treat it as a tiebreaker, not a lever.

## Gate — non-negotiable, every hop on its own

`experiments/041-content-anchor/gate-verdict.sh exp048-cold-control exp048-cold`

1. **`noiseLn` DOWN on every hop.** That is the target metric.
2. **`novel` and `realLn` UNMOVED.** A noise win that moves these has deleted
   real change, which is the one failure this pass could plausibly cause — it
   rewrites names in shipped output.
3. **Zero pure-rename violations.** The pass rewrites rendered code; this is the
   correctness backstop.
4. **Boot gate green ×4.**
5. **Self-hop: judge against the COLD CONTROL's number, not against 0.** Report
   both legs. A candidate at or below the control passes; above it needs a
   reading of which bindings moved and why.
6. **118→119 is the canary** — the calm hop, least to win, most to lose.

### `vendorLn` / `vendorReal` on this experiment

The pass touches `src/` naming only, so vendor should not move. Note from exp047
that **`vendorReal` is NOT draw-stable cold** (±200 between runs of identical
code, because it counts humanify's own vendor-filename rotation as dependency
change). Do not treat a ±200 wobble there as a finding.

## THE CACHE RULE — read this before running anything

**Do NOT run a gate through the LLM cache.** `run.sh` now defaults to **no
cache**; `EVAL_LLM_CACHE=<dir>` opts back in and prints
`LLM CACHE: ON … NOT valid for a gate run`.

exp047's first gate was accidentally cache-pinned: all 24,079 entries pre-dated
the run and **not one new entry was written, so not one prompt reached the
model** across eight pipeline runs. Every reported "LLM call" was a ~22ms disk
read. The gate passed and its `src/` numbers meant nothing — two legs replaying
identical answers must agree.

That matters more here than anywhere: **this experiment's entire subject is
naming stability under LLM draws.** A cached run cannot measure it even in
principle.

- Cache is fine for **iteration** — building the pass, unit tests, probing a
  deterministic surface.
- Any number entering RESULTS.md, a leaderboard row, or a ship/no-ship decision
  must be COLD.
- **A cold candidate needs a COLD CONTROL.** Comparing cold-vs-cached measures
  the cache.

How to verify a run really was cold, all three:

```bash
# 1. no new cache entries
find /work/llm-cache -type f -newermt "<launch time>" | wc -l      # expect 0
# 2. the model actually served requests
curl -s http://host.docker.internal:8000/metrics \
  | awk '/^vllm:request_success_total/ {s+=$2} END {print s+0}'    # must climb
# 3. avg response time is inference, not disk
grep -oE "LLM: +[0-9,]+ calls, avg [0-9]+ms" <results>/*.log | tail -1
#    ~20ms = disk cache. Hundreds of ms = live.
```

Beware one metric name: the coverage summary's **`Cached:` column is NOT the LLM
disk cache** — `src/rename/coverage.ts:28` defines it as "functions with renames
restored from **prior version** matching". It reads ~95% in cold runs too. Do not
use it to judge cache state.

## Operating notes

- **Workdir is `/work`**, not `/tmp/eval-work`.
- **bun lives at `~/.bun/bin` and is NOT on PATH.** Without it `run.sh` silently
  prints "BOOT GATE SKIPPED" and you lose criterion 4.
- **Never edit `src/` while a pipeline is in flight** — and never edit a running
  bash script, since bash reads it incrementally. Check with
  `ps -eo pid,cmd | grep -v grep`; `pgrep -f` matches its own command line.
- **Do not poll a long run with blocking `sleep`.** Use a backgrounded
  `until ! pgrep -f 'run\.sh <label>'; do sleep 20; done` that exits on the
  condition.
- Push needs `export SSH_AUTH_SOCK=$HOME/.ssh/agent.sock`.
- Pre-commit biome is stricter than `npm run check` on cognitive complexity —
  run `npx biome check <file>` before committing.
- Eval result dirs: `.log`/`.stdout` are gitignored, `.json`/`.html` are
  committed. Stage explicitly; never `git add -A` here.

## Reference points

| what                                     | where                                                             |
| ---------------------------------------- | ----------------------------------------------------------------- |
| the pass, and every prior attempt        | `experiments/036-interchangeable-assignment/README.md` §8         |
| self-hop is cache luck; cold methodology | `experiments/047-vendor-residual/RESULTS.md`                      |
| naming slice sizing, conservation table  | `experiments/044-naming-correspondence/RESULTS-correspondence.md` |
| what "irreducible" was based on          | 036 §"Constraints already measured — do NOT rebuild these"        |
| ten measurement rules                    | `docs/measurement-pitfalls.md`                                    |

## Current noise standing (cold, exp047 state — what 048 is trying to move)

| surface               | lines, 4 hops | of which noise |
| --------------------- | ------------: | -------------: |
| `src/`                |       154,983 |     **13,306** |
| `vendor/`             |         4,862 |          1,744 |
| `index.js` (unscored) |         2,060 |   unclassified |

`src/` noise is naming **7,450** / reorder 5,706 / alias 150, and 85→86 alone
carries 7,710 of the 13,306. **This experiment targets the naming slice**, which
is the largest single reducible block identified anywhere in the pipeline.
