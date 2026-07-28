# 050 — The aligner's two residuals: the ambiguity gate, and the slots it leaves on the table

> ## STATUS — TASK 1 SHIPPED. Reorder is no longer a noise axis.
>
> **`reorderLn` 2,146 → 54 across four cold hops (−97.5%), down on every hop**,
> with `novel` (4,188) and `realLn` (416,377) exact, `noise` −118, `noiseLn`
> −1,587, `newName` −110, `mints` −8, FEWER pure-rename violations than control
> (1 vs 2), boot green ×4 on both legs, 0 cache entries. Draw-pinned on 215→216
> the effect is reorder **518 → 0** with naming, alias and real change identical
> to the digit. **Combined with 049, reorder has gone 6,148 → 54 (−99.1%).**
>
> **This brief's central proposal was WRONG, and the ceiling measurement caught
> it before anything was built.** It predicted masked usage context would be the
> evidence, reusing exp048's `assignBucket`. Measured: **1,174 of 1,194 lines
> (98.3%) resolve by NAME IDENTITY and ZERO by context.** The gate was simply
> keyed on the wrong thing — statement hashes MASK identifiers, so `getA`/`getB`
> collide while their names are unique on both sides. Task 1 shipped as a keying
> change, not an evidence change.
>
> **The trap it nearly hit:** `recordEmittedLayout` OVERWRITES `emitHashes` with
> what the runnable emit produced. Writing `emitNames` only in `buildLedger` would
> have left the arrays describing different permutations, so the next release
> would key against a mismatched name sequence and **mis-align silently instead of
> falling back**. Both now move in lockstep.
>
> **Self-hop: control 0, candidate 12 — zero move hunks and a BYTE-IDENTICAL
> ledger**, so layout is a perfect fixed point; the 12 lines are `p2cValue →
pbkdf2IterationCount`, the draw-unstable `p2c`/`p2s` family exp036 documented.
>
> **What did NOT pass cleanly**, recorded rather than smoothed over: the 118→119
> canary's git-line noise rose **518 → 716** (naming +176, alias +74) while its
> reorder went 52 → 0 — emission order feeds the split's ALIAS derivation, and
> alias headers understate their true cost ~9× because usage sites bill to
> naming. Its statement metrics moved the other way (churned 438 → 434, `noiseLn`
> −91). `noiseLn` per hop is +75 / −91 / −1,680 / +109 — up on two hops, inside
> the ±2,800 draw band (rule 11), so down in total but not "every hop".
>
> **Task 2 (the 33 unexplained displacements) was NOT built** — 050's keying
> change absorbed most of the axis, and 54 residual lines do not justify it.
> **The alias cascade is the successor question**, seeded by this experiment's own
> canary data.

Jargon: [034 vocabulary](../034-eval-harness/VOCABULARY.md). Conventions:
_Idea → Evidence (table) → Conclusion_; **ceilings measured before builds**;
totals-first; every hop judged **on its own**.

