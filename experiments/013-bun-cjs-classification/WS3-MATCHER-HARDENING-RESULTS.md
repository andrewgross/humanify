# Workstream 3 measurement — matcher precision hardening (2026-07-06)

Changes measured (commits cc3888b..dee5862): injectivity enforcement,
stop-on-empty cascade, singleton corroboration gate, binding-identity
vote propagation with a two-vote floor, prior-input contract, phantom
graph edges removed.

## Matching-only A/B on the real pair (v119 humanified prior → v120)

One leg per process against the same phase-2 humanified prior and
beautified v120 target; "before" leg run from a worktree pinned at the
pre-change commit.

| leg                | exact  | close | none |
| ------------------ | ------ | ----- | ---- |
| before (cc3888b~1) | 35,241 | 7,682 | 275  |
| after (dee5862)    | 35,157 | 7,810 | 231  |

Transitions before→after: 134 exact→close, 42 close→exact, 37
none→close, 9 none→exact, 1 exact→none, 1 close→none.

- All 135 demoted exacts have their hash PRESENT in the prior with
  bucket sizes 2–3 — multi-candidate buckets the old cascade
  force-resolved via fallback widening or double-claiming. They keep
  prior code as close-match LLM context.
- 51 new exacts are evidence-backed wins that double-claiming
  previously blocked (injectivity demotion + claim-aware propagation).
- Fully-unmatched dropped 275 → 231.
- New gate counters on the real bundle: injectivityDemoted 54,
  singletonRejected 86 (humanified leg) / 71 (beautified leg).

## Cross-leg residual (beautified control vs humanified prior)

Unchanged at 354 moved functions: 158 hash-absent (the known 0.4%
humanify+regenerate hash instability, parking-lot item) and 196 cascade.
The 196 sit in buckets of size 14–44 where the control leg resolves via
the SHINGLE tiebreaker — shingles hash identifier content and are
rename-variant, so they fire for a beautified prior and fail for a
humanified one. This is a recall gap on production legs (falls to
ambiguous → close match), not a precision defect; making shingles
rename-invariant (placeholder-normalized) is the next candidate if the
196 matter.

## Perturbation-lab ground truth

`run.ts --name ws3-check`: 33 rows, zero row-level changes vs the
committed member-key baseline; every TP/FN/FP/TN identical. Avg
precision 99.0%, F1 97.5%. The pre-existing addConsoleLog FN=1 rows are
the perturbed function itself (hash legitimately changed).

**Conclusion:** the precision mechanisms (non-injective matching,
fallback widening, zero-corroboration singletons, name-keyed votes) are
closed with no measurable recall cost on ground truth; the real-pair
cost is 84 net exact matches (−0.24%), all previously evidence-free,
traded for 51 evidence-backed recoveries and 44 fewer full misses.
