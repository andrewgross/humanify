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

## Task 0 — locate the deciding tier (no pipeline run)

For a sample of the 941 identifier pairs (85→86), find each binding's
rename provenance in the diagnostics trail (`--diagnostics` placement
trail + strategy attempt trail are 100%-coverage as of the provenance
fix): which tier decided the fresh name — close-match re-rename, LLM
cold, snap, floor? The lever targets whichever tier owns the mass; a
lever aimed at the wrong tier repeats exp045's scope mistake.

## Success criterion (fixed now)

The three-repeat bands say any `novel`/`realLn` movement is real: the
lever must hold BOTH exactly. Victory = paired name-only mass (85→86)
drops materially below 1,622 with the anchored cross-file mass not
rising, measured by the 055 instruments on cold trees, twice.