**Read first:** [`docs/measurement-pitfalls.md`](../../docs/measurement-pitfalls.md)
(eleven rules; 049 added evidence to 2, 4 and 11 in one afternoon), then
[049's README](../049-reorder-drivers/README.md) for the decomposition and the
tooling this experiment inherits.

## The one-sentence version

After [049](../049-reorder-drivers/) removed the load-order constraint, **the
remaining reorder churn is almost entirely the emitter's own two refusals** — the
precision gate abstaining on same-hash siblings (1,174 ln) and the scheduler
leaving achievable slots unclaimed (427 ln capped) — and neither is a constraint,
so neither has an excuse.

## Where the axis stands, post-049 [cold, 4 pairs]

| bucket                             | pre-049 |                       post-049 | statements | what it is                                                       |
| ---------------------------------- | ------: | -----------------------------: | ---------: | ---------------------------------------------------------------- |
| **ambiguous hash — gate abstains** |   1,240 |                **1,174 (48%)** |        355 | the aligner refuses to place a statement whose hash has siblings |
| **aligner residual**               |   1,670 | 1,036 charged / **427 capped** |         33 | unambiguous AND unblocked, yet displaced                         |
| barrier itself + barrier between   |   3,180 |                  **252 (10%)** |         15 | genuine load order                                               |
| **TOTAL reorder**                  |   6,090 |                      **2,462** |            |                                                                  |

049 collapsed the load-order share from 3,180 to 252. **The ranking flipped: the
ambiguity gate is now the prize, and the residual that motivated this experiment
is second.** Sizing this from 049's own decomposition would have got the order
wrong — a fresh census is task 0 for a reason.

`capped` = each file's charge capped at what git actually prints for it, because
the raw charge carries known artifacts: `files-api.js` is charged **332** where
git prints **4**, and on 118→119 a single statement is charged **266** where the
cap is **6**. **Quote the capped number.**

## Task 0 — re-census before believing any of the above

Every number in this brief was measured on `exp049-cold` artifacts. Re-run it on
whatever is current, because 049 moved the ranking once already:

```bash
for p in "2.1.85-rebased 2.1.86" "2.1.118-rebased 2.1.119" \
         "2.1.197-rebased 2.1.198" "2.1.215-rebased 2.1.216"; do set -- $p
  npx tsx experiments/049-reorder-drivers/ambiguity-split.ts \
    /work/<label>/$1/src /work/<label>/$2/src
done
```

**The decomposition does not generalise from one pair** — on 215→216 the aligner
residual is 20% of the charge, on 118→119 it is 83%. Census all four, always.

## Task 1 — the ambiguity gate (1,174 ln, 355 statements)

### What the gate is, and why it exists

`alignFileStatements` (`src/split/stable-split.ts`) lets a statement claim its
prior slot only when its statement hash occurs **exactly once on each side**:

> Same-hash siblings (noop stubs, tiny getters that differ only in their names)
> are indistinguishable to the hash, so pairing them is a guess that teleports
> their text and MANUFACTURES churn: measured **+2.3%** on the 118→119 hop.

**That measurement is the constraint on this task.** The gate is not timidity; a
naive version was tried and cost more than it saved. Any proposal must explain
why it is not that, and the +2.3% is the number to beat, not to ignore.

### The idea: the evidence exists, one layer up

This is the same problem [exp048](../048-family-permute-cold/) solved for NAMES.
Same-hash members are indistinguishable _by hash_ — but not by their **masked
usage contexts** (reference lines with each side's own name blanked), which is
precisely the evidence the merged `family-permute` pass uses to decide which
member of a bucket owns which prior name. `src/rename/family-permute.ts`'s
`assignBucket` already computes this correspondence, deterministically, with a
strict-improvement bar that refuses to move a merely-ambiguous member.

If the rename phase can say "this fresh member corresponds to that prior member",
the aligner can place it — and the pass runs BEFORE the split, so the answer is
available.

### Ceiling first, and it must be measured, not argued

Before building anything, measure how many of the 355 statements the
family-permute correspondence actually resolves:

1. Extract, for each ambiguous displaced statement, its bucket and the prior
   member the context evidence would pair it with.
2. Count how many pairings are (a) decided at all, (b) decided with support ≥ 2,
   (c) would place the statement at its prior slot.
3. **The ceiling is (c), in git lines, capped per file at what git prints.**

If the ceiling is under ~300 lines, stop: the +2.3% precedent says a
half-confident pairing costs more than it returns.

### Design constraint carried from 048

The strict bar is the safety property. In `assignBucket`, a member moves only when
its context support **strictly beats** staying put — which is why a merely
ambiguous member is never moved. Whatever feeds the aligner must inherit that
bar, or it becomes the naive pairing the gate already refused.

## Task 2 — the slots the scheduler leaves unclaimed (427 ln capped, 33 statements)

33 statements, tree-wide, that are **unambiguous AND unblocked** and still land
somewhere other than their prior slot. No constraint explains them.

### Two candidate mechanisms, neither yet established

Do NOT build against either until one is confirmed by instrumenting the aligner.

**(a) Greedy topological scheduling cannot defer.**
`orderRespectingLoadOrder` pops, among statements whose predecessors are all
emitted, the one the desired order wants soonest. That is deterministic and never
violates an edge, but greedy is not optimal for _displacement_: a low-rank
statement stuck behind a predecessor is emitted after high-rank statements that
happened to be free, and everything shifts.

**(b) Rank assignment for unmatched statements.**
`orderByHashSequence` gives a statement with no claimable prior rank the key
`prevRank + 0.5` — anchored to its predecessor. In a file where much is new or
edited (`otel-exports.js`: only 93 of 119 statements matched), long runs collapse
to the same key and sort by bundle position, which can carry anchored neighbours
with them.

### How to tell them apart

Instrument `alignFileStatements` to record, per statement: its desired rank, the
rank it got, whether it was gate-eligible, and — when it missed — whether it was
blocked at pop time or simply out-ranked. 049's move-trail did exactly this job
for the family-permute pass and is what made that experiment's KPI deltas
attributable; the same instrument answers this in one run.

`otel-exports.js` on 85→86 is the worked example to start from: 274 charged lines,
9 displaced function declarations — and **function declarations are hoisted, so
the DAG imposes nothing on them at all.**

## The gate — non-negotiable, every hop on its own

```bash
experiments/041-content-anchor/gate-verdict.sh <control> <candidate>
```

1. **`reorderLn` down on every hop.** The target.
2. **`novel` and `realLn` unmoved.** These held exactly through both of today's
   changes; there is no excuse for moving them.
3. **Zero NEW pure-rename violations.** `runtime.js` is a known pre-existing
   draw-dependent flake and appears on control legs too — count the delta.
4. **Boot gate green ×4**, both legs. `bun` is at `~/.bun/bin` and NOT on PATH;
   without it `run.sh` silently prints "BOOT GATE SKIPPED".
5. **Self-hop judged PINNED, not cold.** See below.
6. **118→119 is the canary.**

### On the self-hop, and this is 049's hardest-won lesson

**A cold A/B cannot judge a self-hop number.** 049 read 16 → 326 cold and it was
entirely draw variance: pinned off two byte-identical base bundles it was **6 vs
6, zero move hunks, the same binding in the same place**. Cold legs draw different
names, so their self-hops are not comparable.

Use `experiments/049-reorder-drivers/pin-selfhop.sh`. Its validity check is the
**cache-write count of the second leg**: near-zero means its prompts matched and
the legs are comparable; a large count means the prompts themselves differed,
which is itself the finding. This is a legitimate use of the cache under rule 10 —
the subject is layout determinism, so LLM variance is the confound, not the
measurement.

### Bundle-level columns will move, and it will not mean anything

`noiseLn`, `mints` and `newName` are computed on the bundle, which a split-side
change cannot touch. In 049's cold gate they read +2,504 / +14 / +102 with no
per-hop direction while the change was provably bundle-neutral. **Confirm
neutrality with a draw-pinned run** (`049/measure-registrar.sh` is the template)
and then read the git-line layout columns for the verdict.

## Measurement traps, all paid for already

- **A comparison of INDEXES is not a list of reorderings.** One insertion shifts
  every later index. The KPI charges only statements off the LCS. A first cut of
  `why-moved.ts` compared indexes and made alphabetised
  `Object.defineProperty` accessor blocks — which never reorder — look dominant.
- **Check a statement is not a barrier ITSELF**, not merely that no barrier sits
  between its two positions. Missing this put 15 `defineModuleExports` calls in
  the "unexplained" bucket and inflated it 4×.
- **When a probe imports a production model, check the SCOPE it is fed.**
  exp045 closed this axis on a ceiling from `bundleLoadOrderFacts` fed ONE SPLIT
  FILE, where the lazy-init helper's definition is absent, so detection returned
  null and everything counted as a barrier.
- **The decomposition agrees with git in aggregate by cancellation** — per-file
  absolute error 11.6%, cancelling 19.6×. Re-check with
  `049/per-file-fidelity.ts` before trusting a per-mechanism share.

## Inherited tooling

| script                                        | question                                                  |
| --------------------------------------------- | --------------------------------------------------------- |
| `049/ambiguity-split.ts`                      | the census — charge by cause, plus the git-capped ceiling |
| `049/why-moved.ts`                            | in ONE file, what is charged and why                      |
| `049/reorder-census.ts`                       | charge by kind of displaced statement                     |
| `049/per-file-fidelity.ts`                    | does the decomposition match git per file                 |
| `049/measure-registrar.sh`                    | template for a draw-pinned A/B                            |
| `049/pin-selfhop.sh`                          | the self-hop, judged honestly                             |
| `049/cold-ab.sh`                              | the cold 4-pair gate                                      |
| `src/rename/family-permute.ts` `assignBucket` | the correspondence task 1 wants to reuse                  |

## Expected value, stated plainly so it can be wrong

Task 1's ceiling is **unmeasured** — 1,174 lines is the POPULATION, not the
recoverable share, and the +2.3% precedent says the recoverable share may be
small. Task 2 is **427 capped lines** across four hops, and 33 statements is small
enough to read individually before writing any code.

Neither is large next to 049's −3,686. **If task 1's measured ceiling comes in
low, the honest outcome is to close the reorder axis and say so** — with the
decomposition above as the evidence, which is more than exp045 had when it closed
it the first time.
