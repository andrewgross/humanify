# Live test: per-AST cache merge (7b55f81) — what to watch on the next hops

Audience: the agent babysitting walk RUN 4. Main was switched from `ade8eae`
to `7b55f81` (per-AST analysis caches, branch feat/per-ast-analysis-cache)
mid-walk, around hop 2.1.174. The walk itself is now the live test of that
change. This doc is the pass/fail spec. Rollback is one command and is listed
last — read it before you need it.

## 0. Identify the true canary hop

tsx loads all modules at process start; a hop whose worker was already
running at merge time executed OLD code regardless of what's on disk. The
canary is the first hop whose worker STARTED after the merge:

    git -C ~/Development/humanify show -s --format='merge at %ci' 7b55f81
    head -3 ~/Development/unpacked-claude-code/logs/walk-2.1.174.log   # first timestamp = hop start
    head -3 ~/Development/unpacked-claude-code/logs/walk-2.1.175.log

If 174's start predates the merge, 174 is old-code and **175 is the canary**.
Judge the canary AND the hop after it (the second hop consumes a prior that
may itself have been produced by new code — that closes the loop on
cross-code-version inheritance).

## 1. Hard gates (same spec as any RUN-4 hop; RUNNER.md)

For canary version V:

    ~/Development/unpacked-claude-code/scripts/review-version.sh V   # exit 0 = PASS

plus, in `logs/walk-V.log`:

- driver line `[V] done` (humanified.js existing alone is NOT done — it is
  written mid-hop);
- no `parseFailure`, no `semanticFailure`, no "structural signature"
  mismatch, no "does not parse". A structural-signature failure on the
  canary is a red flag aimed exactly at this change (hash-walk behavior) —
  treat as FAIL, capture the log, roll back.

## 2. Fix-specific signals

**a. Inheritance must hold.** The change is supposed to be hash-neutral (33
fingerprint snapshots byte-identical). The canary matches against a prior
produced by OLD code, so any hash drift shows up here as a collapsed match
rate. Compare against hops 171-174:

    grep -E "inheriting|matched|prior" ~/Development/unpacked-claude-code/logs/walk-V.log | tail -20

PASS: inherit % and matched counts in the same band as recent hops (≥85%
gate, typically ~87-96%). FAIL: a step drop (e.g. 90% → 60%) → rollback; that
is hash instability, not noise.

**b. Churn must look normal.** After the hop's commit lands in the history
repo, the diff-vs-prior should look like neighbors' (tens of files, feature
noise) — a mass-rename or mass-moved-file diff on the canary = FAIL.

**c. The hang signature must never appear.** Log silent >10 min AND one core
pinned at 100% AND LLM GPUs idle; confirm with:

    sample $(pgrep -f 'tsx src/index' | tail -1) 4 | grep -cE 'Rehash|WeakCollectionSet'

Nonzero = the pathology. Under per-AST caches this cannot come from the
analysis maps anymore; if you see it, it is a NEW site (likely Babel's own
cache) — capture the full `sample` output to a file, then roll back and
report rather than retrying.

**d. Perf expectations (soft — do not gate on these).** 2.1.175-185
(~17-20MB) should run at parity or a few minutes faster (prior-match and
graph phases). The payoff window is ≥2.1.186 (26-32MB): previously ~1/3 of
those hops nondeterministically ran 28-40 min; they should now trend toward
the 13-18 min band. One slow big hop is contention noise; a >30 min hop with
the section-c signature is a FAIL.

**e. Memory shape.** Naming-phase RSS may sit modestly higher than before
(the new AST's binding cache stays warm through naming instead of being
wiped — that is by design). Post-naming release behavior is unchanged. Only
an OOM-kill is a failure signal here.

## 3. What NOT to be alarmed by

- knip on main reports 2 findings — pre-existing before the merge, identical
  on ade8eae.
- `preserveAstCaches` still exists but now governs only Babel's cache; the
  prior-boundary reset became a Babel-only clear (`clearBabelCacheAfterPriorMatch`).
  Log lines about analysis-cache resets are GONE by design — their absence
  is not a missing step.
- The worktree `~/Development/humanify-percache` still exists (source of the
  merge) — leave it; it is inert.
- Hops that completed on new code and PASSED gates never need rebuilding —
  a valid output tree is valid regardless of which code produced it.

## 4. Rollback (hard-gate failure or hang signature)

Nothing else has landed on main during the walk, so rollback is exact:

1. Stop the chains: kill the `walk-versions.sh` / `walk-resume.sh` drivers
   AND the 30-min re-arm loop (`pgrep -fl 'walk-resume|walk-versions|while true; do sleep 1800'`).
2. `git -C ~/Development/humanify reset --hard ade8eae`
3. Delete the FAILED version's output dir under
   `~/Development/unpacked-claude-code/versions/` (only the failed one —
   passed hops stand).
4. Re-arm: `nohup ./scripts/walk-resume.sh forward 2.1.211 >> logs/BATCH-forward.log 2>&1 &`
   (it rebuilds the deleted hop on old code and continues).
5. Report what failed with the log + any `sample` capture; the branch stays
   intact in the worktree for a post-mortem.

## 5. Context

Implementation, measurements, and the residual-cost attribution (bounded
live-heap GC tax during prior matching; subprocess-FingerprintIndex as the
follow-up lever) are in `docs/issue-ephemeron-cache-thrash.md` and
`experiments/032-prior-match-naming/RESULTS.md` (both updated on the merged
branch), and in memory: `project_per_ast_cache_branch`,
`project_ephemeron_cache_fix`.
