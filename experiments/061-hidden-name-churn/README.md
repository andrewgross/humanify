# 061 — name churn the noise buckets cannot see

> **This is a BRIEF — a hypothesis, including its cautions.** Whoever finishes
> it stamps a STATUS block naming which of its claims did not survive.
>
> Read `055-residual-recount/RESULTS.md` first: this experiment exists because
> its Task 0/0b measurements (2026-08-11) found ≈6,138 lines of defensible
> name churn INSIDE the REAL column — nearly the size of the entire
> classified-noise table (7,598) — and the decision rule fixed in the 055
> brief says that mass is now the target.

## What is known before any lever is designed

All numbers from `noise-band-r1`/`r2` trees (commit `76c012b`, cold,
two-run stable). Three distinct populations:

1. **Paired name-only lines: 3,448** (`real-ledger.ts`). Lines inside
   hash-flipped statements that differ ONLY in identifier tokens.
   Mechanism split (85→86, the dominant hop, measured 2026-08-11):
   - **91% whole-word re-rolls** (741 pairs: `initStubModule →
stubModuleFactory`, `documentTitle → agentName`)
   - 6% stem-contained tweaks (`errorCaught → error`)
   - 3% mint-ordinal drift (`initializeApp152 → initializeApp10`)
   - **Spread across ~941 distinct identifier pairs, avg 1.3 uses each;
     the top ten cover <10%.** There is no hot-binding shortlist — the
     lever must be structural.
2. **Cross-file masked twins: 4,830** (`one-sided-ledger.ts`), of which
   **2,690 string-anchored** (same require path, drifted local alias —
   near-certain identity; relocation+rename compounded) and 2,140 bare
   (upper bound only).
3. Unsized remainder: one-sided statements that changed shape AND names
   (no predicate exists), and string-keyed names (export-key drift).

## The hypothesis

The whole-word re-roll mass is the close-match/LLM tier re-deciding names
for bindings whose ROLE did not change, inside statements whose hash
flipped because the surrounding code genuinely changed. exp052 measured
the re-roll rate directly: two cold legs disagree on 33.4% of
LLM-decided bindings by a different word. The carry machinery
(diff-reconcile, snap-to-prior, bundle carry) reconciles same-hash
statements; a hash-flipped statement's bindings lose that protection even
when the binding itself is unchanged.

**Lever direction:** carry or reconcile the prior name for a binding
whose identity is corroborated (same declaration shape, same callees,
same role) even when its enclosing statement's hash flipped. Where the
carry machinery already owns this question, widen the owner — do not add
a second path (responsibility.md).

## Cautions recorded before measuring

- exp058(B) REFUTED widening the binding-cascade carry — the widened
  carry was the identity function. Re-read `project_exp058_binding_placement`
  before assuming a widening lands anything.
- exp044 alias reservation cost +3,742 lines through second-order
  effects with an honest per-scope ceiling (rule 5/6). Any carry lever
  must be sized with a mechanism-derived ceiling BEFORE the run and must
  LOG every carry it applies (rule 11 — an empty trail proves innocence).
- The eval's noiseLn CANNOT see this mass (that is the point). The
  instrument for the lever is `real-ledger.ts`/`one-sided-ledger.ts`
  re-run on the lever's own trees, plus the ordinary hold columns
  (novel/realLn band 0 — three-repeat fact as of 83545cb).
- A name-only line can still be real change (a call rerouted to a
  same-shaped helper). The 91% figure is a predicate result, not a
  hand-verified rate; sample twenty before shipping any claim.

## Task 0 — locate the deciding tier — EXECUTED 2026-08-11

`tier-provenance.ts` joins the churned fresh-side identifiers (631
unique, 1,262 occurrences on 85→86) against the run's strategy trails by
settled name. Result, weighted by occurrences:

| tier            | share | reading                                                                                            |
| --------------- | ----: | -------------------------------------------------------------------------------------------------- |
| **llm**         | 42.0% | the target: cold re-rolls on re-asked bindings                                                     |
| exact-match     | 13.9% | mostly join ambiguity (exact-match APPLIES the prior name; a same-named different binding matched) |
| binding-cascade |  7.2% | carry produced a different word — worth a look                                                     |
| module-vote     |  3.1% |                                                                                                    |
| close-match     |  2.5% | smaller than hypothesized                                                                          |
| (others)        |   <4% |                                                                                                    |
| unmatched       | 27.3% | name-level join misses (decoration between trail and emission, member exprs)                       |

Two hypothesis revisions: the close-match tier is NOT the owner (2.5%,
not the hypothesized driver) — the mass is bindings that reached the
**LLM tier itself**, i.e. the cascade abstained entirely and the binding
was re-asked cold. The lever is therefore about WHY corroborable
bindings fall through to the LLM (or about prior-biasing the LLM ask),
not about the close tier's re-rename policy. Second: the join is
name-level and 27% unmatched; before designing, re-join at loc level
(trail loc → pre-split loc mapping) if the lever needs per-binding
precision.

## Success criterion (fixed now)

The three-repeat bands say any `novel`/`realLn` movement is real: the
lever must hold BOTH exactly. Victory = paired name-only mass (85→86)
drops materially below 1,622 with the anchored cross-file mass not
rising, measured by the 055 instruments on cold trees, twice.
