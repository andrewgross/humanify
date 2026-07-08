# Slot-keyed exact transfer — A/B results (2026-07-07)

Fix under test: commit `c32e7ea` (branch `exp014-slot-keyed-transfer`) —
`translatePriorNames` keeps placeholder pairs slot-keyed with each pair
carrying the new version's resolved `Binding`, and the transfer renames
through that binding's own scope instead of a name-string lookup.

## The bug it fixes (red/green tested)

Two distinct bindings can share one minified name — a catch param
shadowing a function-scope binding is standard Bun output. The old
`Record<minifiedName, priorName>` kept only the LAST colliding pair, and
the apply side's first-name-wins owned-binding lookup then applied it to
the FIRST binding: the function-scope binding wore the catch binding's
prior name while the catch param stayed minified. A live precision bug
(wrong name on wrong binding), plus two noise entries per collision
(one transfer-gap, one asymmetric).

Repro test: `src/rename/plugin-cross-version.test.ts`
("transfers distinct prior names to a function-scope binding and a catch
param that share one minified name") — before the fix the output was
literally `let errorDetails = buildPayload(input) ... catch (K)`.

## Methodology

Seeded `PHASE2_OUT/cc-119` with the **phase-6 baseline's** fresh v119
output (`/tmp/exp013-phase6/cc-119`, produced at `1d71c4f`), so
`run-phase2.sh` skips the fresh leg and only the incremental v120 leg
re-runs with the fix. Both diffs therefore share the identical v119 side;
the delta is the incremental leg (fix + LLM nondeterminism in the ~1.7K
LLM-named functions). Beware: `/tmp/exp013-phase2/` holds an OLDER
pre-binding-keyed-hashing run — not a valid baseline.

## Results

| metric                    | phase-6 baseline | slot-keyed fix | delta                     |
| ------------------------- | ---------------- | -------------- | ------------------------- |
| diff lines                | 143,106          | 72,157         | −50%                      |
| noise hunks               | 22,998           | 10,128         | −56%                      |
| noise share of hunks      | 92.0%            | 83.8%          | −8.2pp                    |
| genuine-change hunks      | 2,001            | 1,953          | ~stable (hunk re-carving) |
| rename occurrences        | 47,761           | 18,848         | −61%                      |
| distinct renamed bindings | 12,299           | 5,533          | −55%                      |

Buckets (occurrences):

| bucket          | baseline       | slot-keyed fix | delta |
| --------------- | -------------- | -------------- | ----- |
| transfer-gap    | 25,839 (54.1%) | 11,885 (63.1%) | −54%  |
| asymmetric      | 14,572 (30.5%) | 1,586 (8.4%)   | −89%  |
| minifier-reroll | 7,350 (15.4%)  | 5,377 (28.5%)  | −27%  |

Asymmetric split by direction (the mechanism check):

- v119-named → v120-minified (transferred fns leaving shadowed/catch
  bindings minified): **13,916 → 1,060 (−92%)** — the fix's target.
- v119-minified → v120-named: 656 → 526 (v119 fresh-leg coverage gaps;
  out of scope for a v120-side fix).

Transfer stats (`cc-120-diag.json`): exact-match pairs attempted
210,795 → 229,620 (+18,825 — pairs the Record used to collapse),
applied 97,295 → 101,464. Close-match stats byte-identical (path
untouched). Rejections rose (target-visible 315 → 691): more pairs
attempted; recreating the prior version's shadowing is correctly refused.

Run health: both outputs parse; structural invariant clean; coverage
buckets unchanged (cached 34,022, closeMatch 1,409, llm 1,670 → 1,668);
`failed` 7 → 9 (LLM-timeout flake, untouched code path).

## What remains (post-fix top contributors)

- `QH → dH` (474 occ) and the reroll bucket (now the #2 bucket at
  28.5%): module-scope / factory-body bindings neither leg names.
  Archaeology (see git history of `skip-list.ts`, `c1492b1`) says these
  are excluded for LLM cost, not safety → stable-token transfer
  (rename `dH` back to `QH`) via `attemptValidatedRename` is viable but
  needs graph visibility for classified factory bodies.
- `serializeWithErrorHandling → convertObjectToJson` (422 occ):
  structurally CHANGED function (v119 twin truly disappeared per
  `fresh-pool-overlap.ts`) → close-match LLM re-name. LLM-determinism /
  close-match-context territory, not transfer coverage.
- transfer-gap residual (11.9K occ): dominated by ambiguous-bucket
  identity-cracking misses — 5,907 avoidable per the fresh-pool
  diagnostic (76 1:1 gate rejections of which only 5 are real bugs;
  mega-buckets like "default" ~4.6K members and noopFunction ~1K).
