# Phase 5 results — v119→v120 after the item-5 refactor + A1/A2/B3 fixes

Date: 2026-07-07. Runs A⁗/B⁗ from a worktree pinned at `5e0fffa` (the
entire item-5 structural refactor — collector unification, prior-transfer
extraction, Babel-shim deletion, RunConfig, nullable binding fingerprints,
lifecycle state machine — plus the A1 minifier-detection fix, A2
module-binding rename-desync fix, and B3 restored invalid-name retry).
Artifacts: `/tmp/exp013-phase5/`. Same inputs and LLM (gpt-oss-20b,
`--reasoning-effort low`) as Phases 2–4.

## Headline

|                             | Phase 4 (A‴/B‴, `1c83a83`) | **Phase 5 (A⁗/B⁗, `5e0fffa`)** |
| --------------------------- | -------------------------- | ------------------------------ |
| fresh leg wall clock        | 23m37s                     | **23m54s**                     |
| incremental leg wall clock  | 8m56s                      | **9m8s**                       |
| functions exact-transferred | 34,057                     | **34,055** (≈ identical)       |
| module bindings cached      | 84.3% (14,637)             | **80.9% (14,052)** (−585)      |
| functions pure-fresh LLM    | 1,094                      | 1,646                          |
| incremental tokens          | 6.4M in / 416.9K out       | 6.4M in / 457.2K out           |
| diff lines                  | 138,968                    | **143,398** (+3.2%)            |
| rename-noise hunks¹         | 21,436                     | **22,132** (+3.2%)             |

¹ Same classifier (equal-count change hunks identical after
`\b[A-Za-z_$][A-Za-z0-9_$]*\b` → `#`); now checked in as
`classify-diff.py`.

## The refactor is validated

The item-5 refactor was the point of re-running: it rewired the graph
node lifecycle, config resolution, binding collectors, and fingerprints.
At full-bundle scale (43,198 functions, 17,367 module bindings) it is
behaviourally sound:

- **Exact function transfers are stable: 34,055 vs 34,057** — the lifecycle
  state machine + RunConfig + nullable-fingerprint changes did not move the
  exact-match path. This is the load-bearing result.
- **Zero correctness failures.** Both legs' outputs parse (`parseFailure`
  absent in both diag JSONs); both humanify commands exited 0, which — via
  the item-4e CLI gate — means `internalErrors == 0`. The coverage
  summary's "Failed: 7" (0.0%) is the known cosmetic accounting residual in
  `buildCoverageSummary`, not crashes (handoff follow-up #4, still open).
- Transfer-rejection stats are tiny and healthy: exact-match rejected
  `target-visible` 313, `target-in-scope` 20, `shadows-child` 1 (out of
  97,476 applied); zero `invalid-target`. The validated-rename gate works.

## The diff shape (the point of the run)

`diff cc-119/runtime.js cc-120/runtime.js`: **143,398 lines, 24,909 change
hunks**, classified:

| bucket                               | hunks      | lines   |
| ------------------------------------ | ---------- | ------- |
| rename-noise (change, blanked-equal) | **22,132** | 72,760  |
| real modifications                   | 2,205      | 17,420  |
| additions (new code)                 | 410        | 3,163   |
| deletions (removed code)             | 162        | 809     |
| **genuine change (add+del+mod)**     | **2,777**  | ~21,392 |

**88.9% of change hunks are pure rename-noise.** The signal — what actually
changed v119→v120 — is ~2,777 hunks; the rest is the same code with
different minified/humanified identifiers.

## Reading the +3.2% noise honestly

The noise rose in lockstep with the module-binding cache drop (−585
transfers, 84.3% → 80.9%): each un-cached binding gets a fresh LLM name in
v120, and every reference to it becomes a diff hunk. This continues the
designed precision trade first seen in Phase 4 (two-vote floor + phantom
gate). The further drop from 84.3% → 80.9% is not yet root-caused — candidate
causes are (a) B3's restored retry changing some fresh-leg (v119) names,
which shifts cross-function vote agreement for the two-vote binding
transfer, and (b) the 5.4 nullable-fingerprint change altering matchability.
Per-population attribution via the diag JSONs (handoff follow-up #2) is the
way to separate them.

**The top lever is unchanged and now more relevant: recover safe
single-vote binding transfers** (exact-matched voter + prior-unique name, or
downgrade to `suggestedName`). Module-binding noise is still the dominant
reducible block.

### Metric caveat — don't compare `closeMatch` across phases naively

`coverage.functions.closeMatch` is **1,396** here vs Phase 4's reported
"close-matched 7,488" — but these are different slices. Coverage attributes
a close-matched function to `llm` once it also gets LLM names for its
remaining identifiers, so `coverage.functions.closeMatch` undercounts
close-matching. The truer signal is `transferStats.closeMatch`: **5,810
transfers applied / 15,943 attempted** — close-matching is very much active.
Phase 4's "7,488" was the matcher's pair-count (`priorVersionCloseMatch`),
not this coverage slice. (Worth unifying the two into one clearly-named
number — a small reporting cleanup.)

## Harness bug found + fixed (by running)

`run-phase2.sh` imported `validateOutputParses`, which item 4f deleted from
`src/output-validation.ts` as dead code. Under `set -euo pipefail` the
resulting `TypeError` aborted the script **after** both humanify legs
succeeded but **before** the diff step, so `runtime-diff.txt` was never
produced (the diff in this doc was generated by hand). Fixed: the diff now
runs first (it is the deliverable and can't be blocked), the parse check
uses `@babel/core` `parseSync`, and the noise classifier
(`classify-diff.py`) runs automatically. This is a new instance of the
"stale reference to deleted code" hazard the duplication review warns about,
in the experiment harness rather than `src`.

## Follow-ups (value order)

1. Recover safe single-vote binding transfers — the ~585 (now) dominant
   noise block.
2. Per-population noise attribution via the diag JSONs — root-cause the
   84.3% → 80.9% binding-cache drop (B3 name variance vs 5.4 fingerprints).
3. Unify `closeMatch` reporting into one clearly-named metric.
4. Operator normalization (biggest remaining naming-consistency win, from
   the lost cross-version-diff-gaps note).
5. Fix the cosmetic "Failed: N" coverage accounting residual.
