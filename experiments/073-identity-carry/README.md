# 073 — carry names for provably-identical modules

> **STATUS (2026-08-15): BUILT, ceiling clears the floor, awaiting one
> scored run — and its ceiling decomposition found a LARGER lever.**
>
> Funnel-simulated ceiling on the exp070-r1 fossil trees (85→86):
> 2,182 modules provably identical (66.7%), of which 88.1% have bodies
> identical modulo names. **Name churn reachable by this carry: 944
> lines** (mechanism-specific share ~334 proportionally, up to 944 if
> churn concentrates in the ambiguous statements — likely, since those
> are exactly the statements no tier could name). Either bound clears
> the 40–60-line decision floor.
>
> **The bigger finding, from the same decomposition — PATH churn
> dominates NAME churn:**
> - 5,728 require lines (26% of all imports) point at a path absent
>   from the prior tree
> - caused by just **179 distinct targets** — ~32 churned lines each
> - **one module** (`access-property.js`, a universally-imported
>   helper) accounts for **3,272 of them**
>
> Path CARRY is already perfect: of 2,182 matched modules, **zero
> moved**. The churn comes from modules that do NOT match — twins mint
> a fresh path every run — amplified by how widely they are imported.
> Making minted paths deterministic for unmatched modules is a
> several-thousand-line lever needing no naming change at all, versus
> this experiment's several hundred. Sequenced next (exp074).


> **This is a BRIEF — a hypothesis, including its cautions.**
>
> Licensed by exp072: the module fingerprint has **zero false
> identicals** against ground truth on two bundlers, synthetic and real
> corpora. Its verdict is about the EMITTED module ("is this the same
> code we shipped last release"), which is the only artifact we see —
> and exactly the right question for carrying names.
>
> Andrew's design rule (2026-08-14): no statistical placement, no
> guessing. This lever needs none: a module either fingerprints
> identical to its prior counterpart or it does not.

## Hypothesis

If a module is provably identical to its prior-release counterpart,
everything about it should carry forward unchanged — identifier names,
file path, import aliases. This is deterministic, per-module, and needs
no classification of app vs library. It happens to cover the population
that causes the most churn (dependencies sit unchanged for a median of
124 releases), without ever labelling anything.

Measured coverage bound (124 releases, 123 hops): **median 80% of
modules provably identical per hop, covering ~78% of statements**;
worst hop 66%. Ambiguous twins (12.5% here, 36% on date-fns) MUST be
skipped — they are indistinguishable by construction.

## Design

1. Match fresh modules to prior modules by fingerprint. Use the
   ORDER-SENSITIVE form: measured free (8 of 441,614 set-matches
   differ in order) and strictly stronger evidence.
2. A match is usable only when the fingerprint is unique on BOTH sides.
   Twins are skipped — never picked arbitrarily.
3. For a usable match, carry the prior module's names verbatim for its
   bindings, its emitted file path, and the import aliases its
   importers use for it. Everything else follows the existing pipeline.
4. LOG every carry (rule 11: an empty trail proves innocence) and count
   skipped twins separately.

## Cautions pinned before measuring

- The carry is only as good as the fingerprint's SCOPE: it says the
  emitted module is unchanged, not that upstream did not rename
  something. A pure upstream rename reads identical and we will keep
  our old name — the same trade every carry tier already makes.
- Do not let the carry override finer-grained tiers that already
  produce a name; it is a floor, not an authority.
- Coverage varies by codebase (12.5% vs 36% twins). Report coverage
  per run rather than assuming a level.
- exp044's +3,742-line lesson: compute a mechanism-derived ceiling
  BEFORE the run, and count what the carry actually applied.

## Success criterion (fixed now)

Cold scored run: `novel`/`realLn` byte-exact, boot ×4 OK, cache +0.
Hidden name-only churn on 85→86 must fall below the fossil baseline
(exp070-r1: 1,926) and ideally below the pre-fossil ~1,480. Report
carries applied, twins skipped, and coverage of the emitted tree.
