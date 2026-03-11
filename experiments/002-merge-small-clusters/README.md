# Experiment 002: Merge Small Clusters + Absorb Shared

## Background

Experiment 001 showed that pure BFS reachability produces too many tiny clusters
(44 clusters for 95 functions, avg size 1.3) and an excessive shared ratio (36.8%).

**Why proximity won't work (from 001 analysis):** We analyzed the line gaps between
functions in both the unminified and renamed Preact bundle. The maximum gap between
consecutive functions never exceeds 20 lines — the code is too densely packed for
gap-based splitting. Modules are contiguous in the bundle (Rollup preserves order),
but the inter-module gaps are too small to detect.

**What the data tells us instead:** Most clusters have exactly 1 member. The core
problem is that each exported hook function (useState, useEffect, etc.) becomes
its own root → its own 1-member cluster. And the diff/render infrastructure that
everything depends on gets dumped into shared.

## Hypothesis

A two-phase post-processing step after BFS clustering will significantly improve
cluster quality:

1. **Merge small clusters:** Clusters with ≤ N members should be merged into the
   cluster they're most connected to (by call edges). This should collapse the
   20+ individual hook clusters into a few larger groups.

2. **Absorb shared functions:** After merging, re-evaluate shared functions. If
   a shared function's callers now all belong to one cluster (because clusters
   were merged), it should be absorbed into that cluster.

### Success Criteria (same targets as 001)

1. Cluster count: 5-20 (down from 44)
2. Shared ratio: < 15% (down from 36.8%)
3. MQ score: > 0.3 (up from 0.205)

## Variables

| Variable | Value | Notes |
|----------|-------|-------|
| Fixture | Same as 001 | Preact 10.24.3 core+hooks, 95 top-level fns |
| Min cluster size | 3 | Clusters ≤ 3 members get merged |
| Merge target | Most-connected neighbor | By count of call edges (caller+callee) |
| Shared re-eval | After merge | Re-check if shared fns now have single owner |
| Max merge rounds | 10 | Prevent infinite merging |

## Steps

1. Add `mergeSmallClusters()` post-processing to `src/split/cluster.ts`
2. Add `reabsorbShared()` to re-evaluate shared functions after merging
3. Add tests for both
4. Run against Preact fixture, compare to 001 baseline
5. Record results
