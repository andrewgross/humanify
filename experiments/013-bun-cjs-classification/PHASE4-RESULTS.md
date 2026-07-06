# Phase 4 results — v119→v120 after the 2026-07-06 plan execution

Date: 2026-07-06. Runs A‴/B‴ from a worktree pinned at 1c83a83
(workstreams 1–3 + shorthand hash fix + close-match corroboration gate;
retry batching, retry diet, 2-call cap, temperature 0,
--reasoning-effort low). Artifacts: /tmp/exp013-phase4/. Same inputs
and LLM (gpt-oss-20b) as Phases 2/3.

## Headline

|                             | Phase 3 (A″/B″)           | **Phase 4 (A‴/B‴)**                   |
| --------------------------- | ------------------------- | ------------------------------------- |
| fresh leg wall clock        | 1h55m                     | **23m37s (−79%)**                     |
| incremental leg wall clock  | 2h59m                     | **8m56s (−95%)**                      |
| incremental LLM calls       | ~8,724 fns                | 3,236 calls                           |
| incremental tokens          | (retry tail alone ~4M in) | **6.4M total (6.0M in / 416.9K out)** |
| functions exact-transferred | 33,912                    | **34,057**                            |
| functions close-matched     | 7,576                     | 7,488                                 |
| functions pure-fresh LLM    | 1,148                     | **1,094**                             |
| module bindings cached      | 89.7% (15,617)            | **84.3% (14,637)**                    |
| diff lines                  | 131,437                   | 138,968 (+5.7%)                       |
| rename-noise hunks¹         | 20,181                    | 21,436 (+6.2%)                        |
| rename-noise lines¹         | 65,170                    | 68,460                                |

¹ Same classifier (equal-count change hunks identical after
`\b[A-Za-z_$][A-Za-z0-9_$]*\b` → `#`).

Correctness: both outputs parse; zero parse and zero semantic-gate
failures; only Tier-1 (scopeParent) deadlock relaxations — no Tier-2
force-breaks and no graph-closure violations. Coverage's "Failed: 2/8"
is the derived accounting residual in buildCoverageSummary, not
crashes; the real internalErrors counter landed after this run's
pinned commit (d0db823). The 4+2 "straggler batch failed" log lines
are contained provider throws (by design).

## Reading the noise regression honestly

The +1,255 noise hunks track the module-binding cache drop (−980
transfers, 89.7% → 84.3%), which is a DESIGNED precision trade from
2026-07-06: the two-vote floor and the phantom-pair emission gate
removed single-vote and name-string-keyed transfers. Two things are
true at once:

1. Some removed transfers were CORRECT — those bindings now get fresh
   LLM names each run and every reference becomes a diff hunk. This is
   recoverable recall (lever below).
2. Some were consistent LIES — a phantom vote renaming an unrelated
   binding identically in both legs produced ZERO noise while being
   WRONG. The noise metric measures consistency, not correctness, so
   precision fixes can RAISE it. Sample hunks show the new gates
   firing on real phantom pairs (`dropping q→failedFiles: phantom
pair`).

Meanwhile the close-match body-local transfer is active on the other
side of the ledger (7,013 identifiers skipped as pre-transferred in
leg B‴), partially offsetting the binding losses inside close-matched
bodies.

## Next levers (in value order)

1. **Recover safe single-vote binding transfers**: allow one vote when
   the voting function is exact-matched AND the voted prior name is
   unique among prior module bindings (no collision possible), or route
   single votes through `suggestedName` (LLM hint) instead of dropping.
   ~980 bindings × references each is the biggest single noise block.
2. **Attribute noise hunks by population** (exact / close / binding /
   fresh) using cc-119-diag.json + cc-120-diag.json positions — turns
   the aggregate metric into a per-mechanism scoreboard.
3. Re-run this pair at a commit ≥ d0db823 to also pick up the small-bug
   batch (owns-scope var transfers add binding recall).
4. Cosmetic: the queue-state debug line reports done counts including
   pre-done nodes against an active-only total (done=59992/10450) —
   confusing during triage.
