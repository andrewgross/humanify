# 057 — Alias stability: the largest targetable driver of name churn

> ## STATUS: CLOSED at task 0/1 — NO CODE SHIPPED. Read [`RESULTS.md`](RESULTS.md), not this brief.
>
> This brief's headline number is wrong. Require-alias churn is **1,924
> git-capped lines over four hops, 26% of the ~7,360 claimed below** — the
> NS-MEMBER predicate it rests on tests whether an identifier is followed by a
> dot, which is a position, not an alias. Half the class is unrelated lines
> paired by matching masked shape; a quarter is plain local variables.
>
> Both sub-causes came in at or under the decision rule fixed in this document
> (WIDENING 894 gate / 680 walk; MOVED-DECL 1,030 / 1,276), and both are
> **exactly 0 on every calm hop of the clean walk.** RESULTS.md lists the six
> claims below that did not survive, and re-puts the WIDENING product question
> with the real number.

> **This is a BRIEF — a hypothesis, including its cautions.** Whoever finishes it
> stamps a STATUS block here naming which of its claims did not survive.
>
> **This experiment may correctly produce NO CODE.** 051, 052 and 053 each did,
> and each was right. What is different here is the size of the number, and the
> fact that 055 showed the previous measurement of this exact class was ~3x too
> small.
>
> **Do not write a line of pipeline code until Task 0 produces a number.**

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md).
**Read first, in order:** [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md)
(eleven rules), [`055/README.md`](../055-residual-recount/README.md) and the
tables below, [`054/RESULTS.md`](../054-post-split-reconcile/RESULTS.md) for how
the split emits aliases and exports, [`docs/matching-cascades.md`](../../docs/matching-cascades.md)
section D for `buildNsVars`.

---

## Where the noise is, measured on the post-054 gate trees

Four hops, ground truth = what `diff -r` prints. **145,727 lines total.** Of
those, **19,916 differ only in identifier tokens** — the two lines are the same
line with different names.

| driver                        |     4 hops | share of name-only | note                                                 |
| ----------------------------- | ---------: | -----------------: | ---------------------------------------------------- |
| WORD-SWAP (different word)    |     ~8,700 |               ~44% | the LLM floor, 939 distinct on one hop — NOT a lever |
| **NS-MEMBER (require alias)** | **~7,360** |           **~37%** | **this experiment**                                  |
| STEM (shares a word)          |     ~3,570 |               ~18% |                                                      |
| DECORATION / COUNTER / SUFFIX |       ~260 |                ~1% |                                                      |

The NS-MEMBER class is the only one with concentration. On 215→216 a **single**
substitution is 494 occurrences, ~988 git lines, 23% of that hop's entire
name-only churn.

## The two sub-causes, and why the old numbers were too small

### 1. MOVED-DECL — a group of declarations changes split file

```
- fastModeElement = childWithIcon.chalkInstanceVal.jsxs(...)
+ fastModeElement = sessionPicker.ReactRenderer2.jsxs(...)

taskSerializer -> errorMessagesAuthManager      494 occurrences on 215->216
```

Module-level declarations that lived in `floor/cli-interaction/task-serializer.js`
in 2.1.215 live in `storage/error-messages/auth-manager.js` in 2.1.216. Every
consumer imports them by namespace, so every consumer's alias changed at every
usage site.

**exp051 measured this class at 300 lines over four hops** and closed it as too
small. That measurement counted only lines inside hash-matched statements. 055
shows ~988 lines on ONE hop, because most usage sites sit in statements whose
hash flipped and are charged to real change where no noise KPI can see them.

### 2. ALIAS-WIDENING — one shadowing local widens an alias tree-wide

```
statusIndicator -> statusIndicatorsStatusIndicator    152 occurrences
commandEntry    -> interfaceCommandEntry               96
kairosCron      -> logTaskEventKairosCron              68
```

`nsNameIsFree` asks whether the candidate alias is shadowed in ANY importing
file, so one importer gaining a top-level binding named `kairosCron` widens the
alias in all ~20 importers — at every usage site, not just the `require` header.

