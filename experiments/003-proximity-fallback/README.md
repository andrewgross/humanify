# Experiment 003: Source Proximity Fallback for Isolated Functions

## Background

Experiment 002 showed that merge+reabsorb successfully eliminates shared functions
(36.8% → 0%) and produces 3 meaningful clusters that align with Preact's architecture:
- Core diff/render (26 members)
- DOM operations (24 members)
- Hooks (14 members)

However, **23 of 95 top-level functions remain as isolated singletons** — they have
zero call edges to any other top-level function. These include constructors (`BaseComponent`),
exported API functions (`createRef`, `Fragment`), event handlers (`dispatchEvent`),
and functions called via property access patterns (`options._catchError`).

The call graph cannot help here because the connections are invisible to static analysis.

**From experiment 001**: We showed that modules are contiguous in scope-hoisted bundles
(Rollup preserves module order). Although inter-function gaps are too small for
gap-based *splitting*, the relative position of a function tells us which module
region it belongs to.

## Hypothesis

Using source line proximity as a **fallback** (only for functions with no call edges)
will correctly assign most isolated singletons to the cluster they belong to, because:
1. Rollup emits modules contiguously
2. Functions within the same module are co-located in source
3. An isolated function at line 110 is more likely to belong to the same module as
   the cluster spanning lines 100-200 than to a cluster at lines 2000-2500

### Success Criteria

1. Cluster count: 3-10 (down from 29)
2. Shared ratio: remains 0%
3. MQ score: > 0.3 (up from 0.207)
4. Singletons absorbed into structurally appropriate clusters

## Variables

| Variable | Value | Notes |
|----------|-------|-------|
| Fixture | Same as 001/002 | Preact 10.24.3, 95 top-level fns |
| Base clustering | BFS + merge(minSize=3) | From experiment 002 |
| Proximity metric | Nearest cluster by line distance | min(|fn.line - cluster.centroid|) |
| Centroid | Median start line of cluster members | More robust than mean for skewed clusters |
| Scope | Only isolated singletons | Functions with zero top-level callers AND callees |

## Steps

1. Add `mergeByProximity()` post-processing to `src/split/cluster.ts`
2. Compute cluster centroids (median line of members)
3. For each singleton with no edges, find nearest cluster by line distance
4. Merge singleton into that cluster
5. Run against Preact fixture, compare to 002 baseline
6. Map resulting clusters back to original Preact modules for validation
