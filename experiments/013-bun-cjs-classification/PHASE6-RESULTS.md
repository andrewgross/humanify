# Phase 6 results — v119→v120 with the structural invariant + throughput knobs

Date: 2026-07-07. Runs from a worktree pinned at `1d71c4f` (item-5 refactor

- A1/A2/B3 fixes + the rename-only **structural invariant** + the
  env-configurable **LLM throughput knobs**: `HUMANIFY_CONCURRENCY=120`,
  `HUMANIFY_MODULE_CONCURRENCY=40`, `HUMANIFY_MAX_TOKENS=2000`,
  `--reasoning-effort low`). Artifacts: `/tmp/exp013-phase6/`.

## Headline: correctness proven, fresh leg 44% faster

|                             | Phase 5 (`5e0fffa`) | **Phase 6 (`1d71c4f`)** |
| --------------------------- | ------------------- | ----------------------- |
| fresh leg wall clock        | 23m54s              | **13m28s (−44%)**       |
| incremental leg wall clock  | 9m8s                | **8m29s**               |
| peak in-flight LLM requests | ~50 (capped)        | **178 (fresh) / 158**   |
| functions exact-transferred | 34,055              | 34,022 (≈)              |
| functions close-matched     | 1,396               | 1,409 (≈)               |
| module bindings cached      | 80.9% (14,052)      | 80.9% (14,054) (≈)      |
| diff lines / hunks          | 143,398 / 24,909    | 143,106 / 24,999        |
| rename-noise share          | 91.9%               | **92.0%**               |
| genuine-change hunks        | 2,015               | 2,001                   |

## The rename-only invariant passed at full-bundle scale

This is the first full run with the structural invariant live. Both legs
(each ~13 MB, ~185K bindings) completed with **no structural-signature
mismatch** — so humanification provably changed nothing but binding names
across the entire bundle, twice. The script runs under `set -euo pipefail`,
so any invariant failure would have aborted it; exit 0 is the proof.

Also: both outputs parse; `internalErrors == 0` (both humanify legs exited
0 through the item-4e CLI gate); the coverage "Failed: 13 / 7" are the known
cosmetic accounting residual. `exactMatch.applied` 97,295 ≈ Phase 5's
97,476 — matching is stable.

## Throughput: the concurrency fix, not reasoning

Reasoning effort was already `low` in Phase 5, so the 44% fresh-leg gain
came from **#2 (concurrency)**: the global rate limiter was capped at the
function lane alone (~50), silently throttling the module lane; it now spans
both lanes, and with 120/40 the server saw **178 concurrent requests** vs ~50
before. The LLM-heavy fresh leg (27,762 function + 17,221 module LLM calls,
49.7M tokens) benefited most; the mostly-cached incremental leg (1,670 +
3,312 LLM calls) is dominated by non-LLM work (graph build, matching,
transfer, the new invariant), so its −7% is expected. `max_tokens=2000` under
low reasoning produced zero truncation/failures.

## The diff is stable

143,106 lines / 24,999 hunks; **92.0% rename-noise** (22,998 hunks) vs 2,001
genuine-change hunks — essentially identical to Phase 5's corrected split.
The throughput/reasoning/token changes did not perturb the output. The
genuine signal (version bump, build metadata, real logic edits, feature
additions) remains ~2K hunks buried in ~23K of identifier churn, so the top
lever is unchanged: **recover safe single-vote binding transfers** to cut the
dominant noise block.

## Follow-ups (unchanged priority)

1. Recover safe single-vote binding transfers (the ~585 dominant noise block).
2. Per-population noise attribution via the diag JSONs.
3. Tune the throughput knobs further if the fleet still has idle replicas
   (watch client p99 < 25s and the failed/unrenamed count).