**exp051 measured this at ≤256 git lines** and declined it, partly to preserve
the deliberate "one alias per module tree-wide" readability property. The
declining was reasonable on a 256-line number. The number was the `require`
header lines plus what the naming bucket could see; the usage sites were not
counted.

**Neither prior figure was wrong about what it measured. Both were the wrong
scope — rule 8, again.**

## Task 0 — the ceiling, per sub-cause. No pipeline run.

Everything needed is on disk: `/work/exp054d-on-2.1.*` against
`/work/exp050-cold/2.1.*-rebased`. Reuse
[`055/name-drivers.ts`](../055-residual-recount/name-drivers.ts), which already
isolates and classifies the NS-MEMBER population.

Split the ~7,360 NS-MEMBER lines into:

1. **MOVED-DECL** — the alias on both sides resolves to a `require` of a
   DIFFERENT path, and the member read off it is exported by both. State the
   predicate in one sentence and check that sentence is your claim (rule 3);
   051's first attempt at this used a predicate that did not check whether the
   member survived and had to be retracted.
2. **WIDENING** — same require path on both sides, alias spelling differs.
3. **Everything else** — report it, do not fold it in.

Then **git-cap each**: the real `diff -u` over the files touching those aliases
prints an upper bound on lines that changed BECAUSE of them. 051 measured its
ledger over-charging this population by 29%; expect the same direction.

Report per hop, **85→86 separately from the calm three, always.**

**Decision rule, fixed before the measurement:** a sub-cause under **~1,000
git-capped lines across four hops** is not worth a pipeline change — that is
under the ±2,800 per-hop draw band and every lever below it since 051 has been
correctly declined. Over it, proceed to Task 1.

## Task 1 — read twenty, per sub-cause

Rule 1 has refuted a hypothesis in every experiment of this arc. In 054 alone
three of mine died on inspection (shorthand expansion, duplicate substitution
positions, an ALIAS-ONLY "regression" that was two different cold runs); in 055
two predicates had to be rewritten before the numbers reconciled.

For each pair ask: is this the same module under a different alias, or a
DIFFERENT module that happens to export the same member name? 054 found five
cross-module misfires of exactly that shape — `cwdManager → bunDetection`, where
`configFilePath` had moved from `bun-detection.js` to `cwd-manager.js`. A rename
and a relocation are indistinguishable from the usage site alone.

## Task 2 — build, only if a sub-cause survives

State the ceiling in git lines, capped per file, **before writing code.** Behind
a kill switch, TDD red-first.

### For MOVED-DECL, the fix is probably NOT in aliasing

The alias followed the declaration correctly. The churn exists because the
declaration moved files at all. That points at `stable-split.ts`'s placement
cascade, not `buildNsVars` — which means it is a placement-stability change and
should be measured with `relocatedStatements`, NOT the name-keyed `reloc` column
(rule 7: `sameNameMovedFile` ROSE on both experiments that fixed relocation).

Ask first: **why did those declarations move?** `--diagnostics` records which
tier placed each statement (`placement-trail.ts`). If they moved because a tier
changed its mind between releases, that tier is the target. Read the trail
before designing anything.

### For WIDENING, the trade is explicit and belongs to the user

Letting an unshadowed importer keep the short alias refuses nothing to anyone
and the shadowing local keeps its name — but it gives up "one alias per module
tree-wide", so a reader sees `kairosCron.x()` in nineteen files and
`logTaskEventKairosCron.x()` in the twentieth. 051 judged that trade not worth
≤256 lines. **Re-put the question with the real number.** It is a product
decision, not a measurement one — ASK.

### Two hazards this class specifically has

- **Rule 5, paid once already here.** exp044's alias reservation measured its own
  damage correctly, destabilised zero bindings exactly as scoped — and cost
  **+3,742 git lines** because module-binding names feed the split's name votes
  AND the emission-order alignment. Any change inside alias or name allocation
  has a blast radius larger than the population it touches. Measure the cascade.
