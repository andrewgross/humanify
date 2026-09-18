# 089 — line-witness prompt hints: REFUTED, code unmerged

> **STATUS 2026-09-18: negative result, recorded per pre-registration.**
> Implementation lives on branch `exp089-substatement-hints` (gate 8/8,
> TDD-pinned) and is deliberately NOT merged: the walk shows no measurable
> benefit, and the change costs one generate() per close pair plus prompt
> surface. The brief pre-registered exactly this outcome as plausible.

## What was built

`mineLineWitnessHints` (statement-align.ts, on the branch): pairs a
corroborated close-match pair's rendered lines by identifier-blanked
skeleton and adds "previously named X" prompt hints on >=2 unanimous
distinct-shape witnesses — the ask-side twin of exp088's after-the-fact
skeleton-vote restore.

## Walk verdict (GLM era: baseline /work/glm-base-walk, candidate /work/exp089-walk)

| gate                                   | result                                                 |
| -------------------------------------- | ------------------------------------------------------ |
| lever actually FIRED (exp087's lesson) | **YES — hint-decorated prompts 186 → 224 busy (+20%)** |
| `novel`/`realLines` exact              | PASS (986/122,066 busy, 146/33,135 calm)               |
| busy nameOnlyLines                     | 4,144 → 4,120 (**−24 — null**; predicted −100..−300)   |
| busy churnExBuild                      | 24,306 → 24,340 (+34 — null)                           |
| calm hop                               | 277/50 vs 245/16 — draw variance (GLM band unmeasured) |

## Why it's dead (and what that closes)

The model already sees the prior function's full code as context in every
close-match prompt; on GLM, explicit per-binding decoration of line-witness
names adds nothing the model wasn't already inferring. Combined with
exp083's finding (questions don't recur across releases, so answer
determinism is worthless) this **closes the ask-side lever family**: the
remaining word-choice residual is either recovered post-hoc (exp086/088
machinery) or sits behind deliberately-calibrated refusals. Do not
re-propose prompt-decoration levers without new evidence — measure hint
ADOPTION directly first if you do.

Caveat for a future gpt-oss swap-back: this refutation is GLM-era. A weaker
model follows ambient context worse; the branch is kept in case the lever
is worth re-testing after a model downgrade.
