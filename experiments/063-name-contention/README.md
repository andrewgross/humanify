# 063 — name contention: every blocked reuse is an error somewhere

> **STATUS (2026-08-13, branch exp063-name-contention): Task 0 SHIPPED as
> an instrument; Tasks 1–3 are SIZED SKIPS — the adjudication hypothesis
> did not survive its own ceiling.**
>
> `ceilings.ts` (two-run stable on exp061-lever-r1/r2, computed BEFORE
> any lever code, rule 5/6):
>
> | lever                                              | loose bound | STRICT ceiling (ledger lines, 85→86) |
> | -------------------------------------------------- | ----------: | -----------------------------------: |
> | Task 1 adjudicate decorated hint landings          |         142 |                                **2** |
> | Task 2 contradictory-vote holders (superset bound) |           — |                               ≤70–74 |
> | Task 3 lib-instance ordinal carry                  |           — |                            **10–12** |
>
> The repeat spread on this metric is ~40–60 lines. Task 1's strict
> ceiling is 2 because the decorated landings sit on lines whose PRIOR
> side differs for upstream reasons (`initializeApp152 →
initializeModule150Val`: an exact landing still churns) — contention
> caps the HINT's exact-landing rate, but un-capping it would not move
> the diff. Task 3's "≤382 lines" from exp062 measured a different
> population; on the paired name-only ledger it is 10–12 lines. Task 2's
> bound is loose, overlaps the existing reconcile pass, and healing it
> requires knowing the right name — which is the adjudication problem
> whose strict form just measured 2 lines.
>
> What SHIPPED: the name-contention recorder (`nameContention` diag
> section: requested/resolvedTo/oldName/site for every collision-ladder
> decoration, at both resolution sites — wave barrier and remaining
> pass). Diagnostics-only; gated by `npm run check` (8/8) + a warm
> neutrality byte-identity run. The instrument stands so the error
> classes stay measurable when future levers (exp064's edit-pair hints,
> exp065's matcher fixes) shift them.
>
> Holder classification (from exp061's census, unchanged): 71/86
> holders are cascade-named; the i36/Pd8 case is a DUPLICATE-HEIR
> contest, which no adjudication can settle without prior-release
> caller sets — recorded as exp065 input, not implemented here.

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> Read `061-hidden-name-churn/README.md` (case study + fall-through
> census) first. exp061's hint lever measured ~89% LLM compliance but
> 86 of 187 hints landed DECORATED: the exact prior name was already
> claimed — 71 times by a binding the cascade named. Andrew's principle,
> adopted as the design axiom: **a request to reuse a name that is
> already taken proves an error somewhere** — a wrong holder, a
> duplicate heir, or a corrupted vote — and each cause needs a
> different fix.

## Hypothesis

Making contention first-class — counted, classified, and adjudicated by
caller-set evidence — converts most decorated hint landings into exact
restorations and removes the double-count (a mis-held name churns both
its wrongful holder's lines AND its rightful heir's).

## Tasks

0. **Hint-collision counter in the run diag** (holder tier, holder vs
   claimant caller sets, contradictory-vote flag — i36 held votes for
   TWO names and the cascade applied one of them anyway). Measure across
   the four pairs; this instrument is the experiment's gate check.
1. **Adjudication:** when a vote-backed claimant contests a
   cascade-named holder, corroborate by caller sets/roles; the loser
   yields. A cascade assignment whose own vote set is contradictory
   should not win by default.
2. **Caller-set identity for real duplicate copies** (the AWS family:
   85 had ONE copy, 86 had TWO): before the matcher's scope-ordinal
   rung fires on content-identical families, require caller-set
   corroboration; new copies mint a stable derived name instead of a
   collision-ladder accident.

## Cautions pinned before measuring

- exp058(B): a widened carry was the identity function — adjudication
  must move names only on POSITIVE caller-set evidence, never "the
  claimant asked first".
- exp044: +3,742 ln from second-order collision effects with an honest
  ceiling — LOG every adjudication; compute the ceiling from exp061's
  contention census BEFORE the run.
- Positional/ordinal NAME assignment is a documented disaster
  (exp035/036, +50,606 noiseLn). Stable derived names must key off
  content + caller evidence, not order.
- Scope-ordinal is still the only signal for some true twins — demote
  it only where caller evidence EXISTS and contradicts.

## Success criterion (fixed now)

Decorated hint landings on 85→86 drop from ~86 toward 0 with exact
landings rising accordingly; paired name-only mass falls below exp061's
1,440–1,500; contested-name census trends to duplicates-only;
`novel`/`realLn` byte-exact. Cold trees, ledgers, twice.

## Scope addendum (2026-08-13, from exp062's refutation)

exp062 proved the vendor content hash is RENAME-BLIND, so every
one-line forwarding stub collides on one hash (`lib_eb5345cb` is the
hash of the stub SHAPE); the `-N` ordinals are the real per-target
identity and are assigned by CENSUS ORDER, churning ≤382 src lines on
85→86 when instances appear/disappear. That is this experiment's
problem, not a collapse problem: **Task 3 — ordinal carry.**
Disambiguate same-shape vendor hashes by FORWARD-TARGET identity and
inherit `-N` assignments from the prior release (mint only for
genuinely new instances), inside the existing
vendorNamer/priorVendorNames owner. Same axiom as the rest of this
brief: identity comes from what a thing points at and who points at
it, never from the order a walk discovered it.