- **Rule 6.** Blocking a binding from taking an alias moves the collision rather
  than removing it: exp044 saved `kairosCron`, still lost `statusIcon`, and
  CREATED two new displacements, leaving the count unchanged at six across four
  hops. Show why yours is not whack-a-mole before building it.

## Measurement rules this experiment must follow

These are not general advice; each one cost something in 054 or 055.

1. **Ground truth is `diff -r`, not the decomposition.** `composeDiff`'s total is
   off by **−24.0%** on 85→86 and **+17.2%** on 197→198 (055). A single hop read
   −0.4% and was briefly reported as evidence it could be trusted. It cannot.
2. **`noiseLn` scores the BUNDLE.** It is structurally blind to a change that
   only edits split files and read 0 delta on every hop of the 054 arc. Never
   conclude "no effect" from it.
3. **`layout.noise` understates a tree-level change up to 30x.** exp054 removed
   5,026 lines while it moved 448.
4. **Never compare across two cold runs.** Doing so manufactured a phantom +254
   ALIAS-ONLY regression in 055; the same-run control showed 566 → 566.
5. **Per-file attribution is unreliable** even when totals are fine: p90 relative
   error 28.6%, up to 66.7% on 85→86. Quote aggregates, never one file.

## Gate — only if something ships

`experiments/041-content-anchor/gate-verdict.sh`, every hop judged on its own,
plus the 054 additions:

1. **Direct `diff -r` churn down on every hop.** Binding criterion.
2. `novel` / `realLn` unmoved — necessary, NOT sufficient.
3. **Boot gate green, BOTH halves** — `--version` AND a live
   `-p "say exactly: boot-ok"`. `--version` alone proves the module graph loads,
   not that the runtime works. `bun` is at `~/.bun/bin` and is NOT on PATH;
   without it the check is skipped SILENTLY.
4. Self-hop judged **draw-pinned**, never cold (`049/pin-selfhop.sh`).
5. The per-hop draw band is **±2,800 git lines**. If the ceiling is under it, a
   cold A/B cannot resolve the change and will still print a confident sign —
   use `054/pinned-ab.sh`, prove the pinning (both legs write ~0 cache entries),
   and state what the pinned run cannot see.
6. **Run `056/walk.sh` as the final check.** Alias churn is a multi-hop
   phenomenon and a single-hop gate cannot show whether a fix holds or decays.
   The clean 4-hop walk is ~3h unattended and its baseline is recorded in
   `056/RESULTS.md` (−48% vs the old pipeline; calm hops 1,391 and 1,567).

## Instruments to reuse rather than rebuild

| file                  | what it gives you                                                |
| --------------------- | ---------------------------------------------------------------- |
| `055/full-ledger.ts`  | per-hop files/lines/statements/noise, reconciled to ground truth |
| `055/name-drivers.ts` | every name-only substitution, classified, with examples          |
| `054/pinned-ab.sh`    | draw-pinned A/B, four pairs, boot gate, cache-write diagnostic   |
| `054/pin-selfhop.sh`  | self-hop comparing the TREE, not just the bundle                 |
| `056/walk.sh`         | cold build + N-hop walk, per-hop churn and boot                  |
| `placement-trail.ts`  | which tier placed each statement (`--diagnostics`)               |

## State of the tree

`main` at `375fb91`. exp054 (post-split reconcile + bundle carry) is merged and
gated: −5,026 git lines over four hops, `noiseLn` −1,445, boot 8/8, self-hop
unchanged. The clean 4-hop walk is −48% against the old pipeline with all five
builds passing both boot checks.

**Known gap 054 left, adjacent to this work:** 46% of post-split renames cannot
be carried into the bundle, because a split file exports through
`defineProperty(module.exports, "oldName", …)` whose key is a string the tree's
rename cannot reach. Carrying a top-level rename moves that key next hop and
churns every consumer — measured 238 of 238 drifted lines, zero exceptions. If
057 ends up touching export keys, that is the same mechanism from the other side
and the two should be designed together.
