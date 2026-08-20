# 086 — BRIEF (hypothesis): keep the prior name as a LAST RESORT for changed code

> Decision context (Andrew, 2026-08-20): "fine to keep the name, assuming it's
> not breaking assignments, changing the actual value etc. If it's just
> semantic I think it's worth using it as a last resort."

## Safety framing

Renames here are ALWAYS semantics-preserving by construction: everything goes
through the validated rename path (scope-checked, collision-checked — the same
gate every LLM name passes). Nothing about values or assignments can change;
the only risk is a MISLEADING name on repurposed code. Andrew accepts that
risk as a last resort, and exp081's sampled cases show the model's fresh pick
is often worse than the stale name anyway (`chatMessage →
lastProgressTimestamp`).

## Target population (measured, exp083, post-veto ~unchanged)

Of the busy hop's ~5,100 name-only lines, the addressable naming half:

- **~844 line-pairs (~1,700 git lines): the prior name never reached the
  prompt.** The old name sits on a MASKED-IDENTICAL line in the prior tree —
  the correspondence is provable from line content alone — yet neither
  transfer nor hint surfaced it. Example: `tokenData → tokenRecord`.
- ~703 line-pairs upper bound: name was visible in the prompt but the answer
  differed; a chunk is name contention (another binding took it). Secondary.

## Where the mechanism belongs — TO BE ANSWERED FIRST (task 0)

The post-hoc restore pass (`src/rename/diff-reconcile.ts`) is exactly "last
resort": it runs after naming, restores prior names only where the diff shows
name-only change, and its refusal ladder is measured well-calibrated. But on
the busy hop it saw only ~590 candidates against ~2,400 non-alias rename
pairs — the mass never reaches its candidate gate.

**Task 0 (census, no code): why.** Take the ~844 no-hint pairs, run each
through diff-reconcile's candidate discovery, and bucket the drop reasons
(not-in-candidate-set / two-witness / decl-not-clean / ...). The lever is
sized by the biggest bucket, not assumed. Suspects: candidate discovery is
hunk-based and misses pairs inside hunks that also carry real edits nearby;
and multi-occurrence bindings split across hunks.

## Design constraints (from the decision)

- LAST RESORT: only fire where the binding's fresh name came from the model
  (never override a transfer), and only when the prior name is legal in scope
  and unclaimed (existing two-witness/occurrence machinery decides "defensible").
- Respect the calibrated refusals: single-mention bindings stay refused.

## Gates and predictions (pre-registered where possible)

- Task 0 census first; sizing quoted only after sampling what the filter
  wrongly includes (measurement-pitfalls discipline).
- red/green unit tests per new acceptance rule.
- `npm run check` 8/8.
- Cold walk (this lever touches naming → walk is mandatory):
  `nameOnlyLines` down by at least several hundred (exact prediction set
  after task 0); `novel`/`realLines` EXACT; calm hop inside spread.
- Spot-check 20 restored names by hand for the misleading-name risk before
  merging; report the sample in RESULTS.

Ceiling if the whole no-hint bucket were reachable: ~1,700 git lines busy;
realistic target after refusals will be set by task 0.

## Task-0 RESULT (2026-08-20) — the candidate-discovery census

Replayed the per-file reconcile (production options, apply:false) over all
166 file pairs holding non-alias rename pairs on the busy hop. Two traps
found on the way: the strategy trails under-record this pass (per-file skips
never reach them — the replay is ground truth, not the trails), and the
`decl-not-clean` refusal includes SELF-dependency (a binding referenced on
its own declaration line can never satisfy the applied-dependencies proof).

| bucket                                                                   | line-pairs | share |
| ------------------------------------------------------------------------ | ---------: | ----: |
| not-a-candidate (hunk geometry: oversized / add-delete / unbalanced)     |        970 |   40% |
| import-declaration (correct — derived alias lines)                       |        379 |   16% |
| **decl-not-clean → RETRIED by the new tier**                             |        345 |   14% |
| **mixed-dirty-occurrence → RETRIED by the new tier**                     |        228 |    9% |
| corpus-gated (file too dissimilar — protective)                          |        179 |    7% |
| consumer-\* + occurrence-outside-diff + small tail (calibrated refusals) |        323 |   13% |

## Shipped increment + pre-registered walk predictions

`lastResortTier` in diff-reconcile: after all normal rounds settle, groups
refused ONLY for the two retried reasons re-gate with those line-cleanliness
checks relaxed. New `same-name-siblings` guard (the pinned post-split
corruption test caught the relaxed round reaching co-renamed siblings).

- Offline sizing: **547 line-pairs (~1,094 git lines) restore on the busy
  hop**; +16 land on `rename-rejected:target-in-scope` (correct refusals).
- Hand review of 20 restores: most neutral-or-better (`heartbeatCleanup →
hookEvent` reverts model nonsense), 2 mild downgrades
  (`failureThresholdMs → failureThreshold`), 0 misleading catastrophes.
- Walk (baseline = the exp084+085 walk): `nameOnlyLines` down ≈900–1,100;
  `novel`/`realLines` EXACT; calm hop inside the 32-line spread; boot gates
  pass on all four trees.

## Left open (sized, not attempted)

- not-a-candidate (970): needs hunk-geometry work — cross-hunk skeleton
  pairing for add/delete regions, an oversized-hunk policy. Separate
  increment with its own sizing.
- corpus-gated (179): protective; would need module-identity evidence to
  relax safely.

## WALK VERDICT (2026-08-20, /work/exp086-walk) — CONFIRMED

| gate                                   | result                                                          |
| -------------------------------------- | --------------------------------------------------------------- |
| 4 hops exit 0 + boot gates both halves | PASS                                                            |
| `novel` / `realLines` exact            | **PASS — 986 / 122,066 busy, 146 / 33,135 calm**                |
| busy nameOnlyLines                     | **5,166 → 4,380 (−786; predicted −900..−1,100, draw band ±90)** |
| busy churnLinesExBuild                 | **25,424 → 24,629 (−795, ≈23× the 35-line floor)**              |
| calm hop                               | 273 exBuild / 46 nameOnly — at baseline, inside spread          |

The −786 sits at the prediction range's lower edge; walk-1's nameOnly was
itself +86 above the earlier baseline, so the point estimate lands inside
draw variance. Direction and magnitude confirmed.
