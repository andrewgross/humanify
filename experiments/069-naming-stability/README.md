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
