# 069 — naming stability for genuinely-changed code

> **This is a BRIEF — a hypothesis, including its cautions.**
>
> exp065's map: 84% of the ~3,333 fresh LLM asks per hop are CORRECT —
> the code is genuinely new or changed, no prior hash exists. Matching
> cannot help them; yet exp061 proved the resulting churn is
> SYSTEMATIC, not random (the same identifiers re-roll the same way
> across cold repeats — context drift at temperature ~0). The frontier:
> when a function's code changes but its ROLE persists, the ask should
> be anchored so the LLM keeps the prior word instead of drawing a
> synonym (`generateSummary → generateResponse`).

## Hypothesis

The re-roll is driven by what the prompt shows, not by sampling: the
prompt for an edited function carries no memory of what the role was
called last release. Anchoring candidates, in expected-yield order:

1. **Close-match prior names into every re-rename ask** — the
   close-match tier re-renames WITH prior context today; the 84% class
   includes many statements the 055 pairing links to an edited prior
   (masked head + ≥50% overlap) that the ASK never sees. Feed that
   pairing's prior names as "Prior version name" hints (the exp061(b)
   channel, whose LLM compliance measured ~89%).
2. **Neighborhood anchoring** — show the (already settled, carried)
   names of the function's callers/callees in the prompt so the drift
   in surrounding context stops perturbing the draw.
3. **Prompt-order stability** — batch composition and windowed
   used-name lists change per run; measure whether ask-order alone
   moves word choice (two cold runs, same code, prompt dumps diffed).

## Cautions pinned before measuring

- Strict whole-pair ceilings FIRST (exp063 method) per anchoring
  candidate; the 84% class is ~2,800 occurrences/hop but pair-heal
  arithmetic deflates censuses 10–20×. Compute per-candidate ceilings
  before any implementation; sized skips are results.
- exp061(b)'s lesson: hint yield is capped by name contention —
  measure the contention rate for this population with the exp063
  recorder BEFORE predicting yield.
- A changed role SHOULD get a new name (the getTempDirPath principle);
  every anchor is a HINT the model may override, never a carry.
- Rule 10: prompts change ⇒ every verdict needs cold runs; two
  repeats; novel/realLn byte-exact.

## Success criterion (fixed now)

Ceilings first. Any shipped anchor must drop the 055 paired name-only
mass on 85→86 below exp066's 1,476 by more than the ~40–60-line repeat
spread, twice, with hold columns byte-exact and one-sided anchored
mass not rising.

## STATUS (2026-08-14): ceilings EXECUTED, candidate (a) SHIPPED on this branch; (b) and (c) retired by measurement

**Ceilings (three-run stable: exp061-lever-r1/r2 + exp066-r1, 85→86):**

| candidate | strict ceiling (ledger ln) | verdict vs 40–60-ln floor |
|---|---|---|
| (a) edit-pair hints, uncontended | **224 / 182 / 224** (~164/133/164 at the measured 73% landing rate) | **CLEARS — first of six candidates this arc** |
| (a) incl. contended | 308 / 278 / 320 | adjudication remains retired (exp063) |
| (b) neighborhood anchoring | 326 / 284 / 332, but increment over (a) is 6–24 ln | sized skip (dominated; compliance unknowable offline) |
| (c) prompt-order stability | **null result**: batch composition is BYTE-STABLE across cold runs (391/391 identical) yet 46.2% of members drew different names — the drift is prompt CONTENT, not order | nothing to stabilize — skip |

Contention rate in the (a) population: 27–37% (hintable-contended /
all hintable) — consistent with exp061's 86-of-187 decorated landings.

**Shipped (module bindings only, v1):** `src/prior-version/edit-pair-suggest.ts`
pairs no-twin residue statements (masked-head unique both sides, ≥50%
word-token overlap, name-only line pairs, per-identifier unanimity,
below-floor priors excluded) and feeds prior names into the existing
`suggestedName` channel; every hint trail-logged (`edit-pair-suggest`).
Wired in `prior-version.ts` after the twin bridge (finer tiers keep
priority). The function-ask share of the ceiling is NOT shipped — it
needs new prompt surface (exp064's concern) and its share should be
re-measured after this lands.

**Known recall risk, declared before the run:** prior side pairs on
`priorCode` slices, fresh side on regenerated (`path.toString()`)
text — formatting parity is assumed, and any mismatch LOSES pairs
(precision-safe, recall-lossy). If the cold run under-delivers vs the
~120–160-ln module-share expectation, this is the first knob.

**Worktree gate:** typecheck/lint/unit(+5 new tests)/census green;
knip + 25 unit + 27 fingerprint failures proven ENVIRONMENTAL via
stash A/B (identical without the delta; fresh worktrees lack the
gitignored generated fixtures). Parent env is authoritative post-merge.

**Cold runs required (rule 10 — prompts change):**
- `npm run eval -- score exp069-r1` and, if r1 holds, `exp069-r2`.
  Verdict: novel/realLn byte-exact; 055 name-only on 85→86 vs the
  exp066 era's 1,476 — success = below by more than the ~40–60-ln
  spread, twice, anchored one-sided mass not rising; mints must not
  rise above band (below-floor priors are excluded from hints, so the
  exp066 carry-cost class cannot grow through this channel).
