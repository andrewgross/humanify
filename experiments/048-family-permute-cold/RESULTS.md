# 048 — RESULTS (in progress)

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Totals first, every
hop judged on its own, every number below marked **[cold]** or **[cache-pinned]**.

---

## The brief's headline number was already superseded — by a file on its own branch

**This is measurement-pitfalls rule 9, and it landed before a single measurement
was taken.** The 048 brief opens with the family-permute pass as
**"−2,103 noiseLn (−35%) on one hop, shelved for a 6-line self-hop regression"**,
sourced from `036/README.md` §8b's version table. That table's rows are v1–v4.

**The code on `exp036-8b-diff-objective` is none of them.** The branch tip
(`f0c4e30`) is a fifth version — _context-strict_ — committed together with the
handoff doc `experiments/037-noise-source-decomposition/README.md`, which states
its own measurement of the code it ships with:

| source                           | version               | 215→216 noiseLn |            self-hop |
| -------------------------------- | --------------------- | --------------: | ------------------: |
| 036 §8b table (the brief's row)  | v2 greedy             |      **−2,103** |                6 ln |
| 036 §8b table                    | v4 mutual-unique      |             −45 |               14 ln |
| **037 handoff = the branch tip** | **v5 context-strict** |        **−239** | **0** (its own run) |

037 also records 85→86 as **pass-inactive, all KPIs +0**, and its self-hop as
**44 lines on BOTH legs** — pass-independent draw flake — which contradicts the
"references hold strict-0" framing the shelving decision rested on, in the same
document that shipped the code.

So the honest pre-registration for this experiment is **−239 on one hop, ~0 on
the calm hops**, not −2,103. Everything in both tables is cache-pinned and is
re-measured cold below.

### Why that recalibration decides the measurement design

exp047 measured the cold `src/` draw band directly: control-vs-candidate `src/`
moved **−344 / −110 / +354 / −250** per hop for a change proven `src/`-neutral
offline, and its RESULTS state plainly that "a single cold pair cannot resolve a
sub-400-line `src/` effect."

**A −239-line effect is inside that band.** A four-pair cold A/B therefore cannot,
on its own, attribute a `noiseLn` move to this pass — in either direction. The
gate still runs (it is the ship criterion, and it is the only thing that can catch
damage), but it is paired with an attribution instrument that does not depend on
draw luck: **the pass records every rename it applies**, so each hop's KPI delta
can be read against whether the pass fired there at all. A hop where the pass
applied zero moves and the KPI still moved is measuring the model, not the code.

---

## Task 1 — rebase (DONE)

Branch `exp048-family-permute` = the 8 commits of `exp036-8b-diff-objective`
rebased onto `main` (4d92295), plus one rebase-artifact commit.

- **One conflict**, as the brief predicted: `src/rename/plugin.ts`, and it was a
  neighbouring-import collision only (`PriorCarry` vs `runFamilyPermute`), not a
  logic conflict. The wiring hunk applied clean.
- **`run.sh` reverted to main's.** Both versions define `EVAL_PAIRS` and they
  mean different things — main's is a pair FILTER, the branch's was a config
  PATH. Main's wins.
- **`ab-pair.sh` deleted.** It hardcoded a `humanify-lever1v2` checkout,
  `/tmp/eval-work`, and an `--llm-cache` dir: an A/B driver structurally incapable
  of producing a gate-valid number (rule 10). `048/cold-ab.sh` replaces it.
- `npm run check` green: typecheck, prettier + biome, **1,638 unit tests / 0
  fail**, 33 fingerprint tests.

### Byte-identity of the DISABLED pass — PASSED

Two 85→86 runs against the same archive prior, `main` (4d92295) then the rebased
branch with `HUMANIFY_NO_FAMILY_PERMUTE=1`, **both through the LLM cache on
purpose**: the question is whether two code paths issue the same instructions,
which is only visible with the model's own variance pinned. Rule 10 permits the
cache for probing a deterministic surface; no number here is a KPI.

| artifact                                                      | result                        |
| ------------------------------------------------------------- | ----------------------------- |
| `.humanify/humanified.js` (17,780,860 B)                      | **byte-identical**            |
| `.humanify/split-ledger.json`                                 | **byte-identical**            |
| whole emitted tree — `src/`, `vendor/`, `index.js`, `run.cjs` | **`diff -rq` empty**          |
| cache entries written by leg 2                                | **0** — every prompt replayed |

The zero is what makes the comparison mean anything: leg 1 wrote 10 new entries
(main has moved since the cache was filled), leg 2 wrote none, so both legs
answered from the same draws and the only variable left was the code.

Both legs also reproduced the **same** pre-existing `runtime.js` pure-rename
violation, which is main's known draw-dependent flake and not attributable to the
rebase. Note it is present on the control side of everything below.

Post-gate, the pass gained a **move trail** (`FamilyPermuteOutcome.moves`, logged
per shipped rename): reachable only from `finalizeWithFamilyPermute`, i.e. never
on the disabled path, so the identity above still describes the control leg.
`npm run check` green after it — 1,640 unit tests, 33 fingerprint.

---

## Task 2 — the cold A/B

_TBD._

---

## Task 3 — reading the renames

_TBD._
