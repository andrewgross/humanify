# 055 — the residual, recounted after 054, and where the noise actually is

> **STATUS (2026-08-11): Task 0 EXECUTED — see `RESULTS.md`.** Name-only mass
> inside REAL is **3,448 lines** across the four hops (two-run stable, Δ54),
> which EXCEEDS the 2,000-line decision threshold: hidden name churn is a
> live target and the classified-noise buckets are not the map. One claim of
> this brief did not survive: the "at least 4,576" prediction double-counted —
> exp054 had already shipped and removed those lines from the measured trees.
> The number is also a LOWER bound: the instrument cannot see name churn
> inside one-sided add/remove mass (56–86% of REAL), which is unsized.

> **This is a BRIEF — a hypothesis, including its cautions.** Whoever finishes it
> stamps a STATUS block naming which of its claims did not survive.
>
> **Read `054/RESULTS.md` first.** This brief exists because 054 invalidated the
> table every lever since 051 was chosen from.

## The finding that reframes the arc

exp054 removed **5,026 git lines** across the four gate hops. Measured on the
same run, ON leg against OFF leg, the classified naming bucket moved by **450**.

| bucket         |     85→86 | 118→119 | 197→198 | 215→216 |        Δ |
| -------------- | --------: | ------: | ------: | ------: | -------: |
| LOCAL-DRIFT    | 2196→2020 | 130→110 | 458→264 | 198→138 | **−450** |
| TARGET-CHANGED | 3144→3144 |   36→36 | 116→116 |   40→40 |        0 |
| PRIVATE `#x`   |   316→316 |   84→84 |   38→38 |   30→30 |        0 |
| ALIAS-ONLY     |     24→24 | 566→566 |   26→26 |     8→8 |        0 |
| MOVED-DECL     |       8→8 |     0→0 |   48→48 | 244→244 |        0 |

**So 4,576 of the 5,026 lines — 91% — were never in a noise bucket at all.**
They sat in statements `diff-composition` charges to REAL CHANGE, because a
statement is classified as _naming_ only when its `statementHash` matches a prior
statement's. A name-only line inside a statement that was genuinely edited is
charged to real change and is invisible to every noise KPI the arc has used.

The standing residual table is therefore not a map of what is left. It is a map
of what the classifier can see.

## Current totals, post-054, same instrument

|                                                            |  4 hops |
| ---------------------------------------------------------- | ------: |
| tree churn (`diff -r`, the reviewer-facing number)         | 143,987 |
| — classified REAL                                          | 136,287 |
| — classified noise (naming 7,268 + alias 274 + reorder 56) |   7,598 |

exp054 took 4,576 lines out of the "REAL" column without touching one line of
real change — `realLn` and `novel` were exactly 0 on every hop, twice, under two
different placements. That is the existence proof.

## Task 0 — how much of "REAL" is not real? (no pipeline run)

The one measurement that decides what the arc does next. For every statement
`diff-composition` charges to real change, walk its line diff (as
`051/line-ledger.ts` does for naming) and attribute each charged line:

- **name-only** — the two lines differ solely in identifier tokens;
- **genuinely edited** — anything else differs.

State in one sentence what the predicate tests before believing it (rule 3), and
report the name-only mass per hop, 85→86 separately from the calm three.

**Prediction, recorded before the measurement:** it is at least 4,576, because
exp054 already removed that much from this column. The open question is whether
the remainder is thousands more or a long tail.

**Decision rule, fixed now:** if the name-only mass in the REAL column exceeds
~2,000 git lines across four hops, the next experiment targets it and the
classified-noise buckets are no longer the map. Under that, fall back to the
ranked list below.

## Task 1 — read twenty of them

Rule 1 has refuted a hypothesis in every experiment of this arc, including three
of my own inside 054 alone (shorthand expansion, duplicate substitution
positions, and an ALIAS-ONLY "regression" that was an artifact of comparing two
different cold runs). A line that differs only in identifiers can still be real
change — a call rerouted to a different helper reads exactly like a rename.

## If task 0 comes in under the threshold — the ranked fallback

| #   | lever                       |           size | note                                                    |
| --- | --------------------------- | -------------: | ------------------------------------------------------- |
| 056 | multi-hop walk              |              — | the END GOAL; every number so far is single-hop         |
| 057 | export-key stability        | 46% of lineage | see below                                               |
| 058 | `index.js` require ordering |          2,046 | 65% upstream / 35% ours; order is SEMANTIC, do not sort |
| 059 | private fields `#x`         |            468 | unclaimed; no pass reaches them                         |

**056 is arguably first regardless.** The goal is a walk, and every measurement
in 034–055 is one hop. Compounding is unmeasured: run 216→217→218→219 and see
whether churn stays flat. It is also the only way to observe the multi-hop
feedback that every gate in this arc explicitly could not.

**057** closes the half of the lineage that does not carry. A split file exports
through `defineProperty(module.exports, "oldName", …)`, a string key the tree's
rename cannot reach, so carrying a top-level rename moves the key next hop and
churns every consumer (054 measured 238 of 238 drifted lines, zero exceptions).
Closing it means renaming the key AND every importer's `alias.oldName` in the
same hop — a cross-file change with its own churn, which must be ceiling'd first.

## What this experiment must NOT do

1. **Do not compare against `/work/exp050-cold` numbers.** Different cold run,
   different draws. It produced a phantom +254 ALIAS-ONLY "regression" here
   before the same-run control killed it. Compare ON to OFF within one run.
2. **Do not treat `noiseLn` as evidence of no effect.** It scores the BUNDLE and
   was structurally pinned at 0 for the whole 054 arc.
3. **Do not re-derive exp052's re-roll rate from pinned legs.** It needs two COLD
   legs (rule 10); replayed legs agree by construction.
