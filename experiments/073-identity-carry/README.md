# 073 — carry names for provably-identical modules

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
